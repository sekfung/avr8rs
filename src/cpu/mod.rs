#[allow(clippy::module_inception)]
mod cpu;
mod interrupt;

pub use cpu::{ClockEventId, Cpu, CpuReadHook, CpuState, CpuWriteHook, InterruptConfig};
pub use interrupt::avr_interrupt;

pub fn avr_instruction(_cpu: &Cpu) {
    panic!("instruction interpreter is implemented in Task 4");
}

pub(crate) fn avr_instruction_state(_state: &mut CpuState) {
    panic!("instruction interpreter is implemented in Task 4");
}
