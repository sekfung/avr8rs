use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState, InterruptConfig};

// Register bits
const SPCR_SPIE: u8 = 0x80;
const SPCR_SPE: u8 = 0x40;
const SPCR_DORD: u8 = 0x20;
const SPCR_MSTR: u8 = 0x10;
const SPCR_CPOL: u8 = 0x08;
const SPCR_CPHA: u8 = 0x04;
const SPCR_SPR1: u8 = 0x02;
const SPCR_SPR0: u8 = 0x01;
const SPSR_SPR_MASK: u8 = SPCR_SPR1 | SPCR_SPR0;

const SPSR_SPIF: u8 = 0x80;
const SPSR_WCOL: u8 = 0x40;
const SPSR_SPI2X: u8 = 0x01;

const BITS_PER_BYTE: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpiConfig {
    pub spi_interrupt: u8,
    pub spcr: u16,
    pub spsr: u16,
    pub spdr: u16,
}

pub fn spi_config() -> SpiConfig {
    SpiConfig {
        spi_interrupt: 0x22,
        spcr: 0x4c,
        spsr: 0x4d,
        spdr: 0x4e,
    }
}

pub type SpiTransferCallback = dyn FnMut(u8) -> u8;

type SpiByteCallback = Box<dyn FnMut(&mut CpuState, u8)>;

struct SpiState {
    cpu: Weak<RefCell<CpuState>>,
    config: SpiConfig,
    freq_hz: f64,
    transmission_active: bool,
    spi_interrupt: InterruptConfig,
    on_transfer: Option<Rc<RefCell<Box<SpiTransferCallback>>>>,
    on_byte: Option<Rc<RefCell<SpiByteCallback>>>,
    // Cached register values keep getters usable inside callbacks, where the
    // CPU state is already mutably borrowed.
    spcr: u8,
    spsr: u8,
}

#[derive(Clone)]
pub struct Spi(Rc<RefCell<SpiState>>);

