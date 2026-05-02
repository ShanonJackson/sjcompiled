//! Port of `node_modules/postcss-discard-comments@5.1.2/src/lib/commentParser.js`.
//!
//! Folder-mapping deviation: upstream lives at `src/lib/commentParser.js`,
//! Rust ports it to `src/comment_parser.rs` (the parent crate's root file
//! is `src/lib.rs`, which Rust treats as the crate root — a child module
//! literally named `lib` would collide with the crate-root file). Behavior
//! is 1:1 with upstream including its bugs (unclosed-comment edge case
//! produces an out-of-range slice index — same as upstream).

/// Token kind. Mirrors the leading `0`/`1` flag in upstream's
/// `[type, start, end]` triples.
///
/// - `Text` — non-comment span (start..end is the literal text).
/// - `Comment` — comment body (start..end is the body BETWEEN `/*` and
///   `*/`, not including those delimiters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Comment,
}

#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    /// Upstream uses signed integers and tolerates `end == -1` for
    /// unclosed comments (-1 silently flips to a slice that takes
    /// "everything but the last char"). We match the byte-equivalent
    /// behavior by storing the JS-equivalent end value (`usize::MAX`
    /// for the unclosed sentinel) and resolving it at slice time.
    pub end: usize,
}

/// Sentinel for upstream's `end = -1` produced when `*/` is missing.
/// Upstream uses this index to produce a `str.slice(start, -1)` call,
/// which in JS means "everything except the last char". We replicate
/// at slice time.
pub const UNCLOSED_END: usize = usize::MAX;

/// Mirrors upstream `commentParser(input)` byte-for-byte, including
/// the unclosed-comment quirk.
pub fn comment_parser(input: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let length = input.len();
    let mut pos: usize = 0;

    while pos < length {
        match find_substr(input, "/*", pos) {
            Some(next) => {
                tokens.push(Token { kind: TokenKind::Text, start: pos, end: next });
                pos = next;
                let body_start = pos + 2;
                match find_substr(input, "*/", body_start) {
                    Some(close) => {
                        tokens.push(Token { kind: TokenKind::Comment, start: body_start, end: close });
                        pos = close + 2;
                    }
                    None => {
                        // Upstream: `next = input.indexOf('*/', pos+2)` is -1.
                        // tokens.push([1, pos+2, -1]). pos = -1 + 2 = 1.
                        // Loop continues from pos=1 — likely produces garbage
                        // tokens. We replicate end=UNCLOSED_END and pos=1.
                        tokens.push(Token {
                            kind: TokenKind::Comment,
                            start: body_start,
                            end: UNCLOSED_END,
                        });
                        pos = 1;
                    }
                }
            }
            None => {
                tokens.push(Token { kind: TokenKind::Text, start: pos, end: length });
                pos = length;
            }
        }
    }

    tokens
}

/// `String.prototype.indexOf(needle, fromIndex)` semantics — byte-offset
/// search. Returns `None` if not found.
fn find_substr(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from > haystack.len() { return None; }
    haystack[from..].find(needle).map(|p| p + from)
}

/// Slice the original input by a [`Token`]'s offsets, applying
/// upstream's `slice(start, -1)` semantics for unclosed comments.
pub fn token_text<'a>(input: &'a str, t: Token) -> &'a str {
    let end = if t.end == UNCLOSED_END {
        // JS `slice(start, -1)` = "everything but the last char".
        input.len().saturating_sub(1).max(t.start)
    } else {
        t.end.min(input.len())
    };
    let start = t.start.min(input.len());
    if end < start { return ""; }
    &input[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_comments() {
        let toks = comment_parser("hello world");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Text);
        assert_eq!(toks[0].start, 0);
        assert_eq!(toks[0].end, 11);
    }

    #[test]
    fn single_comment() {
        let s = "a/* x */b";
        let toks = comment_parser(s);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0].kind, TokenKind::Text);
        assert_eq!(token_text(s, toks[0]), "a");
        assert_eq!(toks[1].kind, TokenKind::Comment);
        assert_eq!(token_text(s, toks[1]), " x ");
        assert_eq!(toks[2].kind, TokenKind::Text);
        assert_eq!(token_text(s, toks[2]), "b");
    }

    #[test]
    fn multiple_comments() {
        let s = "/*a*//*b*/";
        let toks = comment_parser(s);
        // [Text:"", Comment:"a", Text:"", Comment:"b"]
        assert_eq!(toks.len(), 4);
        assert_eq!(token_text(s, toks[1]), "a");
        assert_eq!(token_text(s, toks[3]), "b");
    }
}
