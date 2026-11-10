use super::cpu::{Cpu, CpuState};

pub fn avr_instruction(cpu: &Cpu) {
    let mut state = cpu.borrow_state_mut();
    avr_instruction_state(&mut state);
}

pub(crate) fn avr_instruction_state(state: &mut CpuState) {
    let opcode = state.prog_word(state.pc());
    execute(state, opcode);
    let program_len = (state.prog_bytes().len() / 2) as u32;
    state.set_pc(state.pc().wrapping_add(1) % program_len.max(1));
    state.add_cycles(1);
}

fn reg_d(opcode: u16) -> u16 {
    (opcode & 0x1f0) >> 4
}

fn reg_r(opcode: u16) -> u16 {
    (opcode & 0x0f) | ((opcode & 0x200) >> 5)
}

fn data(state: &CpuState, addr: u16) -> u8 {
    state.data_byte(addr)
}

fn set_data(state: &mut CpuState, addr: u16, value: u8) {
    state.set_data_byte(addr, value);
}

fn is_two_word_instruction(opcode: u16) -> bool {
    (opcode & 0xfe0f) == 0x9000
        || (opcode & 0xfe0f) == 0x9200
        || (opcode & 0xfe0e) == 0x940e
        || (opcode & 0xfe0e) == 0x940c
}

fn push_return_address(state: &mut CpuState, ret: u32, bytes: u16) {
    let sp = state.sp();
    state.set_data_byte(sp, ret as u8);
    state.set_data_byte(sp.wrapping_sub(1), (ret >> 8) as u8);
    if bytes == 3 {
        state.set_data_byte(sp.wrapping_sub(2), (ret >> 16) as u8);
    }
    state.set_sp(sp.wrapping_sub(bytes));
}

fn displacement(opcode: u16) -> u16 {
    (opcode & 0x07) | ((opcode & 0xc00) >> 7) | ((opcode & 0x2000) >> 8)
}

fn set_logic_flags(state: &mut CpuState, result: u8) {
    let mut sreg = state.sreg() & 0xe1;
    if result == 0 {
        sreg |= 0x02;
    }
    if result & 0x80 != 0 {
        sreg |= 0x04;
    }
    if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
        sreg |= 0x10;
    }
    state.set_sreg(sreg);
}

fn set_sub_flags(state: &mut CpuState, val1: u8, val2: u8, result: i32) {
    let mut sreg = state.sreg() & 0xc0;
    if result == 0 {
        sreg |= 0x02;
    }
    if result & 0x80 != 0 {
        sreg |= 0x04;
    }
    if ((val1 as i32 ^ val2 as i32) & (val1 as i32 ^ result) & 0x80) != 0 {
        sreg |= 0x08;
    }
    if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
        sreg |= 0x10;
    }
    if val2 > val1 {
        sreg |= 0x01;
    }
    let half = (!(val1 as i32) & val2 as i32) | (val2 as i32 & result) | (result & !(val1 as i32));
    if half & 1 != 0 {
        sreg |= 0x20;
    }
    state.set_sreg(sreg);
}

fn set_sub_carry_flags(state: &mut CpuState, val1: u8, val2: u8, result: i32, old_sreg: u8) {
    let mut sreg = (old_sreg & 0xc0)
        | if result == 0 && (old_sreg >> 1) & 1 != 0 {
            0x02
        } else {
            0
        }
        | if val2 as i32 + (old_sreg & 1) as i32 > val1 as i32 {
            0x01
        } else {
            0
        };
    if result & 0x80 != 0 {
        sreg |= 0x04;
    }
    if ((val1 as i32 ^ val2 as i32) & (val1 as i32 ^ result) & 0x80) != 0 {
        sreg |= 0x08;
    }
    if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
        sreg |= 0x10;
    }
    let half = (!(val1 as i32) & val2 as i32) | (val2 as i32 & result) | (result & !(val1 as i32));
    if half & 1 != 0 {
        sreg |= 0x20;
    }
    state.set_sreg(sreg);
}

