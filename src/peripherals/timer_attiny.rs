use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{ClockEventId, Cpu, CpuState, CpuWriteHook, InterruptConfig};
use crate::peripherals::gpio::{IoPort, PinOverrideMode};

/// Configuration for the ATtiny25/45/85 Timer/Counter1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttinyTimer1Config {
    pub tccr1: u16,
    pub gtccr: u16,
    pub tcnt1: u16,
    pub ocr1a: u16,
    pub ocr1b: u16,
    pub ocr1c: u16,
    pub tifr: u16,
    pub timsk: u16,

    pub ovf_interrupt: u8,
    pub comp_a_interrupt: u8,
    pub comp_b_interrupt: u8,

    pub tov1: u8,
    pub ocf1a: u8,
    pub ocf1b: u8,

    pub toie1: u8,
    pub ocie1a: u8,
    pub ocie1b: u8,

    pub comp_port_b: u16,
    pub comp_pin_a: u8,
    pub comp_pin_b: u8,

    pub dividers: [u64; 16],
}

pub fn attiny_timer1_config() -> AttinyTimer1Config {
    AttinyTimer1Config {
        tccr1: 0x50,
        gtccr: 0x4c,
        tcnt1: 0x4f,
        ocr1a: 0x4e,
        ocr1b: 0x4b,
        ocr1c: 0x4d,
        tifr: 0x58,
        timsk: 0x59,

        ovf_interrupt: 0x04,
        comp_a_interrupt: 0x03,
        comp_b_interrupt: 0x09,

        tov1: 1 << 2,
        ocf1a: 1 << 6,
        ocf1b: 1 << 5,

        toie1: 1 << 2,
        ocie1a: 1 << 6,
        ocie1b: 1 << 5,

        comp_port_b: 0x38,
        comp_pin_a: 1,
        comp_pin_b: 4,

        dividers: [
            0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
        ],
    }
}

// TCCR1 bits
const CTC1: u8 = 1 << 7;
const PWM1A: u8 = 1 << 6;
const CS_MASK: u8 = 0x0f;

// GTCCR bits
const PWM1B_BIT: u8 = 1 << 6;
const FOC1B: u8 = 1 << 3;
const FOC1A: u8 = 1 << 2;
const PSR1: u8 = 1 << 1;

#[derive(Clone, Copy)]
enum CompareChannel {
    A,
    B,
}

struct AttinyTimer1State {
    me: Weak<RefCell<AttinyTimer1State>>,
    config: AttinyTimer1Config,
    last_cycle: u64,
    tcnt: u8,
    tcnt_next: u8,
    tcnt_updated: bool,
    ocr_a: u8,
    ocr_b: u8,
    ocr_c: u8,
    divider: u64,
    update_divider: bool,
    counting_up: bool,
    event_id: Option<ClockEventId>,
    pending_pin_overrides: Vec<(IoPort, u8, PinOverrideMode)>,
    ovf_interrupt: InterruptConfig,
    ocfa_interrupt: InterruptConfig,
    ocfb_interrupt: InterruptConfig,
}

impl AttinyTimer1State {
    fn new(config: AttinyTimer1Config) -> Self {
        let make_interrupt = |address: u8, enable_mask: u8, flag_mask: u8| InterruptConfig {
            address,
            enable_register: config.timsk,
            enable_mask,
            flag_register: config.tifr,
            flag_mask,
            constant: false,
            inverse_flag: false,
        };
        Self {
            me: Weak::new(),
            config,
            last_cycle: 0,
            tcnt: 0,
            tcnt_next: 0,
            tcnt_updated: false,
            ocr_a: 0,
            ocr_b: 0,
            ocr_c: 0,
            divider: 0,
            update_divider: false,
            counting_up: true,
            event_id: None,
            pending_pin_overrides: Vec::new(),
            ovf_interrupt: make_interrupt(config.ovf_interrupt, config.toie1, config.tov1),
            ocfa_interrupt: make_interrupt(config.comp_a_interrupt, config.ocie1a, config.ocf1a),
            ocfb_interrupt: make_interrupt(config.comp_b_interrupt, config.ocie1b, config.ocf1b),
        }
    }

