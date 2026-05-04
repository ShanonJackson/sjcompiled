//! 1:1 port of `packages/babel-plugin/src/utils/has-numeric-value.ts`.
//!
//! ```ts
//! export const hasNumericValue = (expression: t.Expression): boolean =>
//!   t.isNumericLiteral(expression) ||
//!   (t.isStringLiteral(expression) && !Number.isNaN(Number(expression.value)));
//! ```
//!
//! Used by `traverse-binary-expression` and `traverse-unary-expression`
//! to decide whether a recursively-evaluated operand can fold to a
//! numeric literal at compile time.
//!
//! ## SWC mapping notes
//!
//! * Babel `t.NumericLiteral` → SWC `Lit::Num(Number)`. Match on
//!   `Expr::Lit(Lit::Num(_))`.
//! * Babel `t.StringLiteral` → SWC `Lit::Str(Str)`. Match on
//!   `Expr::Lit(Lit::Str(_))`.
//! * Babel reads `expression.value` as the JS string; we read
//!   `Str::value` (an `Atom`). The JS `Number(<string>)` parses with
//!   the JS Number-coercion algorithm — including leading whitespace,
//!   `0x` hex, scientific notation, and `'   '` (whitespace-only) →
//!   `0` (NOT NaN). Rust's `str::parse::<f64>` handles most of these
//!   but NOT whitespace-only or `'0x...'` — it would return Err on
//!   those cases where JS returns 0 or 16 respectively.
//!
//!   To match JS exactly we trim leading/trailing whitespace, treat
//!   empty (post-trim) as `0` (matches JS `Number('')` and
//!   `Number(' ')` → `0`), and parse with the JS rules. The full
//!   `Number()` algorithm is non-trivial, but for the shapes
//!   `hasNumericValue` actually sees in practice (CSS-extracted
//!   string literals like `"12"`, `"1.5"`, `"-3"`, `"3.14e2"`,
//!   `"  4  "`, `""`), `f64::from_str` after trim covers it.
//!   Hex/octal literals as STRING values inside CSS expressions are
//!   not produced by upstream's evaluator — they would have already
//!   been folded to numeric literals — so we omit that branch.
//!
//! Drift policy: if a future fixture surfaces a JS `Number(s)` shape
//! Rust's parser disagrees with (most likely a hex-string CSS value),
//! escalate per CLAUDE.md DRIFT DETECTION rather than patching.

use swc_core::ecma::ast::{Expr, Lit};

/// 1:1 port of `hasNumericValue`. Returns true when the expression is
/// either a numeric literal, or a string literal whose value coerces
/// to a non-NaN number under the JS `Number()` algorithm.
pub fn has_numeric_value(expression: &Expr) -> bool {
    match expression {
        Expr::Lit(Lit::Num(_)) => true,
        Expr::Lit(Lit::Str(s)) => {
            let raw_atom = s.value.to_atom_lossy();
            let raw = raw_atom.as_str();
            // JS `Number('   ')` → 0 (NOT NaN). Match by trimming and
            // treating empty as 0. JS `Number('')` → 0 likewise.
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return true;
            }
            trimmed.parse::<f64>().is_ok()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{Ident, Number, Str};

    fn num_lit(value: f64) -> Expr {
        Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value,
            raw: None,
        }))
    }

    fn str_lit(value: &str) -> Expr {
        Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        }))
    }

    #[test]
    fn numeric_literal_is_numeric() {
        assert!(has_numeric_value(&num_lit(42.0)));
        assert!(has_numeric_value(&num_lit(-3.14)));
        assert!(has_numeric_value(&num_lit(0.0)));
    }

    #[test]
    fn string_literal_with_numeric_content_is_numeric() {
        assert!(has_numeric_value(&str_lit("12")));
        assert!(has_numeric_value(&str_lit("1.5")));
        assert!(has_numeric_value(&str_lit("-3")));
        assert!(has_numeric_value(&str_lit("3.14e2")));
    }

    #[test]
    fn whitespace_only_string_matches_js_number_zero() {
        // JS: Number('') === 0, Number('   ') === 0. Both non-NaN.
        assert!(has_numeric_value(&str_lit("")));
        assert!(has_numeric_value(&str_lit("   ")));
    }

    #[test]
    fn string_literal_with_whitespace_padding() {
        // JS: Number('  12  ') === 12.
        assert!(has_numeric_value(&str_lit("  12  ")));
    }

    #[test]
    fn non_numeric_string_is_not_numeric() {
        assert!(!has_numeric_value(&str_lit("hello")));
        assert!(!has_numeric_value(&str_lit("12px")));
        assert!(!has_numeric_value(&str_lit("1.2.3")));
    }

    #[test]
    fn non_literal_is_not_numeric() {
        let ident = Expr::Ident(Ident::new("foo".into(), DUMMY_SP, Default::default()));
        assert!(!has_numeric_value(&ident));
    }
}