impl Spi {
    pub fn new(cpu: Cpu, config: SpiConfig, freq_hz: f64) -> Self {
        let spi_interrupt = InterruptConfig {
            address: config.spi_interrupt,
            enable_register: config.spcr,
            enable_mask: SPCR_SPIE,
            flag_register: config.spsr,
            flag_mask: SPSR_SPIF,
            constant: false,
            inverse_flag: false,
        };
        let spcr = cpu.data_byte(config.spcr);
        let spsr = cpu.data_byte(config.spsr);
        let spi = Spi(Rc::new(RefCell::new(SpiState {
            cpu: cpu.downgrade(),
            config,
            freq_hz,
            transmission_active: false,
            spi_interrupt,
            on_transfer: Some(Rc::new(RefCell::new(Box::new(|_| 0)))),
            on_byte: None,
            spcr,
            spsr,
        })));

        let spi_weak = Rc::downgrade(&spi.0);
        spi.0.borrow_mut().on_byte = Some(Rc::new(RefCell::new(Box::new(
            move |state: &mut CpuState, value| {
                let Some(spi) = spi_weak.upgrade().map(Spi) else {
                    return;
                };
                let received = spi.call_on_transfer(value);
                let cycles = spi.transfer_cycles();
                let completion = spi.clone();
                state.add_clock_event(
                    Box::new(move |state| completion.complete_transfer_with(state, received)),
                    cycles,
                );
            },
        ))));

        let writer = spi.clone();
        cpu.set_write_hook(
            config.spdr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_spdr_write(state, value)
            }),
        );

        let writer = spi.clone();
        cpu.set_write_hook(
            config.spcr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_spcr_write(state, value)
            }),
        );

        let writer = spi.clone();
        cpu.set_write_hook(
            config.spsr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_spsr_write(state, value)
            }),
        );

        spi
    }

    pub fn reset(&self) {
        self.0.borrow_mut().transmission_active = false;
    }

    /// Completes a transfer using the stored CPU handle.
    ///
    /// Inside a callback that already has `&mut CpuState`, use
    /// [`Self::complete_transfer_with`] instead.
    pub fn complete_transfer(&self, received_byte: u8) {
        let cpu = {
            let spi = self.0.borrow();
            spi.cpu
                .upgrade()
                .expect("CPU was dropped while its SPI peripheral is still in use")
        };
        self.complete_transfer_with(&mut cpu.borrow_mut(), received_byte);
    }

    pub fn complete_transfer_with(&self, state: &mut CpuState, received_byte: u8) {
        match self.0.try_borrow_mut() {
            Ok(mut spi) => complete_transfer_inner(&mut spi, state, received_byte),
            Err(_) => {
                let spi = self.clone();
                state.add_clock_event(
                    Box::new(move |state| spi.complete_transfer_with(state, received_byte)),
                    1,
                );
            }
        }
    }

    pub fn is_master(&self) -> bool {
        self.read_spcr() & SPCR_MSTR != 0
    }

    /// Returns 0 for MSB-first and 1 for LSB-first.
    pub fn data_order(&self) -> u8 {
        if self.read_spcr() & SPCR_DORD != 0 {
            1
        } else {
            0
        }
    }

    pub fn spi_mode(&self) -> u8 {
        let spcr = self.read_spcr();
        let cpha = spcr & SPCR_CPHA != 0;
        let cpol = spcr & SPCR_CPOL != 0;
        ((cpha as u8) << 1) | cpol as u8
    }

    pub fn clock_divider(&self) -> u32 {
        clock_divider(self.read_spcr(), self.read_spsr())
    }

    pub fn transfer_cycles(&self) -> u64 {
        self.clock_divider() as u64 * BITS_PER_BYTE as u64
    }

    pub fn spi_frequency(&self) -> f64 {
        let freq_hz = self.0.borrow().freq_hz;
        freq_hz / self.clock_divider() as f64
    }

    pub fn set_on_byte(&self, callback: Option<SpiByteCallback>) {
        self.0.borrow_mut().on_byte = callback.map(|callback| Rc::new(RefCell::new(callback)));
    }

    pub fn set_on_transfer(&self, callback: Option<Box<SpiTransferCallback>>) {
        self.0.borrow_mut().on_transfer = callback.map(|callback| Rc::new(RefCell::new(callback)));
    }

    fn call_on_transfer(&self, value: u8) -> u8 {
        let callback = self.0.borrow().on_transfer.clone();
        callback
            .map(|callback| callback.borrow_mut()(value))
            .unwrap_or(0)
    }

    fn handle_spdr_write(&self, state: &mut CpuState, value: u8) -> bool {
        let callback = {
            let mut spi = self.0.borrow_mut();
            if state.data_byte(spi.config.spcr) & SPCR_SPE == 0 {
                // SPI is disabled: let the register write through.
                return false;
            }

            if spi.transmission_active {
                let spsr = state.data_byte(spi.config.spsr) | SPSR_WCOL;
                state.set_data_byte(spi.config.spsr, spsr);
                spi.spsr = spsr;
                return true;
            }

            let spsr = state.data_byte(spi.config.spsr) & !SPSR_WCOL;
            state.set_data_byte(spi.config.spsr, spsr);
            spi.spsr = spsr;
            let interrupt = spi.spi_interrupt;
            state.clear_interrupt(interrupt);
            spi.spsr = state.data_byte(spi.config.spsr);
            spi.transmission_active = true;
            spi.on_byte.clone()
        };

        if let Some(callback) = callback {
            callback.borrow_mut()(state, value);
        }
        true
    }

    fn handle_spcr_write(&self, state: &mut CpuState, value: u8) -> bool {
        let mut spi = self.0.borrow_mut();
        spi.spcr = value;
        state.update_interrupt_enable(spi.spi_interrupt, value);
        false
    }

    fn handle_spsr_write(&self, state: &mut CpuState, value: u8) -> bool {
        let mut spi = self.0.borrow_mut();
        let interrupt = spi.spi_interrupt;
        spi.spsr = value;
        state.set_data_byte(spi.config.spsr, value);
        state.clear_interrupt_by_flag(interrupt, value);
        // Upstream writes the supplied value back after the hook, so the
        // SPIF bit remains exactly as written while any pending interrupt is
        // cleared.
        false
    }

    fn read_spcr(&self) -> u8 {
        self.read_register(|state| state.config.spcr, |state| state.spcr)
    }

    fn read_spsr(&self) -> u8 {
        self.read_register(|state| state.config.spsr, |state| state.spsr)
    }

    fn read_register(
        &self,
        address: impl Fn(&SpiState) -> u16,
        cached: impl Fn(&SpiState) -> u8,
    ) -> u8 {
        let spi = self.0.borrow();
        if let Some(cpu) = spi.cpu.upgrade() {
            if let Ok(cpu) = cpu.try_borrow() {
                return cpu.data_byte(address(&spi));
            }
        }
        cached(&spi)
    }
}

fn complete_transfer_inner(spi: &mut SpiState, state: &mut CpuState, received_byte: u8) {
    state.set_data_byte(spi.config.spdr, received_byte);
    state.set_interrupt_flag(spi.spi_interrupt);
    spi.transmission_active = false;
    spi.spsr = state.data_byte(spi.config.spsr);
}