    fn cs(&self, state: &CpuState) -> u8 {
        state.data_byte(self.config.tccr1) & CS_MASK
    }

    fn ctc_mode(&self, state: &CpuState) -> bool {
        state.data_byte(self.config.tccr1) & CTC1 != 0
    }

    fn pwm_a(&self, state: &CpuState) -> bool {
        state.data_byte(self.config.tccr1) & PWM1A != 0
    }

    fn pwm_b(&self, state: &CpuState) -> bool {
        state.data_byte(self.config.gtccr) & PWM1B_BIT != 0
    }

    fn phase_pwm(&self, state: &CpuState) -> bool {
        (self.pwm_a(state) || self.pwm_b(state)) && !self.ctc_mode(state)
    }

    fn com_a(&self, state: &CpuState) -> u8 {
        (state.data_byte(self.config.tccr1) >> 4) & 0x3
    }

    fn com_b(&self, state: &CpuState) -> u8 {
        (state.data_byte(self.config.gtccr) >> 4) & 0x3
    }

    /// TOP is OCR1C in CTC/PWM modes and 0xff in normal mode.
    fn top(&self, state: &CpuState) -> u8 {
        if self.ctc_mode(state) || self.pwm_a(state) || self.pwm_b(state) {
            self.ocr_c
        } else {
            0xff
        }
    }

    fn handle_tcnt_read(&mut self, state: &mut CpuState, addr: u16) -> u8 {
        self.count(state, false);
        let value = self.tcnt;
        state.set_data_byte(addr, value);
        value
    }

