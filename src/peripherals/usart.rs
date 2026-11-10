use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState, InterruptConfig};

// Register bits
const UCSRA_RXC: u8 = 0x80;
const UCSRA_TXC: u8 = 0x40;
const UCSRA_UDRE: u8 = 0x20;
const UCSRA_U2X: u8 = 0x02;
const UCSRA_MPCM: u8 = 0x01;
const UCSRA_CFG_MASK: u8 = UCSRA_U2X;

const UCSRB_RXCIE: u8 = 0x80;
const UCSRB_TXCIE: u8 = 0x40;
const UCSRB_UDRIE: u8 = 0x20;
const UCSRB_RXEN: u8 = 0x10;
const UCSRB_TXEN: u8 = 0x08;
const UCSRB_UCSZ2: u8 = 0x04;
const UCSRB_CFG_MASK: u8 = UCSRB_UCSZ2 | UCSRB_RXEN | UCSRB_TXEN;

const UCSRC_UPM1: u8 = 0x20;
const UCSRC_UPM0: u8 = 0x10;
const UCSRC_USBS: u8 = 0x08;
const UCSRC_UCSZ1: u8 = 0x04;
const UCSRC_UCSZ0: u8 = 0x02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsartConfig {
    pub rx_complete_interrupt: u8,
    pub data_register_empty_interrupt: u8,
    pub tx_complete_interrupt: u8,
    pub ucsra: u16,
    pub ucsrb: u16,
    pub ucsrc: u16,
    pub ubrrl: u16,
    pub ubrrh: u16,
    pub udr: u16,
}

pub fn usart0_config() -> UsartConfig {
    UsartConfig {
        rx_complete_interrupt: 0x24,
        data_register_empty_interrupt: 0x26,
        tx_complete_interrupt: 0x28,
        ucsra: 0xc0,
        ucsrb: 0xc1,
        ucsrc: 0xc2,
        ubrrl: 0xc4,
        ubrrh: 0xc5,
        udr: 0xc6,
    }
}

#[derive(Clone, Copy, Default)]
struct RegisterCache {
    ucsra: u8,
    ucsrb: u8,
    ucsrc: u8,
    ubrrh: u8,
    ubrrl: u8,
}

type ByteTransmitCallback = Box<dyn FnMut(u8)>;
type LineTransmitCallback = Box<dyn FnMut(String)>;
type NotificationCallback = Box<dyn FnMut()>;

struct UsartState {
    cpu: Weak<RefCell<CpuState>>,
    config: UsartConfig,
    freq_hz: f64,
    rx_busy: bool,
    rx_byte: u8,
    line_buffer: String,
    rxc: InterruptConfig,
    udre: InterruptConfig,
    txc: InterruptConfig,
    on_byte_transmit: Option<ByteTransmitCallback>,
    on_line_transmit: Option<LineTransmitCallback>,
    on_rx_complete: Option<NotificationCallback>,
    on_configuration_change: Option<NotificationCallback>,
    // The cached values keep getters usable inside callbacks, where the CPU
    // state is already mutably borrowed.
    cached: RegisterCache,
}

#[derive(Clone)]
pub struct Usart(Rc<RefCell<UsartState>>);

