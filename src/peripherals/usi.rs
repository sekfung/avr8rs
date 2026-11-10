use std::cell::RefCell;
use std::rc::Rc;

use crate::cpu::{Cpu, CpuState, InterruptConfig};
use crate::peripherals::gpio::IoPort;

const USICR: u16 = 0x2d;
const USISR: u16 = 0x2e;
const USIDR: u16 = 0x2f;
const USIBR: u16 = 0x30;

// USISR bits
const USICNT_MASK: u8 = 0x0f;
const USIDC: u8 = 1 << 4;
const USIPF: u8 = 1 << 5;
const USIOIF: u8 = 1 << 6;
const USISIF: u8 = 1 << 7;

// USICR bits
const USITC: u8 = 1 << 0;
const USICLK: u8 = 1 << 1;
const USICS0: u8 = 1 << 2;
const USICS1: u8 = 1 << 3;
const USIWM0: u8 = 1 << 4;
const USIWM1: u8 = 1 << 5;
const USIOIE: u8 = 1 << 6;
const USISIE: u8 = 1 << 7;

const START: InterruptConfig = InterruptConfig {
    address: 0xd,
    enable_register: USICR,
    enable_mask: USISIE,
    flag_register: USISR,
    flag_mask: USISIF,
    constant: false,
    inverse_flag: false,
};

const OVF: InterruptConfig = InterruptConfig {
    address: 0xe,
    enable_register: USICR,
    enable_mask: USIOIE,
    flag_register: USISR,
    flag_mask: USIOIF,
    constant: false,
    inverse_flag: false,
};

pub struct UsiState {
    start: InterruptConfig,
    ovf: InterruptConfig,
}

impl UsiState {
    fn new() -> Self {
        Self {
            start: START,
            ovf: OVF,
        }
    }
}

/// Universal Serial Interface.
#[derive(Clone)]
pub struct Usi(Rc<RefCell<UsiState>>);

impl Usi {
    fn start(&self) -> InterruptConfig {
        self.0.borrow().start
    }

    fn ovf(&self) -> InterruptConfig {
        self.0.borrow().ovf
    }

    pub fn new(cpu: Cpu, port: IoPort, port_pin: u8, data_pin: u8, clock_pin: u8) -> Self {
        let state = Rc::new(RefCell::new(UsiState::new()));
        let usi = Self(Rc::clone(&state));

        let pin = u16::from(port_pin);
        let port_addr = pin + 2;
        let data_mask = 1u8 << data_pin;

        let listener_usi = usi.clone();
        port.add_listener(Box::new(move |state, value, _old_value| {
            let start = listener_usi.start();
            let two_wire = state.data_byte(USICR) & USIWM1 == USIWM1;
            if two_wire {
                if value & (1 << clock_pin) != 0 && value & data_mask == 0 {
                    // Start condition detected
                    state.set_interrupt_flag(start);
                }
                if value & (1 << clock_pin) != 0 && value & data_mask != 0 {
                    // Stop condition detected
                    let status = state.data_byte(USISR);
                    state.set_data_byte(USISR, status | USIPF);
                }
            }
        }));

        cpu.set_write_hook(
            USIDR,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                state.set_data_byte(USIDR, value);
                update_output(state, pin, port_addr, data_pin);
                true
            }),
        );

        let isr_usi = usi.clone();
        cpu.set_write_hook(
            USISR,
            Box::new(move |cpu_state, value, _old_value, _addr, _mask| {
                let start = isr_usi.start();
                let ovf = isr_usi.ovf();
                let write_clear_mask = USISIF | USIOIF | USIPF;
                let status = cpu_state.data_byte(USISR);
                cpu_state.set_data_byte(
                    USISR,
                    (status & write_clear_mask & !value) | (value & USICNT_MASK),
                );
                cpu_state.clear_interrupt_by_flag(start, value);
                cpu_state.clear_interrupt_by_flag(ovf, value);
                true
            }),
        );

        let control_usi = usi.clone();
        cpu.set_write_hook(
            USICR,
            Box::new(move |cpu_state, value, _old_value, _addr, _mask| {
                let start = control_usi.start();
                let ovf = control_usi.ovf();
                cpu_state.set_data_byte(USICR, value & !(USICLK | USITC));
                cpu_state.update_interrupt_enable(start, value);
                cpu_state.update_interrupt_enable(ovf, value);
                let clock_src = value & ((USICS1 | USICS0) >> 2);
                let mode = value & ((USIWM1 | USIWM0) >> 4);
                let usi_clk = value & USICLK;
                port.set_open_collector(if mode >= 2 { data_mask } else { 0 });
                let input_value = u8::from(cpu_state.data_byte(pin) & data_mask != 0);
                if usi_clk != 0 && clock_src == 0 {
                    shift(cpu_state, pin, port_addr, data_pin, input_value);
                    count(cpu_state);
                }
                if value & USITC != 0 {
                    cpu_state.write_data_masked(pin, 1 << clock_pin, 0xff);
                    let new_value = cpu_state.data_byte(pin) & (1 << clock_pin);
                    if usi_clk != 0 && (clock_src == 2 || clock_src == 3) {
                        if clock_src == 2 && new_value != 0 {
                            shift(cpu_state, pin, port_addr, data_pin, input_value);
                        }
                        if clock_src == 3 && new_value == 0 {
                            shift(cpu_state, pin, port_addr, data_pin, input_value);
                        }
                        count(cpu_state);
                    }
                    return true;
                }
                false
            }),
        );

        usi
    }
}

fn update_output(state: &mut CpuState, pin: u16, port_addr: u16, data_pin: u8) {
    let old_value = state.data_byte(port_addr);
    let new_value = if state.data_byte(USIDR) & 0x80 != 0 {
        old_value | (1 << data_pin)
    } else {
        old_value & !(1 << data_pin)
    };
    state.write_data_masked(port_addr, new_value, 0xff);
    if new_value & 0x80 != 0 && state.data_byte(pin) & 0x80 == 0 {
        // Output HIGH (pulled-up), but input is LOW
        let status = state.data_byte(USISR);
        state.set_data_byte(USISR, status | USIDC);
    } else {
        let status = state.data_byte(USISR);
        state.set_data_byte(USISR, status & !USIDC);
    }
}

fn count(state: &mut CpuState) {
    let counter = state.data_byte(USISR).wrapping_add(1) & USICNT_MASK;
    let status = state.data_byte(USISR);
    state.set_data_byte(USISR, (status & !USICNT_MASK) | counter);
    if counter == 0 {
        let data = state.data_byte(USIDR);
        state.set_data_byte(USIBR, data);
        state.set_interrupt_flag(OVF);
    }
}

fn shift(state: &mut CpuState, pin: u16, port_addr: u16, data_pin: u8, input_value: u8) {
    let data = state.data_byte(USIDR);
    state.set_data_byte(USIDR, (data << 1) | input_value);
    update_output(state, pin, port_addr, data_pin);
}
