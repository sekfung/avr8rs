use std::collections::BTreeMap;

use super::{assemble, AssembleResult, Line};

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex fixture"))
        .collect()
}

fn expect_bytes(source: &str, hex: &str) {
    let result = assemble(source);
    assert!(
        result.errors.is_empty(),
        "unexpected errors for {source:?}: {:?}",
        result.errors
    );
    assert_eq!(result.bytes, bytes(hex), "assembled bytes for {source:?}");
}

#[test]
fn should_assemble_add_instruction() {
    let result = assemble("ADD r16, r11");
    assert_eq!(
        result,
        AssembleResult {
            bytes: vec![0x0b, 0x0d],
            errors: Vec::new(),
            lines: vec![Line {
                byte_offset: 0,
                bytes: "0d0b".into(),
                line: 1,
                text: "ADD r16, r11".into(),
            }],
            labels: BTreeMap::new(),
        }
    );
}

#[test]
fn should_support_labels() {
    assert_eq!(
        assemble("loop: JMP loop").bytes,
        vec![0x0c, 0x94, 0x00, 0x00]
    );
}

#[test]
fn should_support_multi_line_programs() {
    let input = r#"
      start:
      LDI r16, 15
      EOR r16, r0
      BREQ start
    "#;
    expect_bytes(input, "0fe00025e9f3");
}

#[test]
fn should_successfully_assemble_an_empty_program() {
    let result = assemble("");
    assert_eq!(
        result,
        AssembleResult {
            bytes: Vec::new(),
            errors: Vec::new(),
            lines: Vec::new(),
            labels: BTreeMap::new(),
        }
    );
}

#[test]
fn should_return_empty_bytes_on_program_error() {
    let result = assemble("LDI r15, 20");
    assert_eq!(
        result,
        AssembleResult {
            bytes: Vec::new(),
            errors: vec!["Line 0: Rd out of range: 16<>31".into()],
            lines: Vec::new(),
            labels: BTreeMap::new(),
        }
    );
}

#[test]
fn should_assemble_adc_r0_r1() {
    expect_bytes("ADC r0, r1", "011c");
}

#[test]
fn should_assemble_bclr_2() {
    expect_bytes("BCLR 2", "a894");
}

#[test]
fn should_assemble_bld_r4_7() {
    expect_bytes("BLD r4, 7", "47f8");
}

#[test]
fn should_assemble_brbc_0_plus_8() {
    expect_bytes("BRBC 0, +8", "20f4");
}

#[test]
fn should_assemble_brbs_3_92() {
    expect_bytes("BRBS 3, 92", "73f1");
}

#[test]
fn should_assemble_brbs_3_minus_4() {
    expect_bytes("BRBS 3, -4", "f3f3");
}

#[test]
fn should_assemble_breq_with_forward_label_target() {
    expect_bytes("BREQ next \n next:", "01f0");
}

#[test]
fn should_assemble_brne_with_forward_label_target() {
    expect_bytes("BRNE next \n next:", "01f4");
}

#[test]
fn should_assemble_cbi_0xc_5() {
    expect_bytes("CBI 0xc, 5", "6598");
}

#[test]
fn should_assemble_call_0xb8() {
    expect_bytes("CALL 0xb8", "0e945c00");
}

#[test]
fn should_assemble_cpc_r27_r18() {
    expect_bytes("CPC r27, r18", "b207");
}

#[test]
fn should_assemble_cpc_r24_r1() {
    expect_bytes("CPC r24, r1", "8105");
}

#[test]
fn should_assemble_cpi_r26_0x9() {
    expect_bytes("CPI r26, 0x9", "a930");
}

#[test]
fn should_assemble_cpse_r2_r3() {
    expect_bytes("CPSE r2, r3`", "2310");
}

#[test]
fn should_assemble_icall() {
    expect_bytes("ICALL", "0995");
}

#[test]
fn should_assemble_ijmp() {
    expect_bytes("IJMP", "0994");
}

#[test]
fn should_assemble_in_r5_0xb() {
    expect_bytes("IN r5, 0xb", "5bb0");
}

#[test]
fn should_assemble_inc_r5() {
    expect_bytes("INC r5", "5394");
}

#[test]
fn should_assemble_jmp_0xb8() {
    expect_bytes("JMP 0xb8", "0c945c00");
}

#[test]
fn should_assemble_lac_z_r19() {
    expect_bytes("LAC Z, r19", "3693");
}

#[test]
fn should_assemble_las_z_r17() {
    expect_bytes("LAS Z, r17", "1593");
}

#[test]
fn should_assemble_lat_z_r0() {
    expect_bytes("LAT Z, r0", "0792");
}

#[test]
fn should_assemble_ldi_r28_0xff() {
    expect_bytes("LDI r28, 0xff", "cfef");
}

#[test]
fn should_assemble_lds_r5_0x150() {
    expect_bytes("LDS r5, 0x150", "50905001");
}

#[test]
fn should_assemble_ld_r1_x() {
    expect_bytes("LD r1, X", "1c90");
}

#[test]
fn should_assemble_ld_r17_x_plus() {
    expect_bytes("LD r17, X+", "1d91");
}

#[test]
fn should_assemble_ld_r1_minus_x() {
    expect_bytes("LD r1, -X", "1e90");
}

#[test]
fn should_assemble_ld_r8_y() {
    expect_bytes("LD r8, Y", "8880");
}

#[test]
fn should_assemble_ld_r3_y_plus() {
    expect_bytes("LD r3, Y+", "3990");
}

#[test]
fn should_assemble_ld_r0_minus_y() {
    expect_bytes("LD r0, -Y", "0a90");
}

