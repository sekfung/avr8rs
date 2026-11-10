# Browser demo

The `pkg/` directory contains the built wasm-bindgen output. Start the local
compiler/static server from the repository root:

```bash
cargo run --bin avr8rs-server -- --port 8080
# open http://127.0.0.1:8080
```

The server hosts this directory and exposes `POST /compile`, which runs
`arduino-cli` and `avr-objcopy` to turn the textarea sketch into raw flash
bytes. The page then runs them on the `ArduinoUno` board model, showing PB5
(pin 13), PC, cycle count, and serial output. The built-in blink program is
used until you press **Compile & Run**.

To rebuild the bindings:

```bash
cd wasm
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ../web/pkg \
  target/wasm32-unknown-unknown/release/avr8rs_wasm.wasm
```
