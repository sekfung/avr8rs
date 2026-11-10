use std::collections::BTreeMap;

mod opcodes;
mod parser;

#[cfg(test)]
mod tests;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct AssembleResult {
    pub bytes: Vec<u8>,
    pub errors: Vec<String>,
    pub lines: Vec<Line>,
    pub labels: BTreeMap<String, u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub byte_offset: u32,
    pub bytes: String,
    pub line: usize,
    pub text: String,
}

pub(crate) fn assemble(_source: &str) -> AssembleResult {
    let pass = parser::pass_one(_source);
    if !pass.errors.is_empty() {
        return AssembleResult {
            bytes: Vec::new(),
            errors: pass.errors,
            lines: Vec::new(),
            labels: BTreeMap::new(),
        };
    }
    parser::pass_two(pass.lines, pass.labels)
}
