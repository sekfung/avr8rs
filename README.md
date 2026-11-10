# avr8rs

Rust port of the avr8js 0.21.1 8-bit AVR simulator core.

## Build and test

```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

## Minimal usage

```rust
use avr8rs::{Cpu, IoPort, port_b_config, avr_instruction};

let cpu = Cpu::new(vec![0; 1024]);
let port_b = IoPort::new(cpu.clone(), port_b_config());
avr_instruction(&cpu);
```
