//! Port of `postcss-values-parser/lib/nodes/UnicodeRange.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct UnicodeRange {
    pub common: Common,
}

// Mirror upstream `UnicodeRange.js:26`:
//   /U\+(\d|\w)+(-\w+)?(\?+)?/
// UNANCHORED — tests substring, not whole string. Capital U only.
// `\w` = [A-Za-z0-9_] in JS regex (also Rust regex by default); permits
// non-hex characters by design (upstream bug-for-bug).
static UNICODE_RANGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"U\+(\d|\w)+(-\w+)?(\?+)?").unwrap());

impl UnicodeRange {
    pub fn test(value: &str) -> bool { UNICODE_RANGE_RE.is_match(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Upstream regex is unanchored and capital-U only; verify divergent edges.
    #[test] fn capital_u_hex_matches() { assert!(UnicodeRange::test("U+0025")); }
    #[test] fn lowercase_u_does_not_match() { assert!(!UnicodeRange::test("u+0025")); }
    #[test] fn allows_word_chars_in_range() { assert!(UnicodeRange::test("U+xyz")); }
    #[test] fn allows_question_marks() { assert!(UnicodeRange::test("U+25??")); }
    #[test] fn substring_matches_unanchored() { assert!(UnicodeRange::test("prefixU+0025")); }
}
