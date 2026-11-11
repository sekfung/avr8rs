//! Minimal Arduino Uno (ATmega328P) board wiring.
//!
//! This layer maps Arduino pin numbers to the GPIO ports and instantiates the
//! peripherals used by the Uno: USART0, Timer0/1/2, ADC, watchdog, and the clock
//! prescaler. It intentionally leaves external components (LEDs, buttons,
//! displays) to the caller.

use crate::{
    adc_config, clock_config, port_b_config, port_c_config, port_d_config, timer0_config,
    timer1_config, timer2_config, usart0_config, watchdog_config, Adc, Clock, Cpu, IoPort,
    ListenerId, PinState, Timer, Usart, Watchdog,
};

/// ATmega328P flash capacity in 16-bit words.
pub const UNO_FLASH_WORDS: usize = 0x4000;
const UNO_SRAM_BYTES: usize = 2048;

/// Listener receiving `(cpu_state, high, was_high)` for one Arduino pin.
pub type DigitalPinListener = Box<dyn FnMut(&mut crate::CpuState, bool, bool)>;

/// Arduino pin mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    Input,
    Output,
    InputPullup,
}

/// A wired ATmega328P running the Arduino Uno pin mapping.
pub struct ArduinoUno {
    cpu: Cpu,
    port_b: IoPort,
    port_c: IoPort,
    port_d: IoPort,
    usart: Usart,
    timer0: Timer,
    timer1: Timer,
    timer2: Timer,
    adc: Adc,
    watchdog: Watchdog,
    clock: Clock,
}

impl ArduinoUno {
    /// Build a board with `prog_mem` words of flash and a 16 MHz clock.
    pub fn new(prog_mem: Vec<u16>) -> Self {
        Self::with_frequency(prog_mem, 16_000_000)
    }

    /// Build a board at an arbitrary CPU frequency.
    pub fn with_frequency(prog_mem: Vec<u16>, freq_hz: u32) -> Self {
        let cpu = Cpu::with_sram(prog_mem, UNO_SRAM_BYTES);
        let port_b = IoPort::new(cpu.clone(), port_b_config());
        let port_c = IoPort::new(cpu.clone(), port_c_config());
        let port_d = IoPort::new(cpu.clone(), port_d_config());
        let usart = Usart::new(cpu.clone(), usart0_config(), freq_hz as f64);
        let timer0 = Timer::new(cpu.clone(), timer0_config());
        let timer1 = Timer::new(cpu.clone(), timer1_config());
        let timer2 = Timer::new(cpu.clone(), timer2_config());
        let adc = Adc::new(cpu.clone(), adc_config());
        let clock = Clock::with_config(cpu.clone(), freq_hz, clock_config());
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock.clone());
        Self {
            cpu,
            port_b,
            port_c,
            port_d,
            usart,
            timer0,
            timer1,
            timer2,
            adc,
            watchdog,
            clock,
        }
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn usart(&self) -> &Usart {
        &self.usart
    }

    pub fn adc(&self) -> &Adc {
        &self.adc
    }

    pub fn timer0(&self) -> &Timer {
        &self.timer0
    }

