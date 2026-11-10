# avr8rs

Rust port of the avr8js 0.21.1 8-bit AVR simulator core.

The crate includes the upstream CPU/peripherals, an Arduino Uno board layer,
and a small command-line runner for raw AVR flash images.

## Build and test

```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

`cargo test --lib` runs the 347 ported upstream cases. `cargo test` also runs
the integration regressions under `tests/`.

## Minimal usage

```rust
use avr8rs::{avr_instruction, port_b_config, Cpu, IoPort};

fn main() {
    // A single NOP instruction.
    let cpu = Cpu::new(vec![0x0000]);
    let port_b = IoPort::new(cpu.clone(), port_b_config());

    // Configure PB0-PB3 as outputs.
    cpu.write_data(0x24, 0x0f); // DDRB

    port_b.add_listener(Box::new(|_state, value, old_value| {
        println!("PORTB changed: {old_value:#04x} -> {value:#04x}");
    }));

    // Drive PB0-PB3 high. The listener fires with (0x0f, 0x00).
    cpu.write_data(0x25, 0x0f); // PORTB

    // Execute the NOP.
    avr_instruction(&cpu);
}
```

## Arduino Uno board layer

```rust
use avr8rs::{ArduinoUno, PinMode};

let board = ArduinoUno::new(vec![0; 0x4000]);
board.pin_mode(13, PinMode::Output).unwrap();
board.digital_write(13, true).unwrap();
board.run_instructions(100);
```

Pins 0-13 use Arduino numbering; A0-A5 are pins 14-19. `set_analog_input`,
`set_serial_callback`, and `add_pin_listener` wire external components.

## Command-line runner

The CLI accepts a raw flash image (little-endian instruction bytes), not Intel
HEX:

```bash
cargo run --bin avr8rs-cli -- --bin firmware.bin --cycles 1000000
```

`--trace-pins` prints digital pin transitions; serial bytes are written to
stdout as they are transmitted.
