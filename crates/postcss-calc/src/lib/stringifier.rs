//! Port of `postcss-calc/src/lib/stringifier.js`. Line-numbered references
//! point at upstream `stringifier.js`.
//!
//! Two distinct things live in this file:
//!   1. `stringify(node, prec)` — internal recursive printer for a single
//!      `CalcNode`. Mirrors upstream lines 27-60.
//!   2. `default_export(...)` (named `stringify_calc` in Rust) — the file's
//!      module.exports, which wraps `stringify` and re-prepends `calc(...)`
//!      when the result didn't simplify to a single value. Mirrors lines
//!      72-93. Also emits the `warnWhenCannotResolve` warning.

use postcss_core::js_number_to_string;

use crate::lib::convert_unit::{js_math_round, Precision};
use crate::parser::CalcNode;

/// `order[op]` upstream (lines 2-7). Mul/Div = 0, Add/Sub = 1.
/// Returned `Some(0)` is a falsey-equivalent under JS `order[op] ?` checks
/// (line 38) — but `0` is also `<` `1`, so the precedence comparison still
/// works. The "falsey" check at line 38 (`order[op] ? \` ${op} \` : op`) is
/// special: for `op=='+'/'-'` it adds spaces; for `op=='*'/'/'` it doesn't.
/// We reify that as `op_in_addsub`.
fn op_in_addsub(op: &str) -> bool {
    matches!(op, "+" | "-")
}

fn op_prec(op: &str) -> u8 {
    match op {
        "*" | "/" => 0,
        "+" | "-" => 1,
        _ => 0, // never reached; mirrors `order[op]` returning undefined → NaN comparisons false
    }
}

/// `round(value, prec)` upstream (lines 13-19).
/// IMPORTANT: precision here is used DIRECTLY (no `Math.ceil`, no `|| 5`
/// fallback). `prec=0` rounds to integers; negative `prec` rounds to tens/etc.
fn round(value: f64, prec: Precision) -> f64 {
    match prec {
        Precision::Never => value,
        Precision::At(p) => {
            let factor = (10f64).powf(p);
            js_math_round(value * factor) / factor
        }
    }
}

/// Mirrors upstream `stringify(node, prec)` (lines 27-60).
pub fn stringify(node: &CalcNode, prec: Precision) -> String {
    match node {
        CalcNode::MathExpression { operator, left, right } => {
            let mut s = String::new();
            // Left.
            if let CalcNode::MathExpression { operator: l_op, .. } = left.as_ref() {
                if op_prec(operator) < op_prec(l_op) {
                    s.push('(');
                    s.push_str(&stringify(left, prec));
                    s.push(')');
                } else {
                    s.push_str(&stringify(left, prec));
                }
            } else {
                s.push_str(&stringify(left, prec));
            }

            // Operator.
            // `order[op]` truthy iff op is +/-: emit ` ${op} ` (space-op-space).
            // Falsey (0 — for *,/) emits the bare operator.
            if op_in_addsub(operator) {
                s.push(' ');
                s.push_str(operator);
                s.push(' ');
            } else {
                s.push_str(operator);
            }

            // Right.
            if let CalcNode::MathExpression { operator: r_op, .. } = right.as_ref() {
                if op_prec(operator) < op_prec(r_op) {
                    s.push('(');
                    s.push_str(&stringify(right, prec));
                    s.push(')');
                } else {
                    s.push_str(&stringify(right, prec));
                }
            } else {
                s.push_str(&stringify(right, prec));
            }
            s
        }
        CalcNode::Number { value } => {
            // round(value, prec).toString() (line 52).
            js_number_to_string(round(*value, prec))
        }
        CalcNode::Function { value } => {
            // node.value.toString() — value is already a string.
            value.clone()
        }
        CalcNode::ParenthesizedExpression { content } => {
            format!("({})", stringify(content, prec))
        }
        CalcNode::Dimension { value, unit, .. } => {
            // Default branch (line 58): round(node.value, prec) + node.unit.
            // JS string concatenation `number + string` → `String(number) + string`.
            format!("{}{}", js_number_to_string(round(*value, prec)), unit)
        }
    }
}

