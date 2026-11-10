//! Run a raw AVR flash image on the Arduino Uno board model.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use avr8rs::ArduinoUno;

#[derive(Debug)]
struct Options {
    image: String,
    cycles: usize,
    frequency: u32,
    trace_pins: bool,
}

fn usage() -> &'static str {
    "usage: avr8rs-cli --bin FILE [--cycles N] [--freq HZ] [--trace-pins]"
}

fn parse_args() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let mut image = None;
    let mut cycles = 1_000_000usize;
    let mut frequency = 16_000_000u32;
    let mut trace_pins = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--bin" => image = Some(args.next().ok_or("--bin requires a path")?),
            "--cycles" => {
                cycles = args
                    .next()
                    .ok_or("--cycles requires a number")?
                    .parse()
                    .map_err(|_| "--cycles must be an integer")?
            }
            "--freq" => {
                frequency = args
                    .next()
                    .ok_or("--freq requires a number")?
                    .parse()
                    .map_err(|_| "--freq must be an integer")?
            }
            "--trace-pins" => trace_pins = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Options {
        image: image.ok_or("--bin is required")?,
        cycles,
        frequency,
        trace_pins,
    })
}

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(options) => options,
        Err(message) if message.is_empty() => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n{}", usage());
            return ExitCode::from(2);
        }
    };

    let mut bytes = match fs::read(&options.image) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", options.image);
            return ExitCode::FAILURE;
        }
    };
    if bytes.len() & 1 != 0 {
        bytes.push(0xff);
    }
    if bytes.len() > 0x4000 * 2 {
        eprintln!("image is larger than the ATmega328P flash");
        return ExitCode::FAILURE;
    }

    let board = ArduinoUno::with_frequency(vec![0; 0x4000], options.frequency);
    board.cpu().write_prog_bytes(&bytes, 0);
    board.set_serial_callback(Some(Box::new(|byte| {
        print!("{}", char::from(byte));
        let _ = io::stdout().flush();
    })));
    if options.trace_pins {
        for pin in 0u8..=13 {
            let _ = board.add_pin_listener(
                pin,
                Box::new(move |_state, high, _old| {
                    println!("\n[pin {pin}] {}", if high { "HIGH" } else { "LOW" });
                }),
            );
        }
    }

    board.run_instructions(options.cycles);
    let _ = io::stdout().flush();
    eprintln!(
        "\n[avr8rs] cycles={} pc=0x{:04x}",
        board.cpu().cycles(),
        board.cpu().pc() * 2
    );
    ExitCode::SUCCESS
}
