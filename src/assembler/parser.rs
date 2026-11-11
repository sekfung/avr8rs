use std::collections::{BTreeMap, HashMap};

use super::opcodes::{encode, parse_int_prefix, Encoded};
use super::{AssembleResult, Line};

pub(super) struct PendingLine {
    pub byte_offset: u32,
    pub encoded: Encoded,
    pub line: usize,
    pub size: u32,
    pub text: String,
}

pub(super) struct PassOne {
    pub errors: Vec<String>,
    pub labels: BTreeMap<String, u32>,
    pub lines: Vec<PendingLine>,
}

struct Statement<'a> {
    mnemonic: &'a str,
    a: Option<&'a str>,
    b: Option<&'a str>,
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn strip_comment(line: &str) -> &str {
    let end = line.find(['#', ';']).unwrap_or(line.len());
    &line[..end]
}

fn split_label(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && is_word_byte(bytes[index]) {
        index += 1;
    }
    if index == 0 || index >= bytes.len() || bytes[index] != b':' {
        return None;
    }
    Some((&line[..index], line[index + 1..].trim()))
}

fn parse_statement(line: &str) -> Option<Statement<'_>> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && is_word_byte(bytes[index]) {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let mnemonic = &line[..index];
    if index == bytes.len() {
        return Some(Statement {
            mnemonic,
            a: None,
            b: None,
        });
    }
    if !bytes[index].is_ascii_whitespace() {
        return None;
    }

    let operands = line[index..].trim();
    if operands.is_empty() {
        return Some(Statement {
            mnemonic,
            a: None,
            b: None,
        });
    }

    if let Some((a, b)) = operands.split_once(',') {
        let a = a.trim();
        let b = b.trim();
        if a.is_empty() || b.is_empty() || b.chars().any(|c| c.is_whitespace() || c == ',') {
            return None;
        }
        return Some(Statement {
            mnemonic,
            a: Some(a),
            b: Some(b),
        });
    }

    Some(Statement {
        mnemonic,
        a: Some(operands),
        b: None,
    })
}

pub(super) fn pass_one(input: &str) -> PassOne {
    let mut byte_offset = 0_u32;
    let mut labels = BTreeMap::new();
    let mut replacements: HashMap<String, String> = HashMap::new();
    let mut errors = Vec::new();
    let mut lines = Vec::new();

    for (index, raw_line) in input.split('\n').enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let text = trimmed.to_string();

        let code = strip_comment(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        let code = if let Some((label, rest)) = split_label(code) {
            labels.insert(label.to_string(), byte_offset);
            rest
        } else {
            code
        };
        if code.is_empty() {
            continue;
        }

        let Some(statement) = parse_statement(code) else {
            errors.push(format!("Line {index}: doesn't match as code!"));
            continue;
        };
        let instruction = statement.mnemonic.to_ascii_uppercase();

        match instruction.as_str() {
            "_REPLACE" => {
                replacements.insert(
                    statement.a.unwrap_or("").to_string(),
                    statement.b.unwrap_or("").to_string(),
                );
                continue;
            }
            "_LOC" => {
                let Some(value) = statement.a.and_then(parse_int_prefix) else {
                    errors.push(format!("Line {index}: Location is not a number."));
                    continue;
                };
                if value & 1 != 0 {
                    errors.push(format!("Line {index}: Location is odd"));
                    continue;
                }
                let Ok(value) = u32::try_from(value) else {
                    errors.push(format!("Line {index}: Location is out of range."));
                    continue;
                };
                byte_offset = value;
                continue;
            }
            "_IW" => {
                let Some(value) = statement.a.and_then(parse_int_prefix) else {
                    errors.push(format!("Line {index}: Immeadiate Word is not a number."));
                    continue;
                };
                lines.push(PendingLine {
                    byte_offset,
                    encoded: Encoded::Words(vec![value as u16]),
                    line: index + 1,
                    size: 2,
                    text,
                });
                byte_offset = byte_offset.saturating_add(2);
                continue;
            }
            _ => {}
        }

        let a = statement
            .a
            .map(|value| {
                replacements
                    .get(value)
                    .cloned()
                    .unwrap_or_else(|| value.to_string())
            })
            .unwrap_or_default();
        let b = statement
            .b
            .map(|value| {
                replacements
                    .get(value)
                    .cloned()
                    .unwrap_or_else(|| value.to_string())
            })
            .unwrap_or_default();

        match encode(&instruction, &a, &b, byte_offset, &labels) {
            Ok(encoded) => {
                let size = match &encoded {
                    Encoded::Words(words) => (words.len() * 2) as u32,
                    Encoded::Deferred(_) if instruction == "CALL" || instruction == "JMP" => 4,
                    Encoded::Deferred(_) => 2,
                };
                lines.push(PendingLine {
                    byte_offset,
                    encoded,
                    line: index + 1,
                    size,
                    text,
                });
                byte_offset = byte_offset.saturating_add(size);
            }
            Err(error) => errors.push(format!("Line {index}: {error}")),
        }
    }

    PassOne {
        errors,
        labels,
        lines,
    }
}

pub(super) fn pass_two(lines: Vec<PendingLine>, labels: BTreeMap<String, u32>) -> AssembleResult {
    let byte_size = lines
        .iter()
        .map(|line| (line.byte_offset + line.size) as usize)
        .max()
        .unwrap_or(0);
    let mut bytes = vec![0_u8; byte_size];
    let mut errors = Vec::new();
    let mut output_lines = Vec::with_capacity(lines.len());

    for pending in lines {
        let words = match pending.encoded {
            Encoded::Words(words) => words,
            Encoded::Deferred(resolve) => resolve(&labels),
        };
        if words.is_empty() {
            errors.push(format!("Line: {}: Empty array in lineTable.", pending.line));
            output_lines.push(Line {
                byte_offset: pending.byte_offset,
                bytes: String::new(),
                line: pending.line,
                text: pending.text,
            });
            continue;
        }

        for (word_index, word) in words.iter().enumerate() {
            let offset = pending.byte_offset as usize + word_index * 2;
            if offset + 1 < bytes.len() {
                bytes[offset] = *word as u8;
                bytes[offset + 1] = (*word >> 8) as u8;
            }
        }

        let bytes_text = words
            .iter()
            .map(|word| format!("{word:04x}"))
            .collect::<Vec<_>>()
            .join(" ");
        output_lines.push(Line {
            byte_offset: pending.byte_offset,
            bytes: bytes_text,
            line: pending.line,
            text: pending.text,
        });
    }

    AssembleResult {
        bytes,
        errors,
        lines: output_lines,
        labels,
    }
}
