use std::cell::{Ref, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState, InterruptConfig};

const ADPS_MASK: u8 = 0x7;
const ADIE: u8 = 0x8;
const ADIF: u8 = 0x10;
const ADSC: u8 = 0x40;
const ADEN: u8 = 0x80;

const MUX_MASK: u8 = 0x1f;
const ADLAR: u8 = 0x20;
const MUX5: u8 = 0x8;
const REFS2: u8 = 0x8;
const REFS_MASK: u8 = 0x3;
const REFS_SHIFT: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdcMuxInputType {
    Constant = 2,
    SingleEnded = 0,
    Differential = 1,
    Temperature = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AdcMuxInput {
    Constant {
        voltage: f64,
    },
    SingleEnded {
        channel: u8,
    },
    Differential {
        gain: f64,
        positive_channel: u8,
        negative_channel: u8,
    },
    Temperature,
}

impl AdcMuxInput {
    pub fn input_type(&self) -> AdcMuxInputType {
        match self {
            Self::Constant { .. } => AdcMuxInputType::Constant,
            Self::SingleEnded { .. } => AdcMuxInputType::SingleEnded,
            Self::Differential { .. } => AdcMuxInputType::Differential,
            Self::Temperature => AdcMuxInputType::Temperature,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdcReference {
    Avcc = 0,
    Aref = 1,
    Internal1V1 = 2,
    Internal2V56 = 3,
    Reserved = 4,
}

pub type AdcMuxConfiguration = BTreeMap<u8, AdcMuxInput>;

#[derive(Clone, Debug, PartialEq)]
pub struct AdcConfig {
    pub admux: u16,
    pub adcsra: u16,
    pub adcsrb: u16,
    pub adcl: u16,
    pub adch: u16,
    pub didr0: u16,
    pub adc_interrupt: u8,
    pub num_channels: u8,
    pub mux_input_mask: u8,
    pub mux_channels: AdcMuxConfiguration,
    pub adc_references: Vec<AdcReference>,
}

pub fn atmega328_channels() -> AdcMuxConfiguration {
    let mut channels = AdcMuxConfiguration::new();
    for channel in 0..=7 {
        channels.insert(channel, AdcMuxInput::SingleEnded { channel });
    }
    channels.insert(8, AdcMuxInput::Temperature);
    channels.insert(14, AdcMuxInput::Constant { voltage: 1.1 });
    channels.insert(15, AdcMuxInput::Constant { voltage: 0.0 });
    channels
}

pub fn adc_config() -> AdcConfig {
    AdcConfig {
        admux: 0x7c,
        adcsra: 0x7a,
        adcsrb: 0x7b,
        adcl: 0x78,
        adch: 0x79,
        didr0: 0x7e,
        adc_interrupt: 0x2a,
        num_channels: 8,
        mux_input_mask: 0xf,
        mux_channels: atmega328_channels(),
        adc_references: vec![
            AdcReference::Aref,
            AdcReference::Avcc,
            AdcReference::Reserved,
            AdcReference::Internal1V1,
        ],
    }
}

type AdcReadCallback = Box<dyn FnMut(&mut AdcState, &mut CpuState, AdcMuxInput)>;

pub struct AdcState {
    cpu: Weak<RefCell<CpuState>>,
    config: AdcConfig,
    pub channel_values: Vec<f64>,
    pub avcc: f64,
    pub aref: f64,
    converting: bool,
    conversion_cycles: u64,
    adc_interrupt: InterruptConfig,
    on_adc_read: Option<AdcReadCallback>,
}

impl AdcState {
    pub fn reference_voltage(&self) -> f64 {
        let cpu = self
            .cpu
            .upgrade()
            .expect("CPU was dropped while its ADC is still in use");
        let value = reference_voltage_for(&cpu.borrow(), self);
        value
    }
}

#[derive(Clone)]
pub struct Adc(Rc<RefCell<AdcState>>);

impl Adc {
    pub fn new(cpu: Cpu, config: AdcConfig) -> Self {
        let adc_interrupt = InterruptConfig {
            address: config.adc_interrupt,
            enable_register: config.adcsra,
            enable_mask: ADIE,
            flag_register: config.adcsra,
            flag_mask: ADIF,
            constant: false,
            inverse_flag: false,
        };
        let adc = Adc(Rc::new(RefCell::new(AdcState {
            cpu: cpu.downgrade(),
            channel_values: vec![0.0; config.num_channels as usize],
            avcc: 5.0,
            aref: 5.0,
            config,
            converting: false,
            conversion_cycles: 25,
            adc_interrupt,
            on_adc_read: None,
        })));

        let adc_weak = Rc::downgrade(&adc.0);
        adc.0.borrow_mut().on_adc_read = Some(Box::new(
            move |state: &mut AdcState, cpu: &mut CpuState, input: AdcMuxInput| {
                let voltage = match input {
                    AdcMuxInput::Constant { voltage } => voltage,
                    AdcMuxInput::SingleEnded { channel } => state
                        .channel_values
                        .get(channel as usize)
                        .copied()
                        .unwrap_or(0.0),
                    AdcMuxInput::Differential {
                        gain,
                        positive_channel,
                        negative_channel,
                    } => {
                        let positive = state
                            .channel_values
                            .get(positive_channel as usize)
                            .copied()
                            .unwrap_or(0.0);
                        let negative = state
                            .channel_values
                            .get(negative_channel as usize)
                            .copied()
                            .unwrap_or(0.0);
                        gain * (positive - negative)
                    }
                    AdcMuxInput::Temperature => 0.378125,
                };
                let reference = reference_voltage_for(cpu, state);
                let raw_value = voltage / reference * 1024.0;
                let result = if raw_value.is_nan() {
                    0
                } else {
                    raw_value.floor().clamp(0.0, 1023.0) as u16
                };
                let sample_cycles =
                    state.conversion_cycles * prescaler_for(cpu, &state.config) as u64;
                let adc_weak = adc_weak.clone();
                cpu.add_clock_event(
                    Box::new(move |cpu| {
                        if let Some(adc) = adc_weak.upgrade() {
                            complete_adc_read_inner(&mut adc.borrow_mut(), cpu, result);
                        }
                    }),
                    sample_cycles,
                );
            },
        ));

        let adcsra = adc.0.borrow().config.adcsra;
        let writer = adc.clone();
        cpu.set_write_hook(
            adcsra,
            Box::new(move |cpu, value, old_value, _addr, _mask| {
                writer.handle_adcsra_write(cpu, value, old_value)
            }),
        );

        adc
    }

    pub fn channel_values(&self) -> Ref<'_, [f64]> {
        Ref::map(self.0.borrow(), |state| state.channel_values.as_slice())
    }

    pub fn set_channel_value(&self, channel: usize, voltage: f64) {
        if let Some(value) = self.0.borrow_mut().channel_values.get_mut(channel) {
            *value = voltage;
        }
    }

    pub fn avcc(&self) -> f64 {
        self.0.borrow().avcc
    }

    pub fn set_avcc(&self, value: f64) {
        self.0.borrow_mut().avcc = value;
    }

    pub fn aref(&self) -> f64 {
        self.0.borrow().aref
    }

    pub fn set_aref(&self, value: f64) {
        self.0.borrow_mut().aref = value;
    }

    pub fn reference_voltage(&self) -> f64 {
        self.0.borrow().reference_voltage()
    }

    pub fn prescaler(&self) -> u32 {
        let state = self.0.borrow();
        let cpu = state
            .cpu
            .upgrade()
            .expect("CPU was dropped while its ADC is still in use");
        let value = prescaler_for(&cpu.borrow(), &state.config);
        value
    }

    pub fn sample_cycles(&self) -> u64 {
        let state = self.0.borrow();
        let cpu = state
            .cpu
            .upgrade()
            .expect("CPU was dropped while its ADC is still in use");
        let prescaler = prescaler_for(&cpu.borrow(), &state.config);
        state.conversion_cycles * prescaler as u64
    }

    pub fn complete_adc_read(&self, value: u16) {
        let cpu = self
            .0
            .borrow()
            .cpu
            .upgrade()
            .expect("CPU was dropped while its ADC is still in use");
        self.complete_adc_read_with(&mut cpu.borrow_mut(), value);
    }

    pub fn complete_adc_read_with(&self, state: &mut CpuState, value: u16) {
        match self.0.try_borrow_mut() {
            Ok(mut adc) => complete_adc_read_inner(&mut adc, state, value),
            Err(_) => {
                let adc = self.clone();
                state.add_clock_event(
                    Box::new(move |cpu| {
                        complete_adc_read_inner(&mut adc.0.borrow_mut(), cpu, value);
                    }),
                    1,
                );
            }
        }
    }

    pub fn set_on_adc_read(&self, callback: Option<AdcReadCallback>) {
        self.0.borrow_mut().on_adc_read = callback;
    }

    fn handle_adcsra_write(&self, cpu: &mut CpuState, value: u8, old_value: u8) -> bool {
        let mut adc = self.0.borrow_mut();
        if value & ADEN != 0 && old_value & ADEN == 0 {
            adc.conversion_cycles = 25;
        }

        cpu.set_data_byte(adc.config.adcsra, value);
        cpu.update_interrupt_enable(adc.adc_interrupt, value);

        if adc.converting || value & ADSC == 0 {
            return true;
        }

        let sample_cycles = adc.conversion_cycles * prescaler_for(cpu, &adc.config) as u64;
        if value & ADEN == 0 {
            let adc_handle = self.clone();
            cpu.add_clock_event(
                Box::new(move |cpu| adc_handle.complete_adc_read_with(cpu, 0)),
                sample_cycles,
            );
            return true;
        }

        let mut channel = cpu.data_byte(adc.config.admux) & MUX_MASK;
        if cpu.data_byte(adc.config.adcsrb) & MUX5 != 0 {
            channel |= 0x20;
        }
        channel &= adc.config.mux_input_mask;
        let input = adc
            .config
            .mux_channels
            .get(&channel)
            .copied()
            .unwrap_or(AdcMuxInput::Constant { voltage: 0.0 });

        adc.converting = true;
        if let Some(mut callback) = adc.on_adc_read.take() {
            callback(&mut adc, cpu, input);
            adc.on_adc_read = Some(callback);
        }
        true
    }
}

fn complete_adc_read_inner(state: &mut AdcState, cpu: &mut CpuState, value: u16) {
    state.converting = false;
    state.conversion_cycles = 13;

    if cpu.data_byte(state.config.admux) & ADLAR != 0 {
        cpu.set_data_byte(state.config.adcl, ((value << 6) & 0xff) as u8);
        cpu.set_data_byte(state.config.adch, (value >> 2) as u8);
    } else {
        cpu.set_data_byte(state.config.adcl, (value & 0xff) as u8);
        cpu.set_data_byte(state.config.adch, ((value >> 8) & 0x3) as u8);
    }

    let adcsra = cpu.data_byte(state.config.adcsra) & !ADSC;
    cpu.set_data_byte(state.config.adcsra, adcsra);
    cpu.set_interrupt_flag(state.adc_interrupt);
}

fn prescaler_for(cpu: &CpuState, config: &AdcConfig) -> u32 {
    match cpu.data_byte(config.adcsra) & ADPS_MASK {
        0 | 1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        7 => 128,
        _ => unreachable!(),
    }
}

fn reference_voltage_for(cpu: &CpuState, state: &AdcState) -> f64 {
    let mut reference = ((cpu.data_byte(state.config.admux) >> REFS_SHIFT) & REFS_MASK) as usize;
    if state.config.adc_references.len() > 4 && cpu.data_byte(state.config.admux) & REFS2 != 0 {
        reference |= 0x4;
    }
    match state
        .config
        .adc_references
        .get(reference)
        .copied()
        .unwrap_or(AdcReference::Reserved)
    {
        AdcReference::Avcc | AdcReference::Reserved => state.avcc,
        AdcReference::Aref => state.aref,
        AdcReference::Internal1V1 => 1.1,
        AdcReference::Internal2V56 => 2.56,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::test_support::{asm_program, TestProgramRunner};

    #[test]
    fn should_successfully_perform_an_adc_conversion() {
        let asm = asm_program(
            r#"
            ldi r24, 0x40
            sts 0x7c, r24
            ldi r24, 0xc7
            sts 0x7a, r24
          wait:
            lds r24, 0x7a
            andi r24, 0x40
            brne wait
            lds r16, 0x78
            lds r17, 0x79
            break
            "#,
        );
        let cpu = Cpu::new(asm.program);
        let adc = Adc::new(cpu.clone(), adc_config());
        adc.set_channel_value(0, 2.56);
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(4);
        runner.run_instructions(12);
        cpu.add_cycles(128 * 25);
        cpu.tick();
        runner.run_instructions(5);
        let value = (cpu.data_byte(17) as u16) << 8 | cpu.data_byte(16) as u16;
        assert_eq!(value, 524);
    }

    #[test]
    fn should_read_0_when_the_adc_peripheral_is_not_enabled() {
        let asm = asm_program(
            r#"
            ldi r16, 0xff
            ldi r17, 0xff
            ldi r24, 0x40
            sts 0x7c, r24
            ldi r24, 0x47
            sts 0x7a, r24
          wait:
            lds r24, 0x7a
            andi r24, 0x40
            brne wait
            lds r16, 0x78
            lds r17, 0x79
            break
            "#,
        );
        let cpu = Cpu::new(asm.program);
        let adc = Adc::new(cpu.clone(), adc_config());
        adc.set_channel_value(0, 2.56);
        let runner =
            TestProgramRunner::with_on_break(cpu.clone(), Box::new(|_cpu| { /* do nothing */ }));
        runner.run_instructions(6);
        runner.run_instructions(12);
        cpu.add_cycles(128 * 25);
        cpu.tick();
        runner.run_to_break();
        let value = (cpu.data_byte(17) as u16) << 8 | cpu.data_byte(16) as u16;
        assert_eq!(value, 0);
    }
}
