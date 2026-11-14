//! Errors, as `file:line: message`.

use alloc::string::String;
use core::fmt;

/// Something wrong in a script: found while reading it (`col` is then the
/// column) or while running it (`col` is 0).
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptError {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

impl ScriptError {
    pub fn new(file: &str, line: u32, col: u32, msg: impl Into<String>) -> ScriptError {
        ScriptError { file: String::from(file), line, col, msg: msg.into() }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.col > 0 {
            write!(f, "{}:{}:{}: {}", self.file, self.line, self.col, self.msg)
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.msg)
        }
    }
}
