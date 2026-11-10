use std::cell::{Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};

use crate::peripherals::gpio::IoPort;

use super::interrupt::avr_interrupt_state;

const DEFAULT_SRAM_BYTES: usize = 8192;
const REGISTER_SPACE: usize = 0x100;
const MAX_INTERRUPTS: usize = 128;

pub type CpuReadHook = Box<dyn FnMut(&mut CpuState, u16) -> u8>;
pub type CpuWriteHook = Box<dyn FnMut(&mut CpuState, u8, u8, u16, u8) -> bool>;

type ClockEventCallback = Rc<RefCell<Box<dyn FnMut(&mut CpuState)>>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClockEventId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InterruptConfig {
    pub address: u8,
    pub enable_register: u16,
    pub enable_mask: u8,
    pub flag_register: u16,
    pub flag_mask: u8,
    pub constant: bool,
    pub inverse_flag: bool,
}

#[derive(Clone, Copy, Debug)]
struct ClockEventEntry {
    due: u64,
    id: ClockEventId,
}

#[derive(Clone)]
pub struct Cpu(Rc<RefCell<CpuState>>);

pub struct CpuState {
    data: Vec<u8>,
    prog_mem: Vec<u16>,
    prog_bytes: Vec<u8>,
    pc: u32,
    cycles: u64,
    pc22_bits: bool,
    read_hooks: Vec<Option<CpuReadHook>>,
    write_hooks: Vec<Option<CpuWriteHook>>,
    clock_events: Vec<ClockEventEntry>,
    clock_callbacks: Vec<Option<ClockEventCallback>>,
    next_clock_event_id: u64,
    pending_interrupts: Vec<Option<InterruptConfig>>,
    next_interrupt: i16,
    max_interrupt: i16,
    pub(crate) gpio_ports: Vec<IoPort>,
    pub(crate) gpio_by_port: Vec<Option<IoPort>>,
    on_watchdog_reset: Box<dyn FnMut(&mut CpuState)>,
}

impl Cpu {
    pub fn new(prog_mem: Vec<u16>) -> Self {
        Self::with_sram(prog_mem, DEFAULT_SRAM_BYTES)
    }

    pub(crate) fn downgrade(&self) -> Weak<RefCell<CpuState>> {
        Rc::downgrade(&self.0)
    }

    pub(crate) fn from_weak(cpu: &Weak<RefCell<CpuState>>) -> Option<Self> {
        cpu.upgrade().map(Cpu)
    }

    pub fn with_sram(prog_mem: Vec<u16>, sram_bytes: usize) -> Self {
        let mut state = CpuState::new(prog_mem, sram_bytes);
        state.reset();
        Cpu(Rc::new(RefCell::new(state)))
    }

