use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use avr8rs::{port_b_config, spi_config, usart0_config, Cpu, IoPort, PinState, Spi, Usart};

#[test]
fn external_clock_listener_can_reenter_gpio_without_panicking() {
    let cpu = Cpu::new(vec![0; 16]);
    let port = IoPort::new(cpu.clone(), port_b_config());
    let nested = port.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    port.set_external_clock_listener(
        0,
        Some(Box::new(move |_state, value| {
            count.set(count.get() + 1);
            if value {
                nested.set_pin(1, true);
            }
        })),
    );
    let count = calls.clone();
    port.set_external_clock_listener(
        1,
        Some(Box::new(move |_state, value| {
            if value {
                count.set(count.get() + 1);
            }
        })),
    );

    port.set_pin(0, true);
    assert_eq!(calls.get(), 2);
}

#[test]
fn write_hook_is_restored_after_a_panic() {
    let cpu = Cpu::new(vec![0; 16]);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    cpu.set_write_hook(
        0x100,
        Box::new(move |_state, _value, _old, _addr, _mask| {
            count.set(count.get() + 1);
            if count.get() == 1 {
                panic!("hook panic");
            }
            false
        }),
    );

    let result = catch_unwind(AssertUnwindSafe(|| cpu.write_data(0x100, 1)));
    assert!(result.is_err());
    cpu.write_data(0x100, 2);
    assert_eq!(calls.get(), 2);
}

#[test]
fn usart_reset_keeps_the_ubrr_cache_in_sync() {
    let cpu = Cpu::new(vec![0; 16]);
    let usart = Usart::new(cpu.clone(), usart0_config(), 16_000_000.0);
    cpu.write_data(0xc5, 0);
    cpu.write_data(0xc4, 103);
    assert_eq!(usart.baud_rate(), 9615.0);
    usart.reset();
    assert_eq!(usart.baud_rate(), 9615.0);
}

#[test]
fn gpio_constructor_caches_start_at_zero_like_upstream() {
    let cpu = Cpu::new(vec![0; 16]);
    cpu.write_data(0x24, 0xff); // DDRB
    cpu.write_data(0x25, 0xff); // PORTB
    let port = IoPort::new(cpu.clone(), port_b_config());
    assert_eq!(port.pin_state(0), PinState::Input);
}

#[test]
fn spi_callback_cleared_during_dispatch_is_not_resurrected() {
    let cpu = Cpu::new(vec![0; 16]);
    let spi = Spi::new(cpu.clone(), spi_config(), 16_000_000.0);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let clear = spi.clone();
    spi.set_on_byte(Some(Box::new(move |_state, _value| {
        count.set(count.get() + 1);
        clear.set_on_byte(None);
    })));
    cpu.write_data(0x4c, 0x51); // SPCR: SPE | MSTR | SPR0
    cpu.write_data(0x4e, 0x12); // SPDR
    spi.complete_transfer(0);
    cpu.write_data(0x4e, 0x34);
    assert_eq!(calls.get(), 1);
}

#[test]
fn usart_callback_cleared_during_dispatch_is_not_resurrected() {
    let cpu = Cpu::new(vec![0; 16]);
    let usart = Usart::new(cpu.clone(), usart0_config(), 16_000_000.0);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let clear = usart.clone();
    usart.set_on_configuration_change(Some(Box::new(move || {
        count.set(count.get() + 1);
        clear.set_on_configuration_change(None);
    })));
    cpu.write_data(0xc4, 10);
    cpu.write_data(0xc4, 20);
    assert_eq!(calls.get(), 1);
}
