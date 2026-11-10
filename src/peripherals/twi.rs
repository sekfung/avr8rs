use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState, InterruptConfig};

const TWCR_TWINT: u8 = 0x80;
const TWCR_TWEA: u8 = 0x40;
const TWCR_TWSTA: u8 = 0x20;
const TWCR_TWSTO: u8 = 0x10;
const TWCR_TWEN: u8 = 0x04;
const TWCR_TWIE: u8 = 0x01;

const TWSR_TWS_MASK: u8 = 0xf8;
const TWSR_TWPS_MASK: u8 = 0x03;

const STATUS_TWI_IDLE: u8 = 0xf8;
const STATUS_START: u8 = 0x08;
const STATUS_REPEATED_START: u8 = 0x10;
const STATUS_SLAW_ACK: u8 = 0x18;
const STATUS_SLAW_NACK: u8 = 0x20;
const STATUS_DATA_SENT_ACK: u8 = 0x28;
const STATUS_DATA_SENT_NACK: u8 = 0x30;
const STATUS_SLAR_ACK: u8 = 0x40;
const STATUS_SLAR_NACK: u8 = 0x48;
const STATUS_DATA_RECEIVED_ACK: u8 = 0x50;
const STATUS_DATA_RECEIVED_NACK: u8 = 0x58;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwiConfig {
    pub twi_interrupt: u8,
    pub twbr: u16,
    pub twcr: u16,
    pub twsr: u16,
    pub twdr: u16,
    pub twar: u16,
    pub twamr: u16,
}

pub fn twi_config() -> TwiConfig {
    TwiConfig {
        twi_interrupt: 0x30,
        twbr: 0xb8,
        twsr: 0xb9,
        twar: 0xba,
        twdr: 0xbb,
        twcr: 0xbc,
        twamr: 0xbd,
    }
}

