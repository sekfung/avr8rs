# avr8rs: Rust port of avr8js — design

- Status: approved in chat on 2026-11-11
- Upstream reference: `wokwi/avr8js` v0.21.1, commit `bee6f0a94e0e27786f6bc21aee3c775849fb50fd`
- Upstream license: MIT (Uri Shaked and contributors)
- Deliverable: Rust library only; no CLI, HEX loader, demo UI, or JS/WASM bindings

## 1. Goal

Implement the full behaviour of the avr8js core library in Rust and port all
347 upstream unit tests to native `cargo test`:

| Upstream spec | Cases |
|---|---:|
| `cpu/cpu.spec.ts` | 6 |
| `cpu/instruction.spec.ts` | 97 |
| `cpu/interrupt.spec.ts` | 2 |
| `peripherals/adc.spec.ts` | 2 |
| `peripherals/clock.spec.ts` | 8 |
| `peripherals/eeprom.spec.ts` | 10 |
| `peripherals/gpio.spec.ts` | 22 |
| `peripherals/spi.spec.ts` | 11 |
| `peripherals/timer-attiny.spec.ts` | 10 |
| `peripherals/timer.spec.ts` | 55 |
| `peripherals/twi.spec.ts` | 7 |
| `peripherals/usart.spec.ts` | 28 |
| `peripherals/watchdog.spec.ts` | 6 |
| `utils/assembler.spec.ts` | 83 |
| **Total** | **347** |

`peripherals/usi.ts` has no upstream spec. It is ported for API and functional
parity, but adds no test cases.

## 2. Non-goals

- Intel HEX loading, board wiring, Arduino bootloader behaviour, and serial
  console binaries.
- The upstream demo application, browser UI, and `@wokwi/elements` integration.
- JavaScript or WebAssembly bindings. The test suite is ported, not executed
  unchanged against Rust.
- Bit-for-bit identical JavaScript object identity or property syntax. Rust
  naming and ownership shape the public API where necessary.

## 3. Crate layout

Single library crate `avr8rs`, Rust edition 2021, no runtime dependencies,
`#![forbid(unsafe_code)]`.

```text
Cargo.toml
src/lib.rs
src/cpu/cpu.rs
src/cpu/instruction.rs
src/cpu/interrupt.rs
src/peripherals/adc.rs
src/peripherals/clock.rs
src/peripherals/eeprom.rs
src/peripherals/gpio.rs
src/peripherals/spi.rs
src/peripherals/timer.rs
src/peripherals/timer_attiny.rs
src/peripherals/twi.rs
src/peripherals/usart.rs
src/peripherals/usi.rs
src/peripherals/watchdog.rs
src/assembler/mod.rs        # cfg(test) only
src/test_support.rs         # cfg(test) only
```

Tests live in `#[cfg(test)] mod tests` inside the module they exercise. Keeping
them in-crate allows access to internal state used by upstream specs and keeps
the public surface honest. `cargo test` must report exactly 347 unit tests.
README examples are not doctests.

## 4. Public API

`src/lib.rs` re-exports every symbol exported by upstream `src/index.ts`, with
the following naming map:

