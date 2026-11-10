use std::collections::BTreeMap;

pub(super) type Labels = BTreeMap<String, u32>;
type Resolver = Box<dyn Fn(&Labels) -> Vec<u16>>;

pub(super) enum Encoded {
    Words(Vec<u16>),
    Deferred(Resolver),
}

fn one(word: u16) -> Encoded {
    Encoded::Words(vec![word])
}

/// Find the first `r`/`R` followed by one or two decimal digits, like the
/// upstream `/[Rr](\d{1,2})/` expression.
fn register_number(r: &str) -> Option<i64> {
    let bytes = r.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] != b'r' && bytes[i] != b'R' {
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() && end - start < 2 {
            end += 1;
        }
        if end > start {
            return r[start..end].parse().ok();
        }
    }
    None
}

fn dest_r(r: &str, min: i64, max: i64) -> Result<u16, String> {
    let d = register_number(r).ok_or_else(|| format!("Not a register: {r}"))?;
    if d < min || d > max {
        return Err(format!("Rd out of range: {min}<>{max}"));
    }
    Ok(((d & 0x1f) as u16) << 4)
}

fn src_r(r: &str, min: i64, max: i64) -> Result<u16, String> {
    let d = register_number(r).ok_or_else(|| format!("Not a register: {r}"))?;
    if d < min || d > max {
        return Err(format!("Rd out of range: {min}<>{max}"));
    }
    Ok(((d & 0xf) as u16) | ((((d >> 4) & 1) as u16) << 9))
}

fn dr(r: &str) -> Result<u16, String> {
    dest_r(r, 0, 31)
}

fn sr(r: &str) -> Result<u16, String> {
    src_r(r, 0, 31)
}

/// Parse the numeric prefix with JavaScript `parseInt` semantics: optional
/// whitespace/sign, then either `0x` hexadecimal or decimal digits.
pub(super) fn parse_int_prefix(text: &str) -> Option<i64> {
    let s = text.trim_start();
    let bytes = s.as_bytes();
    let mut index = 0;
    let mut negative = false;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        negative = bytes[index] == b'-';
        index += 1;
    }
    let (base, start) = if index + 1 < bytes.len()
        && bytes[index] == b'0'
        && (bytes[index + 1] == b'x' || bytes[index + 1] == b'X')
    {
        (16, index + 2)
    } else {
        (10, index)
    };
    let mut end = start;
    while end < bytes.len() && (bytes[end] as char).is_digit(base) {
        end += 1;
    }
    if end == start {
        return None;
    }
    let value = i128::from_str_radix(&s[start..end], base).ok()?;
    Some(if negative {
        -(value as i64)
    } else {
        value as i64
    })
}

fn const_value(value: &str, min: i64, max: i64) -> Result<i64, String> {
    let d = parse_int_prefix(value).ok_or_else(|| "constant is not a number.".to_string())?;
    if d < min || d > max {
        return Err(format!("[Ks] out of range: {min}<>{max}"));
    }
    Ok(d)
}

fn cval(value: &str) -> Result<i64, String> {
    const_value(value, 0, 255)
}

fn fit_two_c(value: i64, bits: u32) -> Result<u16, String> {
    if bits < 2 {
        return Err("Need at least 2 bits to be signed.".into());
    }
    if bits > 16 {
        return Err("fitTwoC only works on 16bit numbers for now.".into());
    }
    if value.abs() > (1_i64 << (bits - 1)) {
        return Err(format!("Not enough bits for number. ({value}, {bits})"));
    }
    let value = if value < 0 { 0xffff + value + 1 } else { value };
    Ok((value as u16) & (0xffff_u16 >> (16 - bits)))
}

fn const_or_label(value: &str, labels: &Labels, offset: i64) -> Option<i64> {
    match parse_int_prefix(value) {
        Some(value) => Some(value),
        None => labels.get(value).map(|label| *label as i64 - offset),
    }
}

fn pair_reg(r: &str) -> Option<i64> {
    let bytes = r.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] != b'r' && bytes[i] != b'R' {
            continue;
        }
        let digits = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
        let value = digits.parse::<i64>().ok()?;
        if matches!(value, 24 | 26 | 28 | 30) {
            return Some(value);
        }
    }
    None
}

