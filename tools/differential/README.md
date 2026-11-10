# Differential harness

Compares random ALU/register instruction sequences between upstream
`avr8js@0.21.1` and the Rust CPU.

```bash
cargo build --bin avr8rs-diff
(cd tools/differential && npm install)
node tools/differential/compare.mjs
```

If `avr8js` is already installed elsewhere, point `AVR8JS_PATH` at its CJS
entry point instead of running `npm install`:

```bash
AVR8JS_PATH=/path/to/avr8js/dist/cjs/index.js node tools/differential/compare.mjs
```

`DIFF_CASES` and `DIFF_STEPS` control the search depth. A mismatch prints both
register dumps and exits non-zero.
