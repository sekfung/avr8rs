use avr8rs::{ArduinoUno, BoardError, PinMode, PinState};

#[test]
fn digital_write_and_read_round_trip() {
    let board = ArduinoUno::new(vec![0; 16]);
    board.pin_mode(13, PinMode::Output).unwrap();
    board.digital_write(13, true).unwrap();
    assert_eq!(board.pin_state(13).unwrap(), PinState::High);
}

#[test]
fn rejects_out_of_range_pin() {
    let board = ArduinoUno::new(vec![0; 16]);
    assert_eq!(
        board.pin_mode(20, PinMode::Input),
        Err(BoardError::BadPin(20))
    );
}