impl Usart {
    pub fn new(cpu: Cpu, config: UsartConfig, freq_hz: f64) -> Self {
        let rxc = InterruptConfig {
            address: config.rx_complete_interrupt,
            enable_register: config.ucsrb,
            enable_mask: UCSRB_RXCIE,
            flag_register: config.ucsra,
            flag_mask: UCSRA_RXC,
            constant: true,
            inverse_flag: false,
        };
        let udre = InterruptConfig {
            address: config.data_register_empty_interrupt,
            enable_register: config.ucsrb,
            enable_mask: UCSRB_UDRIE,
            flag_register: config.ucsra,
            flag_mask: UCSRA_UDRE,
            constant: false,
            inverse_flag: false,
        };
        let txc = InterruptConfig {
            address: config.tx_complete_interrupt,
            enable_register: config.ucsrb,
            enable_mask: UCSRB_TXCIE,
            flag_register: config.ucsra,
            flag_mask: UCSRA_TXC,
            constant: false,
            inverse_flag: false,
        };

        let usart = Usart(Rc::new(RefCell::new(UsartState {
            cpu: cpu.downgrade(),
            config,
            freq_hz,
            rx_busy: false,
            rx_byte: 0,
            line_buffer: String::new(),
            rxc,
            udre,
            txc,
            on_byte_transmit: None,
            on_line_transmit: None,
            on_rx_complete: None,
            on_configuration_change: None,
            cached: RegisterCache::default(),
        })));

        {
            let mut state = cpu.borrow_state_mut();
            usart.reset_with_state(&mut state);
        }

        let writer = usart.clone();
        cpu.set_write_hook(
            config.ucsra,
            Box::new(move |state, value, old_value, _addr, _mask| {
                writer.handle_ucsra_write(state, value, old_value)
            }),
        );

        let writer = usart.clone();
        cpu.set_write_hook(
            config.ucsrb,
            Box::new(move |state, value, old_value, _addr, _mask| {
                writer.handle_ucsrb_write(state, value, old_value)
            }),
        );

        let writer = usart.clone();
        cpu.set_write_hook(
            config.ucsrc,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_ucsrc_write(state, value)
            }),
        );

        let reader = usart.clone();
        cpu.set_read_hook(
            config.udr,
            Box::new(move |state, _addr| reader.handle_udr_read(state)),
        );

        let writer = usart.clone();
        cpu.set_write_hook(
            config.udr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_udr_write(state, value)
            }),
        );

        let writer = usart.clone();
        cpu.set_write_hook(
            config.ubrrh,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_ubrrh_write(state, value)
            }),
        );

        let writer = usart.clone();
        cpu.set_write_hook(
            config.ubrrl,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_ubrrl_write(state, value)
            }),
        );

        usart
    }

    pub fn reset(&self) {
        let cpu = self
            .0
            .borrow()
            .cpu
            .upgrade()
            .expect("CPU was dropped while its USART peripheral is still in use");
        let mut state = cpu.borrow_mut();
        self.reset_with_state(&mut state);
    }

    /// Schedules a received byte. Returns false when the receiver is disabled
    /// or already waiting for a byte.
    pub fn write_byte(&self, value: u8, immediate: bool) -> bool {
        let cpu = self
            .0
            .borrow()
            .cpu
            .upgrade()
            .expect("CPU was dropped while its USART peripheral is still in use");
        let mut state = cpu.borrow_mut();
        self.write_byte_with(&mut state, value, immediate)
    }

    pub fn rx_busy(&self) -> bool {
        self.0.borrow().rx_busy
    }

    pub fn rx_enable(&self) -> bool {
        self.ucsrb() & UCSRB_RXEN != 0
    }

    pub fn tx_enable(&self) -> bool {
        self.ucsrb() & UCSRB_TXEN != 0
    }

    pub fn baud_rate(&self) -> f64 {
        let freq_hz = self.0.borrow().freq_hz;
        let multiplier = if self.ucsra() & UCSRA_U2X != 0 {
            8.0
        } else {
            16.0
        };
        (freq_hz / (multiplier * (1.0 + self.ubrr() as f64))).floor()
    }

    pub fn bits_per_char(&self) -> u8 {
        bits_per_char(self.ucsrc(), self.ucsrb())
    }

    pub fn stop_bits(&self) -> u8 {
        if self.ucsrc() & UCSRC_USBS != 0 {
            2
        } else {
            1
        }
    }

    pub fn parity_enabled(&self) -> bool {
        self.ucsrc() & UCSRC_UPM1 != 0
    }

    pub fn parity_odd(&self) -> bool {
        self.ucsrc() & UCSRC_UPM0 != 0
    }

    pub fn set_on_byte_transmit(&self, callback: Option<ByteTransmitCallback>) {
        self.0.borrow_mut().on_byte_transmit = callback;
    }

    pub fn set_on_line_transmit(&self, callback: Option<LineTransmitCallback>) {
        self.0.borrow_mut().on_line_transmit = callback;
    }

    pub fn set_on_rx_complete(&self, callback: Option<NotificationCallback>) {
        self.0.borrow_mut().on_rx_complete = callback;
    }

    pub fn set_on_configuration_change(&self, callback: Option<NotificationCallback>) {
        self.0.borrow_mut().on_configuration_change = callback;
    }

    fn reset_with_state(&self, state: &mut CpuState) {
        let config = self.0.borrow().config;
        state.set_data_byte(config.ucsra, UCSRA_UDRE);
        state.set_data_byte(config.ucsrb, 0);
        state.set_data_byte(config.ucsrc, UCSRC_UCSZ1 | UCSRC_UCSZ0);
        let mut usart = self.0.borrow_mut();
        usart.rx_busy = false;
        usart.rx_byte = 0;
        usart.line_buffer.clear();
        usart.cached = RegisterCache {
            ucsra: UCSRA_UDRE,
            ucsrb: 0,
            ucsrc: UCSRC_UCSZ1 | UCSRC_UCSZ0,
            ubrrh: 0,
            ubrrl: 0,
        };
    }

    fn write_byte_with(&self, state: &mut CpuState, value: u8, immediate: bool) -> bool {
        let config = {
            let mut usart = self.0.borrow_mut();
            if usart.rx_busy || state.data_byte(usart.config.ucsrb) & UCSRB_RXEN == 0 {
                return false;
            }
            if immediate {
                usart.rx_byte = value;
            } else {
                usart.rx_busy = true;
            }
            usart.config
        };

        if immediate {
            let rxc = self.0.borrow().rxc;
            state.set_interrupt_flag(rxc);
            self.call_rx_complete();
            return true;
        }

        let delay = cycles_per_char(state, config);
        let usart = Rc::downgrade(&self.0);
        state.add_clock_event(
            Box::new(move |state| {
                let Some(usart) = usart.upgrade() else {
                    return;
                };
                let usart = Usart(usart);
                if let Ok(mut state) = usart.0.try_borrow_mut() {
                    state.rx_busy = false;
                }
                usart.write_byte_with(state, value, true);
            }),
            delay,
        );
        true
    }

    fn handle_ucsra_write(&self, state: &mut CpuState, value: u8, old_value: u8) -> bool {
        let (config, txc) = {
            let usart = self.0.borrow();
            (usart.config, usart.txc)
        };
        let stored = value & (UCSRA_MPCM | UCSRA_U2X);
        state.set_data_byte(config.ucsra, stored);
        self.0.borrow_mut().cached.ucsra = stored;
        state.clear_interrupt_by_flag(txc, value);
        if (value & UCSRA_CFG_MASK) != (old_value & UCSRA_CFG_MASK) {
            self.call_configuration_change();
        }
        true
    }

    fn handle_ucsrb_write(&self, state: &mut CpuState, value: u8, old_value: u8) -> bool {
        let (config, rxc, udre, txc) = {
            let usart = self.0.borrow();
            (usart.config, usart.rxc, usart.udre, usart.txc)
        };
        state.update_interrupt_enable(rxc, value);
        state.update_interrupt_enable(udre, value);
        state.update_interrupt_enable(txc, value);
        if value & UCSRB_RXEN != 0 && old_value & UCSRB_RXEN != 0 {
            state.clear_interrupt(rxc);
        }
        if value & UCSRB_TXEN != 0 && old_value & UCSRB_TXEN == 0 {
            // Enabling the transmission - mark UDR as empty.
            state.set_interrupt_flag(udre);
        }
        state.set_data_byte(config.ucsrb, value);
        self.0.borrow_mut().cached.ucsrb = value;
        if (value & UCSRB_CFG_MASK) != (old_value & UCSRB_CFG_MASK) {
            self.call_configuration_change();
        }
        true
    }

    fn handle_ucsrc_write(&self, state: &mut CpuState, value: u8) -> bool {
        let config = self.0.borrow().config;
        state.set_data_byte(config.ucsrc, value);
        self.0.borrow_mut().cached.ucsrc = value;
        self.call_configuration_change();
        true
    }

    fn handle_ubrrh_write(&self, state: &mut CpuState, value: u8) -> bool {
        let config = self.0.borrow().config;
        state.set_data_byte(config.ubrrh, value);
        self.0.borrow_mut().cached.ubrrh = value;
        self.call_configuration_change();
        true
    }

    fn handle_ubrrl_write(&self, state: &mut CpuState, value: u8) -> bool {
        let config = self.0.borrow().config;
        state.set_data_byte(config.ubrrl, value);
        self.0.borrow_mut().cached.ubrrl = value;
        self.call_configuration_change();
        true
    }

    fn handle_udr_read(&self, state: &mut CpuState) -> u8 {
        let config = self.0.borrow().config;
        let bits = bits_per_char(state.data_byte(config.ucsrc), state.data_byte(config.ucsrb));
        let (result, rxc) = {
            let mut usart = self.0.borrow_mut();
            let result = usart.rx_byte & rx_mask(bits);
            usart.rx_byte = 0;
            (result, usart.rxc)
        };
        state.clear_interrupt(rxc);
        result
    }

    fn handle_udr_write(&self, state: &mut CpuState, value: u8) -> bool {
        self.call_byte_transmit(value);
        self.call_line_transmit(value);
        let config = self.0.borrow().config;
        let delay = cycles_per_char(state, config);
        let (udre, txc) = {
            let usart = self.0.borrow();
            (usart.udre, usart.txc)
        };
        state.add_clock_event(
            Box::new(move |state| {
                state.set_interrupt_flag(udre);
                state.set_interrupt_flag(txc);
            }),
            delay,
        );
        state.clear_interrupt(txc);
        state.clear_interrupt(udre);
        false
    }

    fn call_byte_transmit(&self, value: u8) {
        let mut callback = self.0.borrow_mut().on_byte_transmit.take();
        if let Some(callback) = callback.as_mut() {
            callback(value);
        }
        let mut usart = self.0.borrow_mut();
        if usart.on_byte_transmit.is_none() {
            usart.on_byte_transmit = callback;
        }
    }

    fn call_line_transmit(&self, value: u8) {
        let mut callback = self.0.borrow_mut().on_line_transmit.take();
        if let Some(callback) = callback.as_mut() {
            let character = char::from(value);
            let line = {
                let mut usart = self.0.borrow_mut();
                if character == '\n' {
                    Some(std::mem::take(&mut usart.line_buffer))
                } else {
                    usart.line_buffer.push(character);
                    None
                }
            };
            if let Some(line) = line {
                callback(line);
            }
        }
        let mut usart = self.0.borrow_mut();
        if usart.on_line_transmit.is_none() {
            usart.on_line_transmit = callback;
        }
    }

    fn call_rx_complete(&self) {
        let mut callback = self.0.borrow_mut().on_rx_complete.take();
        if let Some(callback) = callback.as_mut() {
            callback();
        }
        let mut usart = self.0.borrow_mut();
        if usart.on_rx_complete.is_none() {
            usart.on_rx_complete = callback;
        }
    }

    fn call_configuration_change(&self) {
        let mut callback = self.0.borrow_mut().on_configuration_change.take();
        if let Some(callback) = callback.as_mut() {
            callback();
        }
        let mut usart = self.0.borrow_mut();
        if usart.on_configuration_change.is_none() {
            usart.on_configuration_change = callback;
        }
    }

    fn ucsra(&self) -> u8 {
        self.read_register(|state| state.config.ucsra, |state| state.cached.ucsra)
    }

    fn ucsrb(&self) -> u8 {
        self.read_register(|state| state.config.ucsrb, |state| state.cached.ucsrb)
    }

    fn ucsrc(&self) -> u8 {
        self.read_register(|state| state.config.ucsrc, |state| state.cached.ucsrc)
    }

    fn ubrr(&self) -> u16 {
        let high = self.read_register(|state| state.config.ubrrh, |state| state.cached.ubrrh);
        let low = self.read_register(|state| state.config.ubrrl, |state| state.cached.ubrrl);
        (high as u16) << 8 | low as u16
    }

    fn read_register(
        &self,
        address: impl Fn(&UsartState) -> u16,
        cached: impl Fn(&UsartState) -> u8,
    ) -> u8 {
        let usart = self.0.borrow();
        if let Some(cpu) = usart.cpu.upgrade() {
            if let Ok(cpu) = cpu.try_borrow() {
                return cpu.data_byte(address(&usart));
            }
        }
        cached(&usart)
    }
}

