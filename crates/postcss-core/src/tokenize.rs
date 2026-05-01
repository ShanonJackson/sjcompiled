//! Port of `postcss/lib/tokenize.js` (postcss@8.4.31).
//! Mirrored line-for-line. Token shape upstream is a JS array:
//!   `[type, content]` for simple punctuation,
//!   `[type, content, pos]` for control chars and unmatched parens,
//!   `[type, content, pos, next]` for ranged tokens (string, brackets, at-word, word, comment).
//!
//! In Rust we use a tagged [`Token`] with an explicit kind and optional
//! `next` end-offset. The byte-content is preserved in `content` so the
//! parser/stringifier path round-trips losslessly.
//!
//! All regex semantics are ported via `regex` crate. Where upstream relies on
//! `RegExp.prototype.lastIndex` (global regex + `test()` advancing the index),
//! we implement the equivalent with `regex::Regex::find_at(haystack, pos)`.

use crate::css_syntax_error::CssSyntaxError;
use crate::input::Input;
use once_cell::sync::Lazy;
use regex::Regex;

const SINGLE_QUOTE: u8 = b'\'';
const DOUBLE_QUOTE: u8 = b'"';
const BACKSLASH: u8 = b'\\';
const SLASH: u8 = b'/';
const NEWLINE: u8 = b'\n';
const SPACE: u8 = b' ';
const FEED: u8 = 0x0C; // \f
const TAB: u8 = b'\t';
const CR: u8 = b'\r';
const OPEN_SQUARE: u8 = b'[';
const CLOSE_SQUARE: u8 = b']';
const OPEN_PARENTHESES: u8 = b'(';
const CLOSE_PARENTHESES: u8 = b')';
const OPEN_CURLY: u8 = b'{';
const CLOSE_CURLY: u8 = b'}';
const SEMICOLON: u8 = b';';
const ASTERISK: u8 = b'*';
const COLON: u8 = b':';
const AT: u8 = b'@';

// `/[\t\n\f\r "#'()/;[\\\]{}]/g`
static RE_AT_END: Lazy<Regex> =
    Lazy::new(|| Regex::new(r##"[\t\n\x0c\r "#'()/;\[\\\]{}]"##).unwrap());
// `/[\t\n\f\r !"#'():;@[\\\]{}]|\/(?=\*)/g` — note `/(?=*)` lookahead.
// Rust `regex` crate doesn't support lookahead. Mirror by scanning for the
// character class first, then handling the `/*` case as a fallback. The
// caller always inspects the byte at `next` so the lookahead-equivalent is
// implemented in [`find_word_end`].
static RE_WORD_END_CLASS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r##"[\t\n\x0c\r !"#'():;@\[\\\]{}]"##).unwrap());
// `/.[\r\n"'(/\\]/` — single byte then a forbidden byte.
static RE_BAD_BRACKET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#".[\r\n"'(/\\]"#).unwrap());
// `/[\da-f]/i`
static RE_HEX_ESCAPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[\da-f]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Space,
    /// `[` `]` `{` `}` `:` `;` `)` — content matches kind char.
    OpenSquare,
    CloseSquare,
    OpenCurly,
    CloseCurly,
    Colon,
    Semicolon,
    CloseParen,
    /// `(` only, when unmatched.
    OpenParen,
    Brackets,
    String,
    AtWord,
    Word,
    Comment,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub content: String,
    pub pos: Option<usize>,
    pub next: Option<usize>,
}

pub struct Tokenizer<'a> {
    css: &'a str,
    bytes: &'a [u8],
    length: usize,
    pos: usize,
    buffer: Vec<Token>,
    returned: Vec<Token>,
    ignore: bool,
}

pub fn tokenizer<'a>(input: &'a Input, ignore_errors: bool) -> Tokenizer<'a> {
    let css = input.css.as_str();
    Tokenizer {
        css,
        bytes: css.as_bytes(),
        length: css.len(),
        pos: 0,
        buffer: Vec::new(),
        returned: Vec::new(),
        ignore: ignore_errors,
    }
}

impl<'a> Tokenizer<'a> {
    pub fn position(&self) -> usize { self.pos }

    pub fn end_of_file(&self) -> bool {
        self.returned.is_empty() && self.pos >= self.length
    }

    pub fn back(&mut self, t: Token) { self.returned.push(t); }

    fn unclosed(&self, what: &str) -> CssSyntaxError {
        CssSyntaxError::new(format!("Unclosed {}", what), None, Some(self.pos))
    }

