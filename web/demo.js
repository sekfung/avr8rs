import init, { Uno } from './pkg/avr8rs_wasm.js';

// sbi DDRB,5; loop: sbi PORTB,5; delay; cbi PORTB,5; delay; rjmp loop
const blink = new Uint8Array([
  0x25, 0x9a, 0x2d, 0x9a, 0x0f, 0xef, 0x0a, 0x95, 0xf1, 0xf7,
  0x2d, 0x98, 0x0f, 0xef, 0x0a, 0x95, 0xf1, 0xf7, 0xf7, 0xcf,
]);

await init();
let uno = new Uno(blink, 16_000_000);
const led = document.querySelector('#led');
const pc = document.querySelector('#pc');
const cycles = document.querySelector('#cycles');
const serial = document.querySelector('#serial');
const toggle = document.querySelector('#toggle');
const speed = document.querySelector('#speed');
const compile = document.querySelector('#compile');
const source = document.querySelector('#source');
let running = true;

function frame() {
  if (running) {
    uno.run(5000 * Number(speed.value));
    const state = uno.pin_state(13);
    led.classList.toggle('on', state === 1);
    pc.textContent = `PC 0x${uno.pc().toString(16).padStart(4, '0')}`;
    cycles.textContent = `${Math.round(uno.cycles()).toLocaleString()} cycles`;
    const bytes = uno.take_serial();
    if (bytes.length) serial.textContent += new TextDecoder().decode(bytes);
  }
  requestAnimationFrame(frame);
}

toggle.addEventListener('click', () => {
  running = !running;
  toggle.textContent = running ? 'Pause' : 'Run';
});

compile.addEventListener('click', async () => {
  compile.disabled = true;
  compile.textContent = 'Compiling…';
  try {
    const response = await fetch('/compile', { method: 'POST', body: source.value });
    if (!response.ok) throw new Error(await response.text());
    const binary = new Uint8Array(await response.arrayBuffer());
    uno = new Uno(binary, 16_000_000);
    serial.textContent = '';
    running = true;
    toggle.textContent = 'Pause';
  } catch (error) {
    serial.textContent = `Compile failed: ${error.message}\n\nStart the local compiler server with:\n  cargo run --bin avr8rs-server`;
  } finally {
    compile.disabled = false;
    compile.textContent = 'Compile & Run';
  }
});
requestAnimationFrame(frame);