fn execute(state: &mut CpuState, opcode: u16) {
    if (opcode & 0xfc00) == 0x1c00 {
        /* ADC, 0001 11rd dddd rrrr */
        let d_addr = reg_d(opcode);
        let r_addr = reg_r(opcode);
        let d = data(state, d_addr) as u32;
        let r = data(state, r_addr) as u32;
        let sum = d + r + (state.sreg() & 1) as u32;
        let result = (sum & 0xff) as u8;
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xc0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if ((result ^ (r as u8)) & ((d as u8) ^ result) & 0x80) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        if sum & 0x100 != 0 {
            sreg |= 0x01;
        }
        let half = (d & r) | (r & !(result as u32)) | (!(result as u32) & d);
        if half & 1 != 0 {
            sreg |= 0x20;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xfc00) == 0x0c00 {
        /* ADD, 0000 11rd dddd rrrr */
        let d_addr = reg_d(opcode);
        let r_addr = reg_r(opcode);
        let d = data(state, d_addr) as u32;
        let r = data(state, r_addr) as u32;
        let sum = d + r;
        let result = (sum & 0xff) as u8;
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xc0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if ((result ^ (r as u8)) & ((d as u8) ^ result) & 0x80) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        if sum & 0x100 != 0 {
            sreg |= 0x01;
        }
        let half = (d & r) | (r & !(result as u32)) | (!(result as u32) & d);
        if half & 1 != 0 {
            sreg |= 0x20;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xff00) == 0x9600 {
        /* ADIW, 1001 0110 KKdd KKKK */
        let addr = 2 * ((opcode & 0x30) >> 4) + 24;
        let value = state.read_u16(addr);
        let k = (opcode & 0x0f) | ((opcode & 0xc0) >> 2);
        let result = value.wrapping_add(k);
        state.write_u16(addr, result);
        let mut sreg = state.sreg() & 0xe0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x8000 != 0 {
            sreg |= 0x04;
        }
        if (!(value as u32) & (result as u32) & 0x8000) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        if (!(result as u32) & (value as u32) & 0x8000) != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xfc00) == 0x2000 {
        /* AND, 0010 00rd dddd rrrr */
        let result = data(state, reg_d(opcode)) & data(state, reg_r(opcode));
        set_data(state, reg_d(opcode), result);
        set_logic_flags(state, result);
    } else if (opcode & 0xf000) == 0x7000 {
        /* ANDI, 0111 KKKK dddd KKKK */
        let d_addr = 16 + ((opcode & 0xf0) >> 4);
        let k = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        let result = data(state, d_addr) & k;
        set_data(state, d_addr, result);
        set_logic_flags(state, result);
    } else if (opcode & 0xfe0f) == 0x9405 {
        /* ASR, 1001 010d dddd 0101 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let result = (value >> 1) | (value & 0x80);
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xe0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if value & 1 != 0 {
            sreg |= 0x01;
        }
        if (((sreg >> 2) & 1) ^ (sreg & 1)) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xff8f) == 0x9488 {
        /* BCLR, 1001 0100 1sss 1000 */
        let bit = ((opcode & 0x70) >> 4) as u8;
        state.set_sreg(state.sreg() & !(1u8 << bit));
    } else if (opcode & 0xfe08) == 0xf800 {
        /* BLD, 1111 100d dddd 0bbb */
        let b = (opcode & 7) as u8;
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let bit = (state.sreg() >> 6) & 1;
        set_data(state, d_addr, (!(1u8 << b) & value) | (bit << b));
    } else if (opcode & 0xfc00) == 0xf400 {
        /* BRBC, 1111 01kk kkkk ksss */
        let bit = (opcode & 7) as u8;
        if state.sreg() & (1u8 << bit) == 0 {
            let offset =
                ((opcode & 0x1f8) >> 3) as i64 - if opcode & 0x200 != 0 { 0x40 } else { 0 };
            state.set_pc((state.pc() as i64 + offset) as u32);
            state.add_cycles(1);
        }
    } else if (opcode & 0xfc00) == 0xf000 {
        /* BRBS, 1111 00kk kkkk ksss */
        let bit = (opcode & 7) as u8;
        if state.sreg() & (1u8 << bit) != 0 {
            let offset =
                ((opcode & 0x1f8) >> 3) as i64 - if opcode & 0x200 != 0 { 0x40 } else { 0 };
            state.set_pc((state.pc() as i64 + offset) as u32);
            state.add_cycles(1);
        }
    } else if (opcode & 0xff8f) == 0x9408 {
        /* BSET, 1001 0100 0sss 1000 */
        let bit = ((opcode & 0x70) >> 4) as u8;
        state.set_sreg(state.sreg() | (1u8 << bit));
    } else if (opcode & 0xfe08) == 0xfa00 {
        /* BST, 1111 101d dddd 0bbb */
        let value = data(state, reg_d(opcode));
        let b = (opcode & 7) as u8;
        let bit = if (value >> b) & 1 != 0 { 0x40 } else { 0 };
        state.set_sreg((state.sreg() & 0xbf) | bit);
    } else if (opcode & 0xfe0e) == 0x940e {
        /* CALL, 1001 010k kkkk 111k kkkk kkkk kkkk kkkk */
        let k = (state.prog_word(state.pc() + 1) as u32)
            | ((opcode & 1) as u32) << 16
            | ((opcode & 0x1f0) as u32) << 13;
        let ret = state.pc() + 2;
        let bytes = if state.pc22_bits() { 3 } else { 2 };
        push_return_address(state, ret, bytes);
        state.set_pc(k.wrapping_sub(1));
        state.add_cycles(bytes as u64 + 1);
    } else if (opcode & 0xff00) == 0x9800 {
        /* CBI, 1001 1000 AAAA Abbb */
        let a = opcode & 0xf8;
        let b = (opcode & 7) as u8;
        let addr = (a >> 3) + 32;
        let value = state.read_data(addr);
        let mask = 1u8 << b;
        state.write_data_masked(addr, value & !mask, mask);
    } else if (opcode & 0xfe0f) == 0x9400 {
        /* COM, 1001 010d dddd 0000 */
        let d_addr = reg_d(opcode);
        let result = 255 - data(state, d_addr);
        set_data(state, d_addr, result);
        let mut sreg = (state.sreg() & 0xe1) | 0x01;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xfc00) == 0x1400 {
        /* CP, 0001 01rd dddd rrrr */
        let val1 = data(state, reg_d(opcode));
        let val2 = data(state, reg_r(opcode));
        let result = val1 as i32 - val2 as i32;
        set_sub_flags(state, val1, val2, result);
    } else if (opcode & 0xfc00) == 0x0400 {
        /* CPC, 0000 01rd dddd rrrr */
        let old_sreg = state.sreg();
        let val1 = data(state, reg_d(opcode));
        let val2 = data(state, reg_r(opcode));
        let result = val1 as i32 - val2 as i32 - (old_sreg & 1) as i32;
        set_sub_carry_flags(state, val1, val2, result, old_sreg);
    } else if (opcode & 0xf000) == 0x3000 {
        /* CPI, 0011 KKKK dddd KKKK */
        let d_addr = 16 + ((opcode & 0xf0) >> 4);
        let val1 = data(state, d_addr);
        let val2 = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        let result = val1 as i32 - val2 as i32;
        set_sub_flags(state, val1, val2, result);
    } else if (opcode & 0xfc00) == 0x1000 {
        /* CPSE, 0001 00rd dddd rrrr */
        if data(state, reg_d(opcode)) == data(state, reg_r(opcode)) {
            let next_opcode = state.prog_word(state.pc() + 1);
            let skip = if is_two_word_instruction(next_opcode) {
                2
            } else {
                1
            };
            state.set_pc(state.pc() + skip);
            state.add_cycles(skip as u64);
        }
    } else if (opcode & 0xfe0f) == 0x940a {
        /* DEC, 1001 010d dddd 1010 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let result = value as i32 - 1;
        set_data(state, d_addr, result as u8);
        let mut sreg = state.sreg() & 0xe1;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if value == 0x80 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if opcode == 0x9519 {
        /* EICALL, 1001 0101 0001 1001 */
        let ret = state.pc() + 1;
        push_return_address(state, ret, 3);
        let eind = state.data_byte(0x5c) as u32;
        let z = state.read_u16(30) as u32;
        state.set_pc(((eind << 16) | z).wrapping_sub(1));
        state.add_cycles(3);
    } else if opcode == 0x9419 {
        /* EIJMP, 1001 0100 0001 1001 */
        let eind = state.data_byte(0x5c) as u32;
        let z = state.read_u16(30) as u32;
        state.set_pc(((eind << 16) | z).wrapping_sub(1));
        state.add_cycles(1);
    } else if opcode == 0x95d8 {
        /* ELPM, 1001 0101 1101 1000 */
        let rampz = state.data_byte(0x5b) as u32;
        let z = state.read_u16(30) as u32;
        let value = state.prog_byte((rampz << 16) | z);
        set_data(state, 0, value);
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9006 {
        /* ELPM(REG), 1001 000d dddd 0110 */
        let rampz = state.data_byte(0x5b) as u32;
        let z = state.read_u16(30) as u32;
        let value = state.prog_byte((rampz << 16) | z);
        set_data(state, reg_d(opcode), value);
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9007 {
        /* ELPM(INC), 1001 000d dddd 0111 */
        let rampz = state.data_byte(0x5b);
        let z = state.read_u16(30);
        let value = state.prog_byte(((rampz as u32) << 16) | z as u32);
        set_data(state, reg_d(opcode), value);
        state.write_u16(30, z.wrapping_add(1));
        if z == 0xffff {
            let pages = (state.prog_bytes().len() >> 16).max(1);
            state.set_data_byte(0x5b, ((rampz as usize + 1) % pages) as u8);
        }
        state.add_cycles(2);
    } else if (opcode & 0xfc00) == 0x2400 {
        /* EOR, 0010 01rd dddd rrrr */
        let result = data(state, reg_d(opcode)) ^ data(state, reg_r(opcode));
        set_data(state, reg_d(opcode), result);
        set_logic_flags(state, result);
    } else if (opcode & 0xff88) == 0x0308 {
        /* FMUL, 0000 0011 0ddd 1rrr */
        let v1 = data(state, 16 + ((opcode & 0x70) >> 4)) as u32;
        let v2 = data(state, 16 + (opcode & 0x07)) as u32;
        let product = v1 * v2;
        let result = (product << 1) as u16;
        state.write_u16(0, result);
        let mut sreg = state.sreg() & 0xfc;
        if result == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xff88) == 0x0380 {
        /* FMULS, 0000 0011 1ddd 0rrr */
        let v1 = data(state, 16 + ((opcode & 0x70) >> 4)) as i8 as i32;
        let v2 = data(state, 16 + (opcode & 0x07)) as i8 as i32;
        let product = v1 * v2;
        let result = (product << 1) as u16;
        state.write_u16(0, result);
        let mut sreg = state.sreg() & 0xfc;
        if result == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xff88) == 0x0388 {
        /* FMULSU, 0000 0011 1ddd 1rrr */
        let v1 = data(state, 16 + ((opcode & 0x70) >> 4)) as i8 as i32;
        let v2 = data(state, 16 + (opcode & 0x07)) as i32;
        let product = v1 * v2;
        let result = (product << 1) as u16;
        state.write_u16(0, result);
        let mut sreg = state.sreg() & 0xfc;
        if result == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if opcode == 0x9509 {
        /* ICALL, 1001 0101 0000 1001 */
        let return_address = state.pc() + 1;
        let bytes = if state.pc22_bits() { 3 } else { 2 };
        push_return_address(state, return_address, bytes);
        let z = state.read_u16(30) as u32;
        state.set_pc(z.wrapping_sub(1));
        state.add_cycles(bytes as u64);
    } else if opcode == 0x9409 {
        /* IJMP, 1001 0100 0000 1001 */
        let z = state.read_u16(30) as u32;
        state.set_pc(z.wrapping_sub(1));
        state.add_cycles(1);
    } else if (opcode & 0xf800) == 0xb000 {
        /* IN, 1011 0AAd dddd AAAA */
        let addr = ((opcode & 0x0f) | ((opcode & 0x600) >> 5)) + 32;
        let value = state.read_data(addr);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x9403 {
        /* INC, 1001 010d dddd 0011 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let result = value.wrapping_add(1);
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xe1;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if value == 0x7f {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xfe0e) == 0x940c {
        /* JMP, 1001 010k kkkk 110k kkkk kkkk kkkk kkkk */
        let high = state.prog_word(state.pc() + 1) as u32;
        let k = high | (((opcode & 1) as u32) << 16) | (((opcode & 0x1f0) as u32) << 13);
        state.set_pc(k.wrapping_sub(1));
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9206 {
        /* LAC, 1001 001r rrrr 0110 */
        let r_addr = reg_d(opcode);
        let clear = data(state, r_addr);
        let z = state.read_u16(30);
        let value = state.read_data(z);
        state.write_data(z, value & (255 - clear));
        set_data(state, r_addr, value);
    } else if (opcode & 0xfe0f) == 0x9205 {
        /* LAS, 1001 001r rrrr 0101 */
        let r_addr = reg_d(opcode);
        let set = data(state, r_addr);
        let z = state.read_u16(30);
        let value = state.read_data(z);
        state.write_data(z, value | set);
        set_data(state, r_addr, value);
    } else if (opcode & 0xfe0f) == 0x9207 {
        /* LAT, 1001 001r rrrr 0111 */
        let r_addr = reg_d(opcode);
        let r = data(state, r_addr);
        let z = state.read_u16(30);
        let value = state.read_data(z);
        state.write_data(z, r ^ value);
        set_data(state, r_addr, value);
    } else if (opcode & 0xf000) == 0xe000 {
        /* LDI, 1110 KKKK dddd KKKK */
        let k = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        set_data(state, 16 + ((opcode & 0xf0) >> 4), k);
    } else if (opcode & 0xfe0f) == 0x9000 {
        /* LDS, 1001 000d dddd 0000 kkkk kkkk kkkk kkkk */
        state.add_cycles(1);
        let addr = state.prog_word(state.pc() + 1);
        let value = state.read_data(addr);
        set_data(state, reg_d(opcode), value);
        state.set_pc(state.pc() + 1);
    } else if (opcode & 0xfe0f) == 0x900c {
        /* LDX, 1001 000d dddd 1100 */
        state.add_cycles(1);
        let x = state.read_u16(26);
        let value = state.read_data(x);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x900d {
        /* LDX(INC), 1001 000d dddd 1101 */
        let x = state.read_u16(26);
        state.add_cycles(1);
        let value = state.read_data(x);
        set_data(state, reg_d(opcode), value);
        state.write_u16(26, x.wrapping_add(1));
    } else if (opcode & 0xfe0f) == 0x900e {
        /* LDX(DEC), 1001 000d dddd 1110 */
        let x = state.read_u16(26).wrapping_sub(1);
        state.write_u16(26, x);
        state.add_cycles(1);
        let value = state.read_data(x);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x8008 {
        /* LDY, 1000 000d dddd 1000 */
        state.add_cycles(1);
        let y = state.read_u16(28);
        let value = state.read_data(y);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x9009 {
        /* LDY(INC), 1001 000d dddd 1001 */
        let y = state.read_u16(28);
        state.add_cycles(1);
        let value = state.read_data(y);
        set_data(state, reg_d(opcode), value);
        state.write_u16(28, y.wrapping_add(1));
    } else if (opcode & 0xfe0f) == 0x900a {
        /* LDY(DEC), 1001 000d dddd 1010 */
        let y = state.read_u16(28).wrapping_sub(1);
        state.write_u16(28, y);
        state.add_cycles(1);
        let value = state.read_data(y);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xd208) == 0x8008 && displacement(opcode) != 0 {
        /* LDDY, 10q0 qq0d dddd 1qqq */
        state.add_cycles(1);
        let y = state.read_u16(28);
        let value = state.read_data(y.wrapping_add(displacement(opcode)));
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x8000 {
        /* LDZ, 1000 000d dddd 0000 */
        state.add_cycles(1);
        let z = state.read_u16(30);
        let value = state.read_data(z);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xfe0f) == 0x9001 {
        /* LDZ(INC), 1001 000d dddd 0001 */
        let z = state.read_u16(30);
        state.add_cycles(1);
        let value = state.read_data(z);
        set_data(state, reg_d(opcode), value);
        state.write_u16(30, z.wrapping_add(1));
    } else if (opcode & 0xfe0f) == 0x9002 {
        /* LDZ(DEC), 1001 000d dddd 0010 */
        let z = state.read_u16(30).wrapping_sub(1);
        state.write_u16(30, z);
        state.add_cycles(1);
        let value = state.read_data(z);
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xd208) == 0x8000 && displacement(opcode) != 0 {
        /* LDDZ, 10q0 qq0d dddd 0qqq */
        state.add_cycles(1);
        let z = state.read_u16(30);
        let value = state.read_data(z.wrapping_add(displacement(opcode)));
        set_data(state, reg_d(opcode), value);
    } else if opcode == 0x95c8 {
        /* LPM, 1001 0101 1100 1000 */
        let z = state.read_u16(30);
        let value = state.prog_byte(z as u32);
        state.set_data_byte(0, value);
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9004 {
        /* LPM(REG), 1001 000d dddd 0100 */
        let z = state.read_u16(30);
        let value = state.prog_byte(z as u32);
        set_data(state, reg_d(opcode), value);
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9005 {
        /* LPM(INC), 1001 000d dddd 0101 */
        let z = state.read_u16(30);
        let value = state.prog_byte(z as u32);
        set_data(state, reg_d(opcode), value);
        state.write_u16(30, z.wrapping_add(1));
        state.add_cycles(2);
    } else if (opcode & 0xfe0f) == 0x9406 {
        /* LSR, 1001 010d dddd 0110 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let result = value >> 1;
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xe0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if value & 1 != 0 {
            sreg |= 0x01;
        }
        if (((sreg >> 2) & 1) ^ (sreg & 1)) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xfc00) == 0x2c00 {
        /* MOV, 0010 11rd dddd rrrr */
        let value = data(state, reg_r(opcode));
        set_data(state, reg_d(opcode), value);
    } else if (opcode & 0xff00) == 0x0100 {
        /* MOVW, 0000 0001 dddd rrrr */
        let r2 = 2 * (opcode & 0x0f);
        let d2 = 2 * ((opcode & 0xf0) >> 4);
        let low = data(state, r2);
        let high = data(state, r2 + 1);
        set_data(state, d2, low);
        set_data(state, d2 + 1, high);
    } else if (opcode & 0xfc00) == 0x9c00 {
        /* MUL, 1001 11rd dddd rrrr */
        let product = data(state, reg_d(opcode)) as u16 * data(state, reg_r(opcode)) as u16;
        state.write_u16(0, product);
        let mut sreg = state.sreg() & 0xfc;
        if product == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xff00) == 0x0200 {
        /* MULS, 0000 0010 dddd rrrr */
        let v1 = data(state, 16 + ((opcode & 0xf0) >> 4)) as i8 as i32;
        let v2 = data(state, 16 + (opcode & 0x0f)) as i8 as i32;
        let product = v1 * v2;
        state.write_u16(0, product as u16);
        let mut sreg = state.sreg() & 0xfc;
        if product & 0xffff == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xff88) == 0x0300 {
        /* MULSU, 0000 0011 0ddd 0rrr */
        let v1 = data(state, 16 + ((opcode & 0x70) >> 4)) as i8 as i32;
        let v2 = data(state, 16 + (opcode & 0x07)) as i32;
        let product = v1 * v2;
        state.write_u16(0, product as u16);
        let mut sreg = state.sreg() & 0xfc;
        if product & 0xffff == 0 {
            sreg |= 0x02;
        }
        if product & 0x8000 != 0 {
            sreg |= 0x01;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x9401 {
        /* NEG, 1001 010d dddd 0001 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr) as i32;
        let result = -value;
        set_data(state, d_addr, result as u8);
        let mut sreg = state.sreg() & 0xc0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if result == 128 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        if result != 0 {
            sreg |= 0x01;
        }
        if (result | value) & 1 != 0 {
            sreg |= 0x20;
        }
        state.set_sreg(sreg);
    } else if opcode == 0 {
        /* NOP, 0000 0000 0000 0000 */
    } else if (opcode & 0xfc00) == 0x2800 {
        /* OR, 0010 10rd dddd rrrr */
        let result = data(state, reg_d(opcode)) | data(state, reg_r(opcode));
        set_data(state, reg_d(opcode), result);
        set_logic_flags(state, result);
    } else if (opcode & 0xf000) == 0x6000 {
        /* SBR, 0110 KKKK dddd KKKK */
        let d_addr = 16 + ((opcode & 0xf0) >> 4);
        let k = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        let result = data(state, d_addr) | k;
        set_data(state, d_addr, result);
        set_logic_flags(state, result);
    } else if (opcode & 0xf800) == 0xb800 {
        /* OUT, 1011 1AAr rrrr AAAA */
        let addr = ((opcode & 0x0f) | ((opcode & 0x600) >> 5)) + 32;
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
    } else if (opcode & 0xfe0f) == 0x900f {
        /* POP, 1001 000d dddd 1111 */
        let value = state.read_u16(93).wrapping_add(1);
        state.write_u16(93, value);
        let byte = state.data_byte(value);
        set_data(state, reg_d(opcode), byte);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x920f {
        /* PUSH, 1001 001d dddd 1111 */
        let value = state.sp();
        let byte = data(state, reg_d(opcode));
        state.set_data_byte(value, byte);
        state.set_sp(value.wrapping_sub(1));
        state.add_cycles(1);
    } else if (opcode & 0xf000) == 0xd000 {
        /* RCALL, 1101 kkkk kkkk kkkk */
        let offset = ((opcode & 0x7ff) as i64) - if opcode & 0x800 != 0 { 0x800 } else { 0 };
        let return_address = state.pc() + 1;
        let bytes = if state.pc22_bits() { 3 } else { 2 };
        push_return_address(state, return_address, bytes);
        state.set_pc((state.pc() as i64 + offset) as u32);
        state.add_cycles(bytes as u64);
    } else if opcode == 0x9508 {
        /* RET, 1001 0101 0000 1000 */
        let bytes: u16 = if state.pc22_bits() { 3 } else { 2 };
        let sp = state.sp();
        let i = sp.wrapping_add(bytes);
        state.set_sp(i);
        let low = state.data_byte(i) as u32;
        let high = state.data_byte(i.wrapping_sub(1)) as u32;
        let mut pc = ((high << 8) + low).wrapping_sub(1);
        if state.pc22_bits() {
            pc |= (state.data_byte(i.wrapping_sub(2)) as u32) << 16;
        }
        state.set_pc(pc);
        state.add_cycles(bytes as u64 + 1);
    } else if opcode == 0x9518 {
        /* RETI, 1001 0101 0001 1000 */
        let bytes: u16 = if state.pc22_bits() { 3 } else { 2 };
        let sp = state.sp();
        let i = sp.wrapping_add(bytes);
        state.set_sp(i);
        let low = state.data_byte(i) as u32;
        let high = state.data_byte(i.wrapping_sub(1)) as u32;
        let mut pc = ((high << 8) + low).wrapping_sub(1);
        if state.pc22_bits() {
            pc |= (state.data_byte(i.wrapping_sub(2)) as u32) << 16;
        }
        state.set_pc(pc);
        state.add_cycles(bytes as u64 + 1);
        state.set_sreg(state.sreg() | 0x80);
    } else if (opcode & 0xf000) == 0xc000 {
        /* RJMP, 1100 kkkk kkkk kkkk */
        let offset = ((opcode & 0x7ff) as i64) - if opcode & 0x800 != 0 { 0x800 } else { 0 };
        state.set_pc((state.pc() as i64 + offset) as u32);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x9407 {
        /* ROR, 1001 010d dddd 0111 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        let result = (value >> 1) | ((state.sreg() & 1) << 7);
        set_data(state, d_addr, result);
        let mut sreg = state.sreg() & 0xe0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x80 != 0 {
            sreg |= 0x04;
        }
        if value & 1 != 0 {
            sreg |= 0x01;
        }
        if (((sreg >> 2) & 1) ^ (sreg & 1)) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        state.set_sreg(sreg);
    } else if (opcode & 0xfc00) == 0x0800 {
        /* SBC, 0000 10rd dddd rrrr */
        let val1 = data(state, reg_d(opcode));
        let val2 = data(state, reg_r(opcode));
        let old_sreg = state.sreg();
        let result = val1 as i32 - val2 as i32 - (old_sreg & 1) as i32;
        set_data(state, reg_d(opcode), result as u8);
        set_sub_carry_flags(state, val1, val2, result, old_sreg);
    } else if (opcode & 0xf000) == 0x4000 {
        /* SBCI, 0100 KKKK dddd KKKK */
        let d_addr = 16 + ((opcode & 0xf0) >> 4);
        let val1 = data(state, d_addr);
        let val2 = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        let old_sreg = state.sreg();
        let result = val1 as i32 - val2 as i32 - (old_sreg & 1) as i32;
        set_data(state, d_addr, result as u8);
        set_sub_carry_flags(state, val1, val2, result, old_sreg);
    } else if (opcode & 0xff00) == 0x9a00 {
        /* SBI, 1001 1010 AAAA Abbb */
        let target = ((opcode & 0xf8) >> 3) + 32;
        let mask = 1u8 << (opcode & 0x07);
        let value = state.read_data(target);
        state.write_data_masked(target, value | mask, mask);
        state.add_cycles(1);
    } else if (opcode & 0xff00) == 0x9900 {
        /* SBIC, 1001 1001 AAAA Abbb */
        let target = ((opcode & 0xf8) >> 3) + 32;
        let value = state.read_data(target);
        let bit = 1u8 << (opcode & 0x07);
        if value & bit == 0 {
            let next = state.prog_word(state.pc() + 1);
            let skip = if is_two_word_instruction(next) { 2 } else { 1 };
            state.set_pc(state.pc() + skip);
            state.add_cycles(skip as u64);
        }
    } else if (opcode & 0xff00) == 0x9b00 {
        /* SBIS, 1001 1011 AAAA Abbb */
        let target = ((opcode & 0xf8) >> 3) + 32;
        let value = state.read_data(target);
        let bit = 1u8 << (opcode & 0x07);
        if value & bit != 0 {
            let next = state.prog_word(state.pc() + 1);
            let skip = if is_two_word_instruction(next) { 2 } else { 1 };
            state.set_pc(state.pc() + skip);
            state.add_cycles(skip as u64);
        }
    } else if (opcode & 0xff00) == 0x9700 {
        /* SBIW, 1001 0111 KKdd KKKK */
        let addr = 2 * ((opcode & 0x30) >> 4) + 24;
        let a = state.read_u16(addr);
        let k = (opcode & 0x0f) | ((opcode & 0xc0) >> 2);
        let result = a as i32 - k as i32;
        state.write_u16(addr, result as u16);
        let mut sreg = state.sreg() & 0xc0;
        if result == 0 {
            sreg |= 0x02;
        }
        if result & 0x8000 != 0 {
            sreg |= 0x04;
        }
        if ((a as u32) & (!(result as u32)) & 0x8000) != 0 {
            sreg |= 0x08;
        }
        if (((sreg >> 2) & 1) ^ ((sreg >> 3) & 1)) != 0 {
            sreg |= 0x10;
        }
        if k > a {
            sreg |= 0x01;
        }
        if (((!(a as u32)) & k as u32) | (k as u32 & result as u32) | (result as u32 & !(a as u32)))
            & 1
            != 0
        {
            sreg |= 0x20;
        }
        state.set_sreg(sreg);
        state.add_cycles(1);
    } else if (opcode & 0xfe08) == 0xfc00 {
        /* SBRC, 1111 110r rrrr 0bbb */
        let value = data(state, reg_d(opcode));
        let bit = 1u8 << (opcode & 0x07);
        if value & bit == 0 {
            let next = state.prog_word(state.pc() + 1);
            let skip = if is_two_word_instruction(next) { 2 } else { 1 };
            state.set_pc(state.pc() + skip);
            state.add_cycles(skip as u64);
        }
    } else if (opcode & 0xfe08) == 0xfe00 {
        /* SBRS, 1111 111r rrrr 0bbb */
        let value = data(state, reg_d(opcode));
        let bit = 1u8 << (opcode & 0x07);
        if value & bit != 0 {
            let next = state.prog_word(state.pc() + 1);
            let skip = if is_two_word_instruction(next) { 2 } else { 1 };
            state.set_pc(state.pc() + skip);
            state.add_cycles(skip as u64);
        }
    } else if opcode == 0x9588 {
        /* SLEEP, 1001 0101 1000 1000 */
    } else if opcode == 0x95e8 || opcode == 0x95f8 {
        /* SPM, SPM(INC) -- not implemented */
    } else if (opcode & 0xfe0f) == 0x9200 {
        /* STS, 1001 001d dddd 0000 kkkk kkkk kkkk kkkk */
        let value = data(state, reg_d(opcode));
        let addr = state.prog_word(state.pc() + 1);
        state.write_data(addr, value);
        state.set_pc(state.pc() + 1);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x920c {
        /* STX, 1001 001r rrrr 1100 */
        let addr = state.read_u16(26);
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x920d {
        /* STX(INC), 1001 001r rrrr 1101 */
        let x = state.read_u16(26);
        let value = data(state, reg_d(opcode));
        state.write_data(x, value);
        state.write_u16(26, x.wrapping_add(1));
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x920e {
        /* STX(DEC), 1001 001r rrrr 1110 */
        let value = data(state, reg_d(opcode));
        let x = state.read_u16(26).wrapping_sub(1);
        state.write_u16(26, x);
        state.write_data(x, value);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x8208 {
        /* STY, 1000 001r rrrr 1000 */
        let addr = state.read_u16(28);
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x9209 {
        /* STY(INC), 1001 001r rrrr 1001 */
        let y = state.read_u16(28);
        let value = data(state, reg_d(opcode));
        state.write_data(y, value);
        state.write_u16(28, y.wrapping_add(1));
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x920a {
        /* STY(DEC), 1001 001r rrrr 1010 */
        let value = data(state, reg_d(opcode));
        let y = state.read_u16(28).wrapping_sub(1);
        state.write_u16(28, y);
        state.write_data(y, value);
        state.add_cycles(1);
    } else if (opcode & 0xd208) == 0x8208 && displacement(opcode) != 0 {
        /* STDY, 10q0 qq1r rrrr 1qqq */
        let y = state.read_u16(28);
        let addr = y.wrapping_add(displacement(opcode));
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x8200 {
        /* STZ, 1000 001r rrrr 0000 */
        let addr = state.read_u16(30);
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x9201 {
        /* STZ(INC), 1001 001r rrrr 0001 */
        let z = state.read_u16(30);
        let value = data(state, reg_d(opcode));
        state.write_data(z, value);
        state.write_u16(30, z.wrapping_add(1));
        state.add_cycles(1);
    } else if (opcode & 0xfe0f) == 0x9202 {
        /* STZ(DEC), 1001 001r rrrr 0010 */
        let value = data(state, reg_d(opcode));
        let z = state.read_u16(30).wrapping_sub(1);
        state.write_u16(30, z);
        state.write_data(z, value);
        state.add_cycles(1);
    } else if (opcode & 0xd208) == 0x8200 && displacement(opcode) != 0 {
        /* STDZ, 10q0 qq1r rrrr 0qqq */
        let z = state.read_u16(30);
        let addr = z.wrapping_add(displacement(opcode));
        let value = data(state, reg_d(opcode));
        state.write_data(addr, value);
        state.add_cycles(1);
    } else if (opcode & 0xfc00) == 0x1800 {
        /* SUB, 0001 10rd dddd rrrr */
        let val1 = data(state, reg_d(opcode));
        let val2 = data(state, reg_r(opcode));
        let result = val1 as i32 - val2 as i32;
        set_data(state, reg_d(opcode), result as u8);
        set_sub_flags(state, val1, val2, result);
    } else if (opcode & 0xf000) == 0x5000 {
        /* SUBI, 0101 KKKK dddd KKKK */
        let d_addr = 16 + ((opcode & 0xf0) >> 4);
        let val1 = data(state, d_addr);
        let val2 = ((opcode & 0x0f) | ((opcode & 0xf00) >> 4)) as u8;
        let result = val1 as i32 - val2 as i32;
        set_data(state, d_addr, result as u8);
        set_sub_flags(state, val1, val2, result);
    } else if (opcode & 0xfe0f) == 0x9402 {
        /* SWAP, 1001 010d dddd 0010 */
        let d_addr = reg_d(opcode);
        let value = data(state, d_addr);
        set_data(state, d_addr, value.rotate_right(4));
    } else if opcode == 0x95a8 {
        /* WDR, 1001 0101 1010 1000 */
        state.on_watchdog_reset();
    } else if (opcode & 0xfe0f) == 0x9204 {
        /* XCH, 1001 001r rrrr 0100 */
        let r_addr = reg_d(opcode);
        let first = data(state, r_addr);
        let z = state.read_u16(30);
        let second = data(state, z);
        state.set_data_byte(z, first);
        set_data(state, r_addr, second);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{asm_program, Recorder};

    const X: u16 = 26;
    const Y: u16 = 28;
    const Z: u16 = 30;
    const RAMPZ: u16 = 0x5b;
    const EIND: u16 = 0x5c;
    const SP: u16 = 93;

    const SREG_C: u8 = 0b0000_0001;
    const SREG_Z: u8 = 0b0000_0010;
    const SREG_N: u8 = 0b0000_0100;
    const SREG_V: u8 = 0b0000_1000;
    const SREG_S: u8 = 0b0001_0000;
    const SREG_H: u8 = 0b0010_0000;
    const SREG_I: u8 = 0b1000_0000;

    fn cpu_with(source: &str) -> Cpu {
        let mut program = asm_program(source).program;
        program.resize(0x8000, 0);
        Cpu::new(program)
    }

    fn cpu_with_large_flash(source: &str) -> Cpu {
        let mut program = asm_program(source).program;
        program.resize(0x20000, 0);
        Cpu::new(program)
    }

    fn read_u16(cpu: &Cpu, addr: u16) -> u16 {
        u16::from_le_bytes([cpu.data_byte(addr), cpu.data_byte(addr.wrapping_add(1))])
    }

    fn write_u16(cpu: &Cpu, addr: u16, value: u16) {
        let bytes = value.to_le_bytes();
        cpu.set_data_byte(addr, bytes[0]);
        cpu.set_data_byte(addr.wrapping_add(1), bytes[1]);
    }

    #[test]
    fn should_execute_adc_r0_r1_instruction_when_carry_is_on() {
        let cpu = cpu_with("ADC r0, r1");
        cpu.set_data_byte(0, 10);
        cpu.set_data_byte(1, 20);
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 31);
        assert_eq!(cpu.sreg(), 0);
    }

    #[test]
    fn should_execute_adc_r0_r1_instruction_when_carry_is_on_and_the_result_overflows() {
        let cpu = cpu_with("ADC r0, r1");
        cpu.set_data_byte(0, 10);
        cpu.set_data_byte(1, 245);
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 0);
        assert_eq!(cpu.sreg(), SREG_H | SREG_Z | SREG_C);
    }

    #[test]
    fn should_execute_add_r0_r1_instruction_when_result_overflows() {
        let cpu = cpu_with("ADD r0, r1");
        cpu.set_data_byte(0, 11);
        cpu.set_data_byte(1, 245);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 0);
        assert_eq!(cpu.sreg(), SREG_H | SREG_Z | SREG_C);
    }

    #[test]
    fn should_execute_add_r0_r1_instruction_when_carry_is_on() {
        let cpu = cpu_with("ADD r0, r1");
        cpu.set_data_byte(0, 11);
        cpu.set_data_byte(1, 244);
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 255);
        assert_eq!(cpu.sreg(), SREG_S | SREG_N);
    }

    #[test]
    fn should_execute_add_r0_r1_instruction_when_carry_is_on_and_the_result_overflows() {
        let cpu = cpu_with("ADD r0, r1");
        cpu.set_data_byte(0, 11);
        cpu.set_data_byte(1, 245);
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 0);
        assert_eq!(cpu.sreg(), SREG_H | SREG_Z | SREG_C);
    }

    #[test]
    fn should_execute_bclr_2_instruction() {
        let cpu = cpu_with("BCLR 2");
        cpu.set_sreg(0xff);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), 0xfb);
    }

    #[test]
    fn should_execute_bld_r4_7_instruction() {
        let cpu = cpu_with("BLD r4, 7");
        cpu.set_data_byte(4, 0x15);
        cpu.set_sreg(0x40);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(4), 0x95);
        assert_eq!(cpu.sreg(), 0x40);
    }

    #[test]
    fn should_execute_brbc_0_plus_8_instruction_when_sreg_c_is_clear() {
        let cpu = cpu_with("BRBC 0, +8");
        cpu.set_sreg(SREG_V);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1 + 8 / 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_brbc_0_plus_8_instruction_when_sreg_c_is_set() {
        let cpu = cpu_with("BRBC 0, +8");
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
    }

    #[test]
    fn should_execute_brbs_3_92_instruction_when_sreg_v_is_set() {
        let cpu = cpu_with("BRBS 3, 92");
        cpu.set_sreg(SREG_V);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1 + 92 / 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_brbs_3_minus_4_instruction_when_sreg_v_is_set() {
        let cpu = cpu_with("BRBS 3, -4");
        cpu.set_sreg(SREG_V);
        avr_instruction(&cpu);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_brbs_3_minus_4_instruction_when_sreg_v_is_clear() {
        let cpu = cpu_with("BRBS 3, -4");
        cpu.set_sreg(0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
    }

    #[test]
    fn should_execute_call_instruction() {
        let cpu = cpu_with("CALL 0xb8");
        cpu.set_sp(150);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x5c);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.data_byte(150), 2);
        assert_eq!(cpu.sp(), 148);
    }

    #[test]
    fn should_push_3_byte_return_address_when_executing_call_instruction_on_device_with_more_than_128k_flash(
    ) {
        let cpu = cpu_with_large_flash("CALL 0xb8");
        cpu.set_sp(150);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x5c);
        assert_eq!(cpu.cycles(), 5);
        assert_eq!(cpu.data_byte(150), 2);
        assert_eq!(cpu.sp(), 147);
    }

    #[test]
    fn should_execute_cbi_0x0c_5() {
        let cpu = cpu_with("CBI 0x0c, 5");
        cpu.set_data_byte(0x2c, 0b1111_1111);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0x2c), 0b1101_1111);
    }

    #[test]
    fn should_execute_cpc_r27_r18_instruction() {
        let cpu = cpu_with("CPC r27, r18");
        cpu.set_data_byte(18, 0x1);
        cpu.set_data_byte(27, 0x1);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), 0);
    }

    #[test]
    fn should_execute_cpc_r24_r1_instruction_and_set() {
        let cpu = cpu_with("CPC r24, r1");
        cpu.set_data_byte(1, 0);
        cpu.set_data_byte(24, 0);
        cpu.set_sreg(SREG_I | SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), SREG_I | SREG_H | SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_cpi_r26_0x9_instruction() {
        let cpu = cpu_with("CPI r26, 0x9");
        cpu.set_data_byte(26, 0x8);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), SREG_H | SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_cpse_r2_r3_when_r2_is_not_equal_to_r3() {
        let cpu = cpu_with("CPSE r2, r3");
        cpu.set_data_byte(2, 10);
        cpu.set_data_byte(3, 11);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
    }

    #[test]
    fn should_execute_cpse_r2_r3_when_r2_equals_r3() {
        let cpu = cpu_with("CPSE r2, r3");
        cpu.set_data_byte(2, 10);
        cpu.set_data_byte(3, 10);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_cpse_r2_r3_when_r2_equals_r3_and_followed_by_2_word_instruction() {
        let cpu = cpu_with("CPSE r2, r3\nCALL 8");
        cpu.set_data_byte(2, 10);
        cpu.set_data_byte(3, 10);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 3);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_eicall_instruction() {
        let cpu = cpu_with_large_flash("EICALL");
        cpu.set_sp(0x80);
        cpu.set_data_byte(EIND, 1);
        write_u16(&cpu, Z, 0x1234);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x11234);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.sp(), 0x80 - 3);
        assert_eq!(cpu.data_byte(0x80), 1);
    }

    #[test]
    fn should_execute_eijmp_instruction() {
        let cpu = cpu_with_large_flash("EIJMP");
        cpu.set_data_byte(EIND, 1);
        write_u16(&cpu, Z, 0x1040);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x11040);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_elpm_instruction() {
        let cpu = cpu_with_large_flash("ELPM");
        cpu.set_data_byte(Z, 0x50);
        cpu.set_data_byte(RAMPZ, 0x2);
        cpu.write_prog_bytes(&[0x62], 0x20050);
        avr_instruction(&cpu);
        assert_eq!(cpu.data_byte(0), 0x62);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_elpm_r5_z_instruction() {
        let cpu = cpu_with_large_flash("ELPM r5, Z");
        cpu.set_data_byte(Z, 0x11);
        cpu.set_data_byte(RAMPZ, 0x1);
        cpu.write_prog_bytes(&[0x99], 0x10011);
        avr_instruction(&cpu);
        assert_eq!(cpu.data_byte(5), 0x99);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_elpm_r6_z_plus_instruction() {
        let cpu = cpu_with_large_flash("ELPM r6, Z+");
        write_u16(&cpu, Z, 0xffff);
        cpu.set_data_byte(RAMPZ, 0x2);
        cpu.write_prog_bytes(&[0x22], 0x2ffff);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(cpu.data_byte(6), 0x22);
        assert_eq!(read_u16(&cpu, Z), 0x0);
        assert_eq!(cpu.data_byte(RAMPZ), 3);
    }

    #[test]
    fn should_clamp_rampz_when_executing_elpm_r6_z_plus_instruction() {
        let cpu = cpu_with_large_flash("ELPM r6, Z+");
        write_u16(&cpu, Z, 0xffff);
        cpu.set_data_byte(RAMPZ, 0x3);
        avr_instruction(&cpu);
        assert_eq!(cpu.data_byte(RAMPZ), 0x0);
    }

    #[test]
    fn should_execute_icall_instruction() {
        let cpu = cpu_with("ICALL");
        cpu.set_sp(0x80);
        write_u16(&cpu, Z, 0x2020);
        avr_instruction(&cpu);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(cpu.pc(), 0x2020);
        assert_eq!(cpu.data_byte(0x80), 1);
        assert_eq!(cpu.sp(), 0x7e);
    }

    #[test]
    fn should_push_3_byte_return_address_when_executing_icall_instruction_on_device_with_more_than_128k_flash(
    ) {
        let cpu = cpu_with_large_flash("ICALL");
        cpu.set_sp(0x80);
        write_u16(&cpu, Z, 0x2020);
        avr_instruction(&cpu);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.pc(), 0x2020);
        assert_eq!(cpu.data_byte(0x80), 1);
        assert_eq!(cpu.sp(), 0x7d);
    }

    #[test]
    fn should_execute_ijmp_instruction() {
        let cpu = cpu_with("IJMP");
        write_u16(&cpu, Z, 0x1040);
        avr_instruction(&cpu);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.pc(), 0x1040);
    }

    #[test]
    fn should_execute_in_r5_0xb_instruction() {
        let cpu = cpu_with("IN r5, 0xb");
        cpu.set_data_byte(0x2b, 0xaf);
        avr_instruction(&cpu);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.data_byte(5), 0xaf);
    }

    #[test]
    fn should_execute_inc_r5_instruction() {
        let cpu = cpu_with("INC r5");
        cpu.set_data_byte(5, 0x7f);
        avr_instruction(&cpu);
        assert_eq!(cpu.data_byte(5), 0x80);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), SREG_N | SREG_V);
    }

    #[test]
    fn should_execute_inc_r5_instruction_when_r5_is_0xff() {
        let cpu = cpu_with("INC r5");
        cpu.set_data_byte(5, 0xff);
        avr_instruction(&cpu);
        assert_eq!(cpu.data_byte(5), 0);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), SREG_Z);
    }

    #[test]
    fn should_execute_jmp_0xb8_instruction() {
        let cpu = cpu_with("JMP 0xb8");
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x5c);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_lac_z_r19_instruction() {
        let cpu = cpu_with("LAC Z, r19");
        cpu.set_data_byte(19, 0x02);
        write_u16(&cpu, Z, 0x100);
        cpu.set_data_byte(0x100, 0x96);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(19), 0x96);
        assert_eq!(read_u16(&cpu, Z), 0x100);
        assert_eq!(cpu.data_byte(0x100), 0x94);
    }

    #[test]
    fn should_execute_las_z_r17_instruction() {
        let cpu = cpu_with("LAS Z, r17");
        cpu.set_data_byte(17, 0x11);
        cpu.set_data_byte(Z, 0x80);
        cpu.set_data_byte(0x80, 0x44);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(17), 0x44);
        assert_eq!(cpu.data_byte(Z), 0x80);
        assert_eq!(cpu.data_byte(0x80), 0x55);
    }

    #[test]
    fn should_execute_lat_z_r0_instruction() {
        let cpu = cpu_with("LAT Z, r0");
        cpu.set_data_byte(0, 0x33);
        cpu.set_data_byte(Z, 0x80);
        cpu.set_data_byte(0x80, 0x66);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 0x66);
        assert_eq!(cpu.data_byte(Z), 0x80);
        assert_eq!(cpu.data_byte(0x80), 0x55);
    }

    #[test]
    fn should_execute_ld_r1_x_instruction() {
        let cpu = cpu_with("LD r1, X");
        cpu.set_data_byte(0xc0, 0x15);
        write_u16(&cpu, X, 0xc0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(1), 0x15);
        assert_eq!(read_u16(&cpu, X), 0xc0);
    }

    #[test]
    fn should_execute_ld_r17_x_plus_instruction() {
        let cpu = cpu_with("LD r17, X+");
        cpu.set_data_byte(0xc0, 0x15);
        write_u16(&cpu, X, 0xc0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(17), 0x15);
        assert_eq!(read_u16(&cpu, X), 0xc1);
    }

    #[test]
    fn should_execute_ld_r1_minus_x_instruction() {
        let cpu = cpu_with("LD r1, -X");
        cpu.set_data_byte(0x98, 0x22);
        write_u16(&cpu, X, 0x99);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(1), 0x22);
        assert_eq!(read_u16(&cpu, X), 0x98);
    }

    #[test]
    fn should_execute_ld_r8_y_instruction() {
        let cpu = cpu_with("LD r8, Y");
        cpu.set_data_byte(0xc0, 0x15);
        write_u16(&cpu, Y, 0xc0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(8), 0x15);
        assert_eq!(read_u16(&cpu, Y), 0xc0);
    }

    #[test]
    fn should_execute_ld_r3_y_plus_instruction() {
        let cpu = cpu_with("LD r3, Y+");
        cpu.set_data_byte(0xc0, 0x15);
        write_u16(&cpu, Y, 0xc0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(3), 0x15);
        assert_eq!(read_u16(&cpu, Y), 0xc1);
    }

    #[test]
    fn should_execute_ld_r0_minus_y_instruction() {
        let cpu = cpu_with("LD r0, -Y");
        cpu.set_data_byte(0x98, 0x22);
        write_u16(&cpu, Y, 0x99);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0), 0x22);
        assert_eq!(read_u16(&cpu, Y), 0x98);
    }

    #[test]
    fn should_execute_ldd_r4_y_plus_2_instruction() {
        let cpu = cpu_with("LDD r4, Y+2");
        cpu.set_data_byte(0x82, 0x33);
        write_u16(&cpu, Y, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(4), 0x33);
        assert_eq!(read_u16(&cpu, Y), 0x80);
    }

    #[test]
    fn should_execute_ld_r5_z_instruction() {
        let cpu = cpu_with("LD r5, Z");
        cpu.set_data_byte(0xcc, 0xf5);
        write_u16(&cpu, Z, 0xcc);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(5), 0xf5);
        assert_eq!(read_u16(&cpu, Z), 0xcc);
    }

    #[test]
    fn should_execute_ld_r7_z_plus_instruction() {
        let cpu = cpu_with("LD r7, Z+");
        cpu.set_data_byte(0xc0, 0x25);
        write_u16(&cpu, Z, 0xc0);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(7), 0x25);
        assert_eq!(read_u16(&cpu, Z), 0xc1);
    }

    #[test]
    fn should_execute_ld_r0_minus_z_instruction() {
        let cpu = cpu_with("LD r0, -Z");
        cpu.set_data_byte(0x9e, 0x66);
        write_u16(&cpu, Z, 0x9f);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0), 0x66);
        assert_eq!(read_u16(&cpu, Z), 0x9e);
    }

    #[test]
    fn should_execute_ldd_r15_z_plus_31_instruction() {
        let cpu = cpu_with("LDD r15, Z+31");
        cpu.set_data_byte(0x9f, 0x33);
        write_u16(&cpu, Z, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(15), 0x33);
        assert_eq!(read_u16(&cpu, Z), 0x80);
    }

    #[test]
    fn should_execute_ldi_r28_0xff_instruction() {
        let cpu = cpu_with("LDI r28, 0xff");
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(Y), 0xff);
    }

    #[test]
    fn should_execute_lds_r5_0x150_instruction() {
        let cpu = cpu_with("LDS r5, 0x150");
        cpu.set_data_byte(0x150, 0x7a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 2);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(5), 0x7a);
    }

    #[test]
    fn should_execute_lpm_instruction() {
        let cpu = cpu_with("LPM");
        cpu.set_prog_word(0x40, 0xa0);
        cpu.set_data_byte(Z, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(cpu.data_byte(0), 0xa0);
        assert_eq!(cpu.data_byte(Z), 0x80);
    }

    #[test]
    fn should_execute_lpm_r2_z_instruction() {
        let cpu = cpu_with("LPM r2, Z");
        cpu.set_prog_word(0x40, 0xa0);
        cpu.set_data_byte(Z, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(cpu.data_byte(2), 0xa0);
        assert_eq!(cpu.data_byte(Z), 0x80);
    }

    #[test]
    fn should_execute_lpm_r1_z_plus_instruction() {
        let cpu = cpu_with("LPM r1, Z+");
        cpu.set_prog_word(0x40, 0xa0);
        cpu.set_data_byte(Z, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(cpu.data_byte(1), 0xa0);
        assert_eq!(cpu.data_byte(Z), 0x81);
    }

    #[test]
    fn should_execute_lsr_r7_instruction() {
        let cpu = cpu_with("LSR r7");
        cpu.set_data_byte(7, 0x45);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(7), 0x22);
        assert_eq!(cpu.sreg(), SREG_S | SREG_V | SREG_C);
    }

    #[test]
    fn should_execute_mov_r7_r8_instruction() {
        let cpu = cpu_with("MOV r7, r8");
        cpu.set_data_byte(8, 0x45);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(7), 0x45);
    }

    #[test]
    fn should_execute_movw_r26_r22_instruction() {
        let cpu = cpu_with("MOVW r26, r22");
        cpu.set_data_byte(22, 0x45);
        cpu.set_data_byte(23, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(X), 0x45);
        assert_eq!(cpu.data_byte(27), 0x9a);
    }

    #[test]
    fn should_execute_mul_r5_r6_instruction() {
        let cpu = cpu_with("MUL r5, r6");
        cpu.set_data_byte(5, 100);
        cpu.set_data_byte(6, 5);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(read_u16(&cpu, 0), 500);
        assert_eq!(cpu.sreg(), 0);
    }

    #[test]
    fn should_execute_mul_r5_r6_instruction_and_update_carry_flag_when_numbers_are_big() {
        let cpu = cpu_with("MUL r5, r6");
        cpu.set_data_byte(5, 200);
        cpu.set_data_byte(6, 200);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(read_u16(&cpu, 0), 40000);
        assert_eq!(cpu.sreg(), SREG_C);
    }

    #[test]
    fn should_execute_mul_r0_r1_and_update_the_zero_flag() {
        let cpu = cpu_with("MUL r0, r1");
        cpu.set_data_byte(0, 0);
        cpu.set_data_byte(1, 9);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(read_u16(&cpu, 0), 0);
        assert_eq!(cpu.sreg(), SREG_Z);
    }

    #[test]
    fn should_execute_muls_r18_r19_instruction() {
        let cpu = cpu_with("MULS r18, r19");
        cpu.set_data_byte(18, (-5i8) as u8);
        cpu.set_data_byte(19, 100);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(read_u16(&cpu, 0) as i16, -500);
        assert_eq!(cpu.sreg(), SREG_C);
    }

    #[test]
    fn should_execute_mulsu_r16_r17_instruction() {
        let cpu = cpu_with("MULSU r16, r17");
        cpu.set_data_byte(16, (-5i8) as u8);
        cpu.set_data_byte(17, 200);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(read_u16(&cpu, 0) as i16, -1000);
        assert_eq!(cpu.sreg(), SREG_C);
    }

    #[test]
    fn should_execute_neg_r20_instruction() {
        let cpu = cpu_with("NEG r20");
        cpu.set_data_byte(20, 0x56);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(20), 0xaa);
        assert_eq!(cpu.sreg(), SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_nop_instruction() {
        let cpu = cpu_with("NOP");
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
    }

    #[test]
    fn should_execute_out_0x3f_r1_instruction() {
        let cpu = cpu_with("OUT 0x3f, r1");
        cpu.set_data_byte(1, 0x5a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0x5f), 0x5a);
    }

    #[test]
    fn should_execute_pop_r26_instruction() {
        let cpu = cpu_with("POP r26");
        cpu.set_sp(0xff);
        cpu.set_data_byte(0x100, 0x1a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(X), 0x1a);
        assert_eq!(read_u16(&cpu, SP), 0x100);
    }

    #[test]
    fn should_execute_push_r11_instruction() {
        let cpu = cpu_with("PUSH r11");
        cpu.set_data_byte(11, 0x2a);
        cpu.set_sp(0xff);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0xff), 0x2a);
        assert_eq!(read_u16(&cpu, SP), 0xfe);
    }

    #[test]
    fn should_execute_rcall_plus_6_instruction() {
        let cpu = cpu_with("RCALL 6");
        cpu.set_sp(0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 4);
        assert_eq!(cpu.cycles(), 3);
        assert_eq!(read_u16(&cpu, 0x80), 1);
        assert_eq!(cpu.data_byte(SP), 0x7e);
    }

    #[test]
    fn should_execute_rcall_minus_4_instruction() {
        let cpu = cpu_with("NOP\nRCALL -4");
        cpu.set_sp(0x80);
        avr_instruction(&cpu);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(read_u16(&cpu, 0x80), 2);
        assert_eq!(cpu.data_byte(SP), 0x7e);
    }

    #[test]
    fn should_push_3_byte_return_address_when_executing_rcall_instruction_on_device_with_more_than_128k_flash(
    ) {
        let cpu = cpu_with_large_flash("RCALL 6");
        cpu.set_sp(0x80);
        write_u16(&cpu, Z, 0x2020);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 4);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(read_u16(&cpu, 0x80), 1);
        assert_eq!(cpu.data_byte(SP), 0x7d);
    }

    #[test]
    fn should_execute_ret_instruction() {
        let cpu = cpu_with("RET");
        cpu.set_sp(0x90);
        cpu.set_data_byte(0x92, 16);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 16);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.data_byte(SP), 0x92);
    }

    #[test]
    fn should_execute_ret_instruction_on_device_with_more_than_128k_flash() {
        let cpu = cpu_with_large_flash("RET");
        cpu.set_sp(0x90);
        cpu.set_data_byte(0x91, 0x1);
        cpu.set_data_byte(0x93, 0x16);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x10016);
        assert_eq!(cpu.cycles(), 5);
        assert_eq!(cpu.data_byte(SP), 0x93);
    }

    #[test]
    fn should_execute_reti_instruction() {
        let cpu = cpu_with("RETI");
        cpu.set_sp(0xc0);
        cpu.set_data_byte(0xc2, 200);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 200);
        assert_eq!(cpu.cycles(), 4);
        assert_eq!(cpu.data_byte(SP), 0xc2);
        assert_eq!(cpu.sreg(), SREG_I);
    }

    #[test]
    fn should_execute_reti_instruction_on_device_with_more_than_128k_flash() {
        let cpu = cpu_with_large_flash("RETI");
        cpu.set_sp(0xc0);
        cpu.set_data_byte(0xc1, 0x1);
        cpu.set_data_byte(0xc3, 0x30);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 0x10030);
        assert_eq!(cpu.cycles(), 5);
        assert_eq!(cpu.data_byte(SP), 0xc3);
        assert_eq!(cpu.sreg(), SREG_I);
    }

    #[test]
    fn should_execute_rjmp_2_instruction() {
        let cpu = cpu_with("RJMP 2");
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_ror_r0_instruction() {
        let cpu = cpu_with("ROR r0");
        cpu.set_data_byte(0, 0x11);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 0x08);
        assert_eq!(cpu.sreg(), SREG_S | SREG_V | SREG_C);
    }

    #[test]
    fn should_execute_sbc_r0_r1_instruction_when_carry_is_on_and_result_overflows() {
        let cpu = cpu_with("SBC r0, r1");
        cpu.set_data_byte(0, 0);
        cpu.set_data_byte(1, 10);
        cpu.set_sreg(SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 245);
        assert_eq!(cpu.sreg(), SREG_H | SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_sbci_r23_3_instruction() {
        let cpu = cpu_with("SBCI r23, 3");
        cpu.set_data_byte(23, 3);
        cpu.set_sreg(SREG_I | SREG_C);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.sreg(), SREG_I | SREG_H | SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_sbi_0x0c_5_instruction() {
        let cpu = cpu_with("SBI 0x0c, 5");
        cpu.set_data_byte(0x2c, 0b0000_1111);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x2c), 0b0010_1111);
    }

    #[test]
    fn should_execute_sbis_0x0c_5_instruction_when_bit_is_clear() {
        let cpu = cpu_with("SBIS 0x0c, 5");
        cpu.set_data_byte(0x2c, 0b0000_1111);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
    }

    #[test]
    fn should_execute_sbis_0x0c_5_instruction_when_bit_is_set() {
        let cpu = cpu_with("SBIS 0x0c, 5");
        cpu.set_data_byte(0x2c, 0b0010_1111);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn should_execute_sbis_0x0c_5_instruction_when_bit_is_set_and_followed_by_2_word_instruction() {
        let cpu = cpu_with("SBIS 0x0c, 5\nCALL 0xb8");
        cpu.set_data_byte(0x2c, 0b0010_1111);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 3);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn should_execute_st_x_r1_instruction() {
        let cpu = cpu_with("ST X, r1");
        cpu.set_data_byte(1, 0x5a);
        cpu.set_data_byte(X, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x9a), 0x5a);
        assert_eq!(cpu.data_byte(X), 0x9a);
    }

    #[test]
    fn should_execute_st_x_plus_r1_instruction() {
        let cpu = cpu_with("ST X+, r1");
        cpu.set_data_byte(1, 0x5a);
        cpu.set_data_byte(X, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x9a), 0x5a);
        assert_eq!(cpu.data_byte(X), 0x9b);
    }

    #[test]
    fn should_execute_st_minus_x_r17_instruction() {
        let cpu = cpu_with("ST -X, r17");
        cpu.set_data_byte(17, 0x88);
        cpu.set_data_byte(X, 0x99);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x98), 0x88);
        assert_eq!(cpu.data_byte(X), 0x98);
    }

    #[test]
    fn should_execute_st_y_r2_instruction() {
        let cpu = cpu_with("ST Y, r2");
        cpu.set_data_byte(2, 0x5b);
        cpu.set_data_byte(Y, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x9a), 0x5b);
        assert_eq!(cpu.data_byte(Y), 0x9a);
    }

    #[test]
    fn should_execute_st_y_plus_r1_instruction() {
        let cpu = cpu_with("ST Y+, r1");
        cpu.set_data_byte(1, 0x5a);
        cpu.set_data_byte(Y, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x9a), 0x5a);
        assert_eq!(cpu.data_byte(Y), 0x9b);
    }

    #[test]
    fn should_execute_st_minus_y_r1_instruction() {
        let cpu = cpu_with("ST -Y, r1");
        cpu.set_data_byte(1, 0x5a);
        cpu.set_data_byte(Y, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x99), 0x5a);
        assert_eq!(cpu.data_byte(Y), 0x99);
    }

    #[test]
    fn should_execute_std_y_plus_17_r0_instruction() {
        let cpu = cpu_with("STD Y+17, r0");
        cpu.set_data_byte(0, 0xba);
        cpu.set_data_byte(Y, 0x9a);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x9a + 17), 0xba);
        assert_eq!(cpu.data_byte(Y), 0x9a);
    }

    #[test]
    fn should_execute_st_z_r16_instruction() {
        let cpu = cpu_with("ST Z, r16");
        cpu.set_data_byte(16, 0xdf);
        cpu.set_data_byte(Z, 0x40);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x40), 0xdf);
        assert_eq!(cpu.data_byte(Z), 0x40);
    }

    #[test]
    fn should_execute_st_z_plus_r0_instruction() {
        let cpu = cpu_with("ST Z+, r0");
        cpu.set_data_byte(0, 0x55);
        write_u16(&cpu, Z, 0x155);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x155), 0x55);
        assert_eq!(read_u16(&cpu, Z), 0x156);
    }

    #[test]
    fn should_execute_st_minus_z_r16_instruction() {
        let cpu = cpu_with("ST -Z, r16");
        cpu.set_data_byte(16, 0x5a);
        cpu.set_data_byte(Z, 0xff);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0xfe), 0x5a);
        assert_eq!(cpu.data_byte(Z), 0xfe);
    }

    #[test]
    fn should_execute_std_z_plus_1_r0_instruction() {
        let cpu = cpu_with("STD Z+1, r0");
        cpu.set_data_byte(0, 0xcc);
        cpu.set_data_byte(Z, 0x50);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x51), 0xcc);
        assert_eq!(cpu.data_byte(Z), 0x50);
    }

    #[test]
    fn should_execute_sts_0x151_r31_instruction() {
        let cpu = cpu_with("STS 0x151, r31");
        cpu.set_data_byte(31, 0x80);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 2);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(0x151), 0x80);
    }

    #[test]
    fn should_execute_sub_r0_r1_instruction_when_result_overflows() {
        let cpu = cpu_with("SUB r0, r1");
        cpu.set_data_byte(0, 0);
        cpu.set_data_byte(1, 10);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(0), 246);
        assert_eq!(cpu.sreg(), SREG_S | SREG_N | SREG_C);
    }

    #[test]
    fn should_execute_swap_r1_instruction() {
        let cpu = cpu_with("SWAP r1");
        cpu.set_data_byte(1, 0xa5);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(1), 0x5a);
    }

    #[test]
    fn should_execute_wdr_instruction_and_call_on_watchdog_reset() {
        let cpu = cpu_with("WDR");
        let calls = Recorder::<()>::new();
        let recorder = calls.clone();
        cpu.set_on_watchdog_reset(Box::new(move |_state| recorder.push(())));
        assert_eq!(calls.len(), 0);
        avr_instruction(&cpu);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn should_execute_xch_z_r21_instruction() {
        let cpu = cpu_with("XCH Z, r21");
        cpu.set_data_byte(21, 0xa1);
        cpu.set_data_byte(Z, 0x50);
        cpu.set_data_byte(0x50, 0xb9);
        avr_instruction(&cpu);
        assert_eq!(cpu.pc(), 1);
        assert_eq!(cpu.cycles(), 1);
        assert_eq!(cpu.data_byte(21), 0xb9);
        assert_eq!(cpu.data_byte(0x50), 0xa1);
    }
}
