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

Run a raw flash image:

```bash
cargo run --bin avr8rs-cli -- --bin firmware.bin --instructions 1000000
```

Compile and run an arbitrary `.ino` directly with `arduino-cli`:

```bash
cargo run --bin avr8rs-cli -- --sketch Blink.ino --instructions 2000000
```

`--board` selects another FQBN (default `arduino:avr:uno`). The host needs
`arduino-cli`, the `arduino:avr` core, and its bundled `avr-objcopy`.

`--trace-pins` prints digital pin transitions; serial bytes are written to
stdout as they are transmitted.

## Differential testing

`tools/differential/compare.mjs` runs random ALU/register programs through
upstream avr8js 0.21.1 and the Rust CPU, then compares PC, cycles, SREG, SP, and
the register file. See `tools/differential/README.md`.

## WebAssembly demo

`wasm/` contains wasm-bindgen bindings around the Uno board model, and `web/`
contains a blink demo with prebuilt bindings. Start the local compiler server
to run arbitrary sketches from the textarea:

```bash
cargo run --bin avr8rs-server -- --port 8080
# open http://127.0.0.1:8080
```

The server exposes `POST /compile`, runs `arduino-cli` + `avr-objcopy`, and
returns raw flash bytes to the browser.
