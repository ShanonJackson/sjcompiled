//! Port of `postcss-selector-parser/dist/tokenize.js`.
//!
//! Token shape upstream:
//!   `[type, startLine, startCol, endLine, endCol, startPos, endPos]`.
//! We mirror this with a [`Token`] struct.

use crate::tokenTypes as t;

#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: i32,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub start_pos: usize,
    pub end_pos: usize,
}

#[derive(Debug, Clone)]
pub struct TokenizeError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

const HEX: &[u8] = b"0123456789abcdefABCDEF";

fn is_hex(c: u8) -> bool { HEX.contains(&c) }
fn is_unescapable(code: i32) -> bool {
    code == t::tab || code == t::newline || code == t::cr || code == t::feed
}
fn is_word_delimiter(code: i32) -> bool {
    matches!(code,
        t::space | t::tab | t::newline | t::cr | t::feed
        | t::ampersand | t::asterisk | t::bang | t::comma | t::colon
        | t::semicolon | t::openParenthesis | t::closeParenthesis
        | t::openSquare | t::closeSquare | t::singleQuote | t::doubleQuote
        | t::plus | t::pipe | t::tilde | t::greaterThan | t::equals
        | t::dollar | t::caret | t::slash
    )
}

fn char_code_at(css: &[u8], pos: usize) -> i32 {
    css.get(pos).copied().map(|b| b as i32).unwrap_or(0)
}

/// `consumeWord(css, start)` — line 23 upstream.
fn consume_word(css: &[u8], start: usize) -> usize {
    let mut next = start;
    while next < css.len() {
        let code = char_code_at(css, next);
        if is_word_delimiter(code) { return next - 1; }
        if code == t::backslash {
            next = consume_escape(css, next) + 1;
        } else {
            next += 1;
        }
    }
    next - 1
}

/// `consumeEscape(css, start)` — line 45 upstream.
fn consume_escape(css: &[u8], start: usize) -> usize {
    let mut next = start;
    let code = char_code_at(css, next + 1);
    if is_unescapable(code) {
        // just consume the escape char
    } else if is_hex(code as u8) {
        let mut hex_digits = 0;
        loop {
            next += 1;
            hex_digits += 1;
            let code2 = char_code_at(css, next + 1);
            if !(is_hex(code2 as u8) && hex_digits < 6) { break; }
        }
        let trailing = char_code_at(css, next + 1);
        if hex_digits < 6 && trailing == t::space { next += 1; }
    } else {
        next += 1;
    }
    next
}

