use super::cpu::{Cpu, CpuState};

pub(crate) fn avr_interrupt_state(state: &mut CpuState, addr: u8) {
    let sp = state.sp();
    let pc = state.pc();
    let pc22_bits = state.pc22_bits();

    state.set_data_byte(sp, (pc & 0xff) as u8);
    state.set_data_byte(sp - 1, ((pc >> 8) & 0xff) as u8);
    if pc22_bits {
        state.set_data_byte(sp - 2, ((pc >> 16) & 0xff) as u8);
    }
    state.set_sp(sp - if pc22_bits { 3 } else { 2 });
    state.set_sreg(state.sreg() & 0x7f);
    state.add_cycles(2);
    state.set_pc(addr as u32);
}

pub fn avr_interrupt(cpu: &Cpu, addr: u8) {
    let mut state = cpu.borrow_state_mut();
    avr_interrupt_state(&mut state, addr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;

    #[test]
    fn should_execute_interrupt_handler() {
        let cpu = Cpu::new(vec![0; 0x8000]);
        cpu.set_pc(0x520);
        cpu.set_data_byte(94, 0);
        cpu.set_data_byte(93, 0x80);
        cpu.set_data_byte(95, 0b1000_0001);
        avr_interrupt(&cpu, 5);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.pc(), 5);
        assert_eq!(cpu.data_byte(93), 0x7e);
        assert_eq!(cpu.data_byte(0x80), 0x20);
        assert_eq!(cpu.data_byte(0x7f), 0x05);
        assert_eq!(cpu.data_byte(95), 0b0000_0001);
    }

    #[test]
    fn should_push_a_3_byte_return_address_when_running_in_22_bit_pc_mode() {
        let cpu = Cpu::new(vec![0; 0x80000]);
        assert!(cpu.pc22_bits());

        cpu.set_pc(0x10520);
        cpu.set_data_byte(94, 0);
        cpu.set_data_byte(93, 0x80);
        cpu.set_data_byte(95, 0b1000_0001);

        avr_interrupt(&cpu, 5);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.pc(), 5);
        assert_eq!(cpu.data_byte(93), 0x7d);
        assert_eq!(cpu.data_byte(0x80), 0x20);
        assert_eq!(cpu.data_byte(0x7f), 0x05);
        assert_eq!(cpu.data_byte(0x7e), 0x01);
        assert_eq!(cpu.data_byte(95), 0b0000_0001);
    }
}
