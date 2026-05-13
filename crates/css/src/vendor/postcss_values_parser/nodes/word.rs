//! Port of `postcss-values-parser/lib/nodes/Word.js`.

use super::node::Common;
use crate::vendor::colord::names::NAME_TO_HEX;
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
static HEX_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^#(.+)").unwrap());

// 1:1 with upstream `Word.js:18`:
//   const escapeRegex = /^\\(.+)/;
// Backslash followed by 1+ chars classifies as a Word (CSS identifier
// escape, e.g. `\41` for `A`). The JS `testEscaped` ALSO accepts
// `value === '\\'` when the next token is non-whitespace, hence the
// `next` argument.
static ESCAPE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\\(.+)").unwrap());

// 1:1 with upstream `Word.js:20`:
//   const colorRegex = /^#([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
static COLOR_HEX_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^#([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$").unwrap()
});

// Mirror upstream `Word.js:38`:
//   lastNode.isUrl = value.startsWith('//') ? isUrl(`http:${value}`) : isUrl(value);
//
// `is-url-superb` distilled to the contract that matters here:
//   1. trim
//   2. reject if empty or contains a space
//   3. require `^[^:]+:/{1,2}(?!/)` — protocol prefix with 1 or 2 slashes,
//      not followed by a third slash
//   4. require something after the slashes
//
// Rust's `regex` crate has no lookahead, so we express `(?!/)` by demanding
// at least one non-`/` byte after the 1 or 2 slashes.
static URL_PROTOCOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[^:]+:/{1,2}[^/]").unwrap()
});

impl Word {
    /// Mirror upstream `testVariable` with default `prefixes: ['--']`.
    pub fn is_variable_name(value: &str) -> bool { value.starts_with("--") }

    /// Mirror upstream `testHex` — `^#(.+)`.
    pub fn test_hex(value: &str) -> bool { HEX_REGEX.is_match(value) }

    /// Mirror upstream `Word.testEscaped` (`Word.js:44-52`):
    ///   type==='word' && (escapeRegex.test(value) ||
    ///     (value === '\\' && next && !/^\s+$/.test(next[1])))
    /// Caller passes `next_value` for the bare-backslash branch.
    pub fn test_escaped(value: &str, next_value: Option<&str>) -> bool {
        if ESCAPE_REGEX.is_match(value) { return true; }
        if value == "\\" {
            if let Some(next) = next_value {
                if !next.is_empty() && !next.chars().all(char::is_whitespace) {
                    return true;
                }
            }
        }
        false
    }

    /// Mirror upstream `Word.testWord` (`Word.js:68-72`):
    ///   testEscaped(tokens) || testHex(token) || testVariable(token, parser)
    pub fn test_word(value: &str, next_value: Option<&str>) -> bool {
        Word::test_escaped(value, next_value)
            || Word::test_hex(value)
            || Word::is_variable_name(value)
    }

    /// Mirror upstream:
    ///   `colorRegex.test(value) || colorNames.includes(value.toLowerCase())`
    pub fn test_color(value: &str) -> bool {
        if COLOR_HEX_REGEX.is_match(value) {
            return true;
        }
        // `colorNames = Object.keys(require('color-name'))` — case-insensitive
        // lookup of the 148 CSS named colors. `colord::names::NAME_TO_HEX`
        // covers the same surface, lowercase-keyed.
        NAME_TO_HEX.contains_key(value.to_ascii_lowercase().as_str())
    }

    /// Mirror upstream `Word.js:38`:
    ///   `value.startsWith('//') ? isUrl(`http:${value}`) : isUrl(value)`
    pub fn test_url(value: &str) -> bool {
        let v = value.trim();
        if v.is_empty() { return false; }
        let candidate;
        let target: &str = if v.starts_with("//") {
            candidate = format!("http:{}", v);
            &candidate
        } else {
            v
        };
        if target.contains(' ') { return false; }
        URL_PROTOCOL_REGEX.is_match(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn hex_short_matches() { assert!(Word::test_hex("#fff")); }
    #[test] fn hex_long_matches() { assert!(Word::test_hex("#ff00aa")); }
    #[test] fn bare_hash_no_match() { assert!(!Word::test_hex("#")); }
    #[test] fn no_hash_no_match() { assert!(!Word::test_hex("fff")); }

    #[test] fn variable_double_dash() { assert!(Word::is_variable_name("--foo")); }
    #[test] fn variable_single_dash_no() { assert!(!Word::is_variable_name("-foo")); }

    #[test] fn color_named() { assert!(Word::test_color("red")); }
    #[test] fn color_named_uppercase() { assert!(Word::test_color("RED")); }
    #[test] fn color_named_mixed_case() { assert!(Word::test_color("RebeccaPurple")); }
    #[test] fn color_hex_3() { assert!(Word::test_color("#abc")); }
    #[test] fn color_hex_6() { assert!(Word::test_color("#aabbcc")); }
    #[test] fn color_hex_8() { assert!(Word::test_color("#aabbccdd")); }
    #[test] fn color_hex_5_no() { assert!(!Word::test_color("#abcde")); }
    #[test] fn color_unknown_word_no() { assert!(!Word::test_color("xyzzy")); }

    #[test] fn url_http() { assert!(Word::test_url("http://example.com")); }
    #[test] fn url_https() { assert!(Word::test_url("https://example.com/path")); }
    #[test] fn url_protocol_relative() { assert!(Word::test_url("//cdn.example.com/x")); }
    // Upstream `is-url-superb` requires `:/{1,2}` after the protocol, so
    // `data:` URIs do NOT classify as URL — port the upstream behavior.
    #[test] fn url_data_no_slash_no() { assert!(!Word::test_url("data:image/png;base64,XYZ")); }
    #[test] fn url_with_space_no() { assert!(!Word::test_url("http://exa mple.com")); }
    #[test] fn url_no_protocol_no() { assert!(!Word::test_url("example.com")); }
    #[test] fn url_triple_slash_no() { assert!(!Word::test_url("http:///foo")); }

    // testEscaped — backslash-prefixed identifiers
    #[test] fn escaped_hex_id() { assert!(Word::test_escaped(r"\41", None)); }
    #[test] fn escaped_letter() { assert!(Word::test_escaped(r"\A", None)); }
    #[test] fn bare_backslash_with_next() { assert!(Word::test_escaped(r"\", Some("X"))); }
    #[test] fn bare_backslash_with_space_next() { assert!(!Word::test_escaped(r"\", Some(" "))); }
    #[test] fn bare_backslash_no_next() { assert!(!Word::test_escaped(r"\", None)); }
    #[test] fn unrelated_no_match() { assert!(!Word::test_escaped("abc", None)); }
}