    pub fn watchdog(&self) -> &Watchdog {
        &self.watchdog
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Reset the CPU and board peripherals, preserving SRAM, flash, and attached devices.
    pub fn reset(&self) {
        let mut state = self.cpu.borrow_state_mut();
        state.reset_io_registers();
        state.reset();
        self.clock.reset_with_state(&state);
        self.watchdog.reset_after_cpu();
        self.usart.reset_with_state(&mut state);
        self.adc.reset_after_cpu();
        for timer in [&self.timer0, &self.timer1, &self.timer2] {
            timer.reset_after_cpu();
        }
        for port in [&self.port_b, &self.port_c, &self.port_d] {
            port.reset_with_state(&mut state);
        }
        state.drain_pending_pin_sets();
    }

    pub fn run_instructions(&self, count: usize) {
        self.cpu.run_instructions(count);
    }

    /// Set a digital pin mode using Arduino numbering (0-13, A0-A5 = 14-19).
    pub fn pin_mode(&self, pin: u8, mode: PinMode) -> Result<(), BoardError> {
        let (port, index) = digital_port(pin)?;
        let (ddr, port_addr) = port_registers(port);
        let mask = 1u8 << index;
        let mut ddr_value = self.cpu.read_data(ddr);
        let mut port_value = self.cpu.read_data(port_addr);
        match mode {
            PinMode::Output => {
                ddr_value |= mask;
                port_value &= !mask;
            }
            PinMode::Input => {
                ddr_value &= !mask;
                port_value &= !mask;
            }
            PinMode::InputPullup => {
                ddr_value &= !mask;
                port_value |= mask;
            }
        }
        self.cpu.write_data(port_addr, port_value);
        self.cpu.write_data(ddr, ddr_value);
        Ok(())
    }

    pub fn digital_write(&self, pin: u8, high: bool) -> Result<(), BoardError> {
        let (port, index) = digital_port(pin)?;
        let (_, port_addr) = port_registers(port);
        let mask = 1u8 << index;
        let mut value = self.cpu.read_data(port_addr);
        if high {
            value |= mask;
        } else {
            value &= !mask;
        }
        self.cpu.write_data(port_addr, value);
        Ok(())
    }

    pub fn digital_read(&self, pin: u8) -> Result<bool, BoardError> {
        let (port, index) = digital_port(pin)?;
        Ok(self.cpu.read_data(pin_register(port)) & (1 << index) != 0)
    }

    pub fn pin_state(&self, pin: u8) -> Result<PinState, BoardError> {
        let (port, index) = digital_port(pin)?;
        Ok(self.port_by_letter(port).pin_state(index))
    }

    /// Drive an analog input voltage (0..5 V) for an ADC channel A0-A5.
    pub fn set_analog_input(&self, channel: u8, volts: f64) -> Result<(), BoardError> {
        if channel > 5 {
            return Err(BoardError::BadPin(channel));
        }
        self.adc.set_channel_value(channel as usize, volts);
        Ok(())
    }

    pub fn add_pin_listener(
        &self,
        pin: u8,
        mut listener: DigitalPinListener,
    ) -> Result<ListenerId, BoardError> {
        let (port, index) = digital_port(pin)?;
        let mask = 1u8 << index;
        Ok(self
            .port_by_letter(port)
            .add_listener(Box::new(move |state, value, old| {
                let high = value & mask != 0;
                let old_high = old & mask != 0;
                if high != old_high {
                    listener(state, high, old_high);
                }
            })))
    }

    pub fn set_serial_callback(&self, callback: Option<Box<dyn FnMut(u8)>>) {
        self.usart.set_on_byte_transmit(callback);
    }

    fn port_by_letter(&self, port: u8) -> &IoPort {
        match port {
            b'B' => &self.port_b,
            b'C' => &self.port_c,
            _ => &self.port_d,
        }
    }
}

/// Board-level validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardError {
    BadPin(u8),
}

impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardError::BadPin(pin) => write!(f, "invalid Arduino pin {pin}"),
        }
    }
}

impl std::error::Error for BoardError {}

fn digital_port(pin: u8) -> Result<(u8, u8), BoardError> {
    match pin {
        0..=7 => Ok((b'D', pin)),
        8..=13 => Ok((b'B', pin - 8)),
        14..=19 => Ok((b'C', pin - 14)),
        other => Err(BoardError::BadPin(other)),
    }
}

fn port_registers(port: u8) -> (u16, u16) {
    match port {
        b'B' => (0x24, 0x25),
        b'C' => (0x27, 0x28),
        _ => (0x2a, 0x2b),
    }
}

fn pin_register(port: u8) -> u16 {
    match port {
        b'B' => 0x23,
        b'C' => 0x26,
        _ => 0x29,
    }
}
