//! Port of `postcss-values-parser/lib/nodes/Numeric.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Numeric {
    pub common: Common,
    pub unit: String,
}

// 1:1 with upstream `Numeric.js:29-56`:
//   numberRegex = /^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][+-]?\d+)?)$/
//   unitRegex   = /^(-?(?:[-A-Z_a-z]|[^\x00-\x7F]|\\[^\n\f\r])(?:[-\w]|[^\x00-\x7F]|\\[^\n\f\r])*|%)$/
//   numericRegex = `^${numberRegex.source.slice(1, -1) + unitRegex.source.slice(1, -1)}?$`
//
// The unit half MUST be restrictive; an over-permissive `(.*)` lets things
// like "5%a" classify as Numeric, which JS rejects (drift fix).
//
// Notes:
//   * `\d+(?:\.\d*)?|\.\d+` allows `5`, `5.`, `5.5`, `.5` — but NOT bare `.`.
//   * `\f` (form-feed) is `\x0c`; spelled out for the Rust regex parser.
//   * The whole regex is whole-string anchored via `^...$`.
static NUMERIC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][+-]?\d+)?)(-?(?:[-A-Z_a-z]|[^\x00-\x7F]|\\[^\n\x0c\r])(?:[-\w]|[^\x00-\x7F]|\\[^\n\x0c\r])*|%)?$"
    ).unwrap()
});

impl Numeric {
    pub fn test(value: &str) -> bool { NUMERIC_RE.is_match(value) }
    pub fn split(value: &str) -> Option<(String, String)> {
        let caps = NUMERIC_RE.captures(value)?;
        let value_part = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let unit_part = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        Some((value_part, unit_part))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn basic_int() { assert!(Numeric::test("5")); }
    #[test] fn basic_float() { assert!(Numeric::test("5.5")); }
    #[test] fn trailing_dot() { assert!(Numeric::test("5.")); }
    #[test] fn leading_dot() { assert!(Numeric::test(".5")); }
    #[test] fn px_unit() { assert!(Numeric::test("100px")); }
    #[test] fn percent_unit() { assert!(Numeric::test("100%")); }
    #[test] fn negative() { assert!(Numeric::test("-1.5em")); }
    #[test] fn exponent() { assert!(Numeric::test("1e3px")); }
    #[test] fn no_digit_no_match() { assert!(!Numeric::test("abc")); }
    // Drift fix: upstream rejects `5%a` (unit must be `%` alone or
    // identifier-shaped). Old over-permissive `(.*)` accepted it.
    #[test] fn percent_then_letter_no_match() { assert!(!Numeric::test("5%a")); }
    // `5xyz!` — `!` invalid in unit identifier.
    #[test] fn bang_in_unit_no_match() { assert!(!Numeric::test("5xyz!")); }
    // Trailing-dot value: split keeps `.` on value, not unit.
    #[test] fn trailing_dot_value_split() {
        let (v, u) = Numeric::split("5.").unwrap();
        assert_eq!(v, "5.");
        assert_eq!(u, "");
    }
    // Leading-dot value.
    #[test] fn leading_dot_value_split() {
        let (v, u) = Numeric::split(".5em").unwrap();
        assert_eq!(v, ".5");
        assert_eq!(u, "em");
    }
}