pub trait TwiEventHandler {
    fn start(&mut self, ctx: &mut TwiContext<'_>, repeated: bool);
    fn stop(&mut self, ctx: &mut TwiContext<'_>);
    fn connect_to_slave(&mut self, ctx: &mut TwiContext<'_>, addr: u8, write: bool);
    fn write_byte(&mut self, ctx: &mut TwiContext<'_>, value: u8);
    fn read_byte(&mut self, ctx: &mut TwiContext<'_>, ack: bool);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopTwiEventHandler;

impl TwiEventHandler for NoopTwiEventHandler {
    fn start(&mut self, ctx: &mut TwiContext<'_>, _repeated: bool) {
        ctx.complete_start();
    }

    fn stop(&mut self, ctx: &mut TwiContext<'_>) {
        ctx.complete_stop();
    }

    fn connect_to_slave(&mut self, ctx: &mut TwiContext<'_>, _addr: u8, _write: bool) {
        ctx.complete_connect(false);
    }

    fn write_byte(&mut self, ctx: &mut TwiContext<'_>, _value: u8) {
        ctx.complete_write(false);
    }

    fn read_byte(&mut self, ctx: &mut TwiContext<'_>, _ack: bool) {
        ctx.complete_read(0xff);
    }
}

struct TwiState {
    cpu: Weak<RefCell<CpuState>>,
    config: TwiConfig,
    freq_hz: f64,
    busy: bool,
    twi_interrupt: InterruptConfig,
    event_handler: Option<Box<dyn TwiEventHandler>>,
}

pub struct TwiContext<'a> {
    twi: &'a mut TwiState,
    cpu: &'a mut CpuState,
}

impl TwiContext<'_> {
    pub fn complete_start(&mut self) {
        complete_start_inner(self.twi, self.cpu);
    }

    pub fn complete_stop(&mut self) {
        complete_stop_inner(self.twi, self.cpu);
    }

    pub fn complete_connect(&mut self, ack: bool) {
        complete_connect_inner(self.twi, self.cpu, ack);
    }

    pub fn complete_write(&mut self, ack: bool) {
        complete_write_inner(self.twi, self.cpu, ack);
    }

    pub fn complete_read(&mut self, value: u8) {
        complete_read_inner(self.twi, self.cpu, value);
    }

    pub fn data_byte(&self, addr: u16) -> u8 {
        self.cpu.data_byte(addr)
    }

    pub fn set_data_byte(&mut self, addr: u16, value: u8) {
        self.cpu.set_data_byte(addr, value);
    }
}

#[derive(Clone)]
pub struct Twi(Rc<RefCell<TwiState>>);

impl Twi {
    pub fn new(cpu: Cpu, config: TwiConfig, freq_hz: f64) -> Self {
        let twi_interrupt = InterruptConfig {
            address: config.twi_interrupt,
            enable_register: config.twcr,
            enable_mask: TWCR_TWIE,
            flag_register: config.twcr,
            flag_mask: TWCR_TWINT,
            constant: false,
            inverse_flag: false,
        };
        let twi = Twi(Rc::new(RefCell::new(TwiState {
            cpu: cpu.downgrade(),
            config,
            freq_hz,
            busy: false,
            twi_interrupt,
            event_handler: Some(Box::new(NoopTwiEventHandler)),
        })));
        {
            let mut cpu_state = cpu.borrow_state_mut();
            let mut state = twi.0.borrow_mut();
            update_status_inner(&mut state, &mut cpu_state, STATUS_TWI_IDLE);
        }

        let writer = twi.clone();
        cpu.set_write_hook(
            config.twcr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_twcr_write(state, value)
            }),
        );
        twi
    }

    pub fn prescaler(&self) -> u32 {
        let (cpu, twsr) = {
            let twi = self.0.borrow();
            let cpu = twi
                .cpu
                .upgrade()
                .expect("CPU was dropped while its TWI peripheral is still in use");
            (cpu, twi.config.twsr)
        };
        let value = cpu.borrow().data_byte(twsr);
        prescaler_for(value)
    }

    pub fn scl_frequency(&self) -> f64 {
        let (cpu, twbr, twsr, freq_hz) = {
            let twi = self.0.borrow();
            let cpu = twi
                .cpu
                .upgrade()
                .expect("CPU was dropped while its TWI peripheral is still in use");
            (cpu, twi.config.twbr, twi.config.twsr, twi.freq_hz)
        };
        let cpu = cpu.borrow();
        let prescaler = prescaler_for(cpu.data_byte(twsr));
        freq_hz / (16.0 + 2.0 * cpu.data_byte(twbr) as f64 * prescaler as f64)
    }

    pub fn complete_start(&self) {
        self.with_state(complete_start_inner);
    }

    pub fn complete_stop(&self) {
        self.with_state(complete_stop_inner);
    }

    pub fn complete_connect(&self, ack: bool) {
        self.with_state(|twi, cpu| complete_connect_inner(twi, cpu, ack));
    }

    pub fn complete_write(&self, ack: bool) {
        self.with_state(|twi, cpu| complete_write_inner(twi, cpu, ack));
    }

    pub fn complete_read(&self, value: u8) {
        self.with_state(|twi, cpu| complete_read_inner(twi, cpu, value));
    }

    pub fn set_event_handler(&self, handler: Box<dyn TwiEventHandler>) {
        self.0.borrow_mut().event_handler = Some(handler);
    }

    fn with_state(&self, callback: impl FnOnce(&mut TwiState, &mut CpuState)) {
        let cpu = self
            .0
            .borrow()
            .cpu
            .upgrade()
            .expect("CPU was dropped while its TWI peripheral is still in use");
        let mut cpu_state = cpu.borrow_mut();
        let mut twi = self.0.borrow_mut();
        callback(&mut twi, &mut cpu_state);
    }

    fn handle_twcr_write(&self, state: &mut CpuState, value: u8) -> bool {
        let (config, interrupt, busy) = {
            let twi = self.0.borrow();
            (twi.config, twi.twi_interrupt, twi.busy)
        };

        state.set_data_byte(config.twcr, value);
        state.clear_interrupt_by_flag(interrupt, value);
        state.update_interrupt_enable(interrupt, value);

        let status = state.data_byte(config.twsr) & TWSR_TWS_MASK;
        if value & TWCR_TWINT != 0 && value & TWCR_TWEN != 0 && !busy {
            let twdr = state.data_byte(config.twdr);
            let twi = Rc::downgrade(&self.0);
            state.add_clock_event(
                Box::new(move |state| {
                    let Some(twi) = twi.upgrade() else {
                        return;
                    };
                    if let Ok(mut twi) = twi.try_borrow_mut() {
                        dispatch_event(&mut twi, state, value, status, twdr);
                    };
                }),
                0,
            );
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TwiEvent {
    Start(bool),
    Stop,
    Connect(u8, bool),
    Write(u8),
    Read(bool),
}

fn dispatch_event(twi: &mut TwiState, cpu: &mut CpuState, value: u8, status: u8, twdr: u8) {
    let event = if value & TWCR_TWSTA != 0 {
        TwiEvent::Start(status != STATUS_TWI_IDLE)
    } else if value & TWCR_TWSTO != 0 {
        TwiEvent::Stop
    } else if status == STATUS_START || status == STATUS_REPEATED_START {
        TwiEvent::Connect(twdr >> 1, twdr & 0x01 == 0)
    } else if status == STATUS_SLAW_ACK || status == STATUS_DATA_SENT_ACK {
        TwiEvent::Write(twdr)
    } else if status == STATUS_SLAR_ACK || status == STATUS_DATA_RECEIVED_ACK {
        TwiEvent::Read(value & TWCR_TWEA != 0)
    } else {
        return;
    };

    twi.busy = true;
    let Some(mut handler) = twi.event_handler.take() else {
        return;
    };
    {
        let mut ctx = TwiContext { twi, cpu };
        match event {
            TwiEvent::Start(repeated) => handler.start(&mut ctx, repeated),
            TwiEvent::Stop => handler.stop(&mut ctx),
            TwiEvent::Connect(addr, write) => handler.connect_to_slave(&mut ctx, addr, write),
            TwiEvent::Write(byte) => handler.write_byte(&mut ctx, byte),
            TwiEvent::Read(ack) => handler.read_byte(&mut ctx, ack),
        }
    }
    twi.event_handler = Some(handler);
}

fn prescaler_for(twsr: u8) -> u32 {
    match twsr & TWSR_TWPS_MASK {
        0 => 1,
        1 => 4,
        2 => 16,
        3 => 64,
        _ => unreachable!("Invalid prescaler value!"),
    }
}

fn complete_start_inner(twi: &mut TwiState, cpu: &mut CpuState) {
    twi.busy = false;
    let status = cpu.data_byte(twi.config.twsr) & TWSR_TWS_MASK;
    update_status_inner(
        twi,
        cpu,
        if status == STATUS_TWI_IDLE {
            STATUS_START
        } else {
            STATUS_REPEATED_START
        },
    );
}

fn complete_stop_inner(twi: &mut TwiState, cpu: &mut CpuState) {
    twi.busy = false;
    let twcr = cpu.data_byte(twi.config.twcr) & !TWCR_TWSTO;
    cpu.set_data_byte(twi.config.twcr, twcr);
    update_status_inner(twi, cpu, STATUS_TWI_IDLE);
}

fn complete_connect_inner(twi: &mut TwiState, cpu: &mut CpuState, ack: bool) {
    twi.busy = false;
    let status = if cpu.data_byte(twi.config.twdr) & 0x01 != 0 {
        if ack {
            STATUS_SLAR_ACK
        } else {
            STATUS_SLAR_NACK
        }
    } else if ack {
        STATUS_SLAW_ACK
    } else {
        STATUS_SLAW_NACK
    };
    update_status_inner(twi, cpu, status);
}

fn complete_write_inner(twi: &mut TwiState, cpu: &mut CpuState, ack: bool) {
    twi.busy = false;
    update_status_inner(
        twi,
        cpu,
        if ack {
            STATUS_DATA_SENT_ACK
        } else {
            STATUS_DATA_SENT_NACK
        },
    );
}

fn complete_read_inner(twi: &mut TwiState, cpu: &mut CpuState, value: u8) {
    twi.busy = false;
    let ack = cpu.data_byte(twi.config.twcr) & TWCR_TWEA != 0;
    cpu.set_data_byte(twi.config.twdr, value);
    update_status_inner(
        twi,
        cpu,
        if ack {
            STATUS_DATA_RECEIVED_ACK
        } else {
            STATUS_DATA_RECEIVED_NACK
        },
    );
}

fn update_status_inner(twi: &mut TwiState, cpu: &mut CpuState, value: u8) {
    let twsr = cpu.data_byte(twi.config.twsr);
    cpu.set_data_byte(
        twi.config.twsr,
        (twsr & !TWSR_TWS_MASK) | (value & TWSR_TWS_MASK),
    );
    cpu.set_interrupt_flag(twi.twi_interrupt);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::test_support::{asm_program, Recorder, TestProgramRunner};

    const FREQ_16MHZ: f64 = 16_000_000.0;

    const R17: u16 = 17;
    const SREG: u16 = 95;

    const TWBR: u16 = 0xb8;
    const TWSR: u16 = 0xb9;
    const TWDR: u16 = 0xbb;
    const TWCR: u16 = 0xbc;

    const TWIE: u8 = 0x01;
    const TWEN: u8 = 0x04;
    const TWSTO: u8 = 0x10;
    const TWSTA: u8 = 0x20;
    const TWEA: u8 = 0x40;
    const TWINT: u8 = 0x80;

    #[derive(Clone, Debug, PartialEq)]
    enum TwiCall {
        Start(bool),
        Stop,
        Connect(u8, bool),
        Write(u8),
        Read(bool),
    }

    struct RecordingEventHandler {
        calls: Recorder<TwiCall>,
        complete: bool,
    }

    impl TwiEventHandler for RecordingEventHandler {
        fn start(&mut self, ctx: &mut TwiContext<'_>, repeated: bool) {
            self.calls.push(TwiCall::Start(repeated));
            if self.complete {
                ctx.complete_start();
            }
        }

        fn stop(&mut self, ctx: &mut TwiContext<'_>) {
            self.calls.push(TwiCall::Stop);
            if self.complete {
                ctx.complete_stop();
            }
        }

        fn connect_to_slave(&mut self, ctx: &mut TwiContext<'_>, addr: u8, write: bool) {
            self.calls.push(TwiCall::Connect(addr, write));
            if self.complete {
                ctx.complete_connect(false);
            }
        }

        fn write_byte(&mut self, ctx: &mut TwiContext<'_>, value: u8) {
            self.calls.push(TwiCall::Write(value));
            if self.complete {
                ctx.complete_write(false);
            }
        }

        fn read_byte(&mut self, ctx: &mut TwiContext<'_>, ack: bool) {
            self.calls.push(TwiCall::Read(ack));
            if self.complete {
                ctx.complete_read(0xff);
            }
        }
    }

    fn attach_recorder(twi: &Twi, complete: bool) -> Recorder<TwiCall> {
        let calls = Recorder::new();
        twi.set_event_handler(Box::new(RecordingEventHandler {
            calls: calls.clone(),
            complete,
        }));
        calls
    }

    fn noop_break_handler(_cpu: &Cpu) {}

    #[test]
    fn should_calculate_scl_frequency_from_twbr() {
        let cpu = Cpu::new(vec![0; 1024]);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        cpu.write_data(TWBR, 0x48);
        cpu.write_data(TWSR, 0);
        assert_eq!(twi.scl_frequency(), 100_000.0);
    }

    #[test]
    fn should_account_for_prescaler_when_calculating_scl_frequency() {
        let cpu = Cpu::new(vec![0; 1024]);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        cpu.write_data(TWBR, 0x03);
        cpu.write_data(TWSR, 0x01);
        assert_eq!(twi.scl_frequency(), 400_000.0);
    }

    #[test]
    fn should_trigger_interrupt_when_twint_is_set() {
        let cpu = Cpu::new(vec![0; 1024]);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        cpu.write_data(TWCR, TWIE);
        cpu.set_data_byte(SREG, 0x80);
        twi.complete_start();
        cpu.tick();
        assert_eq!(cpu.pc(), 0x30);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(TWCR) & TWINT, 0);
    }

    #[test]
    fn should_call_start_handler_when_twsta_is_written() {
        let cpu = Cpu::new(vec![0; 1024]);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        let calls = attach_recorder(&twi, true);
        cpu.write_data(TWCR, TWINT | TWSTA | TWEN);
        cpu.add_cycles(1);
        cpu.tick();
        assert_eq!(calls.calls(), vec![TwiCall::Start(false)]);
    }

    #[test]
    fn should_connect_after_repeated_start() {
        let cpu = Cpu::new(vec![0; 1024]);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);

        cpu.write_data(TWCR, TWINT | TWSTA | TWEN);
        cpu.add_cycles(1);
        cpu.tick();

        let calls = attach_recorder(&twi, true);
        cpu.write_data(TWCR, TWINT | TWSTA | TWEN);
        cpu.add_cycles(1);
        cpu.tick();
        assert_eq!(calls.calls(), vec![TwiCall::Start(true)]);

        cpu.write_data(TWDR, 0x80);
        cpu.write_data(TWCR, TWINT | TWEN);
        cpu.add_cycles(1);
        cpu.tick();

        assert_eq!(
            calls.calls(),
            vec![TwiCall::Start(true), TwiCall::Connect(0x40, true),]
        );
    }

    #[test]
    fn should_transmit_byte_to_slave() {
        let asm = asm_program(&format!(
            r#"
            ; register addresses
            _REPLACE TWSR, {TWSR}
            _REPLACE TWDR, {TWDR}
            _REPLACE TWCR, {TWCR}

            ; TWCR bits
            _REPLACE TWEN, {TWEN}
            _REPLACE TWSTO, {TWSTO}
            _REPLACE TWSTA, {TWSTA}
            _REPLACE TWINT, {TWINT}

            ; TWSR states
            _REPLACE START, 0x8
            _REPLACE MT_SLA_ACK, 0x18
            _REPLACE MT_DATA_ACK, 0x28

            ldi r16, TWEN
            sbr r16, TWSTA
            sbr r16, TWINT
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, START
            brne error

            _REPLACE SLA_W, 0x44
            ldi r16, SLA_W
            sts TWDR, r16
            ldi r16, TWINT
            sbr r16, TWEN
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, MT_SLA_ACK
            brne error

            _REPLACE DATA, 0x55
            ldi r16, DATA
            sts TWDR, r16
            ldi r16, TWINT
            sbr r16, TWEN
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, MT_DATA_ACK
            brne error

            ldi r16, TWINT
            sbr r16, TWEN
            sbr r16, TWSTO
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, 0xf8
            brne error

            ldi r17, 0x42

            loop:
            jmp loop

            wait_for_twint:
            lds r16, TWCR
            andi r16, TWINT
            breq wait_for_twint
            ret

            error:
            break
            "#
        ));
        let cpu = Cpu::new(asm.program);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        let calls = attach_recorder(&twi, false);
        let runner = TestProgramRunner::with_on_break(cpu.clone(), Box::new(noop_break_handler));

        runner.run_instructions(4);
        assert_eq!(calls.calls(), vec![TwiCall::Start(false)]);

        runner.run_instructions(16);
        twi.complete_start();

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![TwiCall::Start(false), TwiCall::Connect(0x22, true)]
        );

        runner.run_instructions(16);
        twi.complete_connect(true);

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![
                TwiCall::Start(false),
                TwiCall::Connect(0x22, true),
                TwiCall::Write(0x55),
            ]
        );

        runner.run_instructions(16);
        twi.complete_write(true);

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![
                TwiCall::Start(false),
                TwiCall::Connect(0x22, true),
                TwiCall::Write(0x55),
                TwiCall::Stop,
            ]
        );

        runner.run_instructions(16);
        twi.complete_stop();

        runner.run_instructions(16);
        assert_eq!(cpu.data_byte(R17), 0x42);
    }

