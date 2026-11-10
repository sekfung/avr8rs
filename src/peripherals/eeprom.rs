use std::cell::RefCell;
use std::rc::Rc;

use crate::cpu::{Cpu, CpuState, InterruptConfig};

const EERE: u8 = 1 << 0;
const EEPE: u8 = 1 << 1;
const EEMPE: u8 = 1 << 2;
const EERIE: u8 = 1 << 3;
const EEPM0: u8 = 1 << 4;
const EEPM1: u8 = 1 << 5;
const EECR_WRITE_MASK: u8 = EEPE | EEMPE | EERIE | EEPM0 | EEPM1;

pub trait EepromBackend {
    fn read_memory(&mut self, addr: u16) -> u8;
    fn write_memory(&mut self, addr: u16, value: u8);
    fn erase_memory(&mut self, addr: u16);
}

pub struct EepromMemoryBackend {
    memory: Vec<u8>,
}

impl EepromMemoryBackend {
    pub fn new(size: u16) -> Self {
        Self {
            memory: vec![0xff; size as usize],
        }
    }

    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    pub fn memory_mut(&mut self) -> &mut [u8] {
        &mut self.memory
    }
}

impl EepromBackend for EepromMemoryBackend {
    fn read_memory(&mut self, addr: u16) -> u8 {
        self.memory.get(addr as usize).copied().unwrap_or(0xff)
    }

    fn write_memory(&mut self, addr: u16, value: u8) {
        if let Some(cell) = self.memory.get_mut(addr as usize) {
            *cell &= value;
        }
    }

