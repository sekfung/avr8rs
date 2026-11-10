#![forbid(unsafe_code)]
//! Rust port of the avr8js 8-bit AVR simulator core.
//!
//! The crate exposes the same CPU and peripheral model as avr8js 0.21.1.

pub mod cpu;
pub use cpu::{
    avr_instruction, avr_interrupt, ClockEventId, Cpu, CpuReadHook, CpuState, CpuWriteHook,
    InterruptConfig,
};

#[cfg(test)]
pub(crate) mod assembler;

#[cfg(test)]
pub(crate) mod test_support;
