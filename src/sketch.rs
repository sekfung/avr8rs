//! Compile Arduino sketches with `arduino-cli` and load the resulting flash
//! image. This is intentionally a thin host-side adapter: the simulator itself
//! remains dependency-free.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let path = env::current_dir()?.join(path);
    let work = TemporaryDir::new()?;
    let sketch_dir = prepare_sketch(&path, &work.0)?;
    compile_directory(&sketch_dir, fqbn, &work.0)
}

/// Compile sketch source text (used by the local demo server).
pub fn compile_source(name: &str, source: &str, fqbn: &str) -> Result<Vec<u8>, SketchError> {
    validate_sketch_name(name)?;
    let work = TemporaryDir::new()?;
    let sketch_dir = work.0.join("source").join(name);
    fs::create_dir_all(&sketch_dir)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sketch_dir.join(format!("{name}.ino")))?;
    file.write_all(source.as_bytes())?;
    drop(file);
    compile_directory(&sketch_dir, fqbn, &work.0)
}

fn validate_sketch_name(name: &str) -> io::Result<()> {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        && !name.ends_with('.')
        && name.len() <= 63;
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|suffix| matches!(suffix.as_bytes(), [b'1'..=b'9']))
        });
    if valid && !reserved {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid sketch name {name:?}"),
        ))
    }
}

fn prepare_sketch(path: &Path, work: &Path) -> io::Result<PathBuf> {
    if path.is_dir() {
        return Ok(path.to_path_buf());
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "sketch file has no parent directory",
        )
    })?;
    if path.is_file()
        && parent.file_name().is_some_and(|name| {
            [".ino", ".pde"].iter().any(|extension| {
                let mut main = name.to_os_string();
                main.push(extension);
                parent.join(main).is_file()
            })
        })
    {
        return Ok(parent.to_path_buf());
    }

    let name = path.file_stem().and_then(OsStr::to_str).unwrap_or("sketch");
    validate_sketch_name(name)?;
    let sketch_dir = work.join("source").join(name);
    fs::create_dir_all(&sketch_dir)?;
    fs::copy(path, sketch_dir.join(format!("{name}.ino")))?;
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let source = entry.path();
        let filename = entry.file_name();
        if source == path {
            continue;
        }
        if entry.file_type()?.is_dir() && matches!(filename.to_str(), Some("src" | "data")) {
            copy_tree(&source, &sketch_dir.join(filename))?;
        } else if source.is_file()
            && (matches!(
                filename.to_str(),
                Some("sketch.yaml" | "sketch.yml" | "sketch.json")
            ) || matches!(
                source.extension().and_then(OsStr::to_str),
                Some("ino" | "pde" | "cpp" | "c" | "S" | "h" | "hpp" | "hh" | "tpp" | "ipp")
            ))
        {
            fs::copy(source, sketch_dir.join(filename))?;
        }
    }
    Ok(sketch_dir)
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&path, &target)?;
        } else if path.is_file() {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

fn compile_directory(sketch_dir: &Path, fqbn: &str, work: &Path) -> Result<Vec<u8>, SketchError> {
    let arduino_cli = find_program("ARDUINO_CLI", "arduino-cli", &[".local/bin"])
        .ok_or(SketchError::ArduinoCliNotFound)?;
    let output_dir = work.join("out");
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
        .find(|path| {
            !path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .contains("with_bootloader")
        })
        .or_else(|| find_with_extension(&output_dir, "elf").into_iter().next())
        .ok_or(SketchError::NoOutput)?;
    let objcopy =
        find_objcopy(&arduino_cli, sketch_dir, fqbn).ok_or(SketchError::ObjcopyNotFound)?;
    let binary = output_dir.join("flash.bin");
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

struct TemporaryDir(PathBuf);

impl TemporaryDir {
    fn new() -> io::Result<Self> {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = env::temp_dir();
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("avr8rs-sketch-{}-{stamp}-{id}", std::process::id()));
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for TemporaryDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn executable_in(dir: &Path, name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let candidate = dir.join(format!("{name}.exe"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let candidate = dir.join(name);
    candidate.is_file().then_some(candidate)
}

fn find_program(env_name: &str, name: &str, home_dirs: &[&str]) -> Option<PathBuf> {
    if let Some(path) = env::var_os(env_name).map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            if let Some(candidate) = executable_in(&dir, name) {
                return Some(candidate);
            }
        }
    }
    let home = env::var_os("HOME").map(PathBuf::from)?;
    home_dirs
        .iter()
        .find_map(|relative| executable_in(&home.join(relative), name))
}

fn find_objcopy(arduino_cli: &Path, sketch_dir: &Path, fqbn: &str) -> Option<PathBuf> {
    if let Some(path) = find_program("AVR_OBJCOPY", "avr-objcopy", &[]) {
        return Some(path);
    }
    if let Ok(properties) = Command::new(arduino_cli)
        .args(["compile", "--fqbn", fqbn, "--show-properties=expanded"])
        .arg(sketch_dir)
        .output()
    {
        if properties.status.success() {
            let properties = String::from_utf8_lossy(&properties.stdout);
            if let Some(path) = objcopy_from_properties(&properties) {
                return Some(path);
            }
        }
    }
    let tools = arduino_data_dir()?.join("packages/arduino/tools/avr-gcc");
    let mut versions: Vec<_> = fs::read_dir(tools)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    versions.sort();
    versions
        .into_iter()
        .rev()
        .find_map(|version| executable_in(&version.join("bin"), "avr-objcopy"))
}

fn objcopy_from_properties(properties: &str) -> Option<PathBuf> {
    let property_path = |key| {
        properties
            .lines()
            .filter_map(|line| line.split_once('='))
            .find_map(|(name, value)| (name == key).then_some(value))
            .map(|value| Path::new(value.trim().trim_matches('"')))
            .filter(|path| path.is_absolute())
    };
    property_path("compiler.path")
        .and_then(|path| executable_in(path, "avr-objcopy"))
        .or_else(|| {
            property_path("runtime.tools.avr-gcc.path")
                .and_then(|path| executable_in(&path.join("bin"), "avr-objcopy"))
        })
}

fn arduino_data_dir() -> Option<PathBuf> {
    if let Some(data) = env::var_os("ARDUINO_DIRECTORIES_DATA").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(data));
    }
    if cfg!(windows) {
        let local = env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(|| {
            env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("AppData/Local"))
        })?;
        Some(local.join("Arduino15"))
    } else {
        let home = PathBuf::from(env::var_os("HOME")?);
        Some(home.join(if cfg!(target_os = "macos") {
            "Library/Arduino15"
        } else {
            ".arduino15"
        }))
    }
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
