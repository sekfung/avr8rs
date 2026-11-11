//! Run a raw AVR flash image on the Arduino Uno board model.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use avr8rs::{compile_sketch, ArduinoUno, UNO_FLASH_WORDS, UNO_FQBN};

#[derive(Debug)]
struct Options {
    image: Option<String>,
    sketch: Option<String>,
    instructions: usize,
    frequency: u32,
    trace_pins: bool,
}

fn usage() -> &'static str {
    "usage: avr8rs-cli (--bin FILE | --sketch FILE.ino) [--board arduino:avr:uno] [--instructions N] [--freq HZ] [--trace-pins]"
}

fn parse_args() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let mut image = None;
    let mut sketch = None;
    let mut instructions = 1_000_000usize;
    let mut frequency = 16_000_000u32;
    let mut trace_pins = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--bin" => image = Some(args.next().ok_or("--bin requires a path")?),
            "--sketch" => sketch = Some(args.next().ok_or("--sketch requires a path")?),
            "--board" => {
                let board = args.next().ok_or("--board requires an FQBN")?;
                if board != UNO_FQBN {
                    return Err(format!("unsupported board {board}; expected {UNO_FQBN}"));
                }
            }
            "--instructions" | "--cycles" => {
                instructions = args
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
    if image.is_some() && sketch.is_some() {
        return Err("choose either --bin or --sketch".into());
    }
    if image.is_none() && sketch.is_none() {
        return Err("one of --bin or --sketch is required".into());
    }
    Ok(Options {
        image,
        sketch,
        instructions,
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

    let mut bytes = if let Some(sketch) = &options.sketch {
        match compile_sketch(Path::new(sketch), UNO_FQBN) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("compile failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        match fs::read(options.image.as_deref().expect("validated image")) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("cannot read image: {error}");
                return ExitCode::FAILURE;
            }
        }
    };
    if bytes.len() & 1 != 0 {
        bytes.push(0xff);
    }
    if bytes.len() > UNO_FLASH_WORDS * 2 {
        eprintln!("image is larger than the ATmega328P flash");
        return ExitCode::FAILURE;
    }

    let board = ArduinoUno::with_frequency(vec![0; UNO_FLASH_WORDS], options.frequency);
    board.cpu().write_prog_bytes(&bytes, 0);
    board.set_serial_callback(Some(Box::new(|byte| {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&[byte]).and_then(|()| stdout.flush());
    })));
    if options.trace_pins {
        for pin in 0u8..=13 {
            let _ = board.add_pin_listener(
                pin,
                Box::new(move |_state, high, _old| {
                    eprintln!("\n[pin {pin}] {}", if high { "HIGH" } else { "LOW" });
                }),
            );
        }
    }

    board.run_instructions(options.instructions);
    let _ = io::stdout().flush();
    eprintln!(
        "\n[avr8rs] cycles={} pc=0x{:04x}",
        board.cpu().cycles(),
        board.cpu().pc() * 2
    );
    ExitCode::SUCCESS
}
