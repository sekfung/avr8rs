use std::cell::RefCell;
use std::rc::Rc;

use avr8rs::{adc_config, port_b_config, port_d_config, timer0_config, Adc, Cpu, IoPort, Timer};

#[test]
fn deferred_gpio_updates_run_in_order_across_ports() {
    let cpu = Cpu::new(vec![0; 16]);
    let port_b = IoPort::new(cpu.clone(), port_b_config());
    let port_d = IoPort::new(cpu.clone(), port_d_config());
    let changes = Rc::new(RefCell::new(Vec::new()));
    let b_changes = changes.clone();
    port_b.set_external_clock_listener(
        1,
        Some(Box::new(move |_, high| b_changes.borrow_mut().push(high))),
    );
    let d_changes = changes.clone();
    port_d.set_external_clock_listener(
        2,
        Some(Box::new(move |_, high| d_changes.borrow_mut().push(high))),
    );
    let b = port_b.clone();
    port_b.add_listener(Box::new(move |_, _, _| {
        b.set_pin(1, true);
        port_d.set_pin(2, false);
        b.set_pin(1, false);
        port_d.set_pin(2, true);
    }));

    cpu.write_data(0x24, 1);
    assert_eq!(*changes.borrow(), [true, false, true]);
    assert_eq!(cpu.read_data(0x23) & 2, 0);
    assert_ne!(cpu.read_data(0x29) & 4, 0);
}

#[test]
fn deferred_gpio_updates_bracket_the_clock_event_during_a_step() {
    let steps: [fn(&Cpu); 2] = [Cpu::step, |cpu| cpu.run_instructions(1)];
    for step in steps {
        let cpu = Cpu::new(vec![0x9a28]); // SBI PORTB, 0
        let port = IoPort::new(cpu.clone(), port_b_config());
        let input = port.clone();
        port.add_listener(Box::new(move |_, _, _| input.set_pin(1, true)));
        cpu.add_clock_event(
            Box::new(move |state| {
                assert_eq!(state.data_byte(0x23) & 0b110, 0b010);
                port.set_pin(2, true);
            }),
            1,
        );
        step(&cpu);
        assert_eq!(cpu.data_byte(0x23) & 0b110, 0b110);
    }
}

#[test]
fn a_driven_low_pin_overrides_its_pullup() {
    let cpu = Cpu::new(vec![0]);
    let port = IoPort::new(cpu.clone(), port_b_config());
    cpu.write_data(0x25, 1);
    assert_eq!(cpu.read_data(0x23) & 1, 1);
    port.set_pin(0, false);
    assert_eq!(cpu.read_data(0x23) & 1, 0);
}

#[test]
fn pcicr_queues_latched_flags_and_cancels_disabled_interrupts() {
    let cpu = Cpu::new(vec![0; 16]);
    let port = IoPort::new(cpu.clone(), port_b_config());
    cpu.write_data(0x6b, 1); // PCMSK0
    port.set_pin(0, true);
    assert_eq!(cpu.data_byte(0x3b), 1); // PCIFR

    cpu.write_data(0x68, 1); // PCICR
    cpu.write_data(0x68, 0);
    cpu.set_sreg(0x80);
    cpu.tick();
    assert_eq!(cpu.pc(), 0);

    cpu.write_data(0x68, 1);
    cpu.tick();
    assert_eq!(cpu.pc(), 6);
}

#[test]
fn timer_flag_write_only_clears_selected_flags() {
    let cpu = Cpu::new(vec![0]);
    let config = timer0_config();
    let _timer = Timer::new(cpu.clone(), config);
    cpu.set_data_byte(config.tifr, config.ocfa | config.ocfb);
    cpu.write_data(config.tifr, config.ocfa);
    assert_eq!(cpu.read_data(config.tifr), config.ocfb);
    cpu.write_data(config.tifr, 0);
    assert_eq!(cpu.read_data(config.tifr), config.ocfb);
}

#[test]
fn timer_catches_up_across_multiple_wraps_with_a_custom_divider() {
    let cpu = Cpu::new(vec![0]);
    let mut config = timer0_config();
    config.dividers[1] = 3;
    let _timer = Timer::new(cpu.clone(), config);
    cpu.write_data(config.tccrb, 1);
    cpu.step();

    cpu.add_cycles(3 * 769 + 2);
    cpu.tick();
    assert_eq!(cpu.read_data(config.tcnt), 1);
    assert_ne!(cpu.data_byte(config.tifr) & config.tov, 0);

    cpu.step();
    assert_eq!(cpu.read_data(config.tcnt), 2);
}

#[test]
fn adc_flag_write_clears_pending_interrupt_without_setting_flag() {
    let cpu = Cpu::new(vec![0; 64]);
    let config = adc_config();
    let adcsra = config.adcsra;
    let _adc = Adc::new(cpu.clone(), config);
    cpu.set_sreg(0x80);
    cpu.write_data(adcsra, 0x18); // Clear ADIF and enable ADIE
    cpu.tick();
    assert_eq!(cpu.pc(), 0);
    assert_eq!(cpu.read_data(adcsra), 0x08);

    cpu.set_data_byte(adcsra, 0x10); // Conversion complete, ADIF latched
    cpu.write_data(adcsra, 0x08);
    assert_eq!(cpu.read_data(adcsra), 0x18);
    cpu.write_data(adcsra, 0x18);
    cpu.tick();
    assert_eq!(cpu.pc(), 0);
    assert_eq!(cpu.read_data(adcsra), 0x08);
}