| Upstream | Rust |
|---|---|
| `CPU` | `Cpu` |
| `CPUMemoryHook`, `CPUMemoryHooks` | `CpuMemoryHook`, `CpuMemoryHooks` |
| `avrInstruction`, `avrInterrupt` | `avr_instruction`, `avr_interrupt` |
| `AVRADC`, `adcConfig`, `ADCMuxInputType`, `ADCReference`, `atmega328Channels`, `ADCConfig`, `ADCMuxConfiguration`, `ADCMuxInput` | `Adc`, `adc_config()`, `AdcMuxInputType`, `AdcReference`, `atmega328_channels()`, `AdcConfig`, `AdcMuxConfiguration`, `AdcMuxInput` |
| `AVRClock`, `clockConfig`, `AVRClockConfig` | `Clock`, `clock_config()`, `ClockConfig` |
| `AVREEPROM`, `EEPROMMemoryBackend`, `eepromConfig`, `AVREEPROMConfig`, `EEPROMBackend` | `Eeprom`, `EepromMemoryBackend`, `eeprom_config()`, `EepromConfig`, `EepromBackend` |
| `AVRIOPort`, `INT0`, `INT1`, `PCINT0..2`, `PinState`, `portAConfig`..`portLConfig`, `AVRExternalInterrupt`, `AVRPinChangeInterrupt`, `AVRPortConfig`, `GPIOListener` | `IoPort`, `INT0`, `INT1`, `PCINT0..2`, `PinState`, `port_a_config()`..`port_l_config()`, `ExternalInterrupt`, `PinChangeInterrupt`, `PortConfig`, `GpioListener` |
| `AVRSPI`, `spiConfig`, `SPIConfig`, `SPITransferCallback` | `Spi`, `spi_config()`, `SpiConfig`, `SpiTransferCallback` |
| `AVRTimer`, `timer0Config`, `timer1Config`, `timer2Config`, `AVRTimerConfig` | `Timer`, `timer0_config()`, `timer1_config()`, `timer2_config()`, `TimerConfig` |
| `ATtinyTimer1`, `attinyTimer1Config`, `ATtinyTimer1Config` | `AttinyTimer1`, `attiny_timer1_config()`, `AttinyTimer1Config` |
| `AVRTWI`, `twiConfig`, `TWIConfig`, `TWIEventHandler`, `NoopTWIEventHandler` | `Twi`, `twi_config()`, `TwiConfig`, `TwiEventHandler`, `NoopTwiEventHandler` |
| `AVRUSART`, `usart0Config` | `Usart`, `usart0_config()` |
| `AVRUSI` | `Usi` |
| `AVRWatchdog`, `watchdogConfig`, `WatchdogConfig` | `Watchdog`, `watchdog_config()`, `WatchdogConfig` |

Configuration structs use `snake_case` fields. Every config is returned by an
owned-value function because ADC mux tables and GPIO interrupt lists cannot be
`const`. Enum discriminants match upstream numeric values where the tests
observe them.

## 5. CPU model

`Cpu` is a cheap cloneable handle over `Rc<RefCell<CpuState>>`.

```rust
pub struct Cpu(Rc<RefCell<CpuState>>);

impl Cpu {
    pub fn new(prog_mem: Vec<u16>) -> Cpu;                  // 8192 B SRAM
    pub fn with_sram(prog_mem: Vec<u16>, sram_bytes: usize) -> Cpu;
    pub fn reset(&self);
    pub fn read_data(&self, addr: u16) -> u8;
    pub fn write_data(&self, addr: u16, value: u8);
    pub fn write_data_masked(&self, addr: u16, value: u8, mask: u8);
    pub fn data(&self) -> Ref<'_, [u8]>;
    pub fn data_byte(&self, addr: u16) -> u8;
    pub fn set_data_byte(&self, addr: u16, value: u8);
    pub fn pc(&self) -> u32;
    pub fn set_pc(&self, pc: u32);
    pub fn cycles(&self) -> u64;
    pub fn set_cycles(&self, cycles: u64);
    pub fn add_cycles(&self, delta: u64);
    pub fn write_prog_bytes(&self, bytes: &[u8], offset: usize);
    pub fn prog_word(&self, word_addr: u32) -> u16;
    pub fn pc22_bits(&self) -> bool;
    pub fn sp(&self) -> u16;
    pub fn set_sp(&self, value: u16);
    pub fn sreg(&self) -> u8;
    pub fn set_sreg(&self, value: u8);
    pub fn step(&self);
    pub fn run_instructions(&self, count: usize);
    pub fn tick(&self);
}
```

`CpuState` is public as the low-level execution context used by callbacks. Its
fields are private; it exposes `read_data`, `write_data`, `data_byte`,
`set_data_byte`, `pc`, `set_pc`, `cycles`, `set_cycles`, `add_cycles`,
`add_clock_event`, `update_clock_event`, `clear_clock_event`, interrupt
helpers, `sp`, and `sreg` methods.

State contents mirror upstream:

- `data: Vec<u8>` of length `sram_bytes + 0x100`.
- `prog_mem: Vec<u16>`.
- `pc: u32`, `cycles: u64`, SRAM size, and `pc22_bits` derived from program
  size (`prog_bytes.len() > 0x20000`).
- Sparse-by-address read/write hook tables.
- Up to 128 pending interrupt configurations, `next_interrupt`, and
  `max_interrupt`, with upstream queue/clear/update/clear-by-flag semantics.