#[test]
fn should_assemble_ldd_r4_y_plus_2() {
    expect_bytes("LDD r4, Y+2", "4a80");
}

#[test]
fn should_assemble_ld_r5_z() {
    expect_bytes("LD r5, Z", "5080");
}

#[test]
fn should_assemble_ld_r7_z_plus() {
    expect_bytes("LD r7, Z+", "7190");
}

#[test]
fn should_assemble_ld_r0_minus_z() {
    expect_bytes("LD r0, -Z", "0290");
}

#[test]
fn should_assemble_ldd_r15_z_plus_31() {
    expect_bytes("LDD r15, Z+31", "f78c");
}

#[test]
fn should_assemble_lpm() {
    expect_bytes("LPM", "c895");
}

#[test]
fn should_assemble_lpm_r2_z() {
    expect_bytes("LPM r2, Z", "2490");
}

#[test]
fn should_assemble_lpm_r1_z_plus() {
    expect_bytes("LPM r1, Z+", "1590");
}

#[test]
fn should_assemble_lsr_r7() {
    expect_bytes("LSR r7", "7694");
}

#[test]
fn should_assemble_mov_r7_r8() {
    expect_bytes("MOV r7, r8", "782c");
}

#[test]
fn should_assemble_movw_r26_r22() {
    expect_bytes("MOVW r26, r22", "db01");
}

#[test]
fn should_assemble_mul_r5_r6() {
    expect_bytes("MUL r5, r6", "569c");
}

#[test]
fn should_assemble_muls_r18_r19() {
    expect_bytes("MULS r18, r19", "2302");
}

#[test]
fn should_assemble_mulsu_r16_r17() {
    expect_bytes("MULSU r16, r17", "0103");
}

#[test]
fn should_assemble_neg_r20() {
    expect_bytes("NEG r20", "4195");
}

#[test]
fn should_assemble_nop() {
    expect_bytes("NOP", "0000");
}

#[test]
fn should_assemble_or_r5_r2() {
    expect_bytes("OR r5, r2", "5228");
}

#[test]
fn should_assemble_ori_r22_0x81() {
    expect_bytes("ORI r22, 0x81", "6168");
}

#[test]
fn should_assemble_out_0x3f_r1() {
    expect_bytes("OUT 0x3f, r1", "1fbe");
}

#[test]
fn should_assemble_pop_r26() {
    expect_bytes("POP r26", "af91");
}

#[test]
fn should_assemble_push_r11() {
    expect_bytes("PUSH r11", "bf92");
}

#[test]
fn should_assemble_rcall_plus_6() {
    expect_bytes("RCALL +6", "03d0");
}

#[test]
fn should_assemble_rcall_minus_4() {
    expect_bytes("RCALL -4", "fedf");
}

#[test]
fn should_assemble_ret() {
    expect_bytes("RET", "0895");
}

#[test]
fn should_assemble_reti() {
    expect_bytes("RETI", "1895");
}

#[test]
fn should_assemble_rjmp_2() {
    expect_bytes("RJMP 2", "01c0");
}

#[test]
fn should_assemble_ror_r0() {
    expect_bytes("ROR r0", "0794");
}

#[test]
fn should_assemble_sbci_r23_3() {
    expect_bytes("SBCI r23, 3", "7340");
}

#[test]
fn should_assemble_sbi_0x0c_5() {
    expect_bytes("SBI 0x0c, 5", "659a");
}

#[test]
fn should_assemble_sbis_0x0c_5() {
    expect_bytes("SBIS 0x0c, 5", "659b");
}

#[test]
fn should_assemble_sbiw_r28_2() {
    expect_bytes("SBIW r28, 2", "2297");
}

#[test]
fn should_assemble_sleep() {
    expect_bytes("SLEEP", "8895");
}

#[test]
fn should_assemble_spm() {
    expect_bytes("SPM", "e895");
}

#[test]
fn should_assemble_spm_z_plus() {
    expect_bytes("SPM Z+", "f895");
}

#[test]
fn should_assemble_sts_0x151_r31() {
    expect_bytes("STS 0x151, r31", "f0935101");
}

#[test]
fn should_assemble_st_x_r1() {
    expect_bytes("ST X, r1", "1c92");
}

#[test]
fn should_assemble_st_x_plus_r1() {
    expect_bytes("ST X+, r1", "1d92");
}

#[test]
fn should_assemble_st_minus_x_r17() {
    expect_bytes("ST -X, r17", "1e93");
}

#[test]
fn should_assemble_st_y_r2() {
    expect_bytes("ST Y, r2", "2882");
}

#[test]
fn should_assemble_st_y_plus_r1() {
    expect_bytes("ST Y+, r1", "1992");
}

#[test]
fn should_assemble_st_minus_y_r1() {
    expect_bytes("ST -Y, r1", "1a92");
}

#[test]
fn should_assemble_std_y_plus_17_r0() {
    expect_bytes("STD Y+17, r0", "098a");
}

#[test]
fn should_assemble_st_z_r16() {
    expect_bytes("ST Z, r16", "0083");
}

#[test]
fn should_assemble_st_z_plus_r0() {
    expect_bytes("ST Z+, r0", "0192");
}

#[test]
fn should_assemble_st_minus_z_r16() {
    expect_bytes("ST -Z, r16", "0293");
}

#[test]
fn should_assemble_std_z_plus_1_r0() {
    expect_bytes("STD Z+1, r0", "0182");
}

#[test]
fn should_assemble_swap_r1() {
    expect_bytes("SWAP r1", "1294");
}

#[test]
fn should_assemble_xch_z_r21() {
    expect_bytes("XCH Z, r21", "5493");
}