    /// Mirrors upstream `nextToken(opts)`. Returns `None` at EOF.
    pub fn next_token(&mut self, ignore_unclosed: bool) -> Result<Option<Token>, CssSyntaxError> {
        if let Some(t) = self.returned.pop() { return Ok(Some(t)); }
        if self.pos >= self.length { return Ok(None); }
        let code = self.bytes[self.pos];
        let current: Token;

        match code {
            NEWLINE | SPACE | TAB | CR | FEED => {
                let mut next = self.pos;
                loop {
                    next += 1;
                    if next >= self.length { break; }
                    let c = self.bytes[next];
                    if !(c == SPACE || c == NEWLINE || c == TAB || c == CR || c == FEED) { break; }
                }
                current = Token {
                    kind: TokenKind::Space,
                    content: self.css[self.pos..next].to_string(),
                    pos: None,
                    next: None,
                };
                self.pos = next - 1;
            }
            OPEN_SQUARE | CLOSE_SQUARE | OPEN_CURLY | CLOSE_CURLY | COLON | SEMICOLON | CLOSE_PARENTHESES => {
                let kind = match code {
                    OPEN_SQUARE => TokenKind::OpenSquare,
                    CLOSE_SQUARE => TokenKind::CloseSquare,
                    OPEN_CURLY => TokenKind::OpenCurly,
                    CLOSE_CURLY => TokenKind::CloseCurly,
                    COLON => TokenKind::Colon,
                    SEMICOLON => TokenKind::Semicolon,
                    CLOSE_PARENTHESES => TokenKind::CloseParen,
                    _ => unreachable!(),
                };
                let s = (code as char).to_string();
                current = Token { kind, content: s, pos: Some(self.pos), next: None };
            }
            OPEN_PARENTHESES => {
                let prev = self.buffer.pop().map(|t| t.content).unwrap_or_default();
                let n = if self.pos + 1 < self.length { self.bytes[self.pos + 1] } else { 0 };
                if prev == "url"
                    && n != SINGLE_QUOTE && n != DOUBLE_QUOTE
                    && n != SPACE && n != NEWLINE && n != TAB && n != FEED && n != CR
                {
                    let mut next = self.pos;
                    loop {
                        let mut escaped = false;
                        match self.css[next + 1..].find(')') {
                            Some(rel) => { next = next + 1 + rel; }
                            None => {
                                if self.ignore || ignore_unclosed { next = self.pos; break; }
                                else { return Err(self.unclosed("bracket")); }
                            }
                        }
                        let mut escape_pos = next;
                        while escape_pos > 0 && self.bytes[escape_pos - 1] == BACKSLASH {
                            escape_pos -= 1;
                            escaped = !escaped;
                        }
                        if !escaped { break; }
                    }
                    current = Token {
                        kind: TokenKind::Brackets,
                        content: self.css[self.pos..=next].to_string(),
                        pos: Some(self.pos),
                        next: Some(next),
                    };
                    self.pos = next;
                } else {
                    let next = self.css[self.pos + 1..].find(')').map(|r| self.pos + 1 + r);
                    match next {
                        Some(n) => {
                            let content = &self.css[self.pos..=n];
                            if RE_BAD_BRACKET.is_match(content) {
                                current = Token { kind: TokenKind::OpenParen, content: "(".to_string(), pos: Some(self.pos), next: None };
                            } else {
                                current = Token {
                                    kind: TokenKind::Brackets,
                                    content: content.to_string(),
                                    pos: Some(self.pos),
                                    next: Some(n),
                                };
                                self.pos = n;
                            }
                        }
                        None => {
                            current = Token { kind: TokenKind::OpenParen, content: "(".to_string(), pos: Some(self.pos), next: None };
                        }
                    }
                }
            }
            SINGLE_QUOTE | DOUBLE_QUOTE => {
                let quote = code as char;
                let mut next = self.pos;
                loop {
                    let mut escaped = false;
                    match self.css[next + 1..].find(quote) {
                        Some(rel) => { next = next + 1 + rel; }
                        None => {
                            if self.ignore || ignore_unclosed { next = self.pos + 1; break; }
                            else { return Err(self.unclosed("string")); }
                        }
                    }
                    let mut escape_pos = next;
                    while escape_pos > 0 && self.bytes[escape_pos - 1] == BACKSLASH {
                        escape_pos -= 1;
                        escaped = !escaped;
                    }
                    if !escaped { break; }
                }
                current = Token {
                    kind: TokenKind::String,
                    content: self.css[self.pos..=next].to_string(),
                    pos: Some(self.pos),
                    next: Some(next),
                };
                self.pos = next;
            }
            AT => {
                // RE_AT_END.lastIndex = pos+1; .test(css). If lastIndex stays 0 → EOF.
                let next = match RE_AT_END.find_at(self.css, self.pos + 1) {
                    Some(m) => m.end() - 2, // last_index points after match; -2 to land on char before.
                    None => self.length - 1,
                };
                current = Token {
                    kind: TokenKind::AtWord,
                    content: self.css[self.pos..=next].to_string(),
                    pos: Some(self.pos),
                    next: Some(next),
                };
                self.pos = next;
            }
            BACKSLASH => {
                let mut next = self.pos;
                let mut escape = true;
                while next + 1 < self.length && self.bytes[next + 1] == BACKSLASH {
                    next += 1;
                    escape = !escape;
                }
                let code2 = if next + 1 < self.length { self.bytes[next + 1] } else { 0 };
                if escape
                    && code2 != SLASH && code2 != SPACE && code2 != NEWLINE
                    && code2 != TAB && code2 != CR && code2 != FEED
                {
                    next += 1;
                    if next < self.length && RE_HEX_ESCAPE.is_match(&self.css[next..next + 1]) {
                        while next + 1 < self.length && RE_HEX_ESCAPE.is_match(&self.css[next + 1..next + 2]) {
                            next += 1;
                        }
                        if next + 1 < self.length && self.bytes[next + 1] == SPACE {
                            next += 1;
                        }
                    }
                }
                current = Token {
                    kind: TokenKind::Word,
                    content: self.css[self.pos..=next].to_string(),
                    pos: Some(self.pos),
                    next: Some(next),
                };
                self.pos = next;
            }
            _ => {
                if code == SLASH && self.pos + 1 < self.length && self.bytes[self.pos + 1] == ASTERISK {
                    let next = match self.css[self.pos + 2..].find("*/") {
                        Some(rel) => self.pos + 2 + rel + 1,
                        None => {
                            if self.ignore || ignore_unclosed { self.length } else { return Err(self.unclosed("comment")); }
                        }
                    };
                    current = Token {
                        kind: TokenKind::Comment,
                        content: self.css[self.pos..=next.min(self.length - 1)].to_string(),
                        pos: Some(self.pos),
                        next: Some(next),
                    };
                    self.pos = next;
                } else {
                    let next = find_word_end(self.css, self.pos + 1, self.length);
                    current = Token {
                        kind: TokenKind::Word,
                        content: self.css[self.pos..=next].to_string(),
                        pos: Some(self.pos),
                        next: Some(next),
                    };
                    self.buffer.push(current.clone());
                    self.pos = next;
                }
            }
        }

        self.pos += 1;
        Ok(Some(current))
    }
}

