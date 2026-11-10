use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{ClockEventId, Cpu, CpuState, InterruptConfig};
use crate::peripherals::clock::Clock;

const WATCHDOG_CLOCK_FREQUENCY: f64 = 128_000.0;

const MCUSR_WDRF: u8 = 0x8;

const WDTCSR_WDIF: u8 = 0x80;
const WDTCSR_WDIE: u8 = 0x40;
const WDTCSR_WDP3: u8 = 0x20;
const WDTCSR_WDCE: u8 = 0x10;
const WDTCSR_WDE: u8 = 0x8;
const WDTCSR_WDP2: u8 = 0x4;
const WDTCSR_WDP1: u8 = 0x2;
const WDTCSR_WDP0: u8 = 0x1;
const WDTCSR_WDP210: u8 = WDTCSR_WDP2 | WDTCSR_WDP1 | WDTCSR_WDP0;

const WDTCSR_PROTECT_MASK: u8 = WDTCSR_WDE | WDTCSR_WDP3 | WDTCSR_WDP210;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchdogConfig {
    pub watchdog_interrupt: u8,
    pub mcusr: u16,
    pub wdtcsr: u16,
}

pub fn watchdog_config() -> WatchdogConfig {
    WatchdogConfig {
        watchdog_interrupt: 0x0c,
        mcusr: 0x54,
        wdtcsr: 0x60,
    }
}

fn prescaler_from_wdtcsr(wdtcsr: u8) -> u32 {
    let value = ((wdtcsr & WDTCSR_WDP3) >> 2) | (wdtcsr & WDTCSR_WDP210);
    2048u32 << value
}

struct WatchdogState {
    me: Weak<RefCell<WatchdogState>>,
    cpu: Weak<RefCell<CpuState>>,
    config: WatchdogConfig,
    clock: Clock,
    /// Last cycle at which WDE/WDP* can still be changed after WDCE was written.
    change_enabled_cycles: u64,
    watchdog_timeout: u64,
    enabled_value: bool,
    scheduled: bool,
    event_id: Option<ClockEventId>,
    watchdog_interrupt: InterruptConfig,
}

/// AVR8 watchdog timer.
#[derive(Clone)]
pub struct Watchdog(Rc<RefCell<WatchdogState>>);

impl Watchdog {
    pub fn new(cpu: Cpu, config: WatchdogConfig, clock: Clock) -> Self {
        let watchdog_interrupt = InterruptConfig {
            address: config.watchdog_interrupt,
            enable_register: config.wdtcsr,
            enable_mask: WDTCSR_WDIE,
            flag_register: config.wdtcsr,
            flag_mask: WDTCSR_WDIF,
            constant: false,
            inverse_flag: false,
        };
        let watchdog = Watchdog(Rc::new(RefCell::new(WatchdogState {
            me: Weak::new(),
            cpu: cpu.downgrade(),
            config,
            clock,
            change_enabled_cycles: 0,
            watchdog_timeout: 0,
            enabled_value: false,
            scheduled: false,
            event_id: None,
            watchdog_interrupt,
        })));
        watchdog.0.borrow_mut().me = Rc::downgrade(&watchdog.0);

        let weak = Rc::downgrade(&watchdog.0);
        cpu.set_on_watchdog_reset(Box::new(move |state| {
            if let Some(watchdog) = weak.upgrade() {
                watchdog.borrow_mut().reset_watchdog_with(state);
            }
        }));

        let writer = watchdog.clone();
        cpu.set_write_hook(
            config.wdtcsr,
            Box::new(move |state, value, old_value, _addr, _mask| {
                writer
                    .0
                    .borrow_mut()
                    .handle_wdtcsr_write(state, value, old_value)
            }),
        );

        watchdog
    }

    pub fn reset_watchdog(&self) {
        let cpu = {
            let state = self.0.borrow();
            Cpu::from_weak(&state.cpu).expect("CPU was dropped while its watchdog is still in use")
        };
        let cpu_state = cpu.borrow_state_mut();
        self.0.borrow_mut().reset_watchdog_with(&cpu_state);
    }

    pub fn enabled(&self) -> bool {
        self.0.borrow().enabled_value
    }

    pub fn prescaler(&self) -> u32 {
        let (cpu, wdtcsr_addr) = {
            let state = self.0.borrow();
            let cpu = Cpu::from_weak(&state.cpu)
                .expect("CPU was dropped while its watchdog is still in use");
            (cpu, state.config.wdtcsr)
        };
        prescaler_from_wdtcsr(cpu.data_byte(wdtcsr_addr))
    }
}

impl WatchdogState {
    fn prescaler_with(&self, state: &CpuState) -> u32 {
        prescaler_from_wdtcsr(state.data_byte(self.config.wdtcsr))
    }

    fn reset_watchdog_with(&mut self, state: &CpuState) {
        let cycles = (self.clock.frequency() / WATCHDOG_CLOCK_FREQUENCY
            * self.prescaler_with(state) as f64)
            .floor() as u64;
        self.watchdog_timeout = state.cycles().saturating_add(cycles);
    }

