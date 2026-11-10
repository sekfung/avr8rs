#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::assembler::assemble;
use crate::cpu::{avr_instruction, Cpu};

const BREAK_OPCODE: u16 = 0x9598;

pub(crate) struct AsmProgram {
    pub program: Vec<u16>,
    pub lines: usize,
    pub instruction_count: usize,
    pub labels: BTreeMap<String, u32>,
}

pub(crate) fn asm_program(source: &str) -> AsmProgram {
    let result = assemble(source);
    if !result.errors.is_empty() {
        panic!("Assembly failed: {:?}", result.errors);
    }
    assert!(
        result.bytes.len() % 2 == 0,
        "assembly produced an odd number of bytes"
    );
    let program = result
        .bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let line_count = result.lines.len();
    AsmProgram {
        program,
        lines: line_count,
        instruction_count: line_count,
        labels: result.labels,
    }
}

pub(crate) struct TestProgramRunner {
    cpu: Cpu,
    on_break: RefCell<Option<BreakHandler>>,
}

type BreakHandler = Box<dyn FnMut(&Cpu)>;

impl TestProgramRunner {
    pub fn new(cpu: Cpu) -> Self {
        Self {
            cpu,
            on_break: RefCell::new(None),
        }
    }

    pub fn with_on_break(cpu: Cpu, handler: Box<dyn FnMut(&Cpu)>) -> Self {
        Self {
            cpu,
            on_break: RefCell::new(Some(handler)),
        }
    }

    fn check_break(&self) {
        if self.cpu.prog_word(self.cpu.pc()) == BREAK_OPCODE {
            match self.on_break.borrow_mut().as_mut() {
                Some(handler) => handler(&self.cpu),
                None => panic!("BREAK instruction encountered"),
            }
        }
    }

    pub fn run_instructions(&self, count: usize) {
        for _ in 0..count {
            self.check_break();
            avr_instruction(&self.cpu);
            self.cpu.tick();
        }
    }

    pub fn run_until(&self, predicate: impl Fn(&Cpu) -> bool, max_iterations: usize) {
        for _ in 0..max_iterations {
            self.check_break();
            if predicate(&self.cpu) {
                return;
            }
            avr_instruction(&self.cpu);
            self.cpu.tick();
        }
        panic!("Test program ran for too long, check your predicate");
    }

    pub fn run_to_break(&self) {
        self.run_until(|cpu| cpu.prog_word(cpu.pc()) == BREAK_OPCODE, 5000);
    }

    pub fn run_to_address(&self, byte_addr: u32) {
        self.run_until(|cpu| cpu.pc() * 2 == byte_addr, 5000);
    }
}

#[derive(Clone)]
pub(crate) struct Recorder<T>(Rc<RefCell<Vec<T>>>);

impl<T> Recorder<T> {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn push(&self, value: T) {
        self.0.borrow_mut().push(value);
    }

    pub fn calls(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.0.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

impl<T> Default for Recorder<T> {
    fn default() -> Self {
        Self::new()
    }
}