/// Equivalent of the upstream `RE_WORD_END` lookahead: scans `css[start..]`
/// for either the character class or `/*`. Returns the index of the byte
/// before the match (matching upstream `lastIndex - 2` semantics).
fn find_word_end(css: &str, start: usize, length: usize) -> usize {
    let class_match = RE_WORD_END_CLASS.find_at(css, start);
    let bytes = css.as_bytes();
    // Find next `/*` starting from `start`.
    let mut slash_match: Option<usize> = None;
    let mut i = start;
    while i + 1 < length {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' { slash_match = Some(i); break; }
        i += 1;
    }
    match (class_match, slash_match) {
        (Some(m), Some(s)) => {
            let class_pos = m.start();
            if class_pos < s { m.end() - 2 } else { s.saturating_sub(1) }
        }
        (Some(m), None) => m.end() - 2,
        (None, Some(s)) => s.saturating_sub(1),
        (None, None) => length - 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;

    fn collect(css: &str) -> Vec<(TokenKind, String)> {
        let input = Input::new(css.to_string(), None);
        let mut t = tokenizer(&input, false);
        let mut out = Vec::new();
        while let Some(tok) = t.next_token(false).unwrap() {
            out.push((tok.kind, tok.content));
        }
        out
    }

    #[test]
    fn simple_decl() {
        let toks = collect("a{color:red}");
        assert_eq!(toks[0].0, TokenKind::Word);
        assert_eq!(toks[0].1, "a");
        assert_eq!(toks[1].0, TokenKind::OpenCurly);
        assert_eq!(toks[2].0, TokenKind::Word);
        assert_eq!(toks[2].1, "color");
        assert_eq!(toks[3].0, TokenKind::Colon);
        assert_eq!(toks[4].0, TokenKind::Word);
        assert_eq!(toks[4].1, "red");
        assert_eq!(toks[5].0, TokenKind::CloseCurly);
    }

    #[test]
    fn at_rule() {
        let toks = collect("@media screen");
        assert_eq!(toks[0].0, TokenKind::AtWord);
        assert_eq!(toks[0].1, "@media");
    }

    #[test]
    fn comment_passthrough() {
        let toks = collect("/* hi */a{}");
        assert_eq!(toks[0].0, TokenKind::Comment);
        assert_eq!(toks[0].1, "/* hi */");
    }

    #[test]
    fn string_literal() {
        let toks = collect("a[b='c']");
        assert!(toks.iter().any(|t| t.0 == TokenKind::String && t.1 == "'c'"));
    }

    #[test]
    fn url_brackets() {
        let toks = collect("a{background:url(foo.png)}");
        assert!(toks.iter().any(|t| t.0 == TokenKind::Brackets && t.1 == "(foo.png)"));
    }

    #[test]
    fn whitespace_run() {
        let toks = collect("  \t\n a");
        assert_eq!(toks[0].0, TokenKind::Space);
        assert_eq!(toks[0].1, "  \t\n ");
    }
}
