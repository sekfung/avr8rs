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

## Additional Rust coverage

Beyond the 347 upstream cases, the repository keeps integration regressions for
behaviour that upstream does not test:

| File | Cases | Scope |
|---|---:|---|
| `tests/timer_attiny_hook_chaining.rs` | 3 | Timer0/ATtiny shared-hook chaining |
| `tests/regressions.rs` | 6 | GPIO re-entry, hook RAII, callback lifetimes, USART reset |
| `tests/board.rs` | 6 | Arduino Uno pin mapping, SRAM, PWM, and reset |
| `tests/cpu_review.rs` | 9 | Hook replacement, clock events, ALU flags, PC wrapping, and 22-bit returns |
| `tests/peripheral_review.rs` | 7 | Deferred GPIO, pull-ups, interrupt flags, and timer catch-up |

The library test target includes the 347 ported cases and additional regressions.

## Tooling layers

- `board::ArduinoUno` maps Arduino pins 0-13 and A0-A5 to the ATmega328P
  ports and instantiates USART0, Timer0/1/2, ADC, watchdog, and the clock.
- `avr8rs-cli` runs raw little-endian flash images and can compile Uno
  sketches through the host `arduino-cli` toolchain.
- `avr8rs-server` serves the demo and exposes `POST /compile` for Uno
  sketch text. Intel HEX parsing is not part of the crate.
- `wasm/` exposes the board model through wasm-bindgen; `web/` contains a
  self-contained blink demo with prebuilt bindings.
- `tools/differential/compare.mjs` compares random ALU/register programs with
  upstream avr8js 0.21.1.

## Intentional deviations

Malformed assembler input that upstream reports through internal JavaScript
`TypeError`s gets a descriptive Rust error instead. This only affects the
test-only assembler and is outside the 347 upstream cases.

The CPU computes the H flag from bit 3 and restores the full 22-bit PC on
`RET` and `RETI`. The peripheral models include undriven GPIO pull-ups in
PIN reads, update pending pin-change interrupts on PCICR writes, and preserve
unselected flags on write-one-to-clear timer and ADC registers. A few ported
assertions reflect these hardware behaviors.

## Public export map

Every symbol exported by upstream `src/index.ts` has a Rust re-export. Function
entries use `()`; type entries include structs, enums, traits, and aliases that
correspond to upstream classes, interfaces, and TypeScript types.

| Upstream export | Rust export | Kind |
|---|---|---|
| `CPU` | `Cpu` | struct |
| `CPUMemoryHook` | `CpuMemoryHook` | type alias |
| `CPUMemoryHooks` | `CpuMemoryHooks` | type alias |
| `avrInstruction` | `avr_instruction` | function |
| `avrInterrupt` | `avr_interrupt` | function |
| `ADCMuxInputType` | `AdcMuxInputType` | enum |
| `ADCReference` | `AdcReference` | enum |
| `AVRADC` | `Adc` | struct |
| `adcConfig` | `adc_config()` | function |
| `atmega328Channels` | `atmega328_channels()` | function |
| `ADCConfig` | `AdcConfig` | struct |
| `ADCMuxConfiguration` | `AdcMuxConfiguration` | type alias |
| `ADCMuxInput` | `AdcMuxInput` | enum |
| `AVRClock` | `Clock` | struct |
| `clockConfig` | `clock_config()` | function |
| `AVRClockConfig` | `ClockConfig` | struct |
| `AVREEPROM` | `Eeprom` | struct |
| `EEPROMMemoryBackend` | `EepromMemoryBackend` | struct |
| `eepromConfig` | `eeprom_config()` | function |
| `AVREEPROMConfig` | `EepromConfig` | struct |
| `EEPROMBackend` | `EepromBackend` | trait |
| `AVRIOPort` | `IoPort` | struct |
| `INT0` | `INT0` | constant |
| `INT1` | `INT1` | constant |
| `PCINT0` | `PCINT0` | constant |
| `PCINT1` | `PCINT1` | constant |
| `PCINT2` | `PCINT2` | constant |
| `PinState` | `PinState` | enum |
| `portAConfig` | `port_a_config()` | function |
| `portBConfig` | `port_b_config()` | function |
| `portCConfig` | `port_c_config()` | function |
| `portDConfig` | `port_d_config()` | function |
| `portEConfig` | `port_e_config()` | function |
| `portFConfig` | `port_f_config()` | function |
| `portGConfig` | `port_g_config()` | function |
| `portHConfig` | `port_h_config()` | function |
| `portJConfig` | `port_j_config()` | function |
| `portKConfig` | `port_k_config()` | function |
| `portLConfig` | `port_l_config()` | function |
| `AVRExternalInterrupt` | `ExternalInterrupt` | struct |
| `AVRPinChangeInterrupt` | `PinChangeInterrupt` | struct |
| `AVRPortConfig` | `PortConfig` | struct |
| `GPIOListener` | `GpioListener` | type alias |
| `AVRSPI` | `Spi` | struct |
| `spiConfig` | `spi_config()` | function |
| `SPIConfig` | `SpiConfig` | struct |
| `SPITransferCallback` | `SpiTransferCallback` | type alias |
| `AVRTimer` | `Timer` | struct |
| `timer0Config` | `timer0_config()` | function |
| `timer1Config` | `timer1_config()` | function |
| `timer2Config` | `timer2_config()` | function |
| `AVRTimerConfig` | `TimerConfig` | struct |
| `ATtinyTimer1` | `AttinyTimer1` | struct |
| `attinyTimer1Config` | `attiny_timer1_config()` | function |
| `ATtinyTimer1Config` | `AttinyTimer1Config` | struct |
| `TWIEventHandler` | `TwiEventHandler` | trait |
| `TWIConfig` | `TwiConfig` | struct |
| `twiConfig` | `twi_config()` | function |
| `NoopTWIEventHandler` | `NoopTwiEventHandler` | struct |
| `AVRTWI` | `Twi` | struct |
| `AVRUSART` | `Usart` | struct |
| `usart0Config` | `usart0_config()` | function |
| `AVRUSI` | `Usi` | struct |
| `AVRWatchdog` | `Watchdog` | struct |
| `watchdogConfig` | `watchdog_config()` | function |
| `WatchdogConfig` | `WatchdogConfig` | struct |

Rust also exposes a small number of supporting names required by the ownership
model or callback API (`ClockEventId`, `CpuState`, `CpuReadHook`, `CpuWriteHook`,
`InterruptConfig`, `AdcState`, `ExternalClockListener`, `ListenerId`,
`PinOverrideMode`, `TwiContext`, and `UsartConfig`). These are documented in the
design spec and do not replace any upstream export.
