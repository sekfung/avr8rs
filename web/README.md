# Browser demo

The `pkg/` directory contains the built wasm-bindgen output. Serve this
directory over HTTP (ES modules and WebAssembly do not load from `file://`):

```bash
python3 -m http.server 8080
# open http://localhost:8080
```

The demo runs a hand-assembled blink program on the `ArduinoUno` board model:
PB5 (Arduino pin 13) toggles, and the page shows PC, cycle count, and serial
output. To rebuild the bindings:

```bash
cd wasm
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ../web/pkg \
  target/wasm32-unknown-unknown/release/avr8rs_wasm.wasm
```
