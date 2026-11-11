use std::fs;
use std::process::Command;

#[test]
fn cli_and_server_reject_non_uno_fqbn() {
    for executable in [
        env!("CARGO_BIN_EXE_avr8rs-cli"),
        env!("CARGO_BIN_EXE_avr8rs-server"),
    ] {
        let output = Command::new(executable)
            .args(["--board", "arduino:avr:mega", "--help"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("arduino:avr:uno"));
    }
}

#[test]
fn cli_writes_serial_bytes_without_encoding_them() {
    let image = std::env::temp_dir().join(format!("avr8rs-serial-{}.bin", std::process::id()));
    let words = [0xe008u16, 0x9300, 0x00c1, 0xef0f, 0x9300, 0x00c6, 0xcfff];
    fs::write(
        &image,
        words
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_avr8rs-cli"))
        .arg("--bin")
        .arg(&image)
        .args(["--instructions", "20"])
        .output();
    fs::remove_file(image).unwrap();
    let output = output.unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, [0xff]);
}
