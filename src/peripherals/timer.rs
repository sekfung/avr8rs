use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{ClockEventId, Cpu, CpuState, InterruptConfig};
use crate::peripherals::gpio::{port_b_config, port_d_config, IoPort, PinOverrideMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerConfig {
    pub bits: u8,
    pub dividers: [u64; 8],

    // Interrupt vectors
    pub capture_interrupt: u16,
    pub comp_a_interrupt: u16,
    pub comp_b_interrupt: u16,
    pub comp_c_interrupt: u16,
    pub ovf_interrupt: u16,

    // Register addresses
    pub tifr: u16,
    pub ocra: u16,
    pub ocrb: u16,
    pub ocrc: u16,
    pub icr: u16,
    pub tcnt: u16,
    pub tccra: u16,
    pub tccrb: u16,
    pub tccrc: u16,
    pub timsk: u16,

    // TIFR bits
    pub tov: u8,
    pub ocfa: u8,
    pub ocfb: u8,
    pub ocfc: u8,

    // TIMSK bits
    pub toie: u8,
    pub ociea: u8,
    pub ocieb: u8,
    pub ociec: u8,

    // Output compare pins
    pub comp_port_a: u16,
    pub comp_pin_a: u8,
    pub comp_port_b: u16,
    pub comp_pin_b: u8,
    pub comp_port_c: u16,
    pub comp_pin_c: u8,

    // External clock pin (optional, 0 = unused)
    pub external_clock_port: u16,
    pub external_clock_pin: u8,
}

const TIMER01_DIVIDERS: [u64; 8] = [0, 1, 8, 64, 256, 1024, 0, 0];
const TIMER2_DIVIDERS: [u64; 8] = [0, 1, 8, 32, 64, 128, 256, 1024];

pub fn timer0_config() -> TimerConfig {
    TimerConfig {
        bits: 8,
        capture_interrupt: 0,
        comp_a_interrupt: 0x1c,
        comp_b_interrupt: 0x1e,
        comp_c_interrupt: 0,
        ovf_interrupt: 0x20,
        tifr: 0x35,
        ocra: 0x47,
        ocrb: 0x48,
        ocrc: 0,
        icr: 0,
        tcnt: 0x46,
        tccra: 0x44,
        tccrb: 0x45,
        tccrc: 0,
        timsk: 0x6e,
        dividers: TIMER01_DIVIDERS,
        comp_port_a: port_d_config().port,
        comp_pin_a: 6,
        comp_port_b: port_d_config().port,
        comp_pin_b: 5,
        comp_port_c: 0,
        comp_pin_c: 0,
        external_clock_port: port_d_config().port,
        external_clock_pin: 4,
        tov: 1,
        ocfa: 2,
        ocfb: 4,
        ocfc: 0,
        toie: 1,
        ociea: 2,
        ocieb: 4,
        ociec: 0,
    }
}

pub fn timer1_config() -> TimerConfig {
    TimerConfig {
        bits: 16,
        capture_interrupt: 0x14,
        comp_a_interrupt: 0x16,
        comp_b_interrupt: 0x18,
        comp_c_interrupt: 0,
        ovf_interrupt: 0x1a,
        tifr: 0x36,
        ocra: 0x88,
        ocrb: 0x8a,
        ocrc: 0,
        icr: 0x86,
        tcnt: 0x84,
        tccra: 0x80,
        tccrb: 0x81,
        tccrc: 0x82,
        timsk: 0x6f,
        dividers: TIMER01_DIVIDERS,
        comp_port_a: port_b_config().port,
        comp_pin_a: 1,
        comp_port_b: port_b_config().port,
        comp_pin_b: 2,
        comp_port_c: 0,
        comp_pin_c: 0,
        external_clock_port: port_d_config().port,
        external_clock_pin: 5,
        tov: 1,
        ocfa: 2,
        ocfb: 4,
        ocfc: 0,
        toie: 1,
        ociea: 2,
        ocieb: 4,
        ociec: 0,
    }
}

pub fn timer2_config() -> TimerConfig {
    TimerConfig {
        bits: 8,
        capture_interrupt: 0,
        comp_a_interrupt: 0x0e,
        comp_b_interrupt: 0x10,
        comp_c_interrupt: 0,
        ovf_interrupt: 0x12,
        tifr: 0x37,
        ocra: 0xb3,
        ocrb: 0xb4,
        ocrc: 0,
        icr: 0,
        tcnt: 0xb2,
        tccra: 0xb0,
        tccrb: 0xb1,
        tccrc: 0,
        timsk: 0x70,
        dividers: TIMER2_DIVIDERS,
        comp_port_a: port_b_config().port,
        comp_pin_a: 3,
        comp_port_b: port_d_config().port,
        comp_pin_b: 3,
        comp_port_c: 0,
        comp_pin_c: 0,
        external_clock_port: 0,
        external_clock_pin: 0,
        tov: 1,
        ocfa: 2,
        ocfb: 4,
        ocfc: 0,
        toie: 1,
        ociea: 2,
        ocieb: 4,
        ociec: 0,
    }
}

const OCTOGGLE: u8 = 1;

const FOCA: u8 = 1 << 7;
const FOCB: u8 = 1 << 6;
const FOCC: u8 = 1 << 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerMode {
    Normal,
    PwmPhaseCorrect,
    Ctc,
    FastPwm,
    PwmPhaseFrequencyCorrect,
    Reserved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OcrUpdateMode {
    Immediate,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TovUpdateMode {
    Max,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerTopValue {
    Value(u16),
    OcrA,
    Icr,
}

#[derive(Clone, Copy)]
struct WgmConfig {
    mode: TimerMode,
    top: TimerTopValue,
    ocr_update: OcrUpdateMode,
    tov_update: TovUpdateMode,
    flags: u8,
}

const WGM_MODES_8_BIT: [WgmConfig; 8] = [
    WgmConfig {
        mode: TimerMode::Normal,
        top: TimerTopValue::Value(0xff),
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::Value(0xff),
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::Ctc,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::Value(0xff),
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::Reserved,
        top: TimerTopValue::Value(0xff),
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: OCTOGGLE,
    },
    WgmConfig {
        mode: TimerMode::Reserved,
        top: TimerTopValue::Value(0xff),
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: OCTOGGLE,
    },
];

const WGM_MODES_16_BIT: [WgmConfig; 16] = [
    WgmConfig {
        mode: TimerMode::Normal,
        top: TimerTopValue::Value(0xffff),
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::Value(0x00ff),
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::Value(0x01ff),
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::Value(0x03ff),
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::Ctc,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::Value(0x00ff),
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::Value(0x01ff),
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::Value(0x03ff),
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseFrequencyCorrect,
        top: TimerTopValue::Icr,
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseFrequencyCorrect,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Bottom,
        flags: OCTOGGLE,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::Icr,
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::PwmPhaseCorrect,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Top,
        tov_update: TovUpdateMode::Bottom,
        flags: OCTOGGLE,
    },
    WgmConfig {
        mode: TimerMode::Ctc,
        top: TimerTopValue::Icr,
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::Reserved,
        top: TimerTopValue::Value(0xffff),
        ocr_update: OcrUpdateMode::Immediate,
        tov_update: TovUpdateMode::Max,
        flags: 0,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::Icr,
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: OCTOGGLE,
    },
    WgmConfig {
        mode: TimerMode::FastPwm,
        top: TimerTopValue::OcrA,
        ocr_update: OcrUpdateMode::Bottom,
        tov_update: TovUpdateMode::Top,
        flags: OCTOGGLE,
    },
];

fn comp_to_override(comp: u8) -> PinOverrideMode {
    match comp {
        1 => PinOverrideMode::Toggle,
        2 => PinOverrideMode::Clear,
        3 => PinOverrideMode::Set,
        _ => PinOverrideMode::Enable,
    }
}

#[derive(Clone, Copy)]
enum CompareChannel {
    A,
    B,
    C,
}

struct TimerState {
    me: Weak<RefCell<TimerState>>,
    config: TimerConfig,
    last_cycle: u64,
    ocr_a: u16,
    next_ocr_a: u16,
    ocr_b: u16,
    next_ocr_b: u16,
    ocr_c: u16,
    next_ocr_c: u16,
    ocr_update_mode: OcrUpdateMode,
    tov_update_mode: TovUpdateMode,
    icr: u16,
    timer_mode: TimerMode,
    top_value: TimerTopValue,
    tcnt: u16,
    tcnt_next: u16,
    comp_a: u8,
    comp_b: u8,
    comp_c: u8,
    tcnt_updated: bool,
    update_divider: bool,
    counting_up: bool,
    divider: u64,
    external_clock_port: Option<IoPort>,
    external_clock_edge: Option<bool>,
    high_byte_temp: u8,
    event_id: Option<ClockEventId>,
    pending_pin_overrides: Vec<(IoPort, u8, PinOverrideMode)>,
    ovf_interrupt: InterruptConfig,
    ocfa_interrupt: InterruptConfig,
    ocfb_interrupt: InterruptConfig,
    ocfc_interrupt: InterruptConfig,
    #[cfg(test)]
    override_calls: Vec<(u8, PinOverrideMode)>,
}

impl TimerState {
    fn new(config: TimerConfig) -> Self {
        let make_interrupt = |address: u16, enable_mask: u8, flag_mask: u8| InterruptConfig {
            address: address as u8,
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
            ocr_a: 0,
            next_ocr_a: 0,
            ocr_b: 0,
            next_ocr_b: 0,
            ocr_c: 0,
            next_ocr_c: 0,
            ocr_update_mode: OcrUpdateMode::Immediate,
            tov_update_mode: TovUpdateMode::Max,
            icr: 0,
            timer_mode: TimerMode::Normal,
            top_value: TimerTopValue::Value(if config.bits == 16 { 0xffff } else { 0xff }),
            tcnt: 0,
            tcnt_next: 0,
            comp_a: 0,
            comp_b: 0,
            comp_c: 0,
            tcnt_updated: false,
            update_divider: false,
            counting_up: true,
            divider: 0,
            external_clock_port: None,
            external_clock_edge: None,
            high_byte_temp: 0,
            event_id: None,
            pending_pin_overrides: Vec::new(),
            ovf_interrupt: make_interrupt(config.ovf_interrupt, config.toie, config.tov),
            ocfa_interrupt: make_interrupt(config.comp_a_interrupt, config.ociea, config.ocfa),
            ocfb_interrupt: make_interrupt(config.comp_b_interrupt, config.ocieb, config.ocfb),
            ocfc_interrupt: make_interrupt(config.comp_c_interrupt, config.ociec, config.ocfc),
            #[cfg(test)]
            override_calls: Vec::new(),
        }
    }

    fn max(&self) -> u16 {
        if self.config.bits == 16 {
            0xffff
        } else {
            0xff
        }
    }

    fn has_ocr_c(&self) -> bool {
        self.config.ocrc > 0
    }

    fn wgm(&self, state: &CpuState) -> u8 {
        let mask = if self.config.bits == 16 { 0x18 } else { 0x08 };
        ((state.data_byte(self.config.tccrb) & mask) >> 1)
            | (state.data_byte(self.config.tccra) & 0x3)
    }

    fn cs(&self, state: &CpuState) -> u8 {
        state.data_byte(self.config.tccrb) & 0x7
    }

    fn top(&self) -> u16 {
        match self.top_value {
            TimerTopValue::OcrA => self.ocr_a,
            TimerTopValue::Icr => self.icr,
            TimerTopValue::Value(value) => value,
        }
    }

    fn ocr_mask(&self) -> u16 {
        match self.top_value {
            TimerTopValue::OcrA | TimerTopValue::Icr => 0xffff,
            TimerTopValue::Value(value) => value,
        }
    }

    fn is_phase_pwm(&self) -> bool {
        matches!(
            self.timer_mode,
            TimerMode::PwmPhaseCorrect | TimerMode::PwmPhaseFrequencyCorrect
        )
    }

    fn is_pwm(&self) -> bool {
        matches!(
            self.timer_mode,
            TimerMode::FastPwm | TimerMode::PwmPhaseCorrect | TimerMode::PwmPhaseFrequencyCorrect
        )
    }

    fn reset(&mut self) {
        self.divider = 0;
        self.last_cycle = 0;
        self.ocr_a = 0;
        self.next_ocr_a = 0;
        self.ocr_b = 0;
        self.next_ocr_b = 0;
        self.ocr_c = 0;
        self.next_ocr_c = 0;
        self.icr = 0;
        self.tcnt = 0;
        self.tcnt_next = 0;
        self.tcnt_updated = false;
        self.counting_up = false;
        self.update_divider = true;
    }

    fn update_wgm_config(&mut self, state: &mut CpuState) {
        let wgm = self.wgm(state);
        let config = if self.config.bits == 16 {
            WGM_MODES_16_BIT[wgm as usize]
        } else {
            WGM_MODES_8_BIT[wgm as usize]
        };
        self.timer_mode = config.mode;
        self.top_value = config.top;
        self.ocr_update_mode = config.ocr_update;
        self.tov_update_mode = config.tov_update;

        let tccra = state.data_byte(self.config.tccra);
        let pwm_mode = self.is_pwm();

        let prev_comp_a = self.comp_a;
        self.comp_a = (tccra >> 6) & 0x3;
        if self.comp_a == 1 && pwm_mode && (config.flags & OCTOGGLE) == 0 {
            self.comp_a = 0;
        }
        if (prev_comp_a != 0) != (self.comp_a != 0) {
            let mode = if self.comp_a != 0 {
                PinOverrideMode::Enable
            } else {
                PinOverrideMode::None
            };
            self.update_comp_a(state, mode);
        }

        let prev_comp_b = self.comp_b;
        self.comp_b = (tccra >> 4) & 0x3;
        if self.comp_b == 1 && pwm_mode {
            self.comp_b = 0;
        }
        if (prev_comp_b != 0) != (self.comp_b != 0) {
            let mode = if self.comp_b != 0 {
                PinOverrideMode::Enable
            } else {
                PinOverrideMode::None
            };
            self.update_comp_b(state, mode);
        }

        if self.has_ocr_c() {
            let prev_comp_c = self.comp_c;
            self.comp_c = (tccra >> 2) & 0x3;
            if self.comp_c == 1 && pwm_mode {
                self.comp_c = 0;
            }
            if (prev_comp_c != 0) != (self.comp_c != 0) {
                let mode = if self.comp_c != 0 {
                    PinOverrideMode::Enable
                } else {
                    PinOverrideMode::None
                };
                self.update_comp_c(state, mode);
            }
        }
    }

    fn copy_next_ocr(&mut self) {
        self.ocr_a = self.next_ocr_a;
        self.ocr_b = self.next_ocr_b;
        self.ocr_c = self.next_ocr_c;
    }

    fn handle_tcnt_read(&mut self, state: &mut CpuState, addr: u16) -> u8 {
        self.count(state, false, false);
        if self.config.bits == 16 {
            state.set_data_byte(addr + 1, (self.tcnt >> 8) as u8);
        }
        let value = self.tcnt as u8;
        state.set_data_byte(addr, value);
        value
    }

    fn handle_tcnt_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        self.tcnt_next = ((self.high_byte_temp as u16) << 8) | value as u16;
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

    fn handle_ocra_write(&mut self, value: u8) -> bool {
        self.next_ocr_a = ((self.high_byte_temp as u16) << 8) | value as u16;
        if self.ocr_update_mode == OcrUpdateMode::Immediate {
            self.ocr_a = self.next_ocr_a;
        }
        false
    }

    fn handle_ocrb_write(&mut self, value: u8) -> bool {
        self.next_ocr_b = ((self.high_byte_temp as u16) << 8) | value as u16;
        if self.ocr_update_mode == OcrUpdateMode::Immediate {
            self.ocr_b = self.next_ocr_b;
        }
        false
    }

    fn handle_ocrc_write(&mut self, value: u8) -> bool {
        self.next_ocr_c = ((self.high_byte_temp as u16) << 8) | value as u16;
        if self.ocr_update_mode == OcrUpdateMode::Immediate {
            self.ocr_c = self.next_ocr_c;
        }
        false
    }

    fn handle_icr_write(&mut self, value: u8) -> bool {
        self.icr = ((self.high_byte_temp as u16) << 8) | value as u16;
        false
    }

    fn handle_temp_register_write(&mut self, value: u8) -> bool {
        self.high_byte_temp = value;
        false
    }

    fn handle_ocr_high_write(&mut self, state: &mut CpuState, value: u8, addr: u16) -> bool {
        self.high_byte_temp = value & (self.ocr_mask() >> 8) as u8;
        state.set_data_byte(addr, self.high_byte_temp);
        true
    }

    fn handle_tccra_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        state.set_data_byte(self.config.tccra, value);
        self.update_wgm_config(state);
        true
    }

    fn handle_tccrb_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        let mut value = value;
        if self.config.tccrc == 0 {
            self.check_force_compare(state, value);
            value &= !(FOCA | FOCB);
        }
        state.set_data_byte(self.config.tccrb, value);
        self.update_divider = true;
        self.schedule_count(state, 0);
        self.update_wgm_config(state);
        true
    }

    fn handle_tccrc_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        self.check_force_compare(state, value);
        false
    }

    fn handle_tifr_write(&mut self, state: &mut CpuState, value: u8) -> bool {
        state.clear_interrupt_by_flag(self.ovf_interrupt, value);
        state.clear_interrupt_by_flag(self.ocfa_interrupt, value);
        state.clear_interrupt_by_flag(self.ocfb_interrupt, value);
        state.clear_interrupt_by_flag(self.ocfc_interrupt, value);
        true
    }

    fn handle_timsk_write(&mut self, state: &mut CpuState, value: u8) -> bool {
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
                    Timer(timer).with_state(state, |timer, state| timer.count(state, true, false));
                }
            }),
            delay,
        );
        self.event_id = Some(id);
    }

    fn install_external_clock_listener(&self, port: &IoPort, pin: u8) {
        let me = self.me.clone();
        port.set_external_clock_listener(
            pin as usize,
            Some(Box::new(move |state, value| {
                if let Some(timer) = me.upgrade() {
                    Timer(timer).with_state(state, |timer, state| {
                        if Some(value) == timer.external_clock_edge {
                            timer.count(state, false, true);
                        }
                    });
                }
            })),
        );
    }

    fn detach_external_clock_listener(&mut self) {
        if let (Some(_), Some(port)) = (self.external_clock_edge.take(), &self.external_clock_port)
        {
            port.set_external_clock_listener(self.config.external_clock_pin as usize, None);
        }
    }

    fn count(&mut self, state: &mut CpuState, reschedule: bool, external: bool) {
        let divider = self.divider;
        let cycles = state.cycles();
        let delta = cycles.saturating_sub(self.last_cycle);

        if (divider != 0 && delta >= divider) || external {
            let counter_delta = if external { 1 } else { delta / divider };
            self.last_cycle += counter_delta * divider;
            let value = self.tcnt;
            let top = self.top();
            let phase_pwm = self.is_phase_pwm();
            let new_value = if phase_pwm {
                self.phase_pwm_count(state, value, counter_delta)
            } else {
                ((value as u64 + counter_delta) % (top as u64 + 1)) as u16
            };
            let overflow = value as u64 + counter_delta > top as u64;

            // A CPU write overrides (has priority over) all counter clear or count operations.
            if !self.tcnt_updated {
                self.tcnt = new_value;
                if !phase_pwm {
                    self.timer_updated(state, new_value, value);
                }
            }

            if !phase_pwm {
                if self.timer_mode == TimerMode::FastPwm && overflow {
                    let comp_a = self.comp_a;
                    let comp_b = self.comp_b;
                    if comp_a != 0 {
                        self.update_comp_pin(state, comp_a, CompareChannel::A, true);
                    }
                    if comp_b != 0 {
                        self.update_comp_pin(state, comp_b, CompareChannel::B, true);
                    }
                }

                if self.ocr_update_mode == OcrUpdateMode::Bottom && overflow {
                    self.copy_next_ocr();
                }

                if overflow && (self.tov_update_mode == TovUpdateMode::Top || top == self.max()) {
                    state.set_interrupt_flag(self.ovf_interrupt);
                }
            }
        }

        if self.tcnt_updated {
            self.tcnt = self.tcnt_next;
            self.tcnt_updated = false;
            if (self.tcnt == 0 && self.ocr_update_mode == OcrUpdateMode::Bottom)
                || (self.tcnt == self.top() && self.ocr_update_mode == OcrUpdateMode::Top)
            {
                self.copy_next_ocr();
            }
        }

        if self.update_divider {
            let cs = self.cs(state);
            let new_divider = self.config.dividers[cs as usize];
            self.last_cycle = if new_divider != 0 { state.cycles() } else { 0 };
            self.update_divider = false;
            self.divider = new_divider;
            if self.config.external_clock_port != 0 && self.external_clock_port.is_none() {
                self.external_clock_port = state
                    .gpio_by_port
                    .get(self.config.external_clock_port as usize)
                    .and_then(Clone::clone);
            }
            self.detach_external_clock_listener();
            if new_divider != 0 {
                let delay = self
                    .last_cycle
                    .saturating_add(new_divider)
                    .saturating_sub(state.cycles());
                self.schedule_count(state, delay);
            } else if let Some(port) = self.external_clock_port.clone() {
                if cs == 6 || cs == 7 {
                    self.external_clock_edge = Some(cs == 7);
                    self.install_external_clock_listener(&port, self.config.external_clock_pin);
                }
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

    fn phase_pwm_count(&mut self, state: &mut CpuState, value: u16, delta: u64) -> u16 {
        let ocr_a = self.ocr_a;
        let ocr_b = self.ocr_b;
        let ocr_c = self.ocr_c;
        let has_ocr_c = self.has_ocr_c();
        let top = self.top();
        let max = self.max();
        let tcnt_updated = self.tcnt_updated;
        let mut value = value as i64;
        let mut delta = delta;

        if value == 0 && top == 0 {
            delta = 0;
            if self.ocr_update_mode == OcrUpdateMode::Top {
                self.copy_next_ocr();
            }
        }

        while delta > 0 {
            if self.counting_up {
                value += 1;
                if value == top as i64 && !tcnt_updated {
                    self.counting_up = false;
                    if self.ocr_update_mode == OcrUpdateMode::Top {
                        self.copy_next_ocr();
                    }
                }
            } else {
                value -= 1;
                if value == 0 && !tcnt_updated {
                    self.counting_up = true;
                    state.set_interrupt_flag(self.ovf_interrupt);
                    if self.ocr_update_mode == OcrUpdateMode::Bottom {
                        self.copy_next_ocr();
                    }
                }
            }

            if !tcnt_updated {
                if value == ocr_a as i64 {
                    state.set_interrupt_flag(self.ocfa_interrupt);
                    if self.comp_a != 0 {
                        self.update_comp_pin(state, self.comp_a, CompareChannel::A, false);
                    }
                }
                if value == ocr_b as i64 {
                    state.set_interrupt_flag(self.ocfb_interrupt);
                    if self.comp_b != 0 {
                        self.update_comp_pin(state, self.comp_b, CompareChannel::B, false);
                    }
                }
                if has_ocr_c && value == ocr_c as i64 {
                    state.set_interrupt_flag(self.ocfc_interrupt);
                    if self.comp_c != 0 {
                        self.update_comp_pin(state, self.comp_c, CompareChannel::C, false);
                    }
                }
            }
            delta -= 1;
        }

        (value & max as i64) as u16
    }

    fn timer_updated(&mut self, state: &mut CpuState, value: u16, prev_value: u16) {
        let ocr_a = self.ocr_a;
        let ocr_b = self.ocr_b;
        let ocr_c = self.ocr_c;
        let has_ocr_c = self.has_ocr_c();
        let overflow = prev_value > value;

        if ((prev_value < ocr_a || overflow) && value >= ocr_a) || (prev_value < ocr_a && overflow)
        {
            state.set_interrupt_flag(self.ocfa_interrupt);
            if self.comp_a != 0 {
                self.update_comp_pin(state, self.comp_a, CompareChannel::A, false);
            }
        }
        if ((prev_value < ocr_b || overflow) && value >= ocr_b) || (prev_value < ocr_b && overflow)
        {
            state.set_interrupt_flag(self.ocfb_interrupt);
            if self.comp_b != 0 {
                self.update_comp_pin(state, self.comp_b, CompareChannel::B, false);
            }
        }
        if has_ocr_c
            && (((prev_value < ocr_c || overflow) && value >= ocr_c)
                || (prev_value < ocr_c && overflow))
        {
            state.set_interrupt_flag(self.ocfc_interrupt);
            if self.comp_c != 0 {
                self.update_comp_pin(state, self.comp_c, CompareChannel::C, false);
            }
        }
    }

    fn check_force_compare(&mut self, state: &mut CpuState, value: u8) {
        if self.is_pwm() {
            // The FOCnA/FOCnB/FOCnC bits are only active in non-PWM modes.
            return;
        }
        let comp_a = self.comp_a;
        let comp_b = self.comp_b;
        let comp_c = self.comp_c;
        if value & FOCA != 0 {
            self.update_comp_pin(state, comp_a, CompareChannel::A, false);
        }
        if value & FOCB != 0 {
            self.update_comp_pin(state, comp_b, CompareChannel::B, false);
        }
        if self.config.comp_port_c != 0 && value & FOCC != 0 {
            self.update_comp_pin(state, comp_c, CompareChannel::C, false);
        }
    }

    fn update_comp_pin(
        &mut self,
        state: &mut CpuState,
        comp_value: u8,
        channel: CompareChannel,
        bottom: bool,
    ) {
        let inverting_mode = comp_value == 3;
        let is_set = self.counting_up == inverting_mode;
        let new_value = match self.timer_mode {
            TimerMode::Normal | TimerMode::Ctc => comp_to_override(comp_value),
            TimerMode::FastPwm => {
                if comp_value == 1 {
                    if bottom {
                        PinOverrideMode::None
                    } else {
                        PinOverrideMode::Toggle
                    }
                } else if inverting_mode != bottom {
                    PinOverrideMode::Set
                } else {
                    PinOverrideMode::Clear
                }
            }
            TimerMode::PwmPhaseCorrect | TimerMode::PwmPhaseFrequencyCorrect => {
                if comp_value == 1 {
                    PinOverrideMode::Toggle
                } else if is_set {
                    PinOverrideMode::Set
                } else {
                    PinOverrideMode::Clear
                }
            }
            TimerMode::Reserved => PinOverrideMode::None,
        };

        if new_value != PinOverrideMode::None {
            match channel {
                CompareChannel::A => self.update_comp_a(state, new_value),
                CompareChannel::B => self.update_comp_b(state, new_value),
                CompareChannel::C => self.update_comp_c(state, new_value),
            }
        }
    }

    fn override_pin(
        &mut self,
        state: &mut CpuState,
        port_addr: u16,
        pin: u8,
        mode: PinOverrideMode,
    ) {
        let port = state
            .gpio_by_port
            .get(port_addr as usize)
            .and_then(Clone::clone);
        if let Some(port) = port {
            #[cfg(test)]
            self.override_calls.push((pin, mode));
            self.pending_pin_overrides.push((port, pin, mode));
        }
    }

    fn update_comp_a(&mut self, state: &mut CpuState, value: PinOverrideMode) {
        let port = self.config.comp_port_a;
        let pin = self.config.comp_pin_a;
        self.override_pin(state, port, pin, value);
    }

    fn update_comp_b(&mut self, state: &mut CpuState, value: PinOverrideMode) {
        let port = self.config.comp_port_b;
        let pin = self.config.comp_pin_b;
        self.override_pin(state, port, pin, value);
    }

    fn update_comp_c(&mut self, state: &mut CpuState, value: PinOverrideMode) {
        let port = self.config.comp_port_c;
        let pin = self.config.comp_pin_c;
        self.override_pin(state, port, pin, value);
    }
}

/// AVR-8 timer, supporting the ATmega328P timer 0/1/2 configurations.
#[derive(Clone)]
pub struct Timer(Rc<RefCell<TimerState>>);

impl Timer {
    pub fn new(cpu: Cpu, config: TimerConfig) -> Self {
        let state = Rc::new(RefCell::new(TimerState::new(config)));
        state.borrow_mut().me = Rc::downgrade(&state);
        let timer = Timer(state);
        {
            let mut cpu_state = cpu.borrow_state_mut();
            timer.with_state(&mut cpu_state, |state, cpu_state| {
                state.update_wgm_config(cpu_state);
            });
        }

        timer.install_hooks(&cpu, config);
        timer
    }

    pub fn reset(&self) {
        self.0.borrow_mut().reset();
    }

    pub(crate) fn reset_after_cpu(&self) {
        let mut state = self.0.borrow_mut();
        state.detach_external_clock_listener();
        let mut reset = TimerState::new(state.config);
        reset.me = state.me.clone();
        *state = reset;
    }

    pub fn debug_tcnt(&self) -> u16 {
        self.0.borrow().tcnt
    }

    #[cfg(test)]
    pub(crate) fn override_calls(&self) -> Vec<(u8, PinOverrideMode)> {
        self.0.borrow().override_calls.clone()
    }

    #[cfg(test)]
    pub(crate) fn clear_override_calls(&self) {
        self.0.borrow_mut().override_calls.clear();
    }

    fn with_state<T>(
        &self,
        cpu_state: &mut CpuState,
        callback: impl FnOnce(&mut TimerState, &mut CpuState) -> T,
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

    fn install_hooks(&self, cpu: &Cpu, config: TimerConfig) {
        let timer = self.clone();
        cpu.set_read_hook(
            config.tcnt,
            Box::new(move |state, addr| {
                timer.with_state(state, |timer, state| timer.handle_tcnt_read(state, addr))
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.tcnt,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_tcnt_write(state, value))
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.ocra,
            Box::new(move |_state, value, _old_value, _addr, _mask| {
                timer.with_state(_state, |timer, _state| timer.handle_ocra_write(value))
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.ocrb,
            Box::new(move |_state, value, _old_value, _addr, _mask| {
                timer.with_state(_state, |timer, _state| timer.handle_ocrb_write(value))
            }),
        );

        if config.ocrc > 0 {
            let timer = self.clone();
            cpu.set_write_hook(
                config.ocrc,
                Box::new(move |_state, value, _old_value, _addr, _mask| {
                    timer.with_state(_state, |timer, _state| timer.handle_ocrc_write(value))
                }),
            );
        }

        if config.bits == 16 {
            let timer = self.clone();
            cpu.set_write_hook(
                config.icr,
                Box::new(move |_state, value, _old_value, _addr, _mask| {
                    timer.with_state(_state, |timer, _state| timer.handle_icr_write(value))
                }),
            );

            for addr in [config.tcnt + 1, config.icr + 1] {
                let timer = self.clone();
                cpu.set_write_hook(
                    addr,
                    Box::new(move |_state, value, _old_value, _addr, _mask| {
                        timer.with_state(_state, |timer, _state| {
                            timer.handle_temp_register_write(value)
                        })
                    }),
                );
            }

            let mut high_byte_addrs = vec![config.ocra + 1, config.ocrb + 1];
            if config.ocrc > 0 {
                high_byte_addrs.push(config.ocrc + 1);
            }
            for addr in high_byte_addrs {
                let timer = self.clone();
                cpu.set_write_hook(
                    addr,
                    Box::new(move |state, value, _old_value, addr, _mask| {
                        timer.with_state(state, |timer, state| {
                            timer.handle_ocr_high_write(state, value, addr)
                        })
                    }),
                );
            }
        }

        let timer = self.clone();
        cpu.set_write_hook(
            config.tccra,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_tccra_write(state, value))
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.tccrb,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_tccrb_write(state, value))
            }),
        );

        if config.tccrc > 0 {
            let timer = self.clone();
            cpu.set_write_hook(
                config.tccrc,
                Box::new(move |state, value, _old_value, _addr, _mask| {
                    timer.with_state(state, |timer, state| timer.handle_tccrc_write(state, value))
                }),
            );
        }

        let timer = self.clone();
        cpu.set_write_hook(
            config.tifr,
            Box::new(move |state, value, _old_value, _addr, mask| {
                timer.with_state(state, |timer, state| {
                    timer.handle_tifr_write(state, value & mask)
                })
            }),
        );

        let timer = self.clone();
        cpu.set_write_hook(
            config.timsk,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                timer.with_state(state, |timer, state| timer.handle_timsk_write(state, value))
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peripherals::gpio::{port_b_config, port_d_config};
    use crate::test_support::{asm_program, TestProgramRunner};

    // CPU registers
    const R1: u16 = 1;
    const R17: u16 = 17;
    const R18: u16 = 18;
    const R19: u16 = 19;
    const R20: u16 = 20;
    const R21: u16 = 21;
    const R22: u16 = 22;
    const SREG: u16 = 95;

    // Timer 0 Registers
    const TIFR0: u16 = 0x35;
    const TCCR0A: u16 = 0x44;
    const TCCR0B: u16 = 0x45;
    const TCNT0: u16 = 0x46;
    const OCR0A: u16 = 0x47;
    const OCR0B: u16 = 0x48;
    const TIMSK0: u16 = 0x6e;
    const TIMSK1: u16 = 0x6f;

    // Timer 1 Registers
    const TIFR1: u16 = 0x36;
    const TCCR1A: u16 = 0x80;
    const TCCR1B: u16 = 0x81;
    const TCCR1C: u16 = 0x82;
    const TCNT1: u16 = 0x84;
    const TCNT1H: u16 = 0x85;
    const ICR1: u16 = 0x86;
    const ICR1H: u16 = 0x87;
    const OCR1A: u16 = 0x88;
    const OCR1AH: u16 = 0x89;
    const OCR1B: u16 = 0x8a;
    const OCR1C: u16 = 0x8c;
    const OCR1CH: u16 = 0x8d;

    // Timer 2 Registers
    const TCCR2B: u16 = 0xb1;
    const TCNT2: u16 = 0xb2;

    // Register bit names
    const TOV0: u8 = 1;
    const TOV1: u8 = 1;
    const OCIE0A: u8 = 2;
    const OCIE0B: u8 = 4;
    const TOIE0: u8 = 1;
    const OCF0A: u8 = 2;
    const OCF0B: u8 = 4;
    const OCF1A: u8 = 1 << 1;
    const OCF1B: u8 = 1 << 2;
    const OCF1C: u8 = 1 << 3;
    const WGM00: u8 = 1;
    const WGM10: u8 = 1;
    const WGM01: u8 = 2;
    const WGM11: u8 = 2;
    const WGM12: u8 = 8;
    const WGM13: u8 = 16;
    const CS00: u8 = 1;
    const CS01: u8 = 2;
    const CS02: u8 = 4;
    const CS10: u8 = 1;
    const CS21: u8 = 2;
    const CS22: u8 = 4;
    const COM0B1: u8 = 1 << 5;
    const COM1C0: u8 = 1 << 2;
    const FOC0B: u8 = 1 << 6;
    const FOC1C: u8 = 1 << 5;

    const T0: u8 = 4; // PD4 on ATmega328p

    fn read_word(cpu: &Cpu, addr: u16) -> u16 {
        cpu.data_byte(addr) as u16 | ((cpu.data_byte(addr + 1) as u16) << 8)
    }

    fn nop_count(source: &str) -> usize {
        source
            .lines()
            .filter(|line| {
                let code = line.split([';', '#']).next().unwrap_or("").trim();
                code.eq_ignore_ascii_case("NOP")
            })
            .count()
    }

    #[test]
    fn should_update_timer_every_tick_when_prescaler_is_1() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 1);
    }

    #[test]
    fn should_update_timer_every_64_ticks_when_prescaler_is_3() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0B, CS01 | CS00); // Set prescaler to 64
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(1 + 64);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 1);
    }

    #[test]
    fn should_not_update_timer_if_it_has_been_disabled() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0B, 0); // No prescaler (timer disabled)
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(100000);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
    }

    #[test]
    fn should_set_the_tov_flag_when_timer_wraps_above_top_value() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1

        cpu.set_cycles(1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0xff);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0);

        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
    }

    #[test]
    fn should_set_the_tov_if_timer_overflows_past_top_without_reaching_top() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xfe);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        cpu.set_cycles(cpu.cycles() + 4);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0x2);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
    }

    #[test]
    fn should_clear_the_tov_flag_when_writing_1_to_the_tov_bit_and_not_trigger_the_interrupt() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
        cpu.write_data(TIFR0, TOV0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0);
    }

    #[test]
    fn should_set_tov_if_timer_overflows_in_fast_pwm_mode() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.write_data(OCR0A, 0x7f);
        cpu.write_data(TCCR0A, WGM01 | WGM00); // WGM: Fast PWM
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
    }

    #[test]
    fn should_generate_an_overflow_interrupt_if_timer_overflows_and_interrupts_enabled() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.write_data(TIMSK0, TOIE0);
        cpu.set_data_byte(SREG, 0x80); // SREG: I-------
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 2);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0);
        assert_eq!(cpu.pc(), 0x20);
        assert_eq!(cpu.cycles(), 4);
    }

    #[test]
    fn should_support_overriding_tifr_tov_and_timsk_toie_bits_issue_64() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let config = TimerConfig {
            // The following values correspond ATtiny85 config:
            tov: 2,
            ocfa: 2,
            ocfb: 8,
            toie: 2,
            ociea: 16,
            ocieb: 8,
            ..timer0_config()
        };
        let _timer = Timer::new(cpu.clone(), config);
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.write_data(TIMSK0, 2);
        cpu.set_data_byte(SREG, 0x80); // SREG: I-------
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 2);
        assert_eq!(cpu.data_byte(TIFR0) & 2, 0);
        assert_eq!(cpu.pc(), 0x20);
        assert_eq!(cpu.cycles(), 4);
    }

    #[test]
    fn should_not_generate_an_overflow_interrupt_when_global_interrupts_disabled() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_data_byte(TIMSK0, TOIE0);
        cpu.set_data_byte(SREG, 0x0); // SREG: --------
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_not_generate_an_overflow_interrupt_when_toie0_is_clear() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_data_byte(TIMSK0, 0);
        cpu.set_data_byte(SREG, 0x80); // SREG: I-------
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_set_ocf0a_b_flags_when_ocra_b_equals_0_and_the_timer_equals_ocra_issue_74() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xff);
        cpu.write_data(OCR0A, 0x0);
        cpu.write_data(OCR0B, 0x0);
        cpu.write_data(TCCR0A, 0x0); // WGM: Normal
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & (OCF0A | OCF0B), OCF0A | OCF0B);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_set_the_ocf1a_flag_when_ocr1a_equals_120_and_timer_overflowed_past_120_in_wgm_mode_15_issue_94(
    ) {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1, 118);
        cpu.write_data(OCR1A, 120);
        cpu.write_data(OCR1B, 4); // To avoid getting the OCF1B flag set
        cpu.write_data(TCCR1A, WGM10 | WGM11); // WGM: Fast PWM
        cpu.write_data(TCCR1B, WGM12 | WGM13 | CS10); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(5);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT1), 1);
        assert_eq!(cpu.data_byte(TIFR1) & (OCF1A | OCF1B), OCF1A);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 5);
    }

    #[test]
    fn should_set_ocf0a_flag_when_timer_equals_ocra() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x10);
        cpu.write_data(OCR0A, 0x11);
        cpu.write_data(TCCR0A, 0x0); // WGM: Normal
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR0), OCF0A);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_reset_the_counter_in_ctc_mode_if_it_equals_to_ocra() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x10);
        cpu.write_data(OCR0A, 0x11);
        cpu.write_data(TCCR0A, WGM01); // WGM: CTC
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_not_set_the_tov_bit_when_top_lt_max_in_ctc_mode_issue_75() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x1e);
        cpu.write_data(OCR0A, 0x1f);
        cpu.write_data(TCCR0A, WGM01); // WGM: CTC
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0); // TOV0 clear
    }

    #[test]
    fn should_set_the_tov_bit_when_top_eq_max_in_ctc_mode_issue_75() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xfe);
        cpu.write_data(OCR0A, 0xff);
        cpu.write_data(TCCR0A, WGM01); // WGM: CTC
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();

        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0xff);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0); // TOV clear

        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0); // TOV set
    }

    #[test]
    fn should_not_set_the_tov_bit_twice_on_overflow_issue_80() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0xfe);
        cpu.write_data(OCR0A, 0xff);
        cpu.write_data(TCCR0A, WGM01); // WGM: CTC
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1

        cpu.set_cycles(1);
        cpu.tick();

        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0xff);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, 0); // TOV clear

        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0); // TOV set
    }

    #[test]
    fn should_set_ocf0b_flag_when_timer_equals_ocrb() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x10);
        cpu.write_data(OCR0B, 0x11);
        cpu.write_data(TCCR0A, 0x0); // WGM: (Normal)
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR0), OCF0B);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_generate_timer_compare_a_interrupt_when_tcnt0_equals_tcnta() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x20);
        cpu.write_data(OCR0A, 0x21);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.write_data(TIMSK0, OCIE0A);
        cpu.set_data_byte(95, 0x80); // SREG: I-------
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0x23);
        assert_eq!(cpu.data_byte(TIFR0) & OCF0A, 0);
        assert_eq!(cpu.pc(), 0x1c);
        assert_eq!(cpu.cycles(), 4);
    }

    #[test]
    fn should_not_generate_timer_compare_a_interrupt_when_ociea_is_disabled() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x20);
        cpu.write_data(OCR0A, 0x21);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.write_data(TIMSK0, 0);
        cpu.set_data_byte(95, 0x80); // SREG: I-------
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0x21);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_generate_timer_compare_b_interrupt_when_tcnt0_equals_tcntb() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCNT0, 0x20);
        cpu.write_data(OCR0B, 0x21);
        cpu.write_data(TCCR0B, CS00); // Set prescaler to 1
        cpu.write_data(TIMSK0, OCIE0B);
        cpu.set_data_byte(95, 0x80); // SREG: I-------
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0x23);
        assert_eq!(cpu.data_byte(TIFR0) & OCF0B, 0);
        assert_eq!(cpu.pc(), 0x1e);
        assert_eq!(cpu.cycles(), 4);
    }

    #[test]
    fn should_not_increment_tcnt_on_the_same_cycle_of_tcnt_write_issue_36() {
        let asm = asm_program(
            r#"
            LDI r16, 0x1    ; TCCR0B = 1 << CS00;
            OUT 0x25, r16
            LDI r16, 0x30   ; TCNT0 <- 0x30
            OUT 0x26, r16
            NOP
            IN r17, 0x26    ; r17 <- TCNT
            "#,
        );
        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer0_config());
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);
        assert_eq!(cpu.data_byte(R17), 0x31);
    }

    #[test]
    fn timer2_should_count_every_256_ticks_when_prescaler_is_6_issue_5() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer2_config());
        cpu.write_data(TCCR2B, CS22 | CS21); // Set prescaler to 256
        cpu.set_cycles(1);
        cpu.tick();

        cpu.set_cycles(1 + 511);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT2), 1);

        cpu.set_cycles(1 + 512);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT2), 2);
    }

    #[test]
    fn should_update_tcnt_as_it_is_being_read_by_a_2_cycle_instruction_issue_40() {
        let asm = asm_program(
            r#"
            LDI r16, 0x1      ; TCCR0B = 1 << CS00
            OUT 0x25, r16
            LDI r16, 0x0      ; TCNT0 <- 0
            OUT 0x26, r16
            NOP
            LDS r1, 0x46      ; r1 <- TCNT0 (2 cycles)
            "#,
        );
        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer0_config());
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);
        assert_eq!(cpu.data_byte(R1), 2);
    }

    #[test]
    fn should_not_start_counting_before_the_prescaler_is_first_set_issue_41() {
        let asm = asm_program(
            r#"
            NOP
            NOP
            NOP
            NOP
            LDI r16, 0x1    ; TCCR2B = 1 << CS20;
            STS 0xb1, r16   ; Should start counting after this line
            NOP
            LDS r17, 0xb2   ; TCNT should equal 2 at this point
            "#,
        );
        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer2_config());
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);
        assert_eq!(cpu.read_data(R17), 2);
    }

    #[test]
    fn should_not_keep_counting_for_one_more_instruction_when_the_timer_is_disabled_issue_72() {
        let asm = asm_program(
            r#"
            EOR r1, r1      ; r1 = 0;
            LDI r16, 0x1    ; TCCR2B = 1 << CS20;
            STS 0xb1, r16   ; Should start counting after this instruction,
            STS 0xb1, r1    ; and stop counting *after* this one.
            NOP
            LDS r17, 0xb2   ; TCNT2 should equal 2 at this point (not counting the NOP)
            "#,
        );
        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer2_config());
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);
        assert_eq!(cpu.read_data(R17), 2);
    }

    #[test]
    fn should_clear_oc0b_pin_when_writing_1_to_foc0b() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0A, COM0B1);

        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        cpu.write_data(TCCR0B, FOC0B);

        assert_eq!(timer.override_calls(), vec![(5, PinOverrideMode::Clear)]);
    }

    #[test]
    fn should_set_oc0a_on_compare_match_clear_on_bottom_issue_78() {
        let asm = asm_program(
            r#"
            LDI r16, 0xfc   ; TCNT0 = 0xfc;
            OUT 0x26, r16
            LDI r16, 0xfe   ; OCR0A = 0xfe;
            OUT 0x27, r16
            ; WGM: Fast PWM, enable OC0A mode 3 (set on Compare Match, clear on Bottom)
            LDI r16, 0xc3   ; TCCR0A = (1 << COM0A1) | (1 << COM0A0) | (1 << WGM01) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x1    ; TCCR0B = 1 << CS00;
            OUT 0x25, r16

            NOP             ; TCNT is now 0xfd
            beforeMatch:
            NOP             ; TCNT is now 0xfe (Compare Match)
            afterMatch:
            NOP             ; TCNT is now 0xff
            beforeBottom:
            NOP             ; TCNT is now 0x00 (BOTTOM)
            afterBottom:
            NOP
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());

        runner.run_to_address(asm.labels["beforeMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfd);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Enable)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Set)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["beforeBottom"]);
        assert_eq!(cpu.read_data(TCNT0), 0xff);
        assert!(timer.override_calls().is_empty());
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterBottom"]);
        assert_eq!(cpu.read_data(TCNT0), 0x0);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Clear)]);
    }

    #[test]
    fn should_toggle_oc0a_on_compare_match_when_com0an_eq_1_issue_78() {
        let asm = asm_program(
            r#"
            LDI r16, 0xfc   ; TCNT0 = 0xfc;
            OUT 0x26, r16
            LDI r16, 0xfe   ; OCR0A = 0xfe;
            OUT 0x27, r16
            ; WGM: Fast PWM, enable OC0A mode 1 (Toggle)
            LDI r16, 0x43   ; TCCR0A = (1 << COM0A0) | (1 << WGM01) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16

            NOP             ; TCNT is now 0xfd
            beforeMatch:
            NOP             ; TCNT is now 0xfe (Compare Match, TOP)
            afterMatch:
            NOP             ; TCNT is now 0
            afterOverflow:
            NOP
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());

        runner.run_to_address(asm.labels["beforeMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfd);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Enable)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Toggle)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterOverflow"]);
        assert_eq!(cpu.read_data(TCNT0), 0);
        assert!(timer.override_calls().is_empty());
    }

    #[test]
    fn should_leave_oc0a_disconnected_when_com0an_eq_1_and_wgm02_eq_0_issue_78() {
        let asm = asm_program(
            r#"
            LDI r16, 0xfc   ; TCNT0 = 0xfc;
            OUT 0x26, r16
            LDI r16, 0xfe   ; OCR0A = 0xfe;
            OUT 0x27, r16
            ; WGM: Fast PWM mode 7, enable OC0A mode 1 (Toggle)
            LDI r16, 0x43   ; TCCR0A = (1 << COM0A0) | (1 << WGM01) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16

            beforeClearWGM02:
            LDI r16, 0x01   ; TCCR0B = (1 << CS00);
            OUT 0x25, r16

            afterClearWGM02:
            NOP
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());

        runner.run_to_address(asm.labels["beforeClearWGM02"]);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Enable)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterClearWGM02"]);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::None)]);
    }

    #[test]
    fn should_count_up_to_top_down_to_0_and_then_set_tov_flag() {
        let asm = asm_program(
            r#"
            LDI r16, 0x3   ; OCR0A = 0x3;   // <- TOP value
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to PWM, Phase Correct, top OCR0A
            LDI r16, 0x1   ; TCCR0A = 1 << WGM00;
            OUT 0x24, r16
            LDI r16, 0x9   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0x2   ; TCNT0 = 0x2;
            OUT 0x26, r16

            IN r17, 0x26   ; TCNT0 will be 2
            IN r18, 0x26   ; TCNT0 will be 3
            IN r19, 0x26   ; TCNT0 will be 2
            IN r20, 0x26   ; TCNT0 will be 1
            IN r21, 0x26   ; TCNT0 will be 0
            IN r22, 0x26   ; TCNT0 will be 1 (end of test)
            "#,
        );
        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer0_config());
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 2);
        assert_eq!(cpu.read_data(R18), 3);
        assert_eq!(cpu.read_data(R19), 2);
        assert_eq!(cpu.read_data(R20), 1);
        assert_eq!(cpu.read_data(R21), 0);
        assert_eq!(cpu.read_data(R22), 1);
        assert_eq!(cpu.data_byte(TIFR0) & TOV0, TOV0);
    }

    #[test]
    fn should_clear_oc0a_when_tcnt0_eq_ocr0a_and_counting_up() {
        let source = r#"
            LDI r16, 0xfe   ; OCR0A = 0xfe;   // <- TOP value
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to PWM, Phase Correct
            LDI r16, 0x81   ; TCCR0A = (1 << COM0A1) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x1   ; TCCR0B = (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0xfd   ; TCNT0 = 0xfd;
            OUT 0x26, r16

            NOP   ; TCNT0 will be 0xfe
            NOP   ; TCNT0 will be 0xff
            NOP   ; TCNT0 will be 0xfe again (end of test)
            "#;
        let asm = asm_program(source);

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let nop_count = nop_count(source);
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count - nop_count);

        assert_eq!(cpu.read_data(TCNT0), 0xfd);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Enable)]);
        timer.clear_override_calls();

        runner.run_instructions(1);
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Clear)]);
        timer.clear_override_calls();

        runner.run_instructions(1);
        assert_eq!(cpu.read_data(TCNT0), 0xff);
        assert!(timer.override_calls().is_empty());

        runner.run_instructions(1);
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Set)]);
    }

    #[test]
    fn should_toggle_oc0a_when_tcnt0_eq_ocr0a_and_com0an_eq_1_issue_78() {
        let asm = asm_program(
            r#"
            LDI r16, 0xfe   ; OCR0A = 0xfe;   // <- TOP value
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to PWM, Phase Correct (mode 5)
            LDI r16, 0x41   ; TCCR0A = (1 << COM0A0) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0xfd   ; TCNT0 = 0xfd;
            OUT 0x26, r16

            beforeMatch:
            NOP             ; TCNT0 will be 0xfe
            afterMatch:
            NOP
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());

        runner.run_to_address(asm.labels["beforeMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfd);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Enable)]);
        timer.clear_override_calls();

        runner.run_to_address(asm.labels["afterMatch"]);
        assert_eq!(cpu.read_data(TCNT0), 0xfe);
        assert_eq!(timer.override_calls(), vec![(6, PinOverrideMode::Toggle)]);
    }

    #[test]
    fn should_leave_oc0a_disconnected_tcnt0_eq_ocr0a_and_com0an_eq_1_in_wgm_mode_1_issue_78() {
        let asm = asm_program(
            r#"
            LDI r16, 0xfe   ; OCR0A = 0xfe;   // <- TOP value
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to PWM, Phase Correct (mode 1)
            LDI r16, 0x41   ; TCCR0A = (1 << COM0A0) | (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x01   ; TCCR0B = (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0xfd   ; TCNT0 = 0xfd;
            OUT 0x26, r16
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert!(timer.override_calls().is_empty());
    }

    #[test]
    fn should_not_miss_compare_match_when_executing_multi_cycle_instruction_issue_79() {
        let asm = asm_program(
            r#"
            LDI r16, 0x10   ; OCR0A = 0x10;   // <- TOP value
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to normal, enable OC0A (Set on match)
            LDI r16, 0xc0   ; TCCR0A = (1 << COM0A1) | (1 << COM0A0);
            OUT 0x24, r16
            LDI r16, 0x1    ; TCCR0B = (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0xf    ; TCNT0 = 0xf;
            OUT 0x26, r16
            RJMP 1          ; TCNT0 will be 0x11 (RJMP takes 2 cycles)
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());
        let _port_d = IoPort::new(cpu.clone(), port_d_config());
        timer.clear_override_calls();

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(TCNT0), 0x11);
        assert!(timer
            .override_calls()
            .contains(&(6, PinOverrideMode::Enable)));
        assert!(timer.override_calls().contains(&(6, PinOverrideMode::Set)));
    }

    #[test]
    fn should_only_update_ocr0a_when_tcnt0_eq_top_in_pwm_phase_correct_mode_issue_76() {
        let asm = asm_program(
            r#"
            LDI r16, 0x4   ; OCR0A = 0x4;
            OUT 0x27, r16
            ; Set waveform generation mode (WGM) to PWM, Phase Correct
            LDI r16, 0x01   ; TCCR0A = (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0x0   ; TCNT0 = 0x0;
            OUT 0x26, r16

            LDI r16, 0x2    ; OCR0A = 0x2; // TCNT0 should read 0x0
            OUT 0x27, r16   ; // TCNT0 should read 0x1
            NOP             ; // TCNT0 should read 0x2
            NOP             ; // TCNT0 should read 0x3
            IN r17, 0x26    ; R17 = TCNT;  // TCNT0 should read 0x4 (that's old OCR0A / TOP)
            NOP             ; // TCNT0 should read 0x3
            NOP             ; // TCNT0 should read 0x2
            NOP             ; // TCNT0 should read 0x1
            NOP             ; // TCNT0 should read 0x0
            NOP             ; // TCNT0 should read 0x1
            NOP             ; // TCNT0 should read 0x2
            IN r18, 0x26    ; R18 = TCNT; // TCNT0 should read 0x1
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer0_config());

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 0x4);
        assert_eq!(cpu.read_data(R18), 0x1);
    }

    #[test]
    fn should_update_ocr0a_when_tcnt0_eq_top_and_top_eq_0_in_pwm_phase_correct_mode_issue_119() {
        let asm = asm_program(
            r#"
            ; Set waveform generation mode (WGM) to PWM, Phase Correct
            LDI r16, 0x01   ; TCCR0A = (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0x0   ; TCNT0 = 0x0;
            OUT 0x26, r16

            IN r17, 0x26    ; R17 = TCNT; // TCNT0 should read 0x0
            IN r18, 0x26    ; R18 = TCNT; // TCNT0 should read 0x0
            LDI r16, 0x2    ; OCR0A = 0x2; // TCNT0 should read 0x1
            OUT 0x27, r16   ; // TCNT0 should read 0x1
            NOP             ; // TCNT0 should read 0x2
            IN r19, 0x26    ; R19 = TCNT; // TCNT0 should read 0x1
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer0_config());

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 0);
        assert_eq!(cpu.read_data(R18), 0);
        assert_eq!(cpu.read_data(R19), 0x1);
    }

    #[test]
    fn should_not_overrun_when_top_lt_current_value_in_phase_correct_mode_issue_119() {
        let asm = asm_program(
            r#"
            ; Set waveform generation mode (WGM) to PWM, Phase Correct
            LDI r16, 0x01   ; TCCR0A = (1 << WGM00);
            OUT 0x24, r16
            LDI r16, 0x09   ; TCCR0B = (1 << WGM02) | (1 << CS00);
            OUT 0x25, r16
            LDI r16, 0xff   ; TCNT0 = 0xff;
            OUT 0x26, r16

            IN r17, 0x26    ; R17 = TCNT; // TCNT0 should read 255
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer0_config());

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 255);
        assert_eq!(timer.debug_tcnt(), 0); // TCNT should wrap
    }

    #[test]
    fn should_increment_16_bit_tcnt_by_1() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1H, 0x22); // TCNT1 <- 0x2233
        cpu.write_data(TCNT1, 0x33); // ...
        let timer_low = cpu.read_data(TCNT1);
        let timer_high = cpu.read_data(TCNT1H);
        assert_eq!((timer_high as u16) << 8 | timer_low as u16, 0x2233);
        cpu.write_data(TCCR1A, 0x0); // WGM: Normal
        cpu.write_data(TCCR1B, CS10); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        cpu.read_data(TCNT1);
        assert_eq!(read_word(&cpu, TCNT1), 0x2234);
    }

    #[test]
    fn should_set_ocf1a_flag_when_timer_equals_ocra_16_bit_mode() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1H, 0x10); // TCNT1 <- 0x10ee
        cpu.write_data(TCNT1, 0xee); // ...
        cpu.write_data(OCR1AH, 0x10); // OCR1 <- 0x10ef
        cpu.write_data(OCR1A, 0xef); // ...
        cpu.write_data(TCCR1A, 0x0); // WGM: Normal
        cpu.write_data(TCCR1B, CS10); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR1), OCF1A);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_set_ocf1c_flag_when_timer_equals_ocrc() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let config = TimerConfig {
            ocrc: OCR1C,
            ocfc: OCF1C,
            ..timer1_config()
        };
        let _timer = Timer::new(cpu.clone(), config);
        cpu.write_data(TCNT1H, 0);
        cpu.write_data(TCNT1, 0x10);
        cpu.write_data(OCR1C, 0x11);
        cpu.write_data(OCR1CH, 0x11);
        cpu.write_data(TCCR1A, 0x0); // WGM: (Normal)
        cpu.write_data(TCCR1B, CS00); // Set prescaler to 1
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.data_byte(TIFR1), OCF1C);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_generate_an_overflow_interrupt_if_16_bit_timer_overflows_and_interrupts_enabled() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCCR1A, 0x3); // TCCR1A <- WGM10 | WGM11 (Fast PWM, 10-bit)
        cpu.write_data(TCCR1B, 0x9); // TCCR1B <- WGM12 | CS10
        cpu.write_data(TIMSK1, 0x1); // TIMSK1: TOIE1
        cpu.set_data_byte(SREG, 0x80); // SREG: I-------
        cpu.write_data(TCNT1H, 0x3); // TCNT1 <- 0x3ff
        cpu.set_cycles(1);
        cpu.tick();
        cpu.write_data(TCNT1, 0xff); // ...
        cpu.set_cycles(cpu.cycles() + 1); // This cycle shouldn't be counted
        cpu.tick();
        cpu.set_cycles(cpu.cycles() + 1);
        cpu.tick(); // This is where we cause the overflow
        cpu.read_data(TCNT1); // Refresh TCNT1
        assert_eq!(read_word(&cpu, TCNT1), 2);
        assert_eq!(cpu.data_byte(TIFR1) & TOV1, 0);
        assert_eq!(cpu.pc(), 0x1a);
        assert_eq!(cpu.cycles(), 5);
    }

    #[test]
    fn should_reset_the_timer_once_it_reaches_icr_value_in_mode_12() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1H, 0x50); // TCNT1 <- 0x500f
        cpu.write_data(TCNT1, 0x0f); // ...
        cpu.write_data(ICR1H, 0x50); // ICR1 <- 0x5010
        cpu.write_data(ICR1, 0x10); // ...
        cpu.write_data(TCCR1B, WGM13 | WGM12 | CS10); // Set prescaler to 1, WGM: CTC
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(3); // 2 cycles should increment timer twice, beyond ICR1
        cpu.tick();
        cpu.read_data(TCNT1); // Refresh TCNT1
        assert_eq!(read_word(&cpu, TCNT1), 0); // TCNT should be 0
        assert_eq!(cpu.data_byte(TIFR1) & TOV1, 0);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_not_update_the_high_byte_of_tcnt_if_written_after_the_low_byte_issue_37() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1, 0x22);
        cpu.write_data(TCNT1H, 0x55);
        cpu.set_cycles(1);
        cpu.tick();
        let timer_low = cpu.read_data(TCNT1);
        let timer_high = cpu.read_data(TCNT1H);
        assert_eq!((timer_high as u16) << 8 | timer_low as u16, 0x22);
    }

    #[test]
    fn reading_from_tcnt1h_before_tcnt1l_should_return_old_value_issue_37() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCNT1H, 0xff);
        cpu.write_data(TCNT1, 0xff);
        cpu.write_data(TCCR1B, WGM12 | CS10); // Set prescaler to 1, WGM: CTC
        cpu.set_cycles(1);
        cpu.tick();
        cpu.set_cycles(2);
        cpu.tick();
        // We read the high byte before the low byte, so the high byte should still have
        // the previous value:
        let timer_high = cpu.read_data(TCNT1H);
        let timer_low = cpu.read_data(TCNT1);
        assert_eq!((timer_high as u16) << 8 | timer_low as u16, 0xff00);
    }

    #[test]
    fn should_toggle_oc1b_on_compare_match() {
        let source = r#"
            ; Set waveform generation mode (WGM) to Normal, top 0xFFFF
            LDI r16, 0x10   ; TCCR1A = (1 << COM1B0);
            STS 0x80, r16
            LDI r16, 0x1    ; TCCR1B = (1 << CS00);
            STS 0x81, r16
            LDI r16, 0x0    ; OCR1BH = 0x0;
            STS 0x8B, r16
            LDI r16, 0x4a   ; OCR1BL = 0x4a;
            STS 0x8A, r16
            LDI r16, 0x0    ; TCNT1H = 0x0;
            STS 0x85, r16
            LDI r16, 0x49   ; TCNT1L = 0x49;
            STS 0x84, r16

            NOP   ; TCNT1 will be 0x49
            NOP   ; TCNT1 will be 0x4a
            "#;
        let asm = asm_program(source);

        let cpu = Cpu::new(asm.program.clone());
        let timer = Timer::new(cpu.clone(), timer1_config());
        let _port_b = IoPort::new(cpu.clone(), port_b_config());
        timer.clear_override_calls();

        let nop_count = nop_count(source);
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count - nop_count);

        assert_eq!(cpu.read_data(TCNT1), 0x49);
        assert!(timer
            .override_calls()
            .contains(&(2, PinOverrideMode::Enable)));
        timer.clear_override_calls();

        runner.run_instructions(1);
        assert_eq!(cpu.read_data(TCNT1), 0x4a);
        assert!(timer
            .override_calls()
            .contains(&(2, PinOverrideMode::Toggle)));
    }

    #[test]
    fn should_toggle_oc1c_on_compare_match() {
        let source = r#"
            ; Set waveform generation mode (WGM) to Normal, top 0xFFFF
            LDI r16, 0x04   ; TCCR1A = (1 << COM1C0);
            STS 0x80, r16
            LDI r16, 0x1    ; TCCR1B = (1 << CS00);
            STS 0x81, r16
            LDI r16, 0x0    ; OCR1CH = 0x0;
            STS 0x8D, r16
            LDI r16, 0x4a   ; OCR1C = 0x4a;
            STS 0x8C, r16
            LDI r16, 0x0    ; TCNT1H = 0x0;
            STS 0x85, r16
            LDI r16, 0x49   ; TCNT1 = 0x49;
            STS 0x84, r16

            NOP   ; TCNT1 will be 0x49
            NOP   ; TCNT1 will be 0x4a
            "#;
        let asm = asm_program(source);

        let cpu = Cpu::new(asm.program.clone());
        let config = TimerConfig {
            ocrc: OCR1C,
            ocfc: OCF1C,
            comp_port_c: port_b_config().port,
            comp_pin_c: 3,
            ..timer1_config()
        };
        let timer = Timer::new(cpu.clone(), config);
        let _port_b = IoPort::new(cpu.clone(), port_b_config());
        timer.clear_override_calls();

        let nop_count = nop_count(source);
        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count - nop_count);

        assert_eq!(cpu.read_data(TCNT1), 0x49);
        assert!(timer
            .override_calls()
            .contains(&(3, PinOverrideMode::Enable)));
        timer.clear_override_calls();

        runner.run_instructions(1);
        assert_eq!(cpu.read_data(TCNT1), 0x4a);
        assert!(timer
            .override_calls()
            .contains(&(3, PinOverrideMode::Toggle)));
    }

    #[test]
    fn should_toggle_oc1c_on_when_writing_1_to_foc1c() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let config = TimerConfig {
            ocrc: OCR1C,
            ocfc: OCF1C,
            comp_port_c: port_b_config().port,
            comp_pin_c: 3,
            ..timer1_config()
        };
        let timer = Timer::new(cpu.clone(), config);
        cpu.write_data(TCCR1A, COM1C0);

        let _port_b = IoPort::new(cpu.clone(), port_b_config());
        timer.clear_override_calls();

        cpu.write_data(TCCR1C, FOC1C);

        assert_eq!(timer.override_calls(), vec![(3, PinOverrideMode::Toggle)]);
    }

    #[test]
    fn should_not_toggle_oc1c_on_when_writing_1_to_foc1c_in_pwm_mode() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let config = TimerConfig {
            ocrc: OCR1C,
            ocfc: OCF1C,
            comp_port_c: port_b_config().port,
            comp_pin_c: 3,
            ..timer1_config()
        };
        let timer = Timer::new(cpu.clone(), config);
        cpu.write_data(TCCR1A, COM1C0 | WGM11);

        let _port_b = IoPort::new(cpu.clone(), port_b_config());
        timer.clear_override_calls();

        cpu.write_data(TCCR1C, FOC1C);

        assert!(timer.override_calls().is_empty());
    }

    #[test]
    fn should_only_update_ocr1a_when_tcnt1_eq_bottom_in_pwm_phase_frequency_correct_mode_issue_76()
    {
        let asm = asm_program(
            r#"
            LDI r16, 0x0    ; OCR1AH = 0x0;
            STS 0x89, r16
            LDI r16, 0x4   ; OCR1AL = 0x4;
            STS 0x88, r16
            ; Set waveform generation mode (WGM) to PWM Phase/Frequency Correct mode (9)
            LDI r16, 0x01   ; TCCR1A = (1 << WGM10);
            STS 0x80, r16
            LDI r16, 0x11   ; TCCR1B = (1 << WGM13) | (1 << CS00);
            STS 0x81, r16
            LDI r16, 0x0    ; TCNT1H = 0x0;
            STS 0x85, r16
            LDI r16, 0x0    ; TCNT1L = 0x0;
            STS 0x84, r16

            LDI r16, 0x8   ; OCR1AL = 0x8; // TCNT1 should read 0x0
            STS 0x88, r16  ; // TCNT1 should read 0x2 (going up)
            LDS r17, 0x84  ; // TCNT1 should read 0x4 (going down)
            LDS r18, 0x84  ; // TCNT1 should read 0x2 (going down)
            NOP            ; // TCNT1 should read 0x0 (going up)
            NOP            ; // TCNT1 should read 0x1 (going up)
            NOP            ; // TCNT1 should read 0x2 (going up)
            NOP            ; // TCNT1 should read 0x3 (going up)
            NOP            ; // TCNT1 should read 0x4 (going up)
            NOP            ; // TCNT1 should read 0x5 (going up)
            LDS r19, 0x84  ; // TCNT1 should read 0x6 (going up)
            NOP            ; // TCNT1 should read 0x8 (going up)
            LDS r20, 0x84  ; // TCNT1 should read 0x7 (going up)
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer1_config());

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 0x4);
        assert_eq!(cpu.read_data(R18), 0x2);
        assert_eq!(cpu.read_data(R19), 0x6);
        assert_eq!(cpu.read_data(R20), 0x7);
    }

    #[test]
    fn should_update_ocr1a_when_setting_tcnt_to_0_issue_111() {
        let asm = asm_program(
            r#"
            CLR r1          ; r1 is our zero register
            LDI r16, 0x0    ; OCR1AH = 0x0;
            STS 0x89, r1
            LDI r16, 0x8    ; OCR1AL = 0x8;
            STS 0x88, r16
            ; Set waveform generation mode (WGM) to PWM Phase/Frequency Correct mode (9)
            LDI r16, 0x01   ; TCCR1A = (1 << WGM10);
            STS 0x80, r16
            LDI r16, 0x11   ; TCCR1B = (1 << WGM13) | (1 << CS00);
            STS 0x81, r16
            STS 0x85, r1    ; TCNT1H = 0x0;
            STS 0x84, r1    ; TCNT1L = 0x0;

            LDI r16, 0x5   ; OCR1AL = 0x5; // TCNT1 should read 0x0
            STS 0x88, r16  ; // TCNT1 should read 0x2 (going up)
            STS 0x84, r1   ; TCNT1L = 0x0;
            LDS r17, 0x84  ; // TCNT1 should read 0x1 (going up)
            LDS r18, 0x84  ; // TCNT1 should read 0x3 (going up)
            LDS r19, 0x84  ; // TCNT1 should read 0x5 (going down)
            LDS r20, 0x84  ; // TCNT1 should read 0x3 (going down)
            "#,
        );

        let cpu = Cpu::new(asm.program.clone());
        let _timer = Timer::new(cpu.clone(), timer1_config());

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(cpu.read_data(R17), 0x1);
        assert_eq!(cpu.read_data(R18), 0x3);
        assert_eq!(cpu.read_data(R19), 0x5);
        assert_eq!(cpu.read_data(R20), 0x3);
    }

    #[test]
    fn should_mask_the_unused_bits_of_ocr1a_when_using_fixed_top_values() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let _timer = Timer::new(cpu.clone(), timer1_config());
        cpu.write_data(TCCR1A, WGM10 | WGM11); // WGM: FastPWM, top 0x3ff
        cpu.write_data(TCCR1B, WGM12);
        cpu.write_data(OCR1AH, 0xff);
        cpu.write_data(OCR1A, 0xff);
        assert_eq!(cpu.read_data(OCR1A), 0xff);
        assert_eq!(cpu.read_data(OCR1AH), 0x03);
    }

    #[test]
    fn should_count_on_the_falling_edge_of_t0_when_cs_eq_110() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0B, CS02 | CS01); // Count on falling edge
        cpu.set_cycles(1);
        cpu.tick();

        port.set_pin(T0, true); // Rising edge
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 0);

        port.set_pin(T0, false); // Falling edge
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 1);
    }

    #[test]
    fn should_count_on_the_rising_edge_of_t0_when_cs_eq_111() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        let _timer = Timer::new(cpu.clone(), timer0_config());
        cpu.write_data(TCCR0B, CS02 | CS01 | CS00); // Count on rising edge
        cpu.set_cycles(1);
        cpu.tick();

        port.set_pin(T0, true); // Rising edge
        cpu.set_cycles(2);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 1);

        port.set_pin(T0, false); // Falling edge
        cpu.set_cycles(3);
        cpu.tick();
        assert_eq!(cpu.read_data(TCNT0), 1);
    }
}
