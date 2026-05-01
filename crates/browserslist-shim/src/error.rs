//! Port of `browserslist/error.js`.

use std::fmt;

#[derive(Debug, Clone)]
pub struct BrowserslistError { pub message: String }

impl fmt::Display for BrowserslistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BrowserslistError: {}", self.message)
    }
}

impl std::error::Error for BrowserslistError {}