    pub(crate) fn borrow_state_mut(&self) -> RefMut<'_, CpuState> {
        self.0.borrow_mut()
    }

    pub fn reset(&self) {
        self.0.borrow_mut().reset();
    }

    pub fn read_data(&self, addr: u16) -> u8 {
        self.0.borrow_mut().read_data(addr)
    }

    pub fn write_data(&self, addr: u16, value: u8) {
        self.0.borrow_mut().write_data(addr, value);
    }

    pub fn write_data_masked(&self, addr: u16, value: u8, mask: u8) {
        self.0.borrow_mut().write_data_masked(addr, value, mask);
    }

    pub fn data(&self) -> Ref<'_, [u8]> {
        Ref::map(self.0.borrow(), |state| state.data.as_slice())
    }

    pub fn data_byte(&self, addr: u16) -> u8 {
        self.0.borrow().data_byte(addr)
    }

    pub fn set_data_byte(&self, addr: u16, value: u8) {
        self.0.borrow_mut().set_data_byte(addr, value);
    }

    pub fn pc(&self) -> u32 {
        self.0.borrow().pc()
    }

    pub fn set_pc(&self, pc: u32) {
        self.0.borrow_mut().set_pc(pc);
    }

    pub fn cycles(&self) -> u64 {
        self.0.borrow().cycles()
    }

    pub fn set_cycles(&self, cycles: u64) {
        self.0.borrow_mut().set_cycles(cycles);
    }

    pub fn add_cycles(&self, delta: u64) {
        self.0.borrow_mut().add_cycles(delta);
    }

    pub fn write_prog_bytes(&self, bytes: &[u8], offset: usize) {
        self.0.borrow_mut().write_prog_bytes(bytes, offset);
    }

    pub fn prog_word(&self, word_addr: u32) -> u16 {
        self.0.borrow().prog_word(word_addr)
    }

    pub fn set_prog_word(&self, word_addr: u32, value: u16) {
        self.0.borrow_mut().set_prog_word(word_addr, value);
    }

    pub fn prog_bytes(&self) -> Ref<'_, [u8]> {
        Ref::map(self.0.borrow(), |state| state.prog_bytes.as_slice())
    }

    pub fn prog_byte(&self, byte_addr: u32) -> u8 {
        self.0.borrow().prog_byte(byte_addr)
    }

    pub fn pc22_bits(&self) -> bool {
        self.0.borrow().pc22_bits()
    }

    pub fn sp(&self) -> u16 {
        self.0.borrow().sp()
    }

    pub fn set_sp(&self, value: u16) {
        self.0.borrow_mut().set_sp(value);
    }

    pub fn sreg(&self) -> u8 {
        self.0.borrow().sreg()
    }

    pub fn set_sreg(&self, value: u8) {
        self.0.borrow_mut().set_sreg(value);
    }

    pub fn set_read_hook(&self, addr: u16, hook: CpuReadHook) {
        self.0.borrow_mut().set_read_hook(addr, hook);
    }

    pub fn set_write_hook(&self, addr: u16, hook: CpuWriteHook) {
        self.0.borrow_mut().set_write_hook(addr, hook);
    }

    pub fn take_write_hook(&self, addr: u16) -> Option<CpuWriteHook> {
        self.0.borrow_mut().take_write_hook(addr)
    }

    pub fn clear_read_hook(&self, addr: u16) {
        self.0.borrow_mut().clear_read_hook(addr);
    }

    pub fn clear_write_hook(&self, addr: u16) {
        self.0.borrow_mut().clear_write_hook(addr);
    }

    pub fn add_clock_event(
        &self,
        callback: Box<dyn FnMut(&mut CpuState)>,
        delay: u64,
    ) -> ClockEventId {
        self.0.borrow_mut().add_clock_event(callback, delay)
    }

    pub fn update_clock_event(&self, id: ClockEventId, delay: u64) -> bool {
        self.0.borrow_mut().update_clock_event(id, delay)
    }

    pub fn clear_clock_event(&self, id: ClockEventId) -> bool {
        self.0.borrow_mut().clear_clock_event(id)
    }

    pub fn set_on_watchdog_reset(&self, callback: Box<dyn FnMut(&mut CpuState)>) {
        self.0.borrow_mut().on_watchdog_reset = callback;
    }

    pub fn step(&self) {
        super::avr_instruction(self);
        self.tick();
    }

    pub fn run_instructions(&self, count: usize) {
        let mut state = self.0.borrow_mut();
        for _ in 0..count {
            super::avr_instruction_state(&mut state);
            state.tick();
        }
    }

    pub fn tick(&self) {
        self.0.borrow_mut().tick();
    }
}

impl CpuState {
    fn new(prog_mem: Vec<u16>, sram_bytes: usize) -> Self {
        let data = vec![0; sram_bytes + REGISTER_SPACE];
        let prog_bytes: Vec<_> = prog_mem
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        let pc22_bits = prog_bytes.len() > 0x20000;
        let hook_count = data.len();
        let gpio_by_port = std::iter::repeat_with(|| None).take(hook_count).collect();

        Self {
            data,
            prog_mem,
            prog_bytes,
            pc: 0,
            cycles: 0,
            pc22_bits,
            read_hooks: std::iter::repeat_with(|| None).take(hook_count).collect(),
            write_hooks: std::iter::repeat_with(|| None).take(hook_count).collect(),
            clock_events: Vec::new(),
            clock_callbacks: Vec::new(),
            next_clock_event_id: 0,
            pending_interrupts: vec![None; MAX_INTERRUPTS],
            next_interrupt: -1,
            max_interrupt: 0,
            gpio_ports: Vec::new(),
            gpio_by_port,
            on_watchdog_reset: Box::new(|_| {}),
        }
    }

    pub fn reset(&mut self) {
        self.set_sp((self.data.len() - 1) as u16);
        self.pc = 0;
        self.pending_interrupts.fill(None);
        self.next_interrupt = -1;
        self.clock_events.clear();
        for callback in &mut self.clock_callbacks {
            *callback = None;
        }
    }

