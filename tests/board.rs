use std::cell::Cell;
use std::rc::Rc;

use avr8rs::{ArduinoUno, BoardError, PinMode, PinState};

#[test]
fn digital_write_and_read_round_trip() {
    let board = ArduinoUno::new(vec![0; 16]);
    board.pin_mode(13, PinMode::Output).unwrap();
    assert!(!board.digital_read(13).unwrap());
    board.digital_write(13, true).unwrap();
    assert_eq!(board.pin_state(13).unwrap(), PinState::High);
    assert!(board.digital_read(13).unwrap());
}

#[test]
fn rejects_out_of_range_pin() {
    let board = ArduinoUno::new(vec![0; 16]);
    assert_eq!(
        board.pin_mode(20, PinMode::Input),
        Err(BoardError::BadPin(20))
    );
}

#[test]
fn uno_has_two_kib_of_sram() {
    let board = ArduinoUno::new(vec![0]);
    assert_eq!(board.cpu().sp(), 0x08ff);
    assert_eq!(board.cpu().data().len(), 0x0900);
}

#[test]
fn uno_reads_an_undriven_input_pullup_as_high() {
    let board = ArduinoUno::new(vec![0]);
    board.pin_mode(2, PinMode::InputPullup).unwrap();
    assert!(board.digital_read(2).unwrap());
}

#[test]
fn uno_generates_pwm_on_timer1_and_timer2_pins() {
    let board = ArduinoUno::new(vec![0]);
    let cpu = board.cpu();
    let timer1_edges = Rc::new(Cell::new(0));
    let timer2_edges = Rc::new(Cell::new(0));
    for (pin, edges) in [(9, &timer1_edges), (11, &timer2_edges)] {
        board.pin_mode(pin, PinMode::Output).unwrap();
        let edges = edges.clone();
        board
            .add_pin_listener(pin, Box::new(move |_, _, _| edges.set(edges.get() + 1)))
            .unwrap();
    }

    cpu.write_data(0x80, 0x81); // Timer1: phase-correct 8-bit PWM on OC1A
    cpu.write_data(0x81, 3); // Clock / 64
    cpu.write_data(0x89, 0);
    cpu.write_data(0x88, 128); // OCR1A
    cpu.write_data(0xb0, 0x81); // Timer2: phase-correct PWM on OC2A
    cpu.write_data(0xb1, 4); // Clock / 64
    cpu.write_data(0xb3, 128); // OCR2A

    board.run_instructions(150_000);
    assert!(timer1_edges.get() > 1);
    assert!(timer2_edges.get() > 1);
}

#[test]
fn reset_restarts_adc_and_preserves_external_wiring() {
    let board = ArduinoUno::new(vec![0]);
    let cpu = board.cpu().clone();
    board.set_analog_input(0, 2.5).unwrap();
    board.pin_mode(13, PinMode::Output).unwrap();
    board.digital_write(13, true).unwrap();
    let falling_edges = Rc::new(Cell::new(0));
    let edges = falling_edges.clone();
    board
        .add_pin_listener(
            13,
            Box::new(move |_, high, _| {
                if !high {
                    edges.set(edges.get() + 1);
                }
            }),
        )
        .unwrap();
    cpu.set_data_byte(0x0100, 0x42);
    cpu.write_data(0x45, 1); // Timer0 clock enabled
    cpu.write_data(0xc1, 8); // USART TX enabled
    cpu.write_data(0x7c, 0x40); // ADC reference: AVCC
    cpu.write_data(0x7a, 0xc7); // Start conversion
    board.run_instructions(10);
    board.reset();

    assert_eq!(board.pin_state(13).unwrap(), PinState::Input);
    assert_eq!(falling_edges.get(), 1);
    assert_eq!(cpu.data_byte(0x0100), 0x42);
    assert_eq!(cpu.data_byte(0x45), 0);
    assert_eq!(cpu.data_byte(0xc1), 0);
    assert_eq!(cpu.data_byte(0x7a), 0);
    cpu.write_data(0x7c, 0x40);
    cpu.write_data(0x7a, 0xc7);
    board.run_instructions(10_000);
    assert_eq!(cpu.data_byte(0x7a) & 0x40, 0);
    assert_eq!(cpu.data_byte(0x79), 2); // 2.5 V on a 5 V reference → 512
    assert_eq!(cpu.data_byte(0x78), 0);
}