    fn handle_tcnt_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        self.tcnt_next = value;
        self.counting_up = true;
        self.tcnt_updated = true;
        if let Some(id) = self.event_id {
            state.update_clock_event(id, 0);
        }
        if self.divider != 0 {
            self.timer_updated(state, self.tcnt_next, self.tcnt_next);
        }
        false
    }

    fn handle_ocr1a_write(&mut self, value: u8) -> bool {
        self.ocr_a = value;
        false
    }

    fn handle_ocr1b_write(&mut self, value: u8) -> bool {
        self.ocr_b = value;
        false
    }

    fn handle_ocr1c_write(&mut self, value: u8) -> bool {
        self.ocr_c = value;
        false
    }

    fn handle_tccr1_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        state.set_data_byte(self.config.tccr1, value);
        self.update_divider = true;
        self.schedule_count(state, 0);
        self.update_comp_config(state);
        true
    }

    fn handle_gtccr_write(
        &mut self,
        state: &mut CpuState,
        value: u8,
        previous: Option<&mut CpuWriteHook>,
        old_value: u8,
        addr: u16,
        mask: u8,
    ) -> bool {
        if value & FOC1A != 0 {
            self.force_compare(state, CompareChannel::A);
        }
        if value & FOC1B != 0 {
            self.force_compare(state, CompareChannel::B);
        }
        if value & PSR1 != 0 {
            self.last_cycle = state.cycles();
        }
        let value = value & !(FOC1A | FOC1B | PSR1);
        if let Some(previous) = previous {
            previous(state, value, old_value, addr, mask);
        } else {
            state.set_data_byte(self.config.gtccr, value);
        }
        self.update_comp_config(state);
        true
    }

    fn handle_tifr_write(
        &mut self,
        state: &mut CpuState,
        value: u8,
        previous: Option<&mut CpuWriteHook>,
        old_value: u8,
        addr: u16,
        mask: u8,
    ) -> bool {
        if let Some(previous) = previous {
            previous(state, value, old_value, addr, mask);
        }
        let value = value & mask;
        state.clear_interrupt_by_flag(self.ovf_interrupt, value);
        state.clear_interrupt_by_flag(self.ocfa_interrupt, value);
        state.clear_interrupt_by_flag(self.ocfb_interrupt, value);
        true
    }

    fn handle_timsk_write(
        &mut self,
        state: &mut CpuState,
        value: u8,
        previous: Option<&mut CpuWriteHook>,
        old_value: u8,
        addr: u16,
        mask: u8,
    ) -> bool {
        if let Some(previous) = previous {
            previous(state, value, old_value, addr, mask);
        }
        state.update_interrupt_enable(self.ovf_interrupt, value);
        state.update_interrupt_enable(self.ocfa_interrupt, value);
        state.update_interrupt_enable(self.ocfb_interrupt, value);
        false
    }

    fn schedule_count(&mut self, state: &mut CpuState, delay: u64) {
        if let Some(id) = self.event_id {
            if state.update_clock_event(id, delay) {
                return;
            }
        }
        let me = self.me.clone();
        let id = state.add_clock_event(
            Box::new(move |state| {
                if let Some(timer) = me.upgrade() {
                    AttinyTimer1(timer).with_state(state, |timer, state| timer.count(state, true));
                }
            }),
            delay,
        );
        self.event_id = Some(id);
    }

    fn count(&mut self, state: &mut CpuState, reschedule: bool) {
        let divider = self.divider;
        let cycles = state.cycles();
        let delta = cycles.saturating_sub(self.last_cycle);

        if divider != 0 && delta >= divider {
            let counter_delta = delta / divider;
            self.last_cycle += counter_delta * divider;
            let value = self.tcnt;
            let top = self.top(state);
            let phase_pwm = self.phase_pwm(state);
            let new_value = if phase_pwm {
                self.phase_pwm_count(state, value, counter_delta)
            } else {
                ((value as u64 + counter_delta) % (top as u64 + 1)) as u8
            };
            let overflow = value as u64 + counter_delta > top as u64;

            // A CPU write takes priority over any counter clear or count operation.
            if !self.tcnt_updated {
                self.tcnt = new_value;
                if !phase_pwm {
                    self.timer_updated(state, new_value, value);
                }
            }

            if !phase_pwm && overflow {
                state.set_interrupt_flag(self.ovf_interrupt);
            }
        }

        if self.tcnt_updated {
            self.tcnt = self.tcnt_next;
            self.tcnt_updated = false;
        }

        if self.update_divider {
            let cs = self.cs(state);
            let new_divider = self.config.dividers[cs as usize];
            self.last_cycle = if new_divider != 0 { state.cycles() } else { 0 };
            self.update_divider = false;
            self.divider = new_divider;
            if new_divider != 0 {
                let delay = self
                    .last_cycle
                    .saturating_add(new_divider)
                    .saturating_sub(state.cycles());
                self.schedule_count(state, delay);
            }
            return;
        }

        if reschedule && divider != 0 {
            let delay = self
                .last_cycle
                .saturating_add(divider)
                .saturating_sub(state.cycles());
            self.schedule_count(state, delay);
        }
    }

    fn phase_pwm_count(&mut self, state: &mut CpuState, value: u8, delta: u64) -> u8 {
        let top = self.top(state);
        let ocr_a = self.ocr_a;
        let ocr_b = self.ocr_b;
        let mut value = value as i32;
        let mut delta = delta;

        while delta > 0 {
            if self.counting_up {
                value += 1;
                if value >= top as i32 {
                    value = top as i32;
                    self.counting_up = false;
                }
            } else {
                value -= 1;
                if value <= 0 {
                    value = 0;
                    self.counting_up = true;
                    state.set_interrupt_flag(self.ovf_interrupt);
                }
            }

            if !self.tcnt_updated {
                if value == ocr_a as i32 {
                    state.set_interrupt_flag(self.ocfa_interrupt);
                    self.update_comp_pin_pwm(state, CompareChannel::A);
                }
                if value == ocr_b as i32 {
                    state.set_interrupt_flag(self.ocfb_interrupt);
                    self.update_comp_pin_pwm(state, CompareChannel::B);
                }
            }
            delta -= 1;
        }

        value as u8
    }

    fn timer_updated(&mut self, state: &mut CpuState, value: u8, prev_value: u8) {
        let ocr_a = self.ocr_a;
        let ocr_b = self.ocr_b;
        let pwm_a = self.pwm_a(state);
        let pwm_b = self.pwm_b(state);
        let overflow = prev_value > value;

        if ((prev_value < ocr_a || overflow) && value >= ocr_a) || (prev_value < ocr_a && overflow)
        {
            state.set_interrupt_flag(self.ocfa_interrupt);
            if self.com_a(state) != 0 && !pwm_a {
                self.update_comp_pin_non_pwm(state, CompareChannel::A);
            }
        }
        if ((prev_value < ocr_b || overflow) && value >= ocr_b) || (prev_value < ocr_b && overflow)
        {
            state.set_interrupt_flag(self.ocfb_interrupt);
            if self.com_b(state) != 0 && !pwm_b {
                self.update_comp_pin_non_pwm(state, CompareChannel::B);
            }
        }
    }

    fn force_compare(&mut self, state: &mut CpuState, channel: CompareChannel) {
        match channel {
            CompareChannel::A if !self.pwm_a(state) && self.com_a(state) != 0 => {
                self.update_comp_pin_non_pwm(state, CompareChannel::A);
            }
            CompareChannel::B if !self.pwm_b(state) && self.com_b(state) != 0 => {
                self.update_comp_pin_non_pwm(state, CompareChannel::B);
            }
            _ => {}
        }
    }

    fn update_comp_pin_non_pwm(&mut self, state: &mut CpuState, channel: CompareChannel) {
        let (com, pin) = match channel {
            CompareChannel::A => (self.com_a(state), self.config.comp_pin_a),
            CompareChannel::B => (self.com_b(state), self.config.comp_pin_b),
        };
        let mode = match com {
            1 => PinOverrideMode::Toggle,
            2 => PinOverrideMode::Clear,
            3 => PinOverrideMode::Set,
            _ => return,
        };
        self.queue_pin_override(state, pin, mode);
    }

    fn update_comp_pin_pwm(&mut self, state: &mut CpuState, channel: CompareChannel) {
        let (com, pin) = match channel {
            CompareChannel::A => (self.com_a(state), self.config.comp_pin_a),
            CompareChannel::B => (self.com_b(state), self.config.comp_pin_b),
        };
        let inverting_mode = com == 3;
        let is_set = self.counting_up == inverting_mode;
        let mode = match com {
            1 => PinOverrideMode::Toggle,
            2 | 3 => {
                if is_set {
                    PinOverrideMode::Set
                } else {
                    PinOverrideMode::Clear
                }
            }
            _ => return,
        };
        self.queue_pin_override(state, pin, mode);
    }

    fn update_comp_config(&mut self, state: &mut CpuState) {
        let port = state
            .gpio_by_port
            .get(self.config.comp_port_b as usize)
            .and_then(Clone::clone);
        let Some(port) = port else {
            return;
        };
        let com_a = self.com_a(state);
        let com_b = self.com_b(state);
        self.pending_pin_overrides.push((
            port.clone(),
            self.config.comp_pin_a,
            if com_a != 0 {
                PinOverrideMode::Enable
            } else {
                PinOverrideMode::None
            },
        ));
        self.pending_pin_overrides.push((
            port,
            self.config.comp_pin_b,
            if com_b != 0 {
                PinOverrideMode::Enable
            } else {
                PinOverrideMode::None
            },
        ));
    }

    fn queue_pin_override(&mut self, state: &CpuState, pin: u8, mode: PinOverrideMode) {
        let port = state
            .gpio_by_port
            .get(self.config.comp_port_b as usize)
            .and_then(Clone::clone);
        if let Some(port) = port {
            self.pending_pin_overrides.push((port, pin, mode));
        }
    }
}

