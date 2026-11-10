//! State dump helper for the differential harness.

use std::env;
use std::process::ExitCode;

use avr8rs::{avr_instruction, Cpu};

fn parse_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() & 1 != 0 {
        return Err("hex string has odd length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&text[index..index + 2], 16)
                .map_err(|error| format!("bad hex byte: {error}"))
        })
        .collect()
}

fn main() -> ExitCode {
    let mut program_hex = String::new();
    let mut data_hex = String::new();
    let mut steps = 0usize;
    let mut sram = 256usize;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--program" => program_hex = args.next().unwrap_or_default(),
            "--data" => data_hex = args.next().unwrap_or_default(),
            "--steps" => steps = args.next().unwrap_or_default().parse().unwrap_or(0),
            "--sram" => sram = args.next().unwrap_or_default().parse().unwrap_or(256),
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let program_bytes = match parse_hex(&program_hex) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let data = match parse_hex(&data_hex) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let mut program = program_bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    if program.is_empty() {
        program.push(0);
    }
    let cpu = Cpu::with_sram(program, sram);
    for (index, byte) in data.iter().enumerate() {
        if index < cpu.data().len() {
            cpu.set_data_byte(index as u16, *byte);
        }
    }
    for _ in 0..steps {
        avr_instruction(&cpu);
    }
    let data = cpu.data();
    let register_space = data[..0x100]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!(
        "{} {} {} {} {}",
        cpu.pc(),
        cpu.cycles(),
        cpu.sreg(),
        cpu.sp(),
        register_space
    );
    ExitCode::SUCCESS
}