fn clock_divider(spcr: u8, spsr: u8) -> u32 {
    let base = if spsr & SPSR_SPI2X != 0 { 2 } else { 4 };
    match spcr & SPSR_SPR_MASK {
        0 => base,
        1 => base * 4,
        2 => base * 16,
        3 => base * 32,
        _ => panic!("Invalid divider value!"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{asm_program, Recorder, TestProgramRunner};

    const FREQ_16MHZ: f64 = 16_000_000.0;

    // CPU registers
    const R17: u16 = 17;

    // SPI registers
    const SPCR: u16 = 0x4c;
    const SPSR: u16 = 0x4d;
    const SPDR: u16 = 0x4e;

    // Register bit names
    const SPR0: u8 = 1;
    const SPR1: u8 = 2;
    const CPOL: u8 = 4;
    const CPHA: u8 = 8;
    const MSTR: u8 = 0x10;
    const DORD: u8 = 0x20;
    const SPE: u8 = 0x40;
    const SPIE: u8 = 0x80;
    const WCOL: u8 = 0x40;
    const SPIF: u8 = 0x80;
    const SPI2X: u8 = 1;

    #[test]
    fn should_correctly_calculate_the_frequency_based_on_spcr_spsr_values() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        // Values in this test are based on Table 19-5 in the datasheet, page 177.
        // Standard SPI speed:
        cpu.write_data(SPSR, 0);
        cpu.write_data(SPCR, 0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 4.0);
        cpu.write_data(SPCR, SPR0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 16.0);
        cpu.write_data(SPCR, SPR1);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 64.0);
        cpu.write_data(SPCR, SPR1 | SPR0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 128.0);

        // Double SPI speed:
        cpu.write_data(SPSR, SPI2X);
        cpu.write_data(SPCR, 0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 2.0);
        cpu.write_data(SPCR, SPR0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 8.0);
        cpu.write_data(SPCR, SPR1);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 32.0);
        cpu.write_data(SPCR, SPR1 | SPR0);
        assert_eq!(spi.spi_frequency(), FREQ_16MHZ / 64.0);
    }

    #[test]
    fn should_correctly_report_the_data_order_based_on_spcr_value() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        cpu.write_data(SPCR, 0);
        assert_eq!(spi.data_order(), 0);

        cpu.write_data(SPCR, DORD);
        assert_eq!(spi.data_order(), 1);
    }

    #[test]
    fn should_correctly_report_the_spi_mode_based_on_spcr_value() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        // Values in this test are based on Table 2 in the datasheet, page 174.
        cpu.write_data(SPCR, 0);
        assert_eq!(spi.spi_mode(), 0);

        cpu.write_data(SPCR, CPHA);
        assert_eq!(spi.spi_mode(), 1);

        cpu.write_data(SPCR, CPOL);
        assert_eq!(spi.spi_mode(), 2);

        cpu.write_data(SPCR, CPOL | CPHA);
        assert_eq!(spi.spi_mode(), 3);
    }

    #[test]
    fn should_indicate_slave_or_master_operation_based_on_spcr_value() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        assert!(!spi.is_master());

        cpu.write_data(SPCR, MSTR);
        assert!(spi.is_master());
    }

    #[test]
    fn should_call_the_on_byte_transfer_callback_when_initiating_a_transfer() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);
        let received = Recorder::<u8>::new();
        let recorder = received.clone();
        spi.set_on_byte(Some(Box::new(move |_state: &mut CpuState, value| {
            recorder.push(value);
        })));

        cpu.write_data(SPCR, SPE | MSTR);
        cpu.write_data(SPDR, 0x8f);

        assert_eq!(received.calls(), vec![0x8f]);
    }

    #[test]
    fn should_ignore_spdr_writes_when_the_spe_bit_in_spcr_is_clear() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);
        let received = Recorder::<u8>::new();
        let recorder = received.clone();
        spi.set_on_byte(Some(Box::new(move |_state: &mut CpuState, value| {
            recorder.push(value);
        })));

        cpu.write_data(SPCR, MSTR);
        cpu.write_data(SPDR, 0x8f);

        assert!(received.is_empty());
    }

    #[test]
    fn should_transmit_a_byte_successfully() {
        // Based on code example from section 19.2 of the datasheet, page 172.
        let asm = asm_program(
            "
            ; register addresses
            _REPLACE SPCR, 0x2c
            _REPLACE SPDR, 0x2e
            _REPLACE SPSR, 0x2d
            _REPLACE DDR_SPI, 0x4 ; PORTB

            SPI_MasterInit:
              ; Set MOSI and SCK output, all others input
              LDI r17, 0x28
              OUT DDR_SPI, r17

              ; Enable SPI, Master, set clock rate fck/16
              LDI r17, 0x51   ; (1<<SPE)|(1<<MSTR)|(1<<SPR0)
              OUT SPCR, r17

            SPI_MasterTransmit:
              LDI r16, 0xb8 ; byte to transmit
              OUT SPDR, r16

            Wait_Transmit:
              IN r16, SPSR
              SBRS r16, 7
              RJMP Wait_Transmit

            ; Now read the result into r17
              IN r17, SPDR
              BREAK
            ",
        );

        let cpu = Cpu::new(asm.program);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);
        let received = Recorder::<u8>::new();
        let recorder = received.clone();
        let spi_cb = spi.clone();
        spi.set_on_byte(Some(Box::new(move |state: &mut CpuState, value| {
            recorder.push(value);
            let completion = spi_cb.clone();
            state.add_clock_event(
                Box::new(move |state| completion.complete_transfer_with(state, 0x5b)),
                spi_cb.transfer_cycles(),
            );
        })));

        let runner =
            TestProgramRunner::with_on_break(cpu.clone(), Box::new(|_cpu| { /* do nothing */ }));
        runner.run_to_break();

        assert!(cpu.cycles() >= 128);
        assert_eq!(received.calls(), vec![0xb8]);
        assert_eq!(cpu.data_byte(R17), 0x5b);
    }

    #[test]
    fn should_set_wcol_if_spdr_is_written_while_spi_is_transmitting() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        cpu.write_data(SPCR, SPE | MSTR);
        cpu.write_data(SPDR, 0x50);
        cpu.tick();
        assert_eq!(cpu.read_data(SPSR) & WCOL, 0);

        cpu.write_data(SPDR, 0x51);
        assert_eq!(cpu.read_data(SPSR) & WCOL, WCOL);
    }

    #[test]
    fn should_clear_spif_and_fire_an_interrupt_when_spi_transfer_completes() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        cpu.write_data(SPCR, SPE | SPIE | MSTR);
        cpu.write_data(SPDR, 0x50);
        cpu.set_sreg(0x80); // SREG: I-------

        // At this point, the write shouldn't be complete yet.
        cpu.add_cycles(10);
        cpu.tick();
        assert_eq!(cpu.pc(), 0);

        // 100 cycles later, it should (8 bits * 8 cycles per bit = 64).
        cpu.add_cycles(100);
        cpu.tick();
        assert_eq!(cpu.read_data(SPSR) & SPIF, 0);
        assert_eq!(cpu.pc(), 0x22); // SPI Ready interrupt
    }

    #[test]
    fn should_fire_a_pending_spi_interrupt_when_spie_flag_is_set() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);

        cpu.write_data(SPCR, SPE | MSTR);
        cpu.write_data(SPDR, 0x50);
        cpu.set_sreg(0x80); // SREG: I-------

        // Wait for the transfer to complete (8 bits * 8 cycles per bit = 64).
        cpu.add_cycles(64);
        cpu.tick();

        assert_eq!(cpu.read_data(SPSR) & SPIF, SPIF);
        assert_eq!(cpu.pc(), 0); // Interrupt not taken (yet)

        // Enable the interrupt (SPIE).
        cpu.write_data(SPCR, SPE | MSTR | SPIE);
        cpu.tick();
        assert_eq!(cpu.pc(), 0x22); // SPI Ready interrupt
        assert_eq!(cpu.read_data(SPSR) & SPIF, 0);
    }

    #[test]
    fn should_only_update_spdr_when_transfer_finishes() {
        let cpu = Cpu::new(vec![0; 1024]);
        let spi = Spi::new(cpu.clone(), spi_config(), FREQ_16MHZ);
        let spi_cb = spi.clone();
        spi.set_on_byte(Some(Box::new(move |state: &mut CpuState, _value| {
            let completion = spi_cb.clone();
            state.add_clock_event(
                Box::new(move |state| completion.complete_transfer_with(state, 0x88)),
                spi_cb.transfer_cycles(),
            );
        })));

        cpu.write_data(SPCR, SPE | MSTR);
        cpu.write_data(SPDR, 0x8f);

        cpu.set_cycles(10);
        cpu.tick();
        assert_eq!(cpu.read_data(SPDR), 0);

        cpu.set_cycles(32); // 4 cycles per bit * 8 bits = 32
        cpu.tick();
        assert_eq!(cpu.read_data(SPDR), 0x88);
    }
}
