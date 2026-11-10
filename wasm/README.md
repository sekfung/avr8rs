# avr8rs-wasm

Browser bindings for the Arduino Uno board model.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
wasm-pack build --target web --out-dir ../web/pkg
```

`Uno` exposes `run`, `reset`, `cycles`, `pc`, `pin_mode`, `digital_write`,
`digital_read`, `pin_state`, `set_analog_input`, and `take_serial`.
