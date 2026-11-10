//! Compile Arduino sketches with `arduino-cli` and load the resulting flash
//! image. This is intentionally a thin host-side adapter: the simulator itself
//! remains dependency-free.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// FQBN used for the Arduino Uno board model.
pub const UNO_FQBN: &str = "arduino:avr:uno";

/// Errors from the sketch compilation pipeline.
#[derive(Debug)]
pub enum SketchError {
    Io(std::io::Error),
    ArduinoCliNotFound,
    ObjcopyNotFound,
    CommandFailed {
        program: String,
        status: Option<i32>,
        stderr: String,
    },
    NoOutput,
}

impl std::fmt::Display for SketchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SketchError::Io(error) => write!(f, "I/O error: {error}"),
            SketchError::ArduinoCliNotFound => write!(
                f,
                "arduino-cli not found; install it and the arduino:avr core"
            ),
            SketchError::ObjcopyNotFound => write!(
                f,
                "avr-objcopy not found; install the Arduino AVR toolchain"
            ),
            SketchError::CommandFailed {
                program,
                status,
                stderr,
            } => write!(
                f,
                "{program} failed (status {status:?}):\n{}",
                stderr.trim()
            ),
            SketchError::NoOutput => write!(f, "compiler produced no HEX or ELF output"),
        }
    }
}

impl std::error::Error for SketchError {}

impl From<std::io::Error> for SketchError {
    fn from(error: std::io::Error) -> Self {
        SketchError::Io(error)
    }
}

/// Compile one `.ino` file or a sketch directory into raw flash bytes.
pub fn compile_sketch(path: &Path, fqbn: &str) -> Result<Vec<u8>, SketchError> {
    let (sketch_dir, name) = if path.is_dir() {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("sketch")
            .to_string();
        (path.to_path_buf(), name)
    } else {
        let name = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("sketch")
            .to_string();
        let dir = temporary_dir();
        let sketch_dir = dir.join(&name);
        fs::create_dir_all(&sketch_dir)?;
        fs::copy(path, sketch_dir.join(format!("{name}.ino")))?;
        (sketch_dir, name)
    };
    compile_directory(&sketch_dir, &name, fqbn)
}

/// Compile sketch source text (used by the local demo server).
pub fn compile_source(name: &str, source: &str, fqbn: &str) -> Result<Vec<u8>, SketchError> {
    let dir = temporary_dir();
    let sketch_dir = dir.join(name);
    fs::create_dir_all(&sketch_dir)?;
    fs::write(sketch_dir.join(format!("{name}.ino")), source)?;
    compile_directory(&sketch_dir, name, fqbn)
}

fn compile_directory(sketch_dir: &Path, name: &str, fqbn: &str) -> Result<Vec<u8>, SketchError> {
    let arduino_cli = find_program("ARDUINO_CLI", "arduino-cli", &[".local/bin/arduino-cli"])
        .ok_or(SketchError::ArduinoCliNotFound)?;
    let objcopy = find_objcopy().ok_or(SketchError::ObjcopyNotFound)?;
    let output_dir = temporary_dir().join("out");
    fs::create_dir_all(&output_dir)?;

    let output = Command::new(&arduino_cli)
        .arg("compile")
        .arg("--fqbn")
        .arg(fqbn)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--build-path")
        .arg(output_dir.join("build"))
        .arg(sketch_dir)
        .output()?;
    if !output.status.success() {
        return Err(SketchError::CommandFailed {
            program: "arduino-cli compile".into(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let hex = find_with_extension(&output_dir, "hex")
        .into_iter()
        .find(|path| !path.to_string_lossy().contains("with_bootloader"))
        .or_else(|| find_with_extension(&output_dir, "elf").into_iter().next())
        .ok_or(SketchError::NoOutput)?;
    let binary = output_dir.join(format!("{name}.bin"));
    let format = if hex.extension() == Some(OsStr::new("hex")) {
        "ihex"
    } else {
        "elf32-avr"
    };
    let output = Command::new(&objcopy)
        .arg("-I")
        .arg(format)
        .arg("-O")
        .arg("binary")
        .arg(&hex)
        .arg(&binary)
        .output()?;
    if !output.status.success() {
        return Err(SketchError::CommandFailed {
            program: "avr-objcopy".into(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(fs::read(binary)?)
}

fn temporary_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = env::temp_dir().join(format!("avr8rs-sketch-{}-{stamp}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn find_program(env_name: &str, name: &str, home_candidates: &[&str]) -> Option<PathBuf> {
    if let Some(path) = env::var_os(env_name).map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let home = env::var_os("HOME").map(PathBuf::from)?;
    home_candidates
        .iter()
        .map(|relative| home.join(relative))
        .find(|candidate| candidate.is_file())
}

fn find_objcopy() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AVR_OBJCOPY").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = find_program("AVR_OBJCOPY", "avr-objcopy", &[]) {
        return Some(path);
    }
    let home = env::var_os("HOME").map(PathBuf::from)?;
    let tools = home.join(".arduino15/packages/arduino/tools/avr-gcc");
    let versions = fs::read_dir(tools).ok()?;
    for version in versions.flatten() {
        let candidate = version.path().join("bin/avr-objcopy");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new(extension)))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}