- Ordered clock-event queue with stable callback IDs.
- `on_watchdog_reset` callback.
- GPIO port registries used by PCINT and timer pin overrides.

`SP` is little-endian at data address 93; `SREG` is address 95. Word and
double-word accesses use little-endian ordering, matching JavaScript
`DataView`.

### Hooks

```rust
pub type CpuReadHook  = Box<dyn FnMut(&mut CpuState, u16) -> u8>;
pub type CpuWriteHook = Box<dyn FnMut(&mut CpuState, u8, u8, u16, u8) -> bool>;
```

`Cpu` exposes `set_read_hook(addr, hook)`, `set_write_hook(addr, hook)`, and
`clear_*_hook(addr)`. When a hook fires, the implementation temporarily moves
the boxed callback out of the table, calls it with `&mut CpuState`, and
restores it with a drop guard, so the hook body can safely perform nested CPU
operations through the supplied state. A write hook returning `true` suppresses
the default store, as upstream.

### Clock events and interrupts

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClockEventId(pub u64);

pub fn add_clock_event(
    &self,
    callback: Box<dyn FnMut(&mut CpuState)>,
    delay_cycles: u64,
) -> ClockEventId;
pub fn update_clock_event(&self, id: ClockEventId, delay_cycles: u64) -> bool;
pub fn clear_clock_event(&self, id: ClockEventId) -> bool;
```

Due time is `cycles + max(1, delay_cycles)`. The queue is a `Vec` sorted by due
time; a newly inserted event is placed before existing events with the same due
time, matching upstream insertion order. `tick()` fires at most one due event
and then processes one enabled pending interrupt, exactly as upstream. Callback
storage is a separate registry indexed by `ClockEventId`, so re-scheduling a
recurring timer event does not allocate a new closure.

`avr_instruction(&Cpu)` executes one instruction without ticking.
`Cpu::step()` executes one instruction and one tick. `Cpu::run_instructions(n)`
holds the state borrow once for the whole batch.

## 6. Peripheral model

Every peripheral is a cloneable handle over `Rc<RefCell<State>>`. Constructors
match upstream:

```rust
Adc::new(cpu, adc_config())
Clock::new(cpu, freq_hz, clock_config())
Eeprom::new(cpu, backend, eeprom_config())
IoPort::new(cpu, port_b_config())
Spi::new(cpu, spi_config(), freq_hz)
Timer::new(cpu, timer0_config())
AttinyTimer1::new(cpu, attiny_timer1_config())
Twi::new(cpu, twi_config(), freq_hz)
Usart::new(cpu, usart0_config(), freq_hz)
Usi::new(cpu, port, port_pin, data_pin, clock_pin)
Watchdog::new(cpu, watchdog_config())
```

`EepromBackend` is a trait with `read_memory`, `write_memory`, and
`erase_memory`; `EepromMemoryBackend` is the default implementation.
Backends are held as `Rc<RefCell<dyn EepromBackend>>` so tests and callers can
hold the same instance for observation.

`TwiEventHandler` mirrors the five upstream methods. Each method receives a
mutable TWI context and CPU state so the default handler and custom handlers
can complete the transaction without re-borrowing the `Twi` handle:

```rust
pub trait TwiEventHandler {
    fn start(&mut self, ctx: &mut TwiContext<'_>, repeated: bool);
    fn stop(&mut self, ctx: &mut TwiContext<'_>);
    fn connect_to_slave(&mut self, ctx: &mut TwiContext<'_>, addr: u8, write: bool);
    fn write_byte(&mut self, ctx: &mut TwiContext<'_>, value: u8);
    fn read_byte(&mut self, ctx: &mut TwiContext<'_>, ack: bool);
}
```

`NoopTwiEventHandler` performs the upstream completion calls. The handler is
temporarily moved out of TWI state while it runs and restored afterwards.
`TwiContext` exposes `complete_start`, `complete_stop`, `complete_connect`,
`complete_write`, and `complete_read`, plus read/write access to the configured
TWI registers and CPU state. A custom handler therefore has the same
capabilities as upstream's `TwiEventHandler` without holding a `Twi`
back-reference.

### Callback shapes

Notification-only callbacks keep upstream argument lists:

- `Usart::on_byte_transmit: Option<Box<dyn FnMut(u8)>>`
- `Usart::on_line_transmit: Option<Box<dyn FnMut(String)>>`
- `Usart::on_rx_complete: Option<Box<dyn FnMut()>>`
- `Usart::on_configuration_change: Option<Box<dyn FnMut()>>`
- `Spi::on_transfer: Option<Box<dyn FnMut(u8) -> u8>>`

Callbacks that may drive the simulation receive the execution context:

- `Spi::on_byte: Option<Box<dyn FnMut(&mut CpuState, u8)>>`
- `Adc::on_adc_read: Option<Box<dyn FnMut(&mut AdcState, &mut CpuState, AdcMuxInput)>>`
- `Cpu::on_watchdog_reset: Option<Box<dyn FnMut(&mut CpuState)>>`
- GPIO `GpioListener = Box<dyn FnMut(&mut CpuState, u8, u8)>`
- GPIO external-clock listener `Box<dyn FnMut(&mut CpuState, bool)>`

Methods that mutate both a peripheral and the CPU have `_with` variants, such
as `Spi::complete_transfer_with(&self, state: &mut CpuState, byte: u8)`, for
use inside callbacks. Ordinary methods use the stored `Cpu` handle and are for
code running outside the CPU borrow.

`IoPort::add_listener` returns a `ListenerId`; `remove_listener(id)` removes it.
This replaces JavaScript function-identity comparison.
`IoPort::set_external_clock_listener(pin, Option<Box<dyn FnMut(&mut CpuState,
bool)>>)` is the Rust equivalent of assigning
`externalClockListeners[pin]`; timers use it for external clock inputs.

`AdcState` is public for the same reason as `CpuState`: it exposes
`channel_values` (mutable), `avcc`, `aref`, and `reference_voltage` so an
`on_adc_read` override can read or change the same state as the default
implementation. `SpiTransferCallback` remains exported as a
`dyn FnMut(u8) -> u8` alias for API-discovery parity.

## 7. Behaviour parity

Each peripheral keeps upstream register addresses, bit masks, interrupt
configuration, timing, and state transitions:

- GPIO: `PIN`/`DDR`/`PORT` hooks, `PIN` write toggling, pull-ups,
  open-collector, pin-state reporting, external interrupts, pin-change
  interrupts, listener ordering, timer pin overrides, and cross-port PCINT
  handling.
- Clock: `CLKPR` protected write window, all prescaler values including the
  reserved values measured upstream, and `frequency`, `prescaler`,
  `time_nanos`, `time_micros`, `time_millis`.
- ADC: mux modes (constant, single-ended, differential, temperature),
  references, prescaler, sample cycles, 13/25-cycle conversion behaviour,
  disabled-ADC zero result, channel values, and ADC interrupt.
- EEPROM: `EEMPE` four-cycle write window, erase/write cycle timing, split
  modes, ready interrupt, and backend operations.
- SPI: master/slave, dividers, modes, data order, transfer cycles, WCOL,
  double buffering, interrupt flag, and `complete_transfer`.
- Timer: normal, CTC, fast PWM, and phase-correct PWM; TOP changes; external
  clock; force compare; overflow/compare flags and interrupts; compare-output
  pin overrides.
- ATtiny timer 1: all upstream modes, prescalers, compare outputs, and pin
  overrides.
- TWI: SCL frequency, prescaler, status codes, start/repeated start, stop,
  connect, write, read, interrupt flag and enable behaviour.
- USART: baud rate including double-speed mode, 5-9 bit characters, parity,
  stop bits, RX/TX enable, line buffering, byte/line callbacks, RX complete,
  configuration-change callback, data-register-empty and TX-complete
  interrupts, and `write_byte`.
- USI: two-wire start/stop detection, counter, overflow interrupt, data
  register buffering, clock source, and output pin handling.
- Watchdog: prescaler, timed reset, `WDR` handling, `MCUSR` flags, and
  `reset_watchdog`.

Upstream's two intentional error paths remain panics with the same messages:
`Invalid divider value!` (SPI) and `Invalid prescaler value!` (TWI).

## 8. Test port

- One `#[test]` per upstream `it`, named from the description. `describe`
  blocks become nested `mod` blocks.
- `src/test_support.rs` provides mock recorders, `asm_program`, and
  `TestProgramRunner` with `run_instructions`, `run_until`, `run_to_break`,
  and `run_to_address`. The default break handler panics with
  `BREAK instruction encountered`; the too-long runner path panics with
  `Test program ran for too long, check your predicate`.
- Exact integer comparisons use `assert_eq!`. Floating-point comparisons use
  the upstream exact value when it is exactly representable, otherwise an
  epsilon of `1e-6`.
- `docs/parity.md` maps every upstream spec file and case count to the Rust
  module and records the documented API-shape differences.

The assembler is a faithful two-pass port used only by tests:

- Comments start with `#` or `;`.
- Labels are `name:` and resolve to byte offsets, including forward references.
- Directives `_REPLACE`, `_LOC`, and `_IW` are supported.
- All upstream mnemonics, aliases, operand forms, and error strings are
  supported, including the upstream line-number convention.
- `assemble(&str)` returns `AssembleResult { bytes, errors, lines, labels }`.
  `Line` exposes `line`, `text`, `byte_offset`, and `bytes`; multi-word lines
  join word strings with a space, which is not observed by the upstream suite.

The assembler module and test support are compiled under `cfg(test)` only.

## 9. Documented parity differences

These are API-shape differences required by safe Rust; simulation behaviour is
unchanged:

1. Callbacks that can re-enter the simulator receive `&mut CpuState`.
2. Clock events are addressed by `ClockEventId` instead of function identity.
3. GPIO listeners are removed by `ListenerId`.
4. Direct fields `cpu.data`, `cpu.data16`, `cpu.progMem`, and `cpu.progBytes`
   become accessor methods.
5. `dataView` word helpers become `sp`, `set_sp`, `read_u16`, and `write_u16`
   operations on CPU state.
6. TWI event handlers receive a mutable TWI context instead of storing a
   back-reference to the TWI object.
7. Assembler `Line::bytes` uses a string for multi-word output rather than a
   JavaScript string-or-array union.

## 10. Acceptance

1. `cargo test` passes all 347 ported tests, with the per-module counts in
   section 1.
2. `cargo build --release` succeeds.
3. `cargo clippy --all-targets -- -D warnings` succeeds.
4. The crate contains no `unsafe` code.
5. `lib.rs` exports are checked against upstream `index.ts`; `docs/parity.md`
   records the mapping.
6. Upstream v0.21.1 vitest suite is run once as a baseline and reports 347
   passing cases at commit `bee6f0a`.
7. README contains a minimal CPU + GPIO + callback example. It is plain code,
   not a doctest, so `cargo test` unit-test counts stay exact.

## 11. Implementation order

1. Install the stable Rust toolchain, create the crate, add MIT attribution,
   lint configuration, README skeleton, and parity document.
2. Port the assembler and its 83 tests; this establishes the test harness.
3. Implement CPU state, hooks, clock events, interrupts, and their 8 tests.
4. Implement the instruction interpreter and its 97 tests.
5. Implement GPIO, Clock, EEPROM, and ADC and their 42 tests.
6. Implement Timer and ATtiny Timer 1 and their 65 tests.
7. Implement SPI, TWI, USART, Watchdog, and USI; port their 52 tests.
8. Run the full suite, clippy, release build, export audit, and documentation
   review.

Each module follows red-green porting: translate the upstream spec first, run
it against the new module, then implement until every case passes. Upstream
JavaScript sources and the installed avr8js 0.21.1 package are used as the
behavioural reference when a test expectation is ambiguous.

## 12. Risks and mitigations

- **Borrow re-entrancy**: callbacks and hooks receive `&mut CpuState`;
  peripheral handlers are temporarily moved out while executing; notification
  callbacks are invoked after peripheral state borrows are released.
- **JavaScript number semantics**: shifts, `>>>`, 32-bit cycle counters,
  `parseInt`, `Math.floor`, and NaN defaults are translated explicitly with
  masks, wrapping arithmetic, and `f64` where upstream uses floating point.
- **Timer complexity**: the 55-case timer suite is ported before optimising;
  phase-correct and fast-PWM paths are verified against upstream tests.
- **Assembler output drift**: error strings, line numbers, byte offsets, and
  word byte order are asserted exactly by the 83 assembler tests.
- **Performance**: the instruction loop holds one CPU borrow per batch; clock
  events use stable IDs and no per-event allocation. No benchmark gate is
  required, but release-mode test execution must remain comfortably bounded
  by the upstream 5000-iteration runner limit.
