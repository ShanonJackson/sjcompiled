//! Port of `postcss-selector-parser/dist/tokenTypes.js`.
//!
//! Numeric token-type constants. Real ASCII bytes for punctuation; the
//! virtual `comment` / `word` / `combinator` types are negative ints just
//! like upstream.

#![allow(non_upper_case_globals)]

pub const ampersand: i32 = 38;
pub const asterisk: i32 = 42;
pub const at: i32 = 64;
pub const comma: i32 = 44;
pub const colon: i32 = 58;
pub const semicolon: i32 = 59;
pub const openParenthesis: i32 = 40;
pub const closeParenthesis: i32 = 41;
pub const openSquare: i32 = 91;
pub const closeSquare: i32 = 93;
pub const dollar: i32 = 36;
pub const tilde: i32 = 126;
pub const caret: i32 = 94;
pub const plus: i32 = 43;
pub const equals: i32 = 61;
pub const pipe: i32 = 124;
pub const greaterThan: i32 = 62;
pub const space: i32 = 32;
pub const singleQuote: i32 = 39;
pub const doubleQuote: i32 = 34;
pub const slash: i32 = 47;
pub const bang: i32 = 33;
pub const backslash: i32 = 92;
pub const cr: i32 = 13;
pub const feed: i32 = 12;
pub const newline: i32 = 10;
pub const tab: i32 = 9;
pub const str: i32 = singleQuote;
pub const comment: i32 = -1;
pub const word: i32 = -2;
pub const combinator: i32 = -3;