    fn erase_memory(&mut self, addr: u16) {
        if let Some(cell) = self.memory.get_mut(addr as usize) {
            *cell = 0xff;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EepromConfig {
    pub eeprom_ready_interrupt: u8,
    pub eecr: u16,
    pub eedr: u16,
    pub eearl: u16,
    pub eearh: u16,
    /// The amount of clock cycles erase takes.
    pub erase_cycles: u32,
    /// The amount of clock cycles a write takes.
    pub write_cycles: u32,
}

pub fn eeprom_config() -> EepromConfig {
    EepromConfig {
        eeprom_ready_interrupt: 0x2c,
        eecr: 0x3f,
        eedr: 0x40,
        eearl: 0x41,
        eearh: 0x42,
        erase_cycles: 28800, // 1.8ms at 16MHz
        write_cycles: 28800, // 1.8ms at 16MHz
    }
}

struct EepromState {
    backend: Rc<RefCell<dyn EepromBackend>>,
    config: EepromConfig,
    /// Last cycle at which setting EEPE still triggers a write. From the datasheet:
    /// When EEMPE is set, setting EEPE within four clock cycles will write data to the
    /// EEPROM at the selected address. If EEMPE is zero, setting EEPE has no effect.
    write_enabled_cycles: u64,
    write_complete_cycles: u64,
    eer: InterruptConfig,
}

/// AVR8 EEPROM peripheral with a pluggable memory backend.
#[derive(Clone)]
pub struct Eeprom(Rc<RefCell<EepromState>>);

impl Eeprom {
    pub fn new(cpu: Cpu, backend: Rc<RefCell<dyn EepromBackend>>) -> Self {
        Self::with_config(cpu, backend, eeprom_config())
    }

    pub fn with_config(
        cpu: Cpu,
        backend: Rc<RefCell<dyn EepromBackend>>,
        config: EepromConfig,
    ) -> Self {
        let eer = InterruptConfig {
            address: config.eeprom_ready_interrupt,
            flag_register: config.eecr,
            flag_mask: EEPE,
            enable_register: config.eecr,
            enable_mask: EERIE,
            constant: true,
            inverse_flag: true,
        };
        let eeprom = Eeprom(Rc::new(RefCell::new(EepromState {
            backend,
            config,
            write_enabled_cycles: 0,
            write_complete_cycles: 0,
            eer,
        })));

        let writer = eeprom.clone();
        cpu.set_write_hook(
            config.eecr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                writer.handle_eecr_write(state, value)
            }),
        );

        eeprom
    }

    pub fn backend(&self) -> Rc<RefCell<dyn EepromBackend>> {
        self.0.borrow().backend.clone()
    }

    fn handle_eecr_write(&self, state: &mut CpuState, eecr: u8) -> bool {
        let (eecr_addr, eedr_addr, eearl_addr, eearh_addr) = {
            let eeprom = self.0.borrow();
            (
                eeprom.config.eecr,
                eeprom.config.eedr,
                eeprom.config.eearl,
                eeprom.config.eearh,
            )
        };

        let addr = ((state.data_byte(eearh_addr) as u16) << 8) | state.data_byte(eearl_addr) as u16;

        let current = state.data_byte(eecr_addr);
        state.set_data_byte(
            eecr_addr,
            (current & !EECR_WRITE_MASK) | (eecr & EECR_WRITE_MASK),
        );
        let eer = self.0.borrow().eer;
        state.update_interrupt_enable(eer, eecr);

        if eecr & EERE != 0 {
            state.clear_interrupt(eer);
        }

        if eecr & EEMPE != 0 {
            let eempe_cycles = 4;
            self.0.borrow_mut().write_enabled_cycles = state.cycles().saturating_add(eempe_cycles);
            state.add_clock_event(
                Box::new(move |state| {
                    let current = state.data_byte(eecr_addr);
                    state.set_data_byte(eecr_addr, current & !EEMPE);
                }),
                eempe_cycles,
            );
        }

        if eecr & EERE != 0 {
            let backend = self.0.borrow().backend.clone();
            let value = backend.borrow_mut().read_memory(addr);
            state.set_data_byte(eedr_addr, value);
            // When the EEPROM is read, the CPU is halted for four cycles before the
            // next instruction is executed.
            state.add_cycles(4);
            return true;
        }

        if eecr & EEPE != 0 {
            let cycles = state.cycles();
            let (write_enabled_cycles, write_complete_cycles, erase_cycles, write_cycles) = {
                let eeprom = self.0.borrow();
                (
                    eeprom.write_enabled_cycles,
                    eeprom.write_complete_cycles,
                    eeprom.config.erase_cycles,
                    eeprom.config.write_cycles,
                )
            };

            // If EEMPE is zero, setting EEPE will have no effect.
            if cycles >= write_enabled_cycles {
                let current = state.data_byte(eecr_addr);
                state.set_data_byte(eecr_addr, current & !EEPE);
                return true;
            }

            // Check for write-in-progress.
            if cycles < write_complete_cycles {
                return true;
            }

            let eedr = state.data_byte(eedr_addr);
            let mut write_complete = cycles;
            let backend = self.0.borrow().backend.clone();

            // Erase.
            if eecr & EEPM1 == 0 {
                backend.borrow_mut().erase_memory(addr);
                write_complete = write_complete.saturating_add(erase_cycles as u64);
            }
            // Write.
            if eecr & EEPM0 == 0 {
                backend.borrow_mut().write_memory(addr, eedr);
                write_complete = write_complete.saturating_add(write_cycles as u64);
            }

            self.0.borrow_mut().write_complete_cycles = write_complete;

            let current = state.data_byte(eecr_addr);
            state.set_data_byte(eecr_addr, current | EEPE);

            state.add_clock_event(
                Box::new(move |state| state.set_interrupt_flag(eer)),
                write_complete.saturating_sub(cycles),
            );

            // When EEPE has been set, the CPU is halted for two cycles before the
            // next instruction is executed.
            state.add_cycles(2);
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{asm_program, TestProgramRunner};

    const EECR: u16 = 0x3f;
    const EEDR: u16 = 0x40;
    const EEARL: u16 = 0x41;
    const EEARH: u16 = 0x42;
    const EERE: u8 = 1;
    const EEPE: u8 = 2;
    const EEMPE: u8 = 4;
    const EERIE: u8 = 8;

    fn setup() -> (Cpu, Rc<RefCell<EepromMemoryBackend>>, Eeprom) {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let backend = Rc::new(RefCell::new(EepromMemoryBackend::new(1024)));
        let eeprom = Eeprom::new(cpu.clone(), backend.clone());
        (cpu, backend, eeprom)
    }

    #[test]
    fn should_return_0xff_when_reading_from_an_empty_location() {
        let (cpu, _backend, _eeprom) = setup();
        cpu.write_data(EEARL, 0);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EERE);
        cpu.tick();
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.data_byte(EEDR), 0xff);
    }

    #[test]
    fn should_return_the_value_stored_at_the_given_eeprom_address() {
        let cpu = Cpu::new(vec![0; 0x1000]);
        let backend = Rc::new(RefCell::new(EepromMemoryBackend::new(1024)));
        backend.borrow_mut().memory_mut()[0x250] = 0x42;
        let _eeprom = Eeprom::new(cpu.clone(), backend);
        cpu.write_data(EEARL, 0x50);
        cpu.write_data(EEARH, 0x02);
        cpu.write_data(EECR, EERE);
        cpu.tick();
        assert_eq!(cpu.data_byte(EEDR), 0x42);
    }

    #[test]
    fn should_write_a_byte_to_the_given_eeprom_address() {
        let (cpu, backend, _eeprom) = setup();
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.write_data(EECR, EEPE);
        cpu.tick();
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(backend.borrow().memory()[15], 0x55);
        assert_eq!(cpu.data_byte(EECR) & EEPE, EEPE);
    }

    #[test]
    fn should_not_erase_the_memory_when_writing_if_eepm1_is_high() {
        let asm = asm_program(
            r#"
            ; register addresses
            _REPLACE TWSR, 0x1f
            _REPLACE EEARL, 0x21
            _REPLACE EEDR, 0x20
            _REPLACE EECR, 0x1f

            LDI r16, 0x55
            OUT EEDR, r16
            LDI r16, 9
            OUT EEARL, r16
            SBI EECR, 5     ; EECR |= EEPM1
            SBI EECR, 2     ; EECR |= EEMPE
            SBI EECR, 1     ; EECR |= EEPE
            "#,
        );

        let cpu = Cpu::new(asm.program);
        let backend = Rc::new(RefCell::new(EepromMemoryBackend::new(1024)));
        let _eeprom = Eeprom::new(cpu.clone(), backend.clone());
        backend.borrow_mut().memory_mut()[9] = 0x0f; // high four bits are cleared

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        // EEPROM was 0x0f, and our program wrote 0x55. Since write (without erase)
        // only clears bits, we expect 0x05 now.
        assert_eq!(backend.borrow().memory()[9], 0x05);
    }

    #[test]
    fn should_clear_the_eep_ee_bit_and_fire_an_interrupt_when_write_has_been_completed() {
        let (cpu, backend, _eeprom) = setup();
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.set_sreg(0x80); // SREG: I-------
        cpu.write_data(EECR, EEPE | EERIE);
        cpu.add_cycles(1000);
        cpu.tick();
        // At this point, write shouldn't be complete yet.
        assert_eq!(cpu.data_byte(EECR) & EEPE, EEPE);
        assert_eq!(cpu.pc(), 0);
        cpu.add_cycles(10_000_000);
        // And now, 10 million cycles later, it should.
        cpu.tick();
        assert_eq!(backend.borrow().memory()[15], 0x55);
        assert_eq!(cpu.data_byte(EECR) & EEPE, 0);
        assert_eq!(cpu.pc(), 0x2c); // EEPROM Ready interrupt
    }

    #[test]
    fn should_fire_an_interrupt_when_there_is_a_pending_interrupt_and_the_flag_is_enabled() {
        let (cpu, backend, _eeprom) = setup();
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.set_sreg(0x80); // SREG: I-------
        cpu.write_data(EECR, EEPE);
        cpu.add_cycles(1000);
        cpu.tick();
        // At this point, write shouldn't be complete yet.
        assert_eq!(cpu.data_byte(EECR) & EEPE, EEPE);
        assert_eq!(cpu.pc(), 0);
        cpu.add_cycles(10_000_000);
        // And now, 10 million cycles later, it should.
        cpu.tick();
        assert_eq!(backend.borrow().memory()[15], 0x55);
        assert_eq!(cpu.data_byte(EECR) & EEPE, 0);
        cpu.write_data(EECR, EERIE);
        cpu.tick();
        assert_eq!(cpu.pc(), 0x2c); // EEPROM Ready interrupt
    }

    #[test]
    fn should_skip_the_write_if_eempe_is_clear() {
        let (cpu, backend, _eeprom) = setup();
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.set_cycles(8); // waiting for more than 4 cycles should clear EEMPE
        cpu.tick();
        cpu.write_data(EECR, EEPE);
        cpu.tick();
        // Ensure that nothing was written, and EEPE bit is clear.
        assert_eq!(cpu.cycles(), 8);
        assert_eq!(backend.borrow().memory()[15], 0xff);
        assert_eq!(cpu.data_byte(EECR) & EEPE, 0);
    }

    #[test]
    fn should_skip_the_write_if_another_write_is_already_in_progress() {
        let (cpu, backend, _eeprom) = setup();

        // Write 0x55 to address 15.
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.write_data(EECR, EEPE);
        cpu.tick();
        assert_eq!(cpu.cycles(), 2);

        // Write 0x66 to address 16 (first write is still in progress).
        cpu.write_data(EEDR, 0x66);
        cpu.write_data(EEARL, 16);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.write_data(EECR, EEPE);
        cpu.tick();

        // Ensure that second write didn't happen.
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(backend.borrow().memory()[15], 0x55);
        assert_eq!(backend.borrow().memory()[16], 0xff);
    }

    #[test]
    fn should_write_two_bytes_successfully() {
        let (cpu, backend, _eeprom) = setup();

        // Write 0x55 to address 15.
        cpu.write_data(EEDR, 0x55);
        cpu.write_data(EEARL, 15);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.write_data(EECR, EEPE);
        cpu.tick();
        assert_eq!(cpu.cycles(), 2);

        // Wait long enough time for the first write to finish.
        cpu.add_cycles(10_000_000);
        cpu.tick();

        // Write 0x66 to address 16.
        cpu.write_data(EEDR, 0x66);
        cpu.write_data(EEARL, 16);
        cpu.write_data(EEARH, 0);
        cpu.write_data(EECR, EEMPE);
        cpu.write_data(EECR, EEPE);
        cpu.tick();

        // Ensure both writes took place.
        assert_eq!(cpu.cycles(), 10_000_004);
        assert_eq!(backend.borrow().memory()[15], 0x55);
        assert_eq!(backend.borrow().memory()[16], 0x66);
    }

    #[test]
    fn should_only_erase_the_memory_when_eepm0_is_high() {
        let asm = asm_program(
            r#"
            ; register addresses
            _REPLACE EEARL, 0x21
            _REPLACE EEDR, 0x20
            _REPLACE EECR, 0x1f

            LDI r16, 0x55
            OUT EEDR, r16
            LDI r16, 9
            OUT EEARL, r16
            SBI EECR, 4     ; EECR |= EEPM0
            SBI EECR, 2     ; EECR |= EEMPE
            SBI EECR, 1     ; EECR |= EEPE
            "#,
        );

        let cpu = Cpu::new(asm.program);
        let backend = Rc::new(RefCell::new(EepromMemoryBackend::new(1024)));
        let _eeprom = Eeprom::new(cpu.clone(), backend.clone());
        backend.borrow_mut().memory_mut()[9] = 0x22;

        let runner = TestProgramRunner::new(cpu.clone());
        runner.run_instructions(asm.instruction_count);

        assert_eq!(backend.borrow().memory()[9], 0xff);
    }
}
