//! Port of `postcss/lib/input.js`.
//!
//! `Input` carries the original CSS source and its origin (file path / id).
//! `fromOffset(offset)` upstream returns `{ line, col }` for source maps —
//! we mirror it here even though the parity port doesn't generate source
//! maps. The function is still called by `parser.js` to populate
//! `node.source.start` / `node.source.end`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::css_syntax_error::CssSyntaxError;

static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct Input {
    pub css: String,
    pub has_bom: bool,
    pub from: Option<String>,
    pub id: u64,
    pub file: Option<String>,
    /// Lazy line offsets, computed on first `from_offset` call.
    line_starts: std::cell::RefCell<Option<Vec<usize>>>,
}

impl Input {
    pub fn new(mut css: String, from: Option<String>) -> Self {
        let mut has_bom = false;
        if css.starts_with('\u{FEFF}') {
            has_bom = true;
            css = css.trim_start_matches('\u{FEFF}').to_string();
        }
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let file = from.clone();
        Input { css, has_bom, from, id, file, line_starts: std::cell::RefCell::new(None) }
    }

    /// Mirrors upstream `Input.prototype.fromOffset(offset)` — returns the
    /// 1-based `(line, col)` for the given byte offset.
    pub fn from_offset(&self, offset: usize) -> (usize, usize) {
        let starts = {
            let mut slot = self.line_starts.borrow_mut();
            if slot.is_none() {
                let mut starts = vec![0usize];
                let bytes = self.css.as_bytes();
                for (i, &b) in bytes.iter().enumerate() {
                    if b == b'\n' { starts.push(i + 1); }
                }
                *slot = Some(starts);
            }
            slot.clone().unwrap()
        };
        // Binary search for the largest start <= offset.
        let line_idx = match starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = starts.get(line_idx).copied().unwrap_or(0);
        // 1-based.
        (line_idx + 1, offset - line_start + 1)
    }

    pub fn error(&self, message: &str, offset: usize) -> CssSyntaxError {
        let (line, col) = self.from_offset(offset);
        CssSyntaxError::new(message.to_string(), Some(line), Some(col))
    }
}