/// ATtiny25/45/85 Timer/Counter1.
#[derive(Clone)]
pub struct AttinyTimer1(Rc<RefCell<AttinyTimer1State>>);

impl AttinyTimer1 {
    pub fn new(cpu: Cpu, config: AttinyTimer1Config) -> Self {
        let state = Rc::new(RefCell::new(AttinyTimer1State::new(config)));
        state.borrow_mut().me = Rc::downgrade(&state);
        let timer = Self(state);
        timer.install_hooks(&cpu, config);
        timer
    }

    fn with_state<T>(
        &self,
        cpu_state: &mut CpuState,
        callback: impl FnOnce(&mut AttinyTimer1State, &mut CpuState) -> T,
    ) -> T {
        let result = callback(&mut self.0.borrow_mut(), cpu_state);
        self.flush_pin_overrides(cpu_state);
        result
    }

    fn flush_pin_overrides(&self, cpu_state: &mut CpuState) {
        loop {
            let pending = {
                let mut state = self.0.borrow_mut();
                std::mem::take(&mut state.pending_pin_overrides)
            };
            if pending.is_empty() {
                return;
            }
            for (port, pin, mode) in pending {
                port.timer_override_pin_with(cpu_state, pin, mode);
            }
        }
    }

    fn install_hooks(&self, cpu: &Cpu, config: AttinyTimer1Config) {
        let timer = self.clone();
        cpu.set_read_hook(
            config.tcnt1,
            Box::new(move |state, addr| {
                timer.with_state(state, |timer, state| timer.handle_tcnt_read(state, addr))
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.tcnt1,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_tcnt_write(state, value))
            }),
        );

