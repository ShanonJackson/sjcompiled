//! Port of `postcss-values-parser/lib/nodes/Punctuation.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Punctuation {
    pub common: Common,
}

// 1:1 with upstream `Punctuation.js:27`:
//   static get chars() { return [',', ':', '(', ')', '[', ']', '{', '}']; }
// Upstream uses this list to test the FIRST element of a token tuple
// (the type) inside `ValuesParser#unknownWord`. Token types `,`, `{`, `}`
// never appear directly (the postcss-core tokenizer emits `comma` / `{` / `}`
// kinds), but the upstream array includes `,` for symmetry. We mirror it.
pub static PUNCT_CHARS: &[&str] = &[",", ":", "(", ")", "[", "]", "{", "}"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn matches_upstream_chars_array_exactly() {
        assert_eq!(PUNCT_CHARS, &[",", ":", "(", ")", "[", "]", "{", "}"]);
    }
}
