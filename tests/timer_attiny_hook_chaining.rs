use std::cell::Cell;
use std::rc::Rc;

use avr8rs::{
    attiny_timer1_config, timer0_config, AttinyTimer1, Cpu, IoPort, PortConfig, Timer, TimerConfig,
};

const GTCCR: u16 = 0x4c;
const TIFR: u16 = 0x58;
const TIMSK: u16 = 0x59;
const TCNT0: u16 = 0x46;
const TCCR0B: u16 = 0x45;
const SREG: u16 = 95;

fn create_coexisting_timers() -> Cpu {
    let cpu = Cpu::new(vec![0; 0x1000]);
    let _timer0 = Timer::new(
        cpu.clone(),
        TimerConfig {
            tifr: TIFR,
            timsk: TIMSK,
            ..timer0_config()
        },
    );
    let _port = IoPort::new(
        cpu.clone(),
        PortConfig {
            pin: 0x36,
            ddr: 0x37,
            port: 0x38,
            pin_change: None,
            external_interrupts: Vec::new(),
        },
    );
    let _attiny_timer1 = AttinyTimer1::new(cpu.clone(), attiny_timer1_config());
    cpu
}

#[test]
fn attiny_timer1_preserves_timer0_timsk_interrupt_hook() {
    let cpu = create_coexisting_timers();

    cpu.write_data(TCNT0, 0xff);
    cpu.write_data(TCCR0B, 1);
    cpu.set_cycles(1);
    cpu.tick();
    cpu.set_cycles(2);
    cpu.tick();
    assert_eq!(cpu.data_byte(TIFR) & 1, 1);

    cpu.write_data(TIMSK, 1);
    cpu.set_data_byte(SREG, 0x80);
    cpu.tick();

    assert_eq!(cpu.pc(), 0x20);
}

#[test]
fn attiny_timer1_preserves_timer0_tifr_clear_hook() {
    let cpu = create_coexisting_timers();

    cpu.write_data(TCNT0, 0xff);
    cpu.write_data(TCCR0B, 1);
    cpu.set_cycles(1);
    cpu.tick();
    cpu.set_cycles(2);
    cpu.tick();
    assert_eq!(cpu.data_byte(TIFR) & 1, 1);

    cpu.write_data(TIFR, 1);

    assert_eq!(cpu.data_byte(TIFR) & 1, 0);
}

#[test]
fn attiny_timer1_preserves_existing_gtccr_write_hook() {
    let cpu = Cpu::new(vec![0; 0x1000]);
    let observed = Rc::new(Cell::new(0));
    let hook_observed = observed.clone();
    cpu.set_write_hook(
        GTCCR,
        Box::new(move |state, value, _old_value, _addr, _mask| {
            hook_observed.set(value);
            state.set_data_byte(GTCCR, value);
            true
        }),
    );
    let _attiny_timer1 = AttinyTimer1::new(cpu.clone(), attiny_timer1_config());

    cpu.write_data(GTCCR, 0x80);

    assert_eq!(observed.get(), 0x80);
}
