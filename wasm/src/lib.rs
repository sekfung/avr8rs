use std::cell::RefCell;
use std::rc::Rc;

use avr8rs::{ArduinoUno, PinMode, PinState};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Uno {
    board: ArduinoUno,
    serial: Rc<RefCell<Vec<u8>>>,
}

#[wasm_bindgen]
impl Uno {
    #[wasm_bindgen(constructor)]
    pub fn new(image: &[u8], freq_hz: u32) -> Uno {
        let mut bytes = image.to_vec();
        if bytes.len() & 1 != 0 {
            bytes.push(0xff);
        }
        let mut program = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        if program.is_empty() {
            program.push(0);
        }
        let board = ArduinoUno::with_frequency(program, freq_hz);
        let serial = Rc::new(RefCell::new(Vec::new()));
        let sink = serial.clone();
        board.set_serial_callback(Some(Box::new(move |byte| {
            sink.borrow_mut().push(byte);
        })));
        Uno { board, serial }
    }

    pub fn run(&self, instructions: usize) {
        self.board.run_instructions(instructions);
    }

    pub fn reset(&self) {
        self.board.reset();
    }

    pub fn cycles(&self) -> f64 {
        self.board.cpu().cycles() as f64
    }

    pub fn pc(&self) -> u32 {
        self.board.cpu().pc() * 2
    }

    pub fn pin_mode(&self, pin: u8, mode: u8) -> Result<(), JsValue> {
        let mode = match mode {
            0 => PinMode::Input,
            1 => PinMode::Output,
            2 => PinMode::InputPullup,
            _ => return Err(JsValue::from_str("mode must be 0, 1, or 2")),
        };
        self.board
            .pin_mode(pin, mode)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn digital_write(&self, pin: u8, high: bool) -> Result<(), JsValue> {
        self.board
            .digital_write(pin, high)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn digital_read(&self, pin: u8) -> Result<bool, JsValue> {
        self.board
            .digital_read(pin)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn pin_state(&self, pin: u8) -> Result<u8, JsValue> {
        self.board
            .pin_state(pin)
            .map(|state| match state {
                PinState::Low => 0,
                PinState::High => 1,
                PinState::Input => 2,
                PinState::InputPullUp => 3,
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn set_analog_input(&self, channel: u8, volts: f64) -> Result<(), JsValue> {
        self.board
            .set_analog_input(channel, volts)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn take_serial(&self) -> Vec<u8> {
        std::mem::take(&mut self.serial.borrow_mut())
    }
}
