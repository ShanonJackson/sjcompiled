//! Port of `postcss/lib/css-syntax-error.js`.

use std::fmt;

#[derive(Debug, Clone)]
pub struct CssSyntaxError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub plugin: Option<String>,
    pub file: Option<String>,
    pub source: Option<String>,
}

impl CssSyntaxError {
    pub fn new(message: String, line: Option<usize>, column: Option<usize>) -> Self {
        CssSyntaxError { message, line, column, plugin: None, file: None, source: None }
    }
}

impl fmt::Display for CssSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CssSyntaxError: {}", self.message)?;
        if let (Some(l), Some(c)) = (self.line, self.column) {
            write!(f, " (line {l}, col {c})")?;
        }
        Ok(())
    }
}

impl std::error::Error for CssSyntaxError {}