/// Mirrors `module.exports = function (calc, node, originalValue, options, result, item)`
/// (lines 72-93). Returns the final string AND a warning to be emitted on
/// the postcss `result` (when applicable). The transform layer knows how
/// to attach the warning to the correct node.
///
/// The boolean second return value indicates whether `warnWhenCannotResolve`
/// should fire; the caller emits the warning text:
/// `'Could not reduce expression: ' + originalValue` (upstream line 86).
pub fn stringify_calc(
    calc_word: &str,
    node: &CalcNode,
    prec: Precision,
) -> (String, ShouldPrintCalc) {
    let inner = stringify(node, prec);
    // shouldPrintCalc — true if the reduction returned a MathExpression OR a Function
    // (line 76).
    let should_print_calc = matches!(
        node,
        CalcNode::MathExpression { .. } | CalcNode::Function { .. }
    );
    if should_print_calc {
        (format!("{calc_word}({inner})"), ShouldPrintCalc::Yes)
    } else {
        (inner, ShouldPrintCalc::No)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShouldPrintCalc {
    Yes,
    No,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_basic() {
        let n = CalcNode::Number { value: 1.5 };
        assert_eq!(stringify(&n, Precision::At(5.0)), "1.5");
    }

    #[test]
    fn number_zero() {
        // -0.0 normalizes to "0".
        let n = CalcNode::Number { value: -0.0 };
        assert_eq!(stringify(&n, Precision::At(5.0)), "0");
    }

    #[test]
    fn dimension_basic() {
        let n = CalcNode::Dimension {
            kind: crate::parser::DimensionKind::Length,
            value: 100.0,
            unit: "px".to_string(),
        };
        assert_eq!(stringify(&n, Precision::At(5.0)), "100px");
    }

    #[test]
    fn precision_round_to_5() {
        // 0.142857 -> Math.round(0.142857 * 1e5) / 1e5 = 14286 / 1e5 = 0.14286.
        let n = CalcNode::Dimension {
            kind: crate::parser::DimensionKind::Em,
            value: 0.142857,
            unit: "em".to_string(),
        };
        assert_eq!(stringify(&n, Precision::At(5.0)), "0.14286em");
    }

    #[test]
    fn math_expression_addsub() {
        // 100% + 10px
        let l = CalcNode::Dimension {
            kind: crate::parser::DimensionKind::Percentage,
            value: 100.0,
            unit: "%".to_string(),
        };
        let r = CalcNode::Dimension {
            kind: crate::parser::DimensionKind::Length,
            value: 10.0,
            unit: "px".to_string(),
        };
        let expr = CalcNode::MathExpression {
            operator: "+".to_string(),
            left: Box::new(l),
            right: Box::new(r),
        };
        assert_eq!(stringify(&expr, Precision::At(5.0)), "100% + 10px");
    }

    #[test]
    fn math_expression_muldiv_no_spaces() {
        // 100%/2
        let l = CalcNode::Dimension {
            kind: crate::parser::DimensionKind::Percentage,
            value: 100.0,
            unit: "%".to_string(),
        };
        let r = CalcNode::Number { value: 2.0 };
        let expr = CalcNode::MathExpression {
            operator: "/".to_string(),
            left: Box::new(l),
            right: Box::new(r),
        };
        assert_eq!(stringify(&expr, Precision::At(5.0)), "100%/2");
    }

    #[test]
    fn parens_when_higher_precedence_inside_lower() {
        // (1px + 2px) * 3 — top is mul (prec 0), left child is add (prec 1).
        // 0 < 1 → parens.
        let inner = CalcNode::MathExpression {
            operator: "+".to_string(),
            left: Box::new(CalcNode::Dimension {
                kind: crate::parser::DimensionKind::Length,
                value: 1.0,
                unit: "px".to_string(),
            }),
            right: Box::new(CalcNode::Dimension {
                kind: crate::parser::DimensionKind::Length,
                value: 2.0,
                unit: "px".to_string(),
            }),
        };
        let outer = CalcNode::MathExpression {
            operator: "*".to_string(),
            left: Box::new(inner),
            right: Box::new(CalcNode::Number { value: 3.0 }),
        };
        assert_eq!(stringify(&outer, Precision::At(5.0)), "(1px + 2px)*3");
    }
}