    pub fn sram_bytes(&self) -> usize {
        self.data.len() - REGISTER_SPACE
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_byte(&self, addr: u16) -> u8 {
        self.data[addr as usize]
    }

    pub fn set_data_byte(&mut self, addr: u16, value: u8) {
        self.data[addr as usize] = value;
    }

    pub fn read_u16(&self, addr: u16) -> u16 {
        let addr = addr as usize;
        u16::from_le_bytes([self.data[addr], self.data[addr + 1]])
    }

    pub fn write_u16(&mut self, addr: u16, value: u16) {
        let addr = addr as usize;
        self.data[addr..addr + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn read_data(&mut self, addr: u16) -> u8 {
        if addr >= 32 {
            let hook = self
                .read_hooks
                .get_mut(addr as usize)
                .and_then(Option::take);
            if let Some(mut hook) = hook {
                let value = hook(self, addr);
                self.read_hooks[addr as usize] = Some(hook);
                return value;
            }
        }
        self.data[addr as usize]
    }

    pub fn write_data(&mut self, addr: u16, value: u8) {
        self.write_data_masked(addr, value, 0xff);
    }

    pub fn write_data_masked(&mut self, addr: u16, value: u8, mask: u8) {
        let hook = self
            .write_hooks
            .get_mut(addr as usize)
            .and_then(Option::take);
        if let Some(mut hook) = hook {
            let old_value = self.data[addr as usize];
            let suppress = hook(self, value, old_value, addr, mask);
            self.write_hooks[addr as usize] = Some(hook);
            if suppress {
                return;
            }
        }
        self.data[addr as usize] = value;
    }

    pub fn set_read_hook(&mut self, addr: u16, hook: CpuReadHook) {
        if let Some(slot) = self.read_hooks.get_mut(addr as usize) {
            *slot = Some(hook);
        }
    }

    pub fn set_write_hook(&mut self, addr: u16, hook: CpuWriteHook) {
        if let Some(slot) = self.write_hooks.get_mut(addr as usize) {
            *slot = Some(hook);
        }
    }

    pub(crate) fn take_write_hook(&mut self, addr: u16) -> Option<CpuWriteHook> {
        self.write_hooks
            .get_mut(addr as usize)
            .and_then(Option::take)
    }

    pub fn clear_read_hook(&mut self, addr: u16) {
        if let Some(slot) = self.read_hooks.get_mut(addr as usize) {
            *slot = None;
        }
    }

    pub fn clear_write_hook(&mut self, addr: u16) {
        if let Some(slot) = self.write_hooks.get_mut(addr as usize) {
            *slot = None;
        }
    }

    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn set_cycles(&mut self, cycles: u64) {
        self.cycles = cycles;
    }

    pub fn add_cycles(&mut self, delta: u64) {
        self.cycles = self.cycles.wrapping_add(delta);
    }

    pub fn sp(&self) -> u16 {
        u16::from_le_bytes([self.data[93], self.data[94]])
    }

    pub fn set_sp(&mut self, value: u16) {
        self.data[93..95].copy_from_slice(&value.to_le_bytes());
    }

    pub fn sreg(&self) -> u8 {
        self.data[95]
    }

    pub fn set_sreg(&mut self, value: u8) {
        self.data[95] = value;
    }

    pub fn interrupts_enabled(&self) -> bool {
        self.sreg() & 0x80 != 0
    }

    pub fn prog_word(&self, word_addr: u32) -> u16 {
        self.prog_mem.get(word_addr as usize).copied().unwrap_or(0)
    }

    pub fn set_prog_word(&mut self, word_addr: u32, value: u16) {
        let word_addr = word_addr as usize;
        self.prog_mem[word_addr] = value;
        self.prog_bytes[word_addr * 2..word_addr * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn prog_bytes(&self) -> &[u8] {
        &self.prog_bytes
    }

    pub fn prog_byte(&self, byte_addr: u32) -> u8 {
        self.prog_bytes
            .get(byte_addr as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn write_prog_bytes(&mut self, bytes: &[u8], offset: usize) {
        let end = offset + bytes.len();
        assert!(
            end <= self.prog_bytes.len(),
            "program memory write out of bounds"
        );
        self.prog_bytes[offset..end].copy_from_slice(bytes);
        if bytes.is_empty() {
            return;
        }
        let first_word = offset / 2;
        let last_word = (end - 1) / 2;
        for word_index in first_word..=last_word {
            let low = self.prog_bytes[word_index * 2];
            let high = self.prog_bytes[word_index * 2 + 1];
            self.prog_mem[word_index] = u16::from_le_bytes([low, high]);
        }
    }

    pub fn pc22_bits(&self) -> bool {
        self.pc22_bits
    }

    pub fn add_clock_event(
        &mut self,
        callback: Box<dyn FnMut(&mut CpuState)>,
        delay_cycles: u64,
    ) -> ClockEventId {
        let id = ClockEventId(self.next_clock_event_id);
        self.next_clock_event_id += 1;
        self.clock_callbacks
            .push(Some(Rc::new(RefCell::new(callback))));
        let due = self.cycles.saturating_add(delay_cycles.max(1));
        self.insert_clock_event(due, id);
        id
    }

    pub fn update_clock_event(&mut self, id: ClockEventId, delay_cycles: u64) -> bool {
        let known = self
            .clock_callbacks
            .get(id.0 as usize)
            .is_some_and(Option::is_some);
        if !known {
            return false;
        }
        self.clock_events.retain(|event| event.id != id);
        let due = self.cycles.saturating_add(delay_cycles.max(1));
        self.insert_clock_event(due, id);
        true
    }

    pub fn clear_clock_event(&mut self, id: ClockEventId) -> bool {
        let previous_len = self.clock_events.len();
        self.clock_events.retain(|event| event.id != id);
        let had_entry = self.clock_events.len() != previous_len;
        let had_callback = self
            .clock_callbacks
            .get_mut(id.0 as usize)
            .is_some_and(|slot| slot.take().is_some());
        had_entry || had_callback
    }

    fn insert_clock_event(&mut self, due: u64, id: ClockEventId) {
        let index = self.clock_events.partition_point(|event| event.due < due);
        self.clock_events.insert(index, ClockEventEntry { due, id });
    }

    pub fn tick(&mut self) {
        if let Some(id) = self
            .clock_events
            .first()
            .filter(|event| event.due <= self.cycles)
            .map(|event| event.id)
        {
            self.clock_events.retain(|event| event.id != id);
            let callback = self
                .clock_callbacks
                .get(id.0 as usize)
                .and_then(Clone::clone);
            if let Some(callback) = callback {
                let mut callback = callback.borrow_mut();
                let function = &mut *callback;
                function(self);
            }
            if !self.clock_events.iter().any(|event| event.id == id) {
                if let Some(slot) = self.clock_callbacks.get_mut(id.0 as usize) {
                    *slot = None;
                }
            }
        }

        self.process_interrupt();
    }

    pub fn set_interrupt_flag(&mut self, interrupt: InterruptConfig) {
        let flag_register = interrupt.flag_register as usize;
        if interrupt.inverse_flag {
            self.data[flag_register] &= !interrupt.flag_mask;
        } else {
            self.data[flag_register] |= interrupt.flag_mask;
        }
        if self.data[interrupt.enable_register as usize] & interrupt.enable_mask != 0 {
            self.queue_interrupt(interrupt);
        }
    }

    pub fn update_interrupt_enable(&mut self, interrupt: InterruptConfig, register_value: u8) {
        if register_value & interrupt.enable_mask != 0 {
            let flag_set = self.data[interrupt.flag_register as usize] & interrupt.flag_mask != 0;
            if flag_set != interrupt.inverse_flag {
                self.queue_interrupt(interrupt);
            }
        } else {
            self.clear_interrupt_inner(interrupt, false);
        }
    }

    pub fn queue_interrupt(&mut self, interrupt: InterruptConfig) {
        let address = interrupt.address as usize;
        if address >= self.pending_interrupts.len() {
            self.pending_interrupts.resize(address + 1, None);
        }
        self.pending_interrupts[address] = Some(interrupt);
        if self.next_interrupt < 0 || self.next_interrupt > address as i16 {
            self.next_interrupt = address as i16;
        }
        if address as i16 > self.max_interrupt {
            self.max_interrupt = address as i16;
        }
    }

    pub fn clear_interrupt(&mut self, interrupt: InterruptConfig) {
        self.clear_interrupt_inner(interrupt, true);
    }

    fn clear_interrupt_inner(&mut self, interrupt: InterruptConfig, clear_flag: bool) {
        if clear_flag {
            self.data[interrupt.flag_register as usize] &= !interrupt.flag_mask;
        }
        let address = interrupt.address as usize;
        if !self
            .pending_interrupts
            .get(address)
            .is_some_and(Option::is_some)
        {
            return;
        }
        self.pending_interrupts[address] = None;
        if self.next_interrupt == address as i16 {
            self.next_interrupt = -1;
            for index in address + 1..=self.max_interrupt as usize {
                if self
                    .pending_interrupts
                    .get(index)
                    .is_some_and(Option::is_some)
                {
                    self.next_interrupt = index as i16;
                    break;
                }
            }
        }
    }

    pub fn clear_interrupt_by_flag(&mut self, interrupt: InterruptConfig, register_value: u8) {
        if register_value & interrupt.flag_mask != 0 {
            self.data[interrupt.flag_register as usize] &= !interrupt.flag_mask;
            self.clear_interrupt(interrupt);
        }
    }

    pub fn on_watchdog_reset(&mut self) {
        let mut callback = std::mem::replace(&mut self.on_watchdog_reset, Box::new(|_| {}));
        callback(self);
        self.on_watchdog_reset = callback;
    }

    fn process_interrupt(&mut self) {
        if !self.interrupts_enabled() || self.next_interrupt < 0 {
            return;
        }
        let address = self.next_interrupt as usize;
        let Some(interrupt) = self.pending_interrupts.get(address).copied().flatten() else {
            return;
        };
        avr_interrupt_state(self, interrupt.address);
        if !interrupt.constant {
            self.clear_interrupt(interrupt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Recorder;

    #[test]
    fn should_set_initial_value_of_sp_to_the_last_byte_of_internal_sram() {
        let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
        assert_eq!(cpu.sp(), 0x10ff);
    }

    mod events {
        use super::*;

        #[test]
        fn should_execute_queued_events_after_the_given_number_of_cycles_has_passed() {
            let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
            let events = Recorder::<(u64, u64)>::new();
            for delay in [1u64, 4, 10] {
                let recorder = events.clone();
                cpu.add_clock_event(
                    Box::new(move |state: &mut CpuState| {
                        recorder.push((delay, state.cycles()));
                    }),
                    delay,
                );
            }
            for _ in 0..10 {
                cpu.add_cycles(1);
                cpu.tick();
            }
            assert_eq!(events.calls(), vec![(1, 1), (4, 4), (10, 10)]);
        }

        #[test]
        fn should_correctly_sort_the_events_when_added_in_reverse_order() {
            let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
            let events = Recorder::<(u64, u64)>::new();
            for delay in [10u64, 4, 1] {
                let recorder = events.clone();
                cpu.add_clock_event(
                    Box::new(move |state: &mut CpuState| {
                        recorder.push((delay, state.cycles()));
                    }),
                    delay,
                );
            }
            for _ in 0..10 {
                cpu.add_cycles(1);
                cpu.tick();
            }
            assert_eq!(events.calls(), vec![(1, 1), (4, 4), (10, 10)]);
        }

        mod update_clock_event {
            use super::*;

            #[test]
            fn should_update_the_number_of_cycles_for_the_given_clock_event() {
                let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
                let events = Recorder::<(u64, u64)>::new();
                let mut callbacks = Vec::new();
                for delay in [1u64, 4, 10] {
                    let recorder = events.clone();
                    callbacks.push(cpu.add_clock_event(
                        Box::new(move |state: &mut CpuState| {
                            recorder.push((delay, state.cycles()));
                        }),
                        delay,
                    ));
                }
                cpu.update_clock_event(callbacks[1], 2);
                cpu.update_clock_event(callbacks[0], 12);
                for _ in 0..14 {
                    cpu.add_cycles(1);
                    cpu.tick();
                }
                assert_eq!(events.calls(), vec![(4, 2), (10, 10), (1, 12)]);
            }

            mod clear_clock_event {
                use super::*;

                #[test]
                fn should_remove_the_given_clock_event() {
                    let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
                    let events = Recorder::<(u64, u64)>::new();
                    let mut callbacks = Vec::new();
                    for delay in [1u64, 4, 10] {
                        let recorder = events.clone();
                        callbacks.push(cpu.add_clock_event(
                            Box::new(move |state: &mut CpuState| {
                                recorder.push((delay, state.cycles()));
                            }),
                            delay,
                        ));
                    }
                    cpu.clear_clock_event(callbacks[1]);
                    for _ in 0..10 {
                        cpu.add_cycles(1);
                        cpu.tick();
                    }
                    assert_eq!(events.calls(), vec![(1, 1), (10, 10)]);
                }

                #[test]
                fn should_return_false_if_the_provided_clock_event_is_not_scheduled() {
                    let cpu = Cpu::with_sram(vec![0; 1024], 0x1000);
                    let event4 = cpu.add_clock_event(Box::new(|_state: &mut CpuState| {}), 4);
                    let event10 = cpu.add_clock_event(Box::new(|_state: &mut CpuState| {}), 10);
                    cpu.add_clock_event(Box::new(|_state: &mut CpuState| {}), 1);

                    assert!(cpu.clear_clock_event(event4));
                    assert!(cpu.clear_clock_event(event10));
                    assert!(!cpu.clear_clock_event(event4));
                    assert!(!cpu.clear_clock_event(event10));
                }
            }
        }
    }
}
