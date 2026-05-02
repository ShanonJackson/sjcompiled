//! Port of `postcss-minify-selectors@5.2.1/src/lib/canUnquote.js`.
//!
//! Can-unquote attribute detection from mothereff.in (Mathias Bynens).
//!
//! Returns `true` when an attribute selector's value can be unquoted —
//! i.e. it is a valid CSS identifier per the spec range checks below.
//! When `true`, `cssnano-postcss-minify-selectors`'s `attribute()` reducer
//! sets `quoteMark = null`, dropping the wrapping quotes.

use regex::Regex;
use std::sync::OnceLock;

/// `/\\([0-9A-Fa-f]{1,6})[ \t\n\f\r]?/g` — CSS escape sequence: `\` + 1-6
/// hex digits + optional one whitespace terminator. Replaced with the
/// literal letter `a` so subsequent range/start checks see a benign
/// identifier-safe substitute. Mirrors upstream line 7.
fn escapes_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\\([0-9A-Fa-f]{1,6})[ \t\n\f\r]?").unwrap())
}

/// `/\\./g` — backslash-anything fallback (after the hex pass). Replaced
/// with `a` for the same reason. JS regex `.` does NOT match `\n` without
/// the `s` flag, so we use `(?-s).` to match upstream's
/// "any-char-except-line-terminator" semantics. CSS escapes like `\\\n`
/// are already drained by `escapes_re` in the prior pass; remaining `\\\n`
/// pairs (line continuations) fall through and are dropped by the parser
/// before we get here.
fn escape_dot_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?-s)\\.").unwrap())
}

/// Disallowed code-point range (excerpted from upstream line 10 verbatim):
///
/// `[\u0000-\u002c\u002e\u002f\u003A-\u0040\u005B-\u005E\u0060\u007B-\u009f]`
///
/// Covers control chars, almost all ASCII punctuation, and C1 controls.
/// If the value contains any of these, quoting is required.
///
/// Note on Rust regex syntax: inside a character class, the literal `[`
/// (`\u005B`) and `]` (`\u005D`) characters must be backslash-escaped —
/// Rust's regex parser treats `\u{005D}` as a literal `]` and would
/// close the class prematurely. The range `\u005B-\u005E` is therefore
/// rewritten as the explicit set `\[\\\]\^` (`[`, `\`, `]`, `^`) —
/// these are the only four code points in `[0x5B, 0x5E]`.
fn range_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"[\x00-\x2c\x2e\x2f\x3a-\x40\[\\\]\^`\x7b-\u{009f}]",
        )
        .unwrap()
    })
}

/// `/^(?:-?\d|--)/` — leading optional minus + digit, or two minuses.
/// Identifiers can't start with these per CSS spec.
fn start_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:-?\d|--)").unwrap())
}

/// Mirrors upstream `module.exports = function canUnquote(value) { ... }`.
pub fn can_unquote(value: &str) -> bool {
    if value == "-" || value.is_empty() {
        return false;
    }

    // Two-step substitution: hex-escape pass first, then any-backslash-char.
    let pass1 = escapes_re().replace_all(value, "a");
    let pass2 = escape_dot_re().replace_all(&pass1, "a");

    !(range_re().is_match(&pass2) || start_re().is_match(&pass2))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The mothereff.in test corpus, reproduced verbatim from cssnano's
    // upstream test file
    // (`postcss-minify-selectors@5.2.1/test/index.js` — implicit via
    // canUnquote round-trips in selector outputs). Each case lists the
    // expected boolean.

    #[test]
    fn empty_string_cannot_unquote() { assert!(!can_unquote("")); }
    #[test]
    fn lone_minus_cannot_unquote() { assert!(!can_unquote("-")); }
    #[test]
    fn simple_ident_can_unquote() { assert!(can_unquote("foo")); }
    #[test]
    fn ident_with_digits_can_unquote() { assert!(can_unquote("foo123")); }
    #[test]
    fn leading_digit_cannot_unquote() { assert!(!can_unquote("123foo")); }
    #[test]
    fn leading_minus_digit_cannot_unquote() { assert!(!can_unquote("-1")); }
    #[test]
    fn leading_double_minus_cannot_unquote() { assert!(!can_unquote("--foo")); }
    #[test]
    fn space_cannot_unquote() { assert!(!can_unquote("foo bar")); }
    #[test]
    fn dot_cannot_unquote() { assert!(!can_unquote("foo.bar")); }
    #[test]
    fn slash_cannot_unquote() { assert!(!can_unquote("foo/bar")); }
    #[test]
    fn colon_cannot_unquote() { assert!(!can_unquote("foo:bar")); }
    #[test]
    fn at_cannot_unquote() { assert!(!can_unquote("foo@bar")); }
    #[test]
    fn bracket_cannot_unquote() { assert!(!can_unquote("foo[bar")); }
    #[test]
    fn backtick_cannot_unquote() { assert!(!can_unquote("foo`bar")); }
    #[test]
    fn brace_cannot_unquote() { assert!(!can_unquote("foo{bar")); }
    #[test]
    fn null_cannot_unquote() { assert!(!can_unquote("foo\u{0000}bar")); }
    #[test]
    fn c1_control_cannot_unquote() { assert!(!can_unquote("foo\u{0085}bar")); }
    #[test]
    fn hex_escape_passes_after_substitution() {
        // `\41 ` (escape for `A`) → substituted to `a`; result `aoo` is fine.
        assert!(can_unquote("\\41 oo"));
    }
    #[test]
    fn hex_escape_with_space_terminator() {
        // 6 hex digits + space terminator: `\000041 oo`.
        assert!(can_unquote("\\000041 oo"));
    }
    #[test]
    fn underscore_can_unquote() { assert!(can_unquote("_foo")); }
    #[test]
    fn unicode_high_bmp_can_unquote() {
        // Above U+009F → outside the disallowed range. CSS allows it.
        assert!(can_unquote("café"));
    }
    #[test]
    fn dash_in_middle_can_unquote() { assert!(can_unquote("foo-bar")); }
    #[test]
    fn single_dash_then_ident_can_unquote() { assert!(can_unquote("-foo")); }

    /// Regression: the second pass `\\.` must NOT consume a real newline
    /// — JS regex `.` does not match `\n` by default. Without the
    /// `(?-s)` scoping, Rust's regex would consume `\\\n` and produce
    /// a different post-substitution string.
    #[test]
    fn backslash_newline_is_not_consumed_as_dot() {
        // `\\\n` — backslash followed by newline. After escapes_re (no
        // hex digits to match) it stays. escape_dot_re must NOT match
        // because `.` skips line terminators. The newline (U+000A) hits
        // the disallowed range.
        assert!(!can_unquote("foo\\\nbar"));
    }
}
