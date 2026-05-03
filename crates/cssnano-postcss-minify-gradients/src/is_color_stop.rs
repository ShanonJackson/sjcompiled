//! Port of `postcss-minify-gradients/src/isColorStop.js`.
//!
//! Mirrors the upstream `(color, stop?) -> bool` predicate verbatim,
//! including the `colord(color).isValid() && isStop(stop)` shape.

use colord::colord;
use postcss_value_parser::parse_unit;
use regex::Regex;
use once_cell::sync::Lazy;

// Upstream `lengthUnits` Set, uppercased. The `% ` entry mirrors upstream's
// inclusion of percent in the length-unit predicate.
const LENGTH_UNITS: &[&str] = &[
    "PX", "IN", "CM", "MM", "EM", "REM", "POINTS", "PC", "EX", "CH",
    "VW", "VH", "VMIN", "VMAX", "%",
];

fn is_css_length_unit(input: &str) -> bool {
    let upper = input.to_uppercase();
    LENGTH_UNITS.iter().any(|u| *u == upper.as_str())
}

// Mirrors upstream `isStop(str)`. JS `Number(node.number)` returns NaN for
// the empty string IFF the parser would have rejected it, but `parse_unit`
// already does so via `like_number`. The numeric-zero short-circuit
// matches `number === 0` (loose equality coerces "0" -> 0, "0.0" -> 0,
// "+0"/"-0" -> 0, etc.). Non-finite values: `parseFloat("Infinity")`
// returns Infinity (truthy, !isNaN); CSS-syntax `like_number` rejects
// "Infinity" so it goes through `parse_unit -> None`. Same as upstream.
fn is_stop(s: Option<&str>) -> bool {
    let str_val = match s {
        Some(v) => v,
        None => return true,
    };
    if str_val.is_empty() {
        // JS `if (str)` -> falsy on "", returns true (the no-stop branch).
        return true;
    }

    if let Some(node) = parse_unit(str_val) {
        // Mirrors `Number(node.number)`. Empty number portion `parse_unit`
        // doesn't emit (like_number gate), but be safe.
        let parsed: Result<f64, _> = node.number.parse();
        match parsed {
            Ok(n) => {
                if n == 0.0 {
                    return true;
                }
                if !n.is_nan() && is_css_length_unit(&node.unit) {
                    return true;
                }
                false
            }
            Err(_) => false,
        }
    } else {
        // Upstream: `/^calc\(\S+\)$/g.test(str)`. JS `\S` is non-whitespace.
        // `g` flag is irrelevant for a single .test() call (no global state
        // since `lastIndex` is fresh on a fresh regex). Anchored both ends.
        CALC_RE.is_match(str_val)
    }
}

static CALC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^calc\(\S+\)$").unwrap());

/// `isColorStop(color, stop?)` — returns true iff `colord(color).isValid()`
/// AND `isStop(stop)` (which is true when stop is missing or matches the
/// length/zero/calc predicate).
pub fn is_color_stop(color: &str, stop: Option<&str>) -> bool {
    colord(color).is_valid() && is_stop(stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_color_no_stop() {
        assert!(is_color_stop("red", None));
        assert!(is_color_stop("#fff", None));
    }

    #[test]
    fn invalid_color() {
        assert!(!is_color_stop("not-a-color", None));
    }

    #[test]
    fn zero_stop_any_unit_ok() {
        assert!(is_color_stop("red", Some("0")));
        assert!(is_color_stop("red", Some("0deg"))); // deg isn't a length unit, but zero short-circuits.
    }

    #[test]
    fn length_unit_stop_ok() {
        assert!(is_color_stop("red", Some("10px")));
        assert!(is_color_stop("red", Some("50%")));
    }

    #[test]
    fn non_length_unit_stop_rejected() {
        assert!(!is_color_stop("red", Some("10deg")));
    }

    #[test]
    fn calc_stop_ok() {
        assert!(is_color_stop("red", Some("calc(50%+1px)")));
    }

    #[test]
    fn calc_with_inner_space_rejected() {
        // \S means non-whitespace; inner space breaks the match.
        assert!(!is_color_stop("red", Some("calc(50% + 1px)")));
    }
}
