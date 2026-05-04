//! 1:1 port of `@babel/generator@7.23.0/lib/buffer.js`.
//!
//! Upstream buffer manages source maps + a queue-cursor optimisation
//! for hot append paths. Our port targets the byte-output contract
//! only (`.code` field) — the source-map machinery is irrelevant to
//! `compiled-utils::hash` consumption. We keep the same external API
//! shape so `printer.rs` ports line-for-line.
//!
//! Concretely we drop:
//! - `_map`, `_sourcePosition`, `_inputMap`, `mark`, `source*`,
//!   `withSource`, `_normalizePosition`, `getCurrentLine`,
//!   `sourceIdentifierName` machinery.
//! - The `_appendCount > 4096` `_buf += _str` flush optimisation
//!   (Rust `String` already grows in O(amortised 1)).
//!
//! We keep:
//! - The queue of pending chars (`_queue` / `_queueCursor`) — used
//!   for the `removeTrailingNewline` / `removeLastSemicolon` /
//!   `getLastChar` / `endsWithCharAndNewline` peek-and-rewrite
//!   semantics that printer.rs relies on.
//! - `_last` (last char appended) — used by `getLastChar` after the
//!   queue has drained.
//! - `getNewlineCount`, `getCurrentColumn` — same shape as upstream.

pub struct Buffer {
    /// Materialised output. Equivalent to upstream's `_buf + _str`.
    pub buf: String,
    /// Last char actually appended to `buf` (NOT in the queue).
    pub last: u8,
    /// Pending chars — queued so printer can peek/rewrite the tail
    /// before commit. Each item is `(char_byte, repeat_count)`.
    queue: Vec<(u8, u32)>,
    /// `(line, column)` of the next character. Maintained for
    /// `getCurrentColumn`. line/column are 1-based / 0-based, matching
    /// upstream.
    line: u32,
    column: u32,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            last: 0,
            queue: Vec::with_capacity(16),
            line: 1,
            column: 0,
        }
    }

    /// `_flush` — drain queue into buf.
    fn flush(&mut self) {
        for (ch, repeat) in self.queue.drain(..) {
            for _ in 0..repeat {
                self.buf.push(ch as char);
                if ch == b'\n' {
                    self.line += 1;
                    self.column = 0;
                } else {
                    self.column += 1;
                }
            }
            self.last = ch;
        }
    }

    /// `append(str, _maybeNewline)`.
    pub fn append(&mut self, s: &str) {
        self.flush();
        if s.is_empty() {
            return;
        }
        // Track line/column for `getCurrentColumn`.
        for ch in s.bytes() {
            if ch == b'\n' {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += 1;
            }
        }
        self.last = s.as_bytes()[s.len() - 1];
        self.buf.push_str(s);
    }

    /// `appendChar(char)`.
    pub fn append_char(&mut self, ch: u8) {
        self.flush();
        self.buf.push(ch as char);
        if ch == b'\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        self.last = ch;
    }

    /// `queue(char)` — pending char. Mirrors upstream's behaviour
    /// of dropping trailing whitespace before a queued newline so
    /// the canonical output never carries trailing spaces on a
    /// soft-broken line.
    pub fn queue(&mut self, ch: u8) {
        if ch == b'\n' {
            while let Some((last_ch, _)) = self.queue.last() {
                if *last_ch == b' ' || *last_ch == b'\t' {
                    self.queue.pop();
                } else {
                    break;
                }
            }
        }
        self.queue.push((ch, 1));
    }

    /// `queueIndentation(char, repeat)`.
    pub fn queue_indentation(&mut self, ch: u8, repeat: u32) {
        if repeat > 0 {
            self.queue.push((ch, repeat));
        }
    }

    /// `removeTrailingNewline` — pops a queued '\n' if present.
    pub fn remove_trailing_newline(&mut self) {
        if let Some((b'\n', _)) = self.queue.last() {
            self.queue.pop();
        }
    }

    /// `removeLastSemicolon` — pops a queued ';' if present.
    pub fn remove_last_semicolon(&mut self) {
        if let Some((b';', _)) = self.queue.last() {
            self.queue.pop();
        }
    }

    /// `getLastChar` — returns the last queued char, or `_last`
    /// if the queue is empty. Returns 0 if nothing has been written.
    pub fn get_last_char(&self) -> u8 {
        if let Some((ch, _)) = self.queue.last() {
            *ch
        } else {
            self.last
        }
    }

    /// `getNewlineCount` — count of trailing newlines in queue+last.
    pub fn get_newline_count(&self) -> u32 {
        if self.queue.is_empty() {
            return if self.last == b'\n' { 1 } else { 0 };
        }
        let mut count = 0u32;
        for (ch, _) in self.queue.iter().rev() {
            if *ch != b'\n' {
                return count;
            }
            count += 1;
        }
        // Entire queue is newlines; check `last` too.
        if self.last == b'\n' {
            count + 1
        } else {
            count
        }
    }

    /// `endsWithCharAndNewline` — if the queue ends with `\n`,
    /// return the char immediately before it (or `_last` if `\n`
    /// is the only queued item). Used by upstream to detect
    /// end-of-line punctuation policies.
    pub fn ends_with_char_and_newline(&self) -> Option<u8> {
        if let Some((b'\n', _)) = self.queue.last() {
            if self.queue.len() > 1 {
                Some(self.queue[self.queue.len() - 2].0)
            } else {
                Some(self.last)
            }
        } else {
            None
        }
    }

    /// `hasContent` — anything queued or appended yet?
    pub fn has_content(&self) -> bool {
        !self.queue.is_empty() || self.last != 0
    }

    /// `get` — flush and return the buffer trimmed of trailing
    /// whitespace (matches upstream's `trimRight` on `code`).
    pub fn get(mut self) -> String {
        self.flush();
        self.buf.truncate(self.buf.trim_end().len());
        self.buf
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}
