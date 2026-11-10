# Upstream parity

Upstream: wokwi/avr8js 0.21.1, commit bee6f0a94e0e27786f6bc21aee3c775849fb50fd.
Baseline: `npx vitest run` reports 347 passing tests in 14 files
(verified 2026-11-11 with vitest 4.1.11 on Node.js 24.18.0).

| Upstream spec | Rust module | Cases |
|---|---|---:|
| cpu/cpu.spec.ts | cpu::cpu | 6 |
| cpu/instruction.spec.ts | cpu::instruction | 97 |
| cpu/interrupt.spec.ts | cpu::interrupt | 2 |
| peripherals/adc.spec.ts | peripherals::adc | 2 |
| peripherals/clock.spec.ts | peripherals::clock | 8 |
| peripherals/eeprom.spec.ts | peripherals::eeprom | 10 |
| peripherals/gpio.spec.ts | peripherals::gpio | 22 |
| peripherals/spi.spec.ts | peripherals::spi | 11 |
| peripherals/timer-attiny.spec.ts | peripherals::timer_attiny | 10 |
| peripherals/timer.spec.ts | peripherals::timer | 55 |
| peripherals/twi.spec.ts | peripherals::twi | 7 |
| peripherals/usart.spec.ts | peripherals::usart | 28 |
| peripherals/watchdog.spec.ts | peripherals::watchdog | 6 |
| utils/assembler.spec.ts | assembler | 83 |
| **Total** | | **347** |