    fn handle_wdtcsr_write(&mut self, state: &mut CpuState, value: u8, old_value: u8) -> bool {
        let mut value = value;
        if (value & WDTCSR_WDCE) != 0 && (value & WDTCSR_WDE) != 0 {
            self.change_enabled_cycles = state.cycles().saturating_add(4);
            value &= !WDTCSR_PROTECT_MASK;
        } else {
            if state.cycles() >= self.change_enabled_cycles {
                value = (value & !WDTCSR_PROTECT_MASK) | (old_value & WDTCSR_PROTECT_MASK);
            }
            self.enabled_value = (value & WDTCSR_WDE) != 0 || (value & WDTCSR_WDIE) != 0;
            state.set_data_byte(self.config.wdtcsr, value);
        }

        if self.enabled_value {
            self.reset_watchdog_with(state);
        }

        if self.enabled_value && !self.scheduled {
            self.schedule_check(state);
        }

        state.clear_interrupt_by_flag(self.watchdog_interrupt, value);
        true
    }

    fn schedule_check(&mut self, state: &mut CpuState) {
        let delay = self.watchdog_timeout.saturating_sub(state.cycles());
        if let Some(id) = self.event_id {
            if state.update_clock_event(id, delay) {
                self.scheduled = true;
                return;
            }
        }

        let me = self.me.clone();
        let id = state.add_clock_event(
            Box::new(move |state| {
                if let Some(watchdog) = me.upgrade() {
                    watchdog.borrow_mut().check_watchdog(state);
                }
            }),
            delay,
        );
        self.event_id = Some(id);
        self.scheduled = true;
    }

