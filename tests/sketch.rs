use avr8rs::{compile_source, UNO_FQBN};

#[test]
#[ignore = "requires arduino-cli with the arduino:avr core and avr-objcopy"]
fn compiles_a_real_sketch_to_flash_bytes() {
    let source = r#"
void setup() { pinMode(13, OUTPUT); }
void loop() { digitalWrite(13, HIGH); delay(100); digitalWrite(13, LOW); delay(100); }
"#;
    let bytes = compile_source("sketch", source, UNO_FQBN).expect("compile sketch");
    assert!(!bytes.is_empty());
}
