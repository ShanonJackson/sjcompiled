//! Port of `postcss-values-parser/lib/nodes/Comment.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Comment {
    pub common: Common,
    pub text: String,
    pub inline: bool,
    pub left: String,
    pub right: String,
}

impl Comment {
    /// 1:1 with upstream `Comment.js:14-20`:
    /// ```js
    /// const inlineRegex = /(\/\/)/;
    /// static testInline(token) { return inlineRegex.test(token[1]); }
    /// ```
    /// Returns true when the value contains `//` ANYWHERE, not just at
    /// the start. Used by upstream's `unknownWord` path to reclassify
    /// Word tokens whose value embeds an inline-comment marker.
    pub fn test_inline_word(value: &str) -> bool { value.contains("//") }

    /// Rust-internal path selector: does the value LOOK LIKE an
    /// already-classified inline-comment token (starts with `//`)?
    /// Used to decide trim semantics for tokens of kind=Comment.
    /// Distinct from [`test_inline_word`]: a block comment with `//`
    /// in the middle (`"/* a // b */"`) returns `true` from
    /// `test_inline_word` but `false` here — and that's intentional,
    /// because the trim path applies `//`-prefix stripping which would
    /// silently no-op on a block-comment value.
    pub fn is_inline_marker(value: &str) -> bool { value.starts_with("//") }

    /// Deprecated alias retained for source compat with earlier passes
    /// of the audit. New callers should pick whichever of the two
    /// methods above matches their intent.
    #[deprecated(note = "use `is_inline_marker` (path selector) or `test_inline_word` (upstream contract)")]
    pub fn test_inline(value: &str) -> bool { Self::is_inline_marker(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn upstream_contract_contains_anywhere() {
        assert!(Comment::test_inline_word("// at start"));
        assert!(Comment::test_inline_word("middle // tail"));
        assert!(Comment::test_inline_word("trailing//"));
        assert!(Comment::test_inline_word("/* block with // inside */"));
        assert!(!Comment::test_inline_word("/* no inline marker */"));
        assert!(!Comment::test_inline_word(""));
    }

    #[test] fn path_selector_only_starts_with() {
        assert!(Comment::is_inline_marker("//comment"));
        assert!(!Comment::is_inline_marker("middle // tail"));
        assert!(!Comment::is_inline_marker("/* block with // inside */"));
    }
}