    fn check_watchdog(&mut self, state: &mut CpuState) {
        if self.enabled_value && state.cycles() >= self.watchdog_timeout {
            let wdtcsr = state.data_byte(self.config.wdtcsr);
            if (wdtcsr & WDTCSR_WDIE) != 0 {
                state.set_interrupt_flag(self.watchdog_interrupt);
            }
            if (wdtcsr & WDTCSR_WDE) != 0 {
                if (wdtcsr & WDTCSR_WDIE) != 0 {
                    // Re-read WDTCSR so the WDIF bit set above is preserved, matching
                    // the upstream read-modify-write of the live register value.
                    let current = state.data_byte(self.config.wdtcsr);
                    state.set_data_byte(self.config.wdtcsr, current & !WDTCSR_WDIE);
                } else {
                    state.reset();
                    self.scheduled = false;
                    self.event_id = None;
                    let mcusr = state.data_byte(self.config.mcusr);
                    state.set_data_byte(self.config.mcusr, mcusr | MCUSR_WDRF);
                    return;
                }
            }
            self.reset_watchdog_with(state);
        }

        if self.enabled_value {
            self.scheduled = true;
            self.schedule_check(state);
        } else {
            self.scheduled = false;
            self.event_id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{asm_program, TestProgramRunner};

    const R20: u16 = 20;

    const MCUSR: u16 = 0x54;
    const WDRF: u8 = 1 << 3;

    const WDTCSR: u16 = 0x60;
    const WDP0: u8 = 1 << 0;
    const WDP1: u8 = 1 << 1;
    const WDP2: u8 = 1 << 2;
    const WDE: u8 = 1 << 3;
    const WDCE: u8 = 1 << 4;
    const WDP3: u8 = 1 << 5;
    const WDIE: u8 = 1 << 6;
    const WDIF: u8 = 1 << 7;

    const INT_WDT: u32 = 0xc;

    #[test]
    fn should_correctly_calculate_the_prescaler_from_wdtcsr() {
        let cpu = Cpu::new(vec![0; 1024]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        cpu.write_data(WDTCSR, WDCE | WDE);
        cpu.write_data(WDTCSR, 0);
        assert_eq!(watchdog.prescaler(), 2048);
        cpu.write_data(WDTCSR, WDP2 | WDP1 | WDP0);
        assert_eq!(watchdog.prescaler(), 256 * 1024);
        cpu.write_data(WDTCSR, WDP3 | WDP0);
        assert_eq!(watchdog.prescaler(), 1024 * 1024);
    }

    #[test]
    fn should_not_change_the_prescaler_unless_wdce_is_set() {
        let cpu = Cpu::new(vec![0; 1024]);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        cpu.write_data(WDTCSR, 0);
        assert_eq!(watchdog.prescaler(), 2048);
        cpu.write_data(WDTCSR, WDP2 | WDP1 | WDP0);
        assert_eq!(watchdog.prescaler(), 2048);

        cpu.write_data(WDTCSR, WDCE | WDE);
        cpu.add_cycles(5); // WDCE should expire after 4 cycles
        cpu.write_data(WDTCSR, WDP2 | WDP1 | WDP0);
        assert_eq!(watchdog.prescaler(), 2048);
    }

    #[test]
    fn should_reset_the_cpu_when_the_timer_expires() {
        let asm = asm_program(&format!(
            r#"
    ; register addresses
    _REPLACE WDTCSR, {WDTCSR}

    ; Setup watchdog
    ldi r16, {}
    sts WDTCSR, r16
    ldi r16, {}
    sts WDTCSR, r16

    nop

    break
  "#,
            WDE | WDCE,
            WDE
        ));
        let cpu = Cpu::new(asm.program);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        let runner = TestProgramRunner::new(cpu.clone());

        // Setup: enable watchdog timer
        runner.run_instructions(4);
        assert!(watchdog.enabled());

        // Now we skip 8ms. Watchdog shouldn't fire, yet
        cpu.add_cycles(16000 * 8);
        runner.run_instructions(1);

        // Now we skip an extra 8ms. Watchdog should fire and reset!
        cpu.add_cycles(16000 * 8);
        cpu.tick();
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.read_data(MCUSR), WDRF);
    }

    #[test]
    fn should_extend_the_watchdog_timeout_when_executing_a_wdr_instruction() {
        let asm = asm_program(&format!(
            r#"
    ; register addresses
    _REPLACE WDTCSR, {WDTCSR}

    ; Setup watchdog
    ldi r16, {}
    sts WDTCSR, r16
    ldi r16, {}
    sts WDTCSR, r16

    wdr
    nop

    break
  "#,
            WDE | WDCE,
            WDE
        ));
        let cpu = Cpu::new(asm.program);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        let runner = TestProgramRunner::new(cpu.clone());

        // Setup: enable watchdog timer
        runner.run_instructions(4);
        assert!(watchdog.enabled());

        // Now we skip 8ms. Watchdog shouldn't fire, yet
        cpu.add_cycles(16000 * 8);
        runner.run_instructions(1);
        assert_ne!(cpu.pc(), 0);

        // Now we skip an extra 8ms. We extended the timeout with WDR, so watchdog
        // won't fire yet
        cpu.add_cycles(16000 * 8);
        runner.run_instructions(1);
        assert_ne!(cpu.pc(), 0);

        // Finally, another 8ms bring us to 16ms since last WDR, and watchdog
        // should fire
        cpu.add_cycles(16000 * 8);
        cpu.tick();
        assert_eq!(cpu.pc(), 0);
    }

    #[test]
    fn should_fire_an_interrupt_when_the_watchdog_expires_and_wdie_is_set() {
        let asm = asm_program(&format!(
            r#"
    ; register addresses
    _REPLACE WDTCSR, {WDTCSR}

    ; Setup watchdog
    ldi r16, {}
    sts WDTCSR, r16
    ldi r16, {}
    sts WDTCSR, r16

    nop
    sei

    break
  "#,
            WDE | WDCE,
            WDE | WDIE
        ));
        let cpu = Cpu::new(asm.program);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        let runner = TestProgramRunner::new(cpu.clone());

        // Setup: enable watchdog timer
        runner.run_instructions(4);
        assert!(watchdog.enabled());

        // Now we skip 8ms. Watchdog shouldn't fire, yet
        cpu.add_cycles(16000 * 8);
        runner.run_instructions(1);

        // Now we skip an extra 8ms. Fire the timeout while global interrupts are
        // still disabled, so the WDIF bit is observable before the handler runs.
        cpu.add_cycles(16000 * 8);
        cpu.tick();
        assert_eq!(cpu.read_data(WDTCSR) & (WDIF | WDIE), WDIF);

        // Enabling interrupts should jump to the watchdog interrupt handler.
        runner.run_instructions(1);

        assert_eq!(cpu.pc(), INT_WDT);
        // The watchdog timer should also clean the WDIE bit, so next timeout will
        // reset the MCU.
        assert_eq!(cpu.read_data(WDTCSR) & WDIE, 0);
    }

    #[test]
    fn should_not_reset_the_cpu_if_the_watchdog_has_been_disabled() {
        let asm = asm_program(&format!(
            r#"
    ; register addresses
    _REPLACE WDTCSR, {WDTCSR}

    ; Setup watchdog
    ldi r16, {}
    sts WDTCSR, r16
    ldi r16, {}
    sts WDTCSR, r16

    ; disable watchdog
    ldi r16, {}
    sts WDTCSR, r16
    ldi r16, 0
    sts WDTCSR, r16

    ldi r20, 55

    break
  "#,
            WDE | WDCE,
            WDE,
            WDE | WDCE
        ));
        let cpu = Cpu::new(asm.program);
        let clock = Clock::new(cpu.clone(), 16_000_000);
        let watchdog = Watchdog::new(cpu.clone(), watchdog_config(), clock);
        let runner = TestProgramRunner::new(cpu.clone());

        // Setup: enable watchdog timer
        runner.run_instructions(4);
        assert!(watchdog.enabled());

        // Now we skip 8ms. Watchdog shouldn't fire, yet. We disable it.
        cpu.add_cycles(16000 * 8);
        runner.run_instructions(4);

        // Now we skip an extra 20ms. Watchdog shouldn't reset!
        cpu.add_cycles(16000 * 20);
        runner.run_instructions(1);
        assert_ne!(cpu.pc(), 0);
        assert_eq!(cpu.data_byte(R20), 55); // assert that `ldi r20, 55` ran
    }
}