fn stld_xyz(xyz: &str) -> Result<u16, String> {
    match xyz {
        "X" => Ok(0x900c),
        "X+" => Ok(0x900d),
        "-X" => Ok(0x900e),
        "Y" => Ok(0x8008),
        "Y+" => Ok(0x9009),
        "-Y" => Ok(0x900a),
        "Z" => Ok(0x8000),
        "Z+" => Ok(0x9001),
        "-Z" => Ok(0x9002),
        _ => Err("Not -?[XYZ]\\+?".into()),
    }
}

fn stld_yzq(yzq: &str) -> Result<u16, String> {
    let bytes = yzq.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if (bytes[i] != b'Y' && bytes[i] != b'Z')
            || bytes[i + 1] != b'+'
            || !bytes[i + 2].is_ascii_digit()
        {
            continue;
        }
        let mut end = i + 2;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let q = const_value(&yzq[i + 2..end], 0, 64).map_err(|_| "q is out of range")?;
        let mut r = 0x8000_u16;
        if bytes[i] == b'Y' {
            r |= 0x8;
        }
        let q = q as u16;
        r |= ((q & 0x20) << 8) | ((q & 0x18) << 7) | (q & 0x7);
        return Ok(r);
    }
    Err("Invalid arguments".into())
}

fn se_flag(a: &str) -> Result<u16, String> {
    Ok(0x9408 | (((const_value(a, 0, 7)? as u16) & 0x7) << 4))
}

fn immediate(base: u16, a: &str, b: &str) -> Result<u16, String> {
    let d = dest_r(a, 16, 31)? & 0xf0;
    let k = cval(b)? as u16;
    Ok(base | d | ((k & 0xf0) << 4) | (k & 0x0f))
}

fn word_immediate(base: u16, a: &str, b: &str) -> Result<u16, String> {
    let d = pair_reg(a).ok_or_else(|| "Rd must be 24, 26, 28, or 30".to_string())?;
    let d = ((d - 24) / 2) & 0x3;
    let k = const_value(b, 0, 63)? as u16;
    Ok(base | ((d as u16) << 4) | ((k & 0x30) << 2) | (k & 0x0f))
}

fn io_bit(base: u16, a: &str, b: &str) -> Result<u16, String> {
    let address = const_value(a, 0, 31)? as u16;
    let bit = const_value(b, 0, 7)? as u16;
    Ok(base | (address << 3) | bit)
}

fn branch(
    base: u16,
    bit: u16,
    operand: &str,
    byte_loc: u32,
    labels: &Labels,
) -> Result<Encoded, String> {
    let offset = byte_loc as i64 + 2;
    if let Some(value) = const_or_label(operand, labels, offset) {
        let value = const_value(&(value >> 1).to_string(), -64, 63)?;
        return Ok(one(base | (bit & 0x7) | (fit_two_c(value, 7)? << 3)));
    }

    let operand = operand.to_string();
    Ok(Encoded::Deferred(Box::new(move |labels| {
        let Some(value) = const_or_label(&operand, labels, offset) else {
            return Vec::new();
        };
        let Ok(value) = const_value(&(value >> 1).to_string(), -64, 63) else {
            return Vec::new();
        };
        let Ok(value) = fit_two_c(value, 7) else {
            return Vec::new();
        };
        vec![base | (bit & 0x7) | (value << 3)]
    })))
}

fn relative(
    base: u16,
    low: i64,
    high: i64,
    bits: u32,
    operand: &str,
    byte_loc: u32,
    labels: &Labels,
) -> Result<Encoded, String> {
    let offset = byte_loc as i64 + 2;
    if let Some(value) = const_or_label(operand, labels, offset) {
        let value = const_value(&(value >> 1).to_string(), low, high)?;
        return Ok(one(base | fit_two_c(value, bits)?));
    }

    let operand = operand.to_string();
    Ok(Encoded::Deferred(Box::new(move |labels| {
        let Some(value) = const_or_label(&operand, labels, offset) else {
            return Vec::new();
        };
        let Ok(value) = const_value(&(value >> 1).to_string(), low, high) else {
            return Vec::new();
        };
        let Ok(value) = fit_two_c(value, bits) else {
            return Vec::new();
        };
        vec![base | value]
    })))
}