/// `tokenize(input)` — returns the token list. The Rust version returns
/// `Result` because upstream `unclosed()` throws.
pub fn tokenize(css: &str) -> Result<Vec<Token>, TokenizeError> {
    let bytes = css.as_bytes();
    let length = bytes.len();
    let mut tokens: Vec<Token> = Vec::new();
    let mut offset: i64 = -1;
    let mut line: usize = 1;
    let mut start: usize = 0;
    let mut end: usize;
    let mut next_offset_set: Option<i64> = None;

    while start < length {
        let mut code = char_code_at(bytes, start);
        if code == t::newline {
            offset = start as i64;
            line += 1;
        }
        let token_type;
        let end_line;
        let end_column;
        let next;
        let unclosed = |what: &str, _line: usize, _start: usize, _offset: i64| -> TokenizeError {
            TokenizeError { message: format!("Unclosed {what}"), line: _line, column: (_start as i64 - _offset) as usize }
        };

        match code {
            c if c == t::space || c == t::tab || c == t::newline || c == t::cr || c == t::feed => {
                let mut n = start;
                loop {
                    n += 1;
                    code = char_code_at(bytes, n);
                    if code == t::newline { offset = n as i64; line += 1; }
                    if !(code == t::space || code == t::newline || code == t::tab || code == t::cr || code == t::feed) { break; }
                }
                token_type = t::space;
                end_line = line;
                end_column = (n as i64 - offset - 1) as usize;
                end = n;
                next = n;
            }
            c if c == t::plus || c == t::greaterThan || c == t::tilde || c == t::pipe => {
                let mut n = start;
                loop {
                    n += 1;
                    code = char_code_at(bytes, n);
                    if !(code == t::plus || code == t::greaterThan || code == t::tilde || code == t::pipe) { break; }
                }
                token_type = t::combinator;
                end_line = line;
                end_column = (start as i64 - offset) as usize;
                end = n;
                next = n;
            }
            c if c == t::asterisk || c == t::ampersand || c == t::bang || c == t::comma
                || c == t::equals || c == t::dollar || c == t::caret
                || c == t::openSquare || c == t::closeSquare
                || c == t::colon || c == t::semicolon
                || c == t::openParenthesis || c == t::closeParenthesis => {
                token_type = code;
                end_line = line;
                end_column = (start as i64 - offset) as usize;
                next = start;
                end = next + 1;
            }
            c if c == t::singleQuote || c == t::doubleQuote => {
                let quote_byte = if c == t::singleQuote { b'\'' } else { b'"' };
                let mut n = start;
                loop {
                    let mut escaped = false;
                    let from = n + 1;
                    let found = bytes[from..].iter().position(|&b| b == quote_byte).map(|p| p + from);
                    let pos = match found {
                        Some(p) => p,
                        None => return Err(unclosed("quote", line, start, offset)),
                    };
                    n = pos;
                    let mut escape_pos = n;
                    while escape_pos > 0 && bytes[escape_pos - 1] == b'\\' {
                        escape_pos -= 1;
                        escaped = !escaped;
                    }
                    if !escaped { break; }
                }
                token_type = t::str;
                end_line = line;
                end_column = (start as i64 - offset) as usize;
                next = n;
                end = next + 1;
            }
            _ => {
                if code == t::slash && char_code_at(bytes, start + 1) == t::asterisk {
                    let from = start + 2;
                    let found = bytes[from..].windows(2).position(|w| w == b"*/").map(|p| p + from);
                    let n = match found {
                        Some(p) => p + 1,
                        None => return Err(unclosed("comment", line, start, offset)),
                    };
                    let content = &css[start..=n];
                    let mut last = 0usize;
                    let mut last_nl = 0usize;
                    let mut count = 0usize;
                    for (i, &b) in content.as_bytes().iter().enumerate() {
                        if b == b'\n' { count += 1; last = i; last_nl = i; }
                    }
                    let _ = last;
                    let (next_line, next_off) = if count > 0 {
                        (line + count, (start as i64 + last_nl as i64))
                    } else { (line, offset) };
                    token_type = t::comment;
                    line = next_line;
                    end_line = next_line;
                    end_column = (n as i64 - next_off) as usize;
                    next_offset_set = Some(next_off);
                    next = n;
                    end = next + 1;
                } else if code == t::slash {
                    token_type = code;
                    end_line = line;
                    end_column = (start as i64 - offset) as usize;
                    next = start;
                    end = next + 1;
                } else {
                    let n = consume_word(bytes, start);
                    token_type = t::word;
                    end_line = line;
                    end_column = (n as i64 - offset) as usize;
                    next = n;
                    end = next + 1;
                }
            }
        }

        tokens.push(Token {
            kind: token_type,
            start_line: line,
            start_col: (start as i64 - offset) as usize,
            end_line,
            end_col: end_column,
            start_pos: start,
            end_pos: end,
        });

        if let Some(no) = next_offset_set.take() { offset = no; }
        start = end;
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_selector() {
        let toks = tokenize(".foo").unwrap();
        // token list: word `.foo` (consumeWord starts at `.` which is not a delimiter).
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, t::word);
        assert_eq!(toks[0].start_pos, 0);
        assert_eq!(toks[0].end_pos, 4);
    }

    #[test]
    fn descendant_combinator() {
        let toks = tokenize(".a .b").unwrap();
        // word, space, word.
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0].kind, t::word);
        assert_eq!(toks[1].kind, t::space);
        assert_eq!(toks[2].kind, t::word);
    }

    #[test]
    fn child_combinator() {
        let toks = tokenize("a > b").unwrap();
        // word, space, combinator, space, word.
        assert!(toks.iter().any(|tok| tok.kind == t::combinator));
    }

    #[test]
    fn attribute_selector() {
        let toks = tokenize("a[b='c']").unwrap();
        let kinds: Vec<i32> = toks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&t::openSquare));
        assert!(kinds.contains(&t::closeSquare));
        assert!(kinds.contains(&t::str));
    }

    #[test]
    fn pseudo_with_function() {
        let toks = tokenize(":nth-child(2n+1)").unwrap();
        let kinds: Vec<i32> = toks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&t::colon));
        assert!(kinds.contains(&t::openParenthesis));
        assert!(kinds.contains(&t::closeParenthesis));
    }
}