fn bits_per_char(ucsrc: u8, ucsrb: u8) -> u8 {
    let ucsz = ((ucsrc & (UCSRC_UCSZ1 | UCSRC_UCSZ0)) >> 1) | (ucsrb & UCSRB_UCSZ2);
    match ucsz {
        0 => 5,
        1 => 6,
        2 => 7,
        3 => 8,
        _ => 9,
    }
}

fn rx_mask(bits: u8) -> u8 {
    match bits {
        5 => 0x1f,
        6 => 0x3f,
        7 => 0x7f,
        _ => 0xff,
    }
}

fn cycles_per_char(state: &CpuState, config: UsartConfig) -> u64 {
    let ucsra = state.data_byte(config.ucsra);
    let ucsrc = state.data_byte(config.ucsrc);
    let ucsrb = state.data_byte(config.ucsrb);
    let ubrr = (state.data_byte(config.ubrrh) as u64) << 8 | state.data_byte(config.ubrrl) as u64;
    let multiplier = if ucsra & UCSRA_U2X != 0 { 8 } else { 16 };
    let bits = bits_per_char(ucsrc, ucsrb) as u64;
    let stop_bits = if ucsrc & UCSRC_USBS != 0 { 2 } else { 1 };
    let parity = if ucsrc & UCSRC_UPM1 != 0 { 1 } else { 0 };
    (ubrr + 1) * multiplier * (1 + bits + stop_bits + parity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Recorder;

    const FREQ_16MHZ: f64 = 16_000_000.0;
    const FREQ_11_0529MHZ: f64 = 11_059_200.0;

    // USART0 registers
    const UCSR0A: u16 = 0xc0;
    const UCSR0B: u16 = 0xc1;
    const UCSR0C: u16 = 0xc2;
    const UBRR0L: u16 = 0xc4;
    const UBRR0H: u16 = 0xc5;
    const UDR0: u16 = 0xc6;

    // Register bit names
    const U2X0: u8 = 2;
    const TXEN: u8 = 8;
    const RXEN: u8 = 16;
    const UDRIE: u8 = 0x20;
    const TXCIE: u8 = 0x40;
    const RXC: u8 = 0x80;
    const TXC: u8 = 0x40;
    const UDRE: u8 = 0x20;
    const USBS: u8 = 0x08;
    const UPM0: u8 = 0x10;
    const UPM1: u8 = 0x20;

    // Interrupt addresses
    const PC_INT_UDRE: u32 = 0x26;
    const PC_INT_TXC: u32 = 0x28;

    const UCSZ0: u8 = 2;
    const UCSZ1: u8 = 4;
    const UCSZ2: u8 = 4;

    #[test]
    fn should_correctly_calculate_the_baud_rate_from_ubrr() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_11_0529MHZ);
        cpu.write_data(UBRR0H, 0);
        cpu.write_data(UBRR0L, 5);
        assert_eq!(usart.baud_rate(), 115200.0);
    }

    #[test]
    fn should_correctly_calculate_the_baud_rate_from_ubrr_in_double_speed_mode() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UBRR0H, 3);
        cpu.write_data(UBRR0L, 64);
        cpu.write_data(UCSR0A, U2X0);
        assert_eq!(usart.baud_rate(), 2400.0);
    }

    #[test]
    fn should_call_on_configuration_change_when_the_baud_rate_changes() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let changes = Recorder::<()>::new();
        let recorder = changes.clone();
        usart.set_on_configuration_change(Some(Box::new(move || recorder.push(()))));

        cpu.write_data(UBRR0H, 0);
        assert_eq!(changes.len(), 1);

        changes.clear();
        cpu.write_data(UBRR0L, 5);
        assert_eq!(changes.len(), 1);

        changes.clear();
        cpu.write_data(UCSR0A, U2X0);
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn should_return_5_bits_per_byte_when_ucsz_is_0() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, 0);
        assert_eq!(usart.bits_per_char(), 5);
    }

    #[test]
    fn should_return_6_bits_per_byte_when_ucsz_is_1() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UCSZ0);
        assert_eq!(usart.bits_per_char(), 6);
    }

    #[test]
    fn should_return_7_bits_per_byte_when_ucsz_is_2() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UCSZ1);
        assert_eq!(usart.bits_per_char(), 7);
    }

    #[test]
    fn should_return_8_bits_per_byte_when_ucsz_is_3() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UCSZ0 | UCSZ1);
        assert_eq!(usart.bits_per_char(), 8);
    }

    #[test]
    fn should_return_9_bits_per_byte_when_ucsz_is_7() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UCSZ0 | UCSZ1);
        cpu.write_data(UCSR0B, UCSZ2);
        assert_eq!(usart.bits_per_char(), 9);
    }

    #[test]
    fn should_call_on_configuration_change_when_bits_per_char_change() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let changes = Recorder::<()>::new();
        let recorder = changes.clone();
        usart.set_on_configuration_change(Some(Box::new(move || recorder.push(()))));

        cpu.write_data(UCSR0C, UCSZ0 | UCSZ1);
        assert_eq!(changes.len(), 1);

        changes.clear();
        cpu.write_data(UCSR0B, UCSZ2);
        assert_eq!(changes.len(), 1);

        changes.clear();
        cpu.write_data(UCSR0B, UCSZ2);
        assert_eq!(changes.len(), 0);
    }

    #[test]
    fn should_return_1_stop_bit_when_usbs_is_0() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        assert_eq!(usart.stop_bits(), 1);
    }

    #[test]
    fn should_return_2_stop_bits_when_usbs_is_1() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, USBS);
        assert_eq!(usart.stop_bits(), 2);
    }

    #[test]
    fn should_return_false_for_parity_enabled_when_upm1_is_0() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        assert!(!usart.parity_enabled());
    }

    #[test]
    fn should_return_true_for_parity_enabled_when_upm1_is_1() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UPM1);
        assert!(usart.parity_enabled());
    }

    #[test]
    fn should_return_false_for_parity_odd_when_upm0_is_0() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        assert!(!usart.parity_odd());
    }

    #[test]
    fn should_return_true_for_parity_odd_when_upm0_is_1() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0C, UPM0);
        assert!(usart.parity_odd());
    }

    #[test]
    fn should_invoke_on_byte_transmit_when_udr0_is_written_to() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let transmitted = Recorder::<u8>::new();
        let recorder = transmitted.clone();
        usart.set_on_byte_transmit(Some(Box::new(move |value| recorder.push(value))));
        cpu.write_data(UCSR0B, TXEN);
        cpu.write_data(UDR0, 0x61);
        assert_eq!(transmitted.calls(), vec![0x61]);
    }

    #[test]
    fn tx_enable_should_equal_true_when_the_transmitter_is_enabled() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        assert!(!usart.tx_enable());
        cpu.write_data(UCSR0B, TXEN);
        assert!(usart.tx_enable());
    }

    #[test]
    fn rx_enable_should_equal_true_when_the_receiver_is_enabled() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        assert!(!usart.rx_enable());
        cpu.write_data(UCSR0B, RXEN);
        assert!(usart.rx_enable());
    }

    #[test]
    fn should_trigger_data_register_empty_interrupt_if_udre_is_set() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, UDRIE | TXEN);
        cpu.set_sreg(0x80); // SREG: I-------
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_UDRE);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(UCSR0A) & UDRE, 0);
    }

    #[test]
    fn should_trigger_data_tx_complete_interrupt_if_txcie_is_set() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, TXCIE | TXEN);
        cpu.write_data(UDR0, 0x61);
        cpu.set_sreg(0x80); // SREG: I-------
        cpu.set_cycles(1_000_000);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_TXC);
        assert_eq!(cpu.cycles(), 1_000_002);
        assert_eq!(cpu.data_byte(UCSR0A) & TXC, 0);
    }

    #[test]
    fn should_not_trigger_data_tx_complete_interrupt_if_udr_was_not_written_to() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, TXCIE | TXEN);
        cpu.set_sreg(0x80); // SREG: I-------
        cpu.tick();
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 0);
    }

    #[test]
    fn should_not_trigger_any_interrupt_if_interrupts_are_disabled() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, UDRIE | TXEN);
        cpu.write_data(UDR0, 0x61);
        cpu.set_sreg(0); // SREG: 0 (disable interrupts)
        cpu.set_cycles(1_000_000);
        cpu.tick();
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 1_000_000);
        assert_eq!(cpu.data_byte(UCSR0A), TXC | UDRE);
    }

    #[test]
    fn should_call_on_line_transmit_with_the_current_line_buffer_after_every_newline() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let lines = Recorder::<String>::new();
        let recorder = lines.clone();
        usart.set_on_line_transmit(Some(Box::new(move |line| recorder.push(line))));
        cpu.write_data(UCSR0B, TXEN);
        cpu.write_data(UDR0, 0x48); // 'H'
        cpu.write_data(UDR0, 0x65); // 'e'
        cpu.write_data(UDR0, 0x6c); // 'l'
        cpu.write_data(UDR0, 0x6c); // 'l'
        cpu.write_data(UDR0, 0x6f); // 'o'
        cpu.write_data(UDR0, 0x0a); // '\n'
        assert_eq!(lines.calls(), vec!["Hello".to_string()]);
    }

    #[test]
    fn should_not_call_on_line_transmit_if_no_newline_was_received() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let lines = Recorder::<String>::new();
        let recorder = lines.clone();
        usart.set_on_line_transmit(Some(Box::new(move |line| recorder.push(line))));
        cpu.write_data(UCSR0B, TXEN);
        cpu.write_data(UDR0, 0x48); // 'H'
        cpu.write_data(UDR0, 0x69); // 'i'
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn should_clear_the_line_buffer_after_each_call_to_on_line_transmit() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let lines = Recorder::<String>::new();
        let recorder = lines.clone();
        usart.set_on_line_transmit(Some(Box::new(move |line| recorder.push(line))));
        cpu.write_data(UCSR0B, TXEN);
        cpu.write_data(UDR0, 0x48); // 'H'
        cpu.write_data(UDR0, 0x69); // 'i'
        cpu.write_data(UDR0, 0x0a); // '\n'
        cpu.write_data(UDR0, 0x74); // 't'
        cpu.write_data(UDR0, 0x68); // 'h'
        cpu.write_data(UDR0, 0x65); // 'e'
        cpu.write_data(UDR0, 0x72); // 'r'
        cpu.write_data(UDR0, 0x65); // 'e'
        cpu.write_data(UDR0, 0x0a); // '\n'
        assert_eq!(lines.calls(), vec!["Hi".to_string(), "there".to_string()]);
    }

    #[test]
    fn should_return_false_if_called_when_rx_is_busy() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, RXEN);
        cpu.write_data(UBRR0L, 103); // baud: 9600
        assert!(usart.write_byte(10, false));
        assert!(!usart.write_byte(10, false));
        cpu.tick();
        assert!(!usart.write_byte(10, false));
    }

    #[test]
    fn should_set_the_txc_bit_after_about_1_04_ms_when_baud_rate_set_to_9600() {
        let cpu = Cpu::new(vec![0; 1024]);
        let _usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        cpu.write_data(UCSR0B, TXEN);
        cpu.write_data(UBRR0L, 103); // baud: 9600
        cpu.write_data(UDR0, 0x48); // 'H'
        cpu.add_cycles(16000); // 1ms
        cpu.tick();
        assert_eq!(cpu.data_byte(UCSR0A) & TXC, 0);
        cpu.add_cycles(800); // 0.05ms
        cpu.tick();
        assert_eq!(cpu.data_byte(UCSR0A) & TXC, TXC);
    }

    #[test]
    fn should_be_ready_to_receive_the_next_byte_after_about_1_04_ms_when_baudrate_set_to_9600() {
        let cpu = Cpu::new(vec![0; 1024]);
        let usart = Usart::new(cpu.clone(), usart0_config(), FREQ_16MHZ);
        let received = Recorder::<()>::new();
        let recorder = received.clone();
        usart.set_on_rx_complete(Some(Box::new(move || recorder.push(()))));
        cpu.write_data(UCSR0B, RXEN);
        cpu.write_data(UBRR0L, 103); // baud: 9600
        assert!(usart.write_byte(0x42, false));
        cpu.add_cycles(16000); // 1ms
        cpu.tick();
        assert_eq!(cpu.data_byte(UCSR0A) & RXC, 0); // byte not received yet
        assert!(usart.rx_busy());
        assert_eq!(received.len(), 0);
        cpu.add_cycles(800); // 0.05ms
        cpu.tick();
        assert_eq!(cpu.data_byte(UCSR0A) & RXC, RXC);
        assert!(!usart.rx_busy());
        assert_eq!(received.len(), 1);
        assert_eq!(cpu.read_data(UDR0), 0x42);
        assert_eq!(cpu.read_data(UDR0), 0);
    }
}
