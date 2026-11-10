use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState};

const CLKPCE: u8 = 128;

const PRESCALERS: [u32; 16] = [
    1, 2, 4, 8, 16, 32, 64, 128,
    256, // The following values are "reserved" according to the datasheet, so we measured
    // with a scope to figure them out (on ATmega328p)
    2, 4, 8, 16, 32, 64, 128,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockConfig {
    pub clkpr: u16,
}

pub fn clock_config() -> ClockConfig {
    ClockConfig { clkpr: 0x61 }
}

struct ClockState {
    cpu: Weak<RefCell<CpuState>>,
    base_freq_hz: u32,
    config: ClockConfig,
    clock_enabled_cycles: u64,
    prescaler_value: u32,
    cycles_delta: f64,
}

/// AVR8 clock peripheral, controlling the CLKPR prescaler and time helpers.
#[derive(Clone)]
pub struct Clock(Rc<RefCell<ClockState>>);

impl Clock {
    pub fn new(cpu: Cpu, base_freq_hz: u32) -> Self {
        Self::with_config(cpu, base_freq_hz, clock_config())
    }

    pub fn with_config(cpu: Cpu, base_freq_hz: u32, config: ClockConfig) -> Self {
        let clock = Clock(Rc::new(RefCell::new(ClockState {
            cpu: cpu.downgrade(),
            base_freq_hz,
            config,
            clock_enabled_cycles: 0,
            prescaler_value: 1,
            cycles_delta: 0.0,
        })));

        let writer = clock.clone();
        cpu.set_write_hook(
            config.clkpr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_clkpr_write(state, value)
            }),
        );

        clock
    }

    pub fn frequency(&self) -> f64 {
        let state = self.0.borrow();
        state.base_freq_hz as f64 / state.prescaler_value as f64
    }

    pub fn prescaler(&self) -> u32 {
        self.0.borrow().prescaler_value
    }

    pub fn time_nanos(&self) -> f64 {
        self.elapsed_cycles() / self.frequency() * 1e9
    }

    pub fn time_micros(&self) -> f64 {
        self.elapsed_cycles() / self.frequency() * 1e6
    }

    pub fn time_millis(&self) -> f64 {
        self.elapsed_cycles() / self.frequency() * 1e3
    }

    pub fn cycles_delta(&self) -> f64 {
        self.0.borrow().cycles_delta
    }

    pub fn set_cycles_delta(&self, value: f64) {
        self.0.borrow_mut().cycles_delta = value;
    }

    fn cpu(&self) -> Cpu {
        Cpu::from_weak(&self.0.borrow().cpu)
            .expect("CPU was dropped while its clock is still in use")
    }

    fn elapsed_cycles(&self) -> f64 {
        let cycles = self.cpu().cycles() as f64;
        cycles + self.0.borrow().cycles_delta
    }

    fn handle_clkpr_write(&self, state: &mut CpuState, value: u8) -> bool {
        let cycles = state.cycles();
        let mut clock = self.0.borrow_mut();
        let enabled_cycles = clock.clock_enabled_cycles;

        if (enabled_cycles == 0 || enabled_cycles < cycles) && value == CLKPCE {
            clock.clock_enabled_cycles = cycles.saturating_add(4);
        } else if enabled_cycles != 0 && enabled_cycles >= cycles {
            clock.clock_enabled_cycles = 0;
            let index = (value & 0xf) as usize;
            let old_prescaler = clock.prescaler_value;
            clock.prescaler_value = PRESCALERS[index];
            state.set_data_byte(clock.config.clkpr, value & 0xf);
            if old_prescaler != clock.prescaler_value {
                clock.cycles_delta = (cycles as f64 + clock.cycles_delta)
                    * (old_prescaler as f64 / clock.prescaler_value as f64)
                    - cycles as f64;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLKPC: u16 = 0x61;

    #[test]
    fn should_set_the_prescaler_when_double_writing_clkpc() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 3); // Divide by 8 (2^3)
        assert_eq!(clock.frequency(), 2_000_000.0); // 2MHz
        assert_eq!(cpu.read_data(CLKPC), 3);
    }

    #[test]
    fn should_not_update_the_prescaler_if_clkpce_was_not_set_clkpc() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, 3); // Divide by 8 (2^3)
        assert_eq!(clock.frequency(), 16_000_000.0); // still 16MHz
        assert_eq!(cpu.read_data(CLKPC), 0);
    }

    #[test]
    fn should_not_update_the_prescaler_if_more_than_4_cycles_passed_since_setting_clkpce() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.add_cycles(6);
        cpu.write_data(CLKPC, 3); // Divide by 8 (2^3)
        assert_eq!(clock.frequency(), 16_000_000.0); // still 16MHz
        assert_eq!(cpu.read_data(CLKPC), 0);
    }

    #[test]
    fn should_return_the_current_prescaler_value() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 5); // Divide by 32 (2^5)
        cpu.set_cycles(16_000_000);
        assert_eq!(clock.prescaler(), 32);
    }

    #[test]
    fn should_return_current_number_of_microseconds_derived_from_base_freq_and_prescaler() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 2); // Divide by 4 (2^2)
        cpu.set_cycles(16_000_000);
        assert_eq!(clock.time_millis(), 4000.0); // 4 seconds
    }

    #[test]
    fn should_return_current_number_of_milliseconds_derived_from_base_freq_and_prescaler() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 2); // Divide by 4 (2^2)
        cpu.set_cycles(16_000_000);
        assert_eq!(clock.time_micros(), 4e6); // 4 seconds
    }

    #[test]
    fn should_return_current_number_of_nanoseconds_derived_from_base_freq_and_prescaler() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 2); // Divide by 4 (2^2)
        cpu.set_cycles(16_000_000);
        assert_eq!(clock.time_nanos(), 4e9); // 4 seconds
    }

    #[test]
    fn should_correctly_calculate_time_when_changing_the_prescale_value_at_runtime() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        cpu.set_cycles(16_000_000); // run 1 second at 16MHz
        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 2); // Divide by 4 (2^2)
        cpu.add_cycles(2 * 4_000_000); // run 2 more seconds at 4MhZ
        assert_eq!(clock.time_millis(), 3000.0); // 3 seconds in total

        cpu.write_data(CLKPC, CLKPCE);
        cpu.write_data(CLKPC, 1); // Divide by 2 (2^1)
        cpu.add_cycles(4_000_000); // run 0.5 more seconds at 8MhZ
        assert_eq!(clock.time_millis(), 3500.0); // 3.5 seconds in total
    }
}