        for (addr, handler) in [
            (
                config.ocr1a,
                AttinyTimer1State::handle_ocr1a_write as fn(&mut AttinyTimer1State, u8) -> bool,
            ),
            (config.ocr1b, AttinyTimer1State::handle_ocr1b_write),
            (config.ocr1c, AttinyTimer1State::handle_ocr1c_write),
        ] {
            let timer = self.clone();
            cpu.set_write_hook(
                addr,
                Box::new(move |state, value, _old_value, _addr, _mask| {
                    timer.with_state(state, |timer, _state| handler(timer, value))
                }),
            );
        }

        let timer = self.clone();
        cpu.set_write_hook(
            config.tccr1,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_tccr1_write(state, value))
            }),
        );

        let mut previous_gtccr_hook = cpu.take_write_hook(config.gtccr);
        let timer = self.clone();
        cpu.set_write_hook(
            config.gtccr,
            Box::new(move |state, value, old_value, addr, mask| {
                timer.with_state(state, |timer, state| {
                    timer.handle_gtccr_write(
                        state,
                        value,
                        previous_gtccr_hook.as_mut(),
                        old_value,
                        addr,
                        mask,
                    )
                })
            }),
        );

        let mut previous_tifr_hook = cpu.take_write_hook(config.tifr);
        let timer = self.clone();
        cpu.set_write_hook(
            config.tifr,
            Box::new(move |state, value, old_value, addr, mask| {
                timer.with_state(state, |timer, state| {
                    timer.handle_tifr_write(
                        state,
                        value,
                        previous_tifr_hook.as_mut(),
                        old_value,
                        addr,
                        mask,
                    )
                })
            }),
        );

        let mut previous_timsk_hook = cpu.take_write_hook(config.timsk);
        let timer = self.clone();
        cpu.set_write_hook(
            config.timsk,
            Box::new(move |state, value, old_value, addr, mask| {
                timer.with_state(state, |timer, state| {
                    timer.handle_timsk_write(
                        state,
                        value,
                        previous_timsk_hook.as_mut(),
                        old_value,
                        addr,
                        mask,
                    )
                })
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peripherals::gpio::PortConfig;

    // ATtiny25/45/85 registers
    const TCCR1: u16 = 0x50;
    const TCNT1: u16 = 0x4f;
    const OCR1A: u16 = 0x4e;
    const OCR1B: u16 = 0x4b;
    const OCR1C: u16 = 0x4d;
    const TIFR: u16 = 0x58;
    const TIMSK: u16 = 0x59;

    // Register bits
    const TOV1: u8 = 1 << 2;
    const OCF1A: u8 = 1 << 6;
    const OCF1B: u8 = 1 << 5;
    const OCIE1A: u8 = 1 << 6;
    const CTC1: u8 = 1 << 7;
    const CS10: u8 = 1;
    const CS13: u8 = 1 << 3;

    const SREG: u16 = 95;

    fn create_timer() -> Cpu {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let attiny_port_b = PortConfig {
            pin: 0x36,
            ddr: 0x37,
            port: 0x38,
            pin_change: None,
            external_interrupts: Vec::new(),
        };
        let _port = IoPort::new(cpu.clone(), attiny_port_b);
        let _timer = AttinyTimer1::new(cpu.clone(), attiny_timer1_config());
        cpu
    }

    #[test]
    fn should_update_timer_every_tick_when_prescaler_is_1() {
        let cpu = create_timer();
        cpu.write_data(TCCR1, CS10);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 1);
    }

    #[test]
    fn should_update_timer_every_128_ticks_when_prescaler_is_128() {
        let cpu = create_timer();
        cpu.write_data(TCCR1, CS13);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(1 + 128);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 1);
    }

    #[test]
    fn should_not_update_timer_when_disabled() {
        let cpu = create_timer();
        cpu.write_data(TCCR1, 0);
        cpu.set_cycles(100_000);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 0);
    }

    #[test]
    fn should_clear_timer_on_ocr1c_match_when_ctc1_is_set() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 9);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.write_data(TCNT1, 8);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(1 + 3);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 1);
    }

    #[test]
    fn should_set_tov1_when_timer_overflows_past_ocr1c() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 9);
        cpu.write_data(TCNT1, 9);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 0);
        assert_eq!(cpu.data_byte(TIFR) & TOV1, TOV1);
    }

    #[test]
    fn should_set_ocf1a_when_timer_matches_ocr1a() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 249);
        cpu.write_data(OCR1A, 5);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.write_data(TCNT1, 4);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR) & OCF1A, OCF1A);
    }

    #[test]
    fn should_set_ocf1b_when_timer_matches_ocr1b() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 249);
        cpu.write_data(OCR1B, 10);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.write_data(TCNT1, 9);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR) & OCF1B, OCF1B);
    }

    #[test]
    fn should_fire_compa_interrupt_when_enabled() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 249);
        cpu.write_data(OCR1A, 0);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.write_data(TCNT1, 248);
        cpu.write_data(TIMSK, OCIE1A);
        cpu.write_data(SREG, 0x80);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.pc(), 0x03);
    }

    #[test]
    fn should_overflow_after_full_period_with_prescaler_128() {
        let cpu = create_timer();
        cpu.write_data(TCCR1, CTC1 | CS13);
        cpu.write_data(OCR1C, 249);
        cpu.write_data(TIMSK, OCIE1A);
        cpu.write_data(SREG, 0x80);

        // Full timer period: 250 * 128 = 32000 cycles
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(32_001);
        cpu.tick();

        assert_ne!(cpu.data_byte(TIFR) & TOV1, 0);
    }

    #[test]
    fn should_clear_tov1_by_writing_one_to_tifr() {
        let cpu = create_timer();
        cpu.write_data(OCR1C, 9);
        cpu.write_data(TCNT1, 9);
        cpu.write_data(TCCR1, CTC1 | CS10);
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR) & TOV1, TOV1);
        cpu.write_data(TIFR, TOV1);
        assert_eq!(cpu.data_byte(TIFR) & TOV1, 0);
    }
}
