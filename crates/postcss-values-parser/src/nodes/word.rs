//! Port of `postcss-values-parser/lib/nodes/Word.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Word {
    pub common: Common,
    pub is_variable: bool,
    pub is_hex: bool,
    pub is_color: bool,
    pub is_url: bool,
}

// 1:1 with upstream `Word.js:20`:
//   const hexRegex = /^#(.+)/;
// Mirror exactly: requires `#` followed by at least one character.
// Distinct from `value.starts_with('#')` — the previous port admitted bare
// `"#"` as is_hex, which JS does not.
static HEX_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^#(.+)").unwrap());

impl Word {
    /// Mirror upstream `testVariable` with default `prefixes: ['--']`.
    /// Upstream constructs `new RegExp('^(' + prefixes.join('|') + ')')`,
    /// which for the default reduces to `^--`.
    pub fn is_variable_name(value: &str) -> bool { value.starts_with("--") }

    /// Mirror upstream `testHex` — `^#(.+)`.
    pub fn test_hex(value: &str) -> bool { HEX_REGEX.is_match(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn hex_short_matches() { assert!(Word::test_hex("#fff")); }
    #[test] fn hex_long_matches() { assert!(Word::test_hex("#ff00aa")); }
    // Bare `#` is NOT hex per upstream — `(.+)` requires one+ chars.
    #[test] fn bare_hash_no_match() { assert!(!Word::test_hex("#")); }
    #[test] fn no_hash_no_match() { assert!(!Word::test_hex("fff")); }

    #[test] fn variable_double_dash() { assert!(Word::is_variable_name("--foo")); }
    #[test] fn variable_single_dash_no() { assert!(!Word::is_variable_name("-foo")); }
}