    #[test]
    fn should_receive_byte_from_slave() {
        let asm = asm_program(&format!(
            r#"
            ; register addresses
            _REPLACE TWSR, {TWSR}
            _REPLACE TWDR, {TWDR}
            _REPLACE TWCR, {TWCR}

            ; TWCR bits
            _REPLACE TWEN, {TWEN}
            _REPLACE TWSTO, {TWSTO}
            _REPLACE TWSTA, {TWSTA}
            _REPLACE TWEA, {TWEA}
            _REPLACE TWINT, {TWINT}

            ; TWSR states
            _REPLACE START, 0x8
            _REPLACE MT_SLAR_ACK, 0x40
            _REPLACE MT_DATA_RECV, 0x50
            _REPLACE MT_DATA_RECV_NACK, 0x58

            ldi r16, TWEN
            sbr r16, TWSTA
            sbr r16, TWINT
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            ldi r18, START
            cpse r16, r18
            jmp error

            _REPLACE SLA_R, 0xa1
            ldi r16, SLA_R
            sts TWDR, r16
            ldi r16, TWINT
            sbr r16, TWEN
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, MT_SLAR_ACK
            brne error

            ldi r16, TWINT
            sbr r16, TWEA
            sbr r16, TWEN
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, MT_DATA_RECV
            brne error

            lds r16, TWDR
            cpi r16, 0x66
            brne error

            ldi r16, TWINT
            sbr r16, TWEN
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, MT_DATA_RECV_NACK
            brne error

            lds r16, TWDR
            cpi r16, 0x77
            brne error

            ldi r16, TWINT
            sbr r16, TWEN
            sbr r16, TWSTO
            sts TWCR, r16

            call wait_for_twint

            lds r16, TWSR
            andi r16, 0xf8
            cpi r16, 0xf8
            brne error

            ldi r17, 0x42

            loop:
            jmp loop

            wait_for_twint:
            lds r16, TWCR
            andi r16, TWINT
            breq wait_for_twint
            ret

            error:
            break
            "#
        ));
        let cpu = Cpu::new(asm.program);
        let twi = Twi::new(cpu.clone(), twi_config(), FREQ_16MHZ);
        let calls = attach_recorder(&twi, false);
        let runner = TestProgramRunner::with_on_break(cpu.clone(), Box::new(noop_break_handler));

        runner.run_instructions(4);
        assert_eq!(calls.calls(), vec![TwiCall::Start(false)]);

        runner.run_instructions(16);
        twi.complete_start();

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![TwiCall::Start(false), TwiCall::Connect(0x50, false)]
        );

        runner.run_instructions(16);
        twi.complete_connect(true);

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![
                TwiCall::Start(false),
                TwiCall::Connect(0x50, false),
                TwiCall::Read(true),
            ]
        );

        runner.run_instructions(16);
        twi.complete_read(0x66);

        runner.run_instructions(16);
        assert_eq!(
            calls.calls(),
            vec![
                TwiCall::Start(false),
                TwiCall::Connect(0x50, false),
                TwiCall::Read(true),
                TwiCall::Read(false),
            ]
        );

        runner.run_instructions(16);
        twi.complete_read(0x77);

        runner.run_instructions(24);
        assert_eq!(
            calls.calls(),
            vec![
                TwiCall::Start(false),
                TwiCall::Connect(0x50, false),
                TwiCall::Read(true),
                TwiCall::Read(false),
                TwiCall::Stop,
            ]
        );

        runner.run_instructions(16);
        twi.complete_stop();

        runner.run_instructions(16);
        assert_eq!(cpu.data_byte(R17), 0x42);
    }
}
