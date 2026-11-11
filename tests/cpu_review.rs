use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use avr8rs::{avr_instruction, Cpu};

const HALF_CARRY: u8 = 0x20;

#[test]
fn addition_sets_half_carry_from_bit_three() {
    for (opcode, left, right, carry, expected) in [
        (0x0f01, 8, 8, 0, HALF_CARRY), // ADD r16, r17
        (0x0f01, 1, 1, 0, 0),
        (0x1f01, 7, 8, 1, HALF_CARRY), // ADC r16, r17
        (0x1f01, 0, 1, 1, 0),
    ] {
        let cpu = Cpu::new(vec![opcode]);
        cpu.set_data_byte(16, left);
        cpu.set_data_byte(17, right);
        cpu.set_sreg(HALF_CARRY | carry);
        avr_instruction(&cpu);
        assert_eq!(cpu.sreg() & HALF_CARRY, expected, "opcode {opcode:#06x}");
    }
}

#[test]
fn subtraction_and_comparison_set_half_carry_from_bit_three() {
    for (opcode, uses_carry) in [
        (0x1b01, false), // SUB r16, r17
        (0x0b01, true),  // SBC r16, r17
        (0x1701, false), // CP r16, r17
        (0x0701, true),  // CPC r16, r17
        (0x3001, false), // CPI r16, 1
        (0x5001, false), // SUBI r16, 1
        (0x4001, true),  // SBCI r16, 1
    ] {
        let cpu = Cpu::new(vec![opcode]);
        cpu.set_data_byte(17, 1);
        for (left, carry, expected) in [(0x10, 0, HALF_CARRY), (2, 0, 0)] {
            cpu.set_data_byte(16, left);
            cpu.set_sreg(HALF_CARRY | carry);
            avr_instruction(&cpu);
            assert_eq!(cpu.sreg() & HALF_CARRY, expected, "opcode {opcode:#06x}");
        }
        if uses_carry {
            cpu.set_data_byte(16, 0x11);
            cpu.set_sreg(1);
            avr_instruction(&cpu);
            assert_eq!(cpu.sreg() & HALF_CARRY, HALF_CARRY, "opcode {opcode:#06x}");
        }
    }
}

#[test]
fn neg_uses_bit_three_and_sbiw_preserves_half_carry() {
    let cpu = Cpu::new(vec![0x9501, 0x9701]); // NEG r16; SBIW r25:r24, 1
    cpu.set_data_byte(16, 8);
    cpu.set_data_byte(24, 1);
    avr_instruction(&cpu);
    assert_eq!(cpu.sreg() & HALF_CARRY, HALF_CARRY);
    avr_instruction(&cpu);
    assert_eq!(cpu.sreg() & HALF_CARRY, HALF_CARRY);
}

#[test]
fn return_uses_all_three_address_bytes_at_word_boundary() {
    for opcode in [0x9508, 0x9518] {
        let mut program = vec![0; 0x20000];
        program[0xffff] = 0xd001; // RCALL to 0x10001, returning to 0x10000
        program[0x10001] = opcode;
        let cpu = Cpu::new(program);
        let sp = cpu.sp();
        cpu.set_pc(0xffff);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x10001);
        assert_eq!(cpu.sp(), sp - 3);

        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x10000, "opcode {opcode:#06x}");
        assert_eq!(cpu.sp(), sp);
        assert_eq!(cpu.sreg() & 0x80 != 0, opcode == 0x9518);
    }
}

#[test]
fn write_hook_can_clear_itself() {
    let cpu = Cpu::new(vec![0]);
    let calls = Rc::new(Cell::new(0));
    let calls_in_hook = calls.clone();
    cpu.set_write_hook(
        0x100,
        Box::new(move |state, _, _, addr, _| {
            calls_in_hook.set(calls_in_hook.get() + 1);
            state.clear_write_hook(addr);
            false
        }),
    );
    cpu.write_data(0x100, 1);
    cpu.write_data(0x100, 2);
    assert_eq!(calls.get(), 1);
    assert_eq!(cpu.data_byte(0x100), 2);
}

#[test]
fn read_hook_replacement_survives_unwind() {
    let cpu = Cpu::new(vec![0]);
    cpu.set_data_byte(0x100, 0xff);
    cpu.set_read_hook(
        0x100,
        Box::new(|state, addr| {
            // Nested reads bypass the hook currently executing.
            assert_eq!(state.read_data(addr), 0xff);
            state.set_read_hook(addr, Box::new(|_, _| 0x42));
            panic!("original read hook");
        }),
    );
    assert!(catch_unwind(AssertUnwindSafe(|| cpu.read_data(0x100))).is_err());
    assert_eq!(cpu.read_data(0x100), 0x42);
}

#[test]
fn clock_event_can_reschedule_itself_and_retains_its_id() {
    let cpu = Cpu::new(vec![0]);
    let id = Rc::new(Cell::new(None));
    let id_in_callback = id.clone();
    let calls = Rc::new(Cell::new(0));
    let calls_in_callback = calls.clone();
    let event = cpu.add_clock_event(
        Box::new(move |state| {
            calls_in_callback.set(calls_in_callback.get() + 1);
            if calls_in_callback.get() == 1 {
                assert!(state.update_clock_event(id_in_callback.get().unwrap(), 2));
            }
        }),
        1,
    );
    id.set(Some(event));
    cpu.run_instructions(2);
    assert_eq!(calls.get(), 1);
    cpu.run_instructions(1);
    assert_eq!(calls.get(), 2);
    assert!(!cpu.update_clock_event(event, 1));
    let next = cpu.add_clock_event(Box::new(|_| {}), 1);
    cpu.reset();
    let after_reset = cpu.add_clock_event(Box::new(|_| {}), 1);
    assert_ne!(next, after_reset);
    assert!(!cpu.clear_clock_event(next));
    assert!(cpu.clear_clock_event(after_reset));
}
