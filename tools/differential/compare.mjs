// Repeatable differential harness: random instructions/register state are
// executed by upstream avr8js and by the Rust binary, then compared.
//
// Run from the repository root:
//   cargo build --bin avr8rs-diff
//   (cd tools/differential && npm install)
//   node tools/differential/compare.mjs
//
// Set DIFF_CASES / DIFF_STEPS to change coverage.

import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const binary = resolve(root, 'target/debug/avr8rs-diff');
const modulePath = process.env.AVR8JS_PATH || 'avr8js';
const require = createRequire(import.meta.url);
const { CPU, avrInstruction } = require(modulePath);

const cases = Number(process.env.DIFF_CASES || 200);
const steps = Number(process.env.DIFF_STEPS || 24);
let seed = 0x1234_5678;

function random() {
  seed ^= seed << 13;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  return (seed >>> 0) / 0x1_0000_0000;
}

function hex(bytes) {
  return Buffer.from(bytes).toString('hex');
}

function instruction() {
  const rd = Math.floor(random() * 32);
  const rr = Math.floor(random() * 32);
  const k = Math.floor(random() * 0x100);
  const immediateDestination = Math.floor(random() * 16);
  const destination = rd << 4;
  const registers = destination | ((rr & 0x10) << 5) | (rr & 0x0f);
  const immediate = ((k & 0xf0) << 4) | (immediateDestination << 4) | (k & 0x0f);
  switch (Math.floor(random() * 19)) {
    case 0: return 0x0c00 | registers; // ADD
    case 1: return 0x1c00 | registers; // ADC
    case 2: return 0x1800 | registers; // SUB
    case 3: return 0x0800 | registers; // SBC
    case 4: return 0x2000 | registers; // AND
    case 5: return 0x2800 | registers; // OR
    case 6: return 0x2400 | registers; // EOR
    case 7: return 0x2c00 | registers; // MOV
    case 8: return 0xe000 | immediate; // LDI
    case 9: return 0x3000 | immediate; // CPI
    case 10: return 0x5000 | immediate; // SUBI
    case 11: return 0x7000 | immediate; // ANDI
    case 12: return 0x6000 | immediate; // ORI
    case 13: return 0x9403 | destination; // INC
    case 14: return 0x940a | destination; // DEC
    case 15: return 0x9400 | destination; // COM
    case 16: return 0x9401 | destination; // NEG
    case 17: return 0x9406 | destination; // LSR
    default: return 0x9402 | destination; // SWAP
  }
}

// avr8js 0.21.1 uses bit 0 for half-carry in these ALU instructions.
// Derive the hardware's bit 3 carry/borrow from the input nibbles.
function expectedHalfCarry(data, opcode) {
  const rd = (opcode >> 4) & 0x1f;
  const rr = ((opcode >> 5) & 0x10) | (opcode & 0x0f);
  const left = data[rd] & 0x0f;
  const right = data[rr] & 0x0f;
  const carry = data[95] & 1;
  switch (opcode & 0xfc00) {
    case 0x0c00: return left + right > 0x0f; // ADD
    case 0x1c00: return left + right + carry > 0x0f; // ADC
    case 0x1800: return left < right; // SUB
    case 0x0800: return left < right + carry; // SBC
  }
  const immediateKind = opcode & 0xf000;
  if (immediateKind === 0x3000 || immediateKind === 0x5000) { // CPI, SUBI
    const immediateRegister = 16 + ((opcode >> 4) & 0x0f);
    return (data[immediateRegister] & 0x0f) < (opcode & 0x0f);
  }
  if ((opcode & 0xfe0f) === 0x9401) return left !== 0; // NEG
  return undefined;
}

for (let caseIndex = 0; caseIndex < cases; caseIndex++) {
  const words = Array.from({ length: 64 }, instruction);
  const program = new Uint8Array(words.length * 2);
  words.forEach((word, index) => {
    program[index * 2] = word & 0xff;
    program[index * 2 + 1] = word >>> 8;
  });

  const data = Array.from({ length: 0x100 }, () => Math.floor(random() * 0x100));
  data[93] = 0xff;
  data[94] = 0x01;

  const cpu = new CPU(new Uint16Array(words), 256);
  for (let address = 0; address < data.length; address++) {
    cpu.data[address] = data[address];
  }
  for (let step = 0; step < steps; step++) {
    const opcode = cpu.progMem[cpu.pc];
    if (opcode === undefined) break;
    const halfCarry = expectedHalfCarry(cpu.data, opcode);
    avrInstruction(cpu);
    if (halfCarry !== undefined) {
      cpu.data[95] = (cpu.data[95] & ~0x20) | (halfCarry ? 0x20 : 0);
    }
    cpu.tick();
  }

  const jsRegisters = Array.from(cpu.data.subarray(0, 0x100), (byte) =>
    byte.toString(16).padStart(2, '0'),
  ).join('');
  const js = `${cpu.pc} ${cpu.cycles} ${cpu.data[95]} ${cpu.data[93] | (cpu.data[94] << 8)} ${jsRegisters}`;

  const output = execFileSync(binary, [
    '--program', hex(program),
    '--data', hex(data),
    '--steps', String(steps),
    '--sram', '256',
  ], { encoding: 'utf8' }).trim();

  if (output !== js) {
    console.error(`mismatch in case ${caseIndex}`);
    console.error('js :', js);
    console.error('rs :', output);
    process.exit(1);
  }
}

console.log(`differential OK: ${cases} cases x ${steps} steps`);
