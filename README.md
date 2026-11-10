# avr8rs

Rust port of the avr8js 0.21.1 8-bit AVR simulator core.

## Build and test

```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

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
