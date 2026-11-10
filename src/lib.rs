#![forbid(unsafe_code)]
//! Rust port of the avr8js 8-bit AVR simulator core.
//!
//! The crate exposes the same CPU and peripheral model as avr8js 0.21.1.

pub mod cpu;
pub use cpu::{
    avr_instruction, avr_interrupt, ClockEventId, Cpu, CpuReadHook, CpuState, CpuWriteHook,
    InterruptConfig,
};

pub mod peripherals;
pub use peripherals::{
    adc_config, atmega328_channels, attiny_timer1_config, clock_config, eeprom_config,
    port_a_config, port_b_config, port_c_config, port_d_config, port_e_config, port_f_config,
    port_g_config, port_h_config, port_j_config, port_k_config, port_l_config, spi_config,
    timer0_config, timer1_config, timer2_config, twi_config, usart0_config, Adc, AdcConfig,
    AdcMuxConfiguration, AdcMuxInput, AdcMuxInputType, AdcReference, AdcState, AttinyTimer1,
    AttinyTimer1Config, Clock, ClockConfig, Eeprom, EepromBackend, EepromConfig,
    EepromMemoryBackend, ExternalClockListener, ExternalInterrupt, GpioListener, IoPort,
    ListenerId, NoopTwiEventHandler, PinChangeInterrupt, PinOverrideMode, PinState, PortConfig,
    Spi, SpiConfig, SpiTransferCallback, Timer, TimerConfig, Twi, TwiConfig, TwiContext,
    TwiEventHandler, Usart, UsartConfig, INT0, INT1, PCINT0, PCINT1, PCINT2,
};

#[cfg(test)]
pub(crate) mod assembler;

#[cfg(test)]
pub(crate) mod test_support;