fn absolute_words(base: u16, value: i64) -> Vec<u16> {
    let value = value >> 1;
    let low = (value & 0xffff) as u16;
    let high = (value >> 16) & 0x3f;
    let word = base | (((high & 0x3e) as u16) << 3) | ((high & 1) as u16);
    vec![word, low]
}

fn absolute(base: u16, operand: &str, labels: &Labels) -> Result<Encoded, String> {
    if let Some(value) = const_or_label(operand, labels, 0) {
        let value = const_value(&value.to_string(), 0, 0x40_0000)?;
        return Ok(Encoded::Words(absolute_words(base, value)));
    }

    let operand = operand.to_string();
    Ok(Encoded::Deferred(Box::new(move |labels| {
        let Some(value) = const_or_label(&operand, labels, 0) else {
            return Vec::new();
        };
        let Ok(value) = const_value(&value.to_string(), 0, 0x40_0000) else {
            return Vec::new();
        };
        absolute_words(base, value)
    })))
}

pub(super) fn encode(
    instruction: &str,
    a: &str,
    b: &str,
    byte_loc: u32,
    labels: &Labels,
) -> Result<Encoded, String> {
    match instruction {
        "ADD" => Ok(one(0x0c00 | dr(a)? | sr(b)?)),
        "ADC" => Ok(one(0x1c00 | dr(a)? | sr(b)?)),
        "ADIW" => Ok(one(word_immediate(0x9600, a, b)?)),
        "AND" => Ok(one(0x2000 | dr(a)? | sr(b)?)),
        "ANDI" => Ok(one(immediate(0x7000, a, b)?)),
        "ASR" => Ok(one(0x9405 | dr(a)?)),
        "BCLR" => Ok(one(0x9488 | (((const_value(a, 0, 7)? as u16) & 0x7) << 4))),
        "BLD" => Ok(one(0xf800 | dr(a)? | (cval(b)? as u16 & 0x7))),
        "BRBC" => branch(0xf400, const_value(a, 0, 7)? as u16, b, byte_loc, labels),
        "BRBS" => branch(0xf000, const_value(a, 0, 7)? as u16, b, byte_loc, labels),
        "BRCC" => branch(0xf400, 0, a, byte_loc, labels),
        "BRCS" => branch(0xf000, 0, a, byte_loc, labels),
        "BREAK" => Ok(one(0x9598)),
        "BREQ" => branch(0xf000, 1, a, byte_loc, labels),
        "BRGE" => branch(0xf400, 4, a, byte_loc, labels),
        "BRHC" => branch(0xf400, 5, a, byte_loc, labels),
        "BRHS" => branch(0xf000, 5, a, byte_loc, labels),
        "BRID" => branch(0xf400, 7, a, byte_loc, labels),
        "BRIE" => branch(0xf000, 7, a, byte_loc, labels),
        "BRLO" => branch(0xf000, 0, a, byte_loc, labels),
        "BRLT" => branch(0xf000, 4, a, byte_loc, labels),
        "BRMI" => branch(0xf000, 2, a, byte_loc, labels),
        "BRNE" => branch(0xf400, 1, a, byte_loc, labels),
        "BRPL" => branch(0xf400, 2, a, byte_loc, labels),
        "BRSH" => branch(0xf400, 0, a, byte_loc, labels),
        "BRTC" => branch(0xf400, 6, a, byte_loc, labels),
        "BRTS" => branch(0xf000, 6, a, byte_loc, labels),
        "BRVC" => branch(0xf400, 3, a, byte_loc, labels),
        "BRVS" => branch(0xf000, 3, a, byte_loc, labels),
        "BSET" => Ok(one(0x9408 | (((const_value(a, 0, 7)? as u16) & 0x7) << 4))),
        "BST" => Ok(one(0xfa00 | dr(a)? | (cval(b)? as u16 & 0x7))),
        "CALL" => absolute(0x940e, a, labels),
        "CBI" => Ok(one(io_bit(0x9800, a, b)?)),
        "CRB" => {
            let value = cval(b)?;
            Ok(one(immediate(
                0x7000,
                a,
                &((!value & 0xff) as u16).to_string(),
            )?))
        }
        "CLC" => Ok(one(0x9488)),
        "CLH" => Ok(one(0x94d8)),
        "CLI" => Ok(one(0x94f8)),
        "CLN" => Ok(one(0x94a8)),
        "CLR" => Ok(one(0x2400 | dr(a)? | sr(a)?)),
        "CLS" => Ok(one(0x94c8)),
        "CLT" => Ok(one(0x94e8)),
        "CLV" => Ok(one(0x94b8)),
        "CLZ" => Ok(one(0x9498)),
        "COM" => Ok(one(0x9400 | dr(a)?)),
        "CP" => Ok(one(0x1400 | dr(a)? | sr(b)?)),
        "CPC" => Ok(one(0x0400 | dr(a)? | sr(b)?)),
        "CPI" => Ok(one(immediate(0x3000, a, b)?)),
        "CPSE" => Ok(one(0x1000 | dr(a)? | sr(b)?)),
        "DEC" => Ok(one(0x940a | dr(a)?)),
        "DES" => Ok(one(0x940b | ((const_value(a, 0, 15)? as u16) << 4))),
        "EICALL" => Ok(one(0x9519)),
        "EIJMP" => Ok(one(0x9419)),
        "ELPM" => {
            if a.is_empty() {
                Ok(one(0x95d8))
            } else {
                let mut r = 0x9000 | dr(a)?;
                match b {
                    "Z" => r |= 6,
                    "Z+" => r |= 7,
                    _ => return Err("Bad operand".into()),
                }
                Ok(one(r))
            }
        }
        "EOR" => Ok(one(0x2400 | dr(a)? | sr(b)?)),
        "FMUL" => Ok(one(0x0308
            | (dest_r(a, 16, 23)? & 0x70)
            | (src_r(b, 16, 23)? & 0x7))),
        "FMULS" => Ok(one(0x0380
            | (dest_r(a, 16, 23)? & 0x70)
            | (src_r(b, 16, 23)? & 0x7))),
        "FMULSU" => Ok(one(0x0388
            | (dest_r(a, 16, 23)? & 0x70)
            | (src_r(b, 16, 23)? & 0x7))),
        "ICALL" => Ok(one(0x9509)),
        "IJMP" => Ok(one(0x9409)),
        "IN" => {
            let address = const_value(b, 0, 63)? as u16;
            Ok(one(0xb000
                | dr(a)?
                | ((address & 0x30) << 5)
                | (address & 0x0f)))
        }
        "INC" => Ok(one(0x9403 | dr(a)?)),
        "JMP" => absolute(0x940c, a, labels),
        "LAC" | "LAS" | "LAT" => {
            if a != "Z" {
                return Err("First Operand is not Z".into());
            }
            let base = match instruction {
                "LAC" => 0x9206,
                "LAS" => 0x9205,
                _ => 0x9207,
            };
            Ok(one(base | dr(b)?))
        }
        "LD" => Ok(one(dr(a)? | stld_xyz(b)?)),
        "LDD" => Ok(one(dr(a)? | stld_yzq(b)?)),
        "LDI" => Ok(one(immediate(0xe000, a, b)?)),
        "LDS" => {
            let k = const_value(b, 0, 65535)? as u16;
            Ok(Encoded::Words(vec![0x9000 | dr(a)?, k]))
        }
        "LPM" => {
            if a.is_empty() {
                Ok(one(0x95c8))
            } else {
                let mut r = 0x9000 | dr(a)?;
                match b {
                    "Z" => r |= 4,
                    "Z+" => r |= 5,
                    _ => return Err("Bad operand".into()),
                }
                Ok(one(r))
            }
        }
        "LSL" => Ok(one(0x0c00 | dr(a)? | sr(a)?)),
        "LSR" => Ok(one(0x9406 | dr(a)?)),
        "MOV" => Ok(one(0x2c00 | dr(a)? | sr(b)?)),
        "MOVW" => Ok(one(0x0100
            | ((dr(a)? >> 1) & 0xf0)
            | ((dr(b)? >> 5) & 0x0f))),
        "MUL" => Ok(one(0x9c00 | dr(a)? | sr(b)?)),
        "MULS" => Ok(one(0x0200
            | (dest_r(a, 16, 31)? & 0xf0)
            | (src_r(b, 16, 31)? & 0x0f))),
        "MULSU" => Ok(one(0x0300
            | (dest_r(a, 16, 23)? & 0x70)
            | (src_r(b, 16, 23)? & 0x7))),
        "NEG" => Ok(one(0x9401 | dr(a)?)),
        "NOP" => Ok(one(0x0000)),
        "OR" => Ok(one(0x2800 | dr(a)? | sr(b)?)),
        "ORI" => Ok(one(immediate(0x6000, a, b)?)),
        "OUT" => {
            let address = const_value(a, 0, 63)? as u16;
            Ok(one(0xb800
                | dr(b)?
                | ((address & 0x30) << 5)
                | (address & 0x0f)))
        }
        "POP" => Ok(one(0x900f | dr(a)?)),
        "PUSH" => Ok(one(0x920f | dr(a)?)),
        "RCALL" => relative(0xd000, -2048, 2047, 12, a, byte_loc, labels),
        "RET" => Ok(one(0x9508)),
        "RETI" => Ok(one(0x9518)),
        "RJMP" => relative(0xc000, -2048, 2047, 12, a, byte_loc, labels),
        "ROL" => Ok(one(0x1c00 | dr(a)? | sr(a)?)),
        "ROR" => Ok(one(0x9407 | dr(a)?)),
        "SBC" => Ok(one(0x0800 | dr(a)? | sr(b)?)),
        "SBCI" => Ok(one(immediate(0x4000, a, b)?)),
        "SBI" => Ok(one(io_bit(0x9a00, a, b)?)),
        "SBIC" => Ok(one(io_bit(0x9900, a, b)?)),
        "SBIS" => Ok(one(io_bit(0x9b00, a, b)?)),
        "SBIW" => Ok(one(word_immediate(0x9700, a, b)?)),
        "SBR" => Ok(one(immediate(0x6000, a, b)?)),
        "SBRC" => Ok(one(0xfc00 | dr(a)? | (cval(b)? as u16 & 0x7))),
        "SBRS" => Ok(one(0xfe00 | dr(a)? | (cval(b)? as u16 & 0x7))),
        "SEC" => Ok(one(se_flag("0")?)),
        "SEH" => Ok(one(se_flag("5")?)),
        "SEI" => Ok(one(se_flag("7")?)),
        "SEN" => Ok(one(se_flag("2")?)),
        "SER" => Ok(one(0xef0f | (dest_r(a, 16, 31)? & 0xf0))),
        "SES" => Ok(one(se_flag("4")?)),
        "SET" => Ok(one(se_flag("6")?)),
        "SEV" => Ok(one(se_flag("3")?)),
        "SEZ" => Ok(one(se_flag("1")?)),
        "SLEEP" => Ok(one(0x9588)),
        "SPM" => {
            if a.is_empty() {
                Ok(one(0x95e8))
            } else if a == "Z+" {
                Ok(one(0x95f8))
            } else {
                Err("Bad param to SPM".into())
            }
        }
        "ST" => Ok(one(0x0200 | dr(b)? | stld_xyz(a)?)),
        "STD" => Ok(one(0x0200 | dr(b)? | stld_yzq(a)?)),
        "STS" => {
            let k = const_value(a, 0, 65535)? as u16;
            Ok(Encoded::Words(vec![0x9200 | dr(b)?, k]))
        }
        "SUB" => Ok(one(0x1800 | dr(a)? | sr(b)?)),
        "SUBI" => Ok(one(immediate(0x5000, a, b)?)),
        "SWAP" => Ok(one(0x9402 | dr(a)?)),
        "TST" => Ok(one(0x2000 | dr(a)? | sr(a)?)),
        "WDR" => Ok(one(0x95a8)),
        "XCH" => {
            let word = 0x9204 | dr(b)?;
            if a != "Z" {
                return Err("Bad param, not Z".into());
            }
            Ok(one(word))
        }
        _ => Err(format!("No such instruction: {instruction}")),
    }
}
