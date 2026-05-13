//! Port of `postcss-values-parser/lib/tokenize.js`.
//!
//! Upstream wraps `postcss/lib/tokenize` and post-processes:
//!   * `brackets` -> recursively re-tokenize the contents and emit `(`+inner+`)`.
//!   * `word` whose value is one of `* - % + /` -> retag as `operator`.
//!   * `word` matching `/[*\/]/` -> split into `operator` + `word` chunks.
//!   * `word` containing `,` -> split into `comma` + `word` chunks.
//!
//! This Rust port owns the same wrapping responsibility but stays inside the
//! crate (it doesn't re-export postcss-core's tokenizer through public API).

use postcss_core::tokenize::{tokenizer, Token as CoreToken, TokenKind as CoreKind};
use postcss_core::Input;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VKind {
    Space,
    Word,
    Operator,
    Comma,
    Punctuation, // colon, semicolon, brackets, parens, square brackets
    String,
    AtWord,
    Comment,
    OpenParen,
    CloseParen,
    OpenSquare,
    CloseSquare,
    OpenCurly,
    CloseCurly,
    Colon,
    Semicolon,
}

#[derive(Debug, Clone)]
pub struct VToken {
    pub kind: VKind,
    pub value: String,
    pub source_index: usize,
    pub source_end_index: usize,
}

const OPERATORS: &[&str] = &["*", "-", "%", "+", "/"];

fn from_core(core: &CoreToken) -> VToken {
    let (kind, value) = match core.kind {
        CoreKind::Space => (VKind::Space, core.content.clone()),
        CoreKind::OpenSquare => (VKind::OpenSquare, core.content.clone()),
        CoreKind::CloseSquare => (VKind::CloseSquare, core.content.clone()),
        CoreKind::OpenCurly => (VKind::OpenCurly, core.content.clone()),
        CoreKind::CloseCurly => (VKind::CloseCurly, core.content.clone()),
        CoreKind::Colon => (VKind::Colon, core.content.clone()),
        CoreKind::Semicolon => (VKind::Semicolon, core.content.clone()),
        CoreKind::CloseParen => (VKind::CloseParen, core.content.clone()),
        CoreKind::OpenParen => (VKind::OpenParen, core.content.clone()),
        CoreKind::Brackets => (VKind::Punctuation, core.content.clone()), // expand below.
        CoreKind::String => (VKind::String, core.content.clone()),
        CoreKind::AtWord => (VKind::AtWord, core.content.clone()),
        CoreKind::Word => (VKind::Word, core.content.clone()),
        CoreKind::Comment => (VKind::Comment, core.content.clone()),
    };
    VToken {
        kind,
        value,
        source_index: core.pos.unwrap_or(0),
        source_end_index: core.next.unwrap_or(core.pos.unwrap_or(0)),
    }
}

/// Run the underlying postcss-core tokenizer and apply the values-parser
/// post-processing: split brackets, retag operator-words, split comma-words.
pub fn get_tokens(input: &str) -> Vec<VToken> {
    let css = Input::new(input.to_string(), None);
    let mut t = tokenizer(&css, false);
    let mut out: Vec<VToken> = Vec::new();
    while let Ok(Some(tok)) = t.next_token(false) {
        post_process(&tok, &mut out);
    }
    out
}

fn post_process(core: &CoreToken, out: &mut Vec<VToken>) {
    if matches!(core.kind, CoreKind::Brackets) {
        // Strip the outer `(` `)` and recursively tokenize.
        let s = &core.content;
        let inner = &s[1..s.len() - 1];
        let start = core.pos.unwrap_or(0);
        out.push(VToken {
            kind: VKind::OpenParen,
            value: "(".to_string(),
            source_index: start,
            source_end_index: start + 1,
        });
        for sub in get_tokens(inner) {
            // shift source indices by `start + 1`.
            out.push(VToken {
                kind: sub.kind,
                value: sub.value,
                source_index: sub.source_index + start + 1,
                source_end_index: sub.source_end_index + start + 1,
            });
        }
        let end = core.next.unwrap_or(start);
        out.push(VToken {
            kind: VKind::CloseParen,
            value: ")".to_string(),
            source_index: end,
            source_end_index: end + 1,
        });
        return;
    }
    if matches!(core.kind, CoreKind::Word) {
        let v = &core.content;
        if OPERATORS.iter().any(|op| *op == v.as_str()) {
            out.push(VToken {
                kind: VKind::Operator, value: v.clone(),
                source_index: core.pos.unwrap_or(0),
                source_end_index: core.next.map(|n| n + 1).unwrap_or(0),
            });
            return;
        }
        // Operator-split: split runs that contain `*` or `/` into chunks.
        if v.contains('*') || v.contains('/') {
            let pos = core.pos.unwrap_or(0);
            split_word_by(v, &['*', '/'], pos, |chunk, sub_pos| {
                let kind = if OPERATORS.iter().any(|op| *op == chunk) { VKind::Operator } else { VKind::Word };
                out.push(VToken { kind, value: chunk.to_string(), source_index: sub_pos, source_end_index: sub_pos + chunk.len() });
            });
            return;
        }
        // Comma-split: split words with embedded commas.
        if v.len() > 1 && v.contains(',') {
            let pos = core.pos.unwrap_or(0);
            split_word_by(v, &[','], pos, |chunk, sub_pos| {
                let kind = if chunk == "," { VKind::Comma } else { VKind::Word };
                out.push(VToken { kind, value: chunk.to_string(), source_index: sub_pos, source_end_index: sub_pos + chunk.len() });
            });
            return;
        }
    }
    out.push(from_core(core));
}

fn split_word_by<F: FnMut(&str, usize)>(value: &str, seps: &[char], start_pos: usize, mut emit: F) {
    let bytes = value.as_bytes();
    let mut buf_start = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if seps.contains(&c) {
            if i > buf_start {
                emit(&value[buf_start..i], start_pos + buf_start);
            }
            emit(&value[i..i + 1], start_pos + i);
            buf_start = i + 1;
        }
        i += 1;
    }
    if buf_start < bytes.len() {
        emit(&value[buf_start..], start_pos + buf_start);
    }
}
