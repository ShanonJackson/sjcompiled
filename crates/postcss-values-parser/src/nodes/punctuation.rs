//! Port of `postcss-values-parser/lib/nodes/Punctuation.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Punctuation {
    pub common: Common,
}

pub static PUNCT_CHARS: &[&str] = &[":", ";", "(", ")", "[", "]", ",", "comma"];
