//! Port of `postcss-calc/src/lib/reducer.js`. Line-numbered references
//! point at upstream `reducer.js`.
//!
//! The reducer collapses additions/subtractions, distributes multiplication
//! and division, converts compatible units, and lifts redundant parens.
//! Float math here is the highest-risk surface in the entire postcss-calc
//! port — every arithmetic step must produce bit-identical IEEE-754
//! results to V8 so the post-stringify text round-trips.

use crate::lib::convert_unit::{convert_unit, ConvertUnitError, Precision};
use crate::parser::{CalcNode, DimensionKind};

/// `isValueType(node)` upstream (lines 8-28). Numbers and most dimension
/// kinds qualify; `UnknownDimension` does NOT.
fn is_value_type(node: &CalcNode) -> bool {
    match node {
        CalcNode::Number { .. } => true,
        CalcNode::Dimension { kind, .. } => kind.is_value_type(),
        _ => false,
    }
}

/// Same `node.type` shape used by `findIndex` upstream — value-types match
/// when their dimension-kind matches. `Number` matches `Number` only.
fn type_tag(node: &CalcNode) -> Option<&'static str> {
    match node {
        CalcNode::Number { .. } => Some("Number"),
        CalcNode::Dimension { kind, .. } => Some(match kind {
            DimensionKind::Length => "LengthValue",
            DimensionKind::Angle => "AngleValue",
            DimensionKind::Time => "TimeValue",
            DimensionKind::Frequency => "FrequencyValue",
            DimensionKind::Resolution => "ResolutionValue",
            DimensionKind::Em => "EmValue",
            DimensionKind::Ex => "ExValue",
            DimensionKind::Ch => "ChValue",
            DimensionKind::Rem => "RemValue",
            DimensionKind::Vh => "VhValue",
            DimensionKind::Vw => "VwValue",
            DimensionKind::Vmin => "VminValue",
            DimensionKind::Vmax => "VmaxValue",
            DimensionKind::Percentage => "PercentageValue",
            DimensionKind::Unknown => "UnknownDimension",
        }),
        _ => None,
    }
}

/// `flip(operator)` upstream (line 32).
fn flip(operator: char) -> char {
    if operator == '+' { '-' } else { '+' }
}

/// `isAddSubOperator` (line 39).
fn is_add_sub_operator(s: &str) -> bool {
    matches!(s, "+" | "-")
}

/// Reduce errors mirror upstream's thrown `Error` instances. The transform
/// layer catches and emits `result.warn(error.message, ...)`.
#[derive(Debug, Clone)]
pub struct ReduceError(pub String);

impl std::fmt::Display for ReduceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ReduceError {}

impl From<ConvertUnitError> for ReduceError {
    fn from(e: ConvertUnitError) -> Self { ReduceError(e.0) }
}

#[derive(Debug, Clone)]
struct Collectible {
    pre_operator: char,
    node: CalcNode,
}

/// `collectAddSubItems(preOperator, node, collected, precision)` upstream
/// (lines 53-127).
fn collect_add_sub_items(
    pre_operator: char,
    node: CalcNode,
    collected: &mut Vec<Collectible>,
    precision: Precision,
) -> Result<(), ReduceError> {
    if pre_operator != '+' && pre_operator != '-' {
        return Err(ReduceError(format!("invalid operator {}", pre_operator)));
    }
    if is_value_type(&node) {
        // Find collectible whose node.type matches.
        let node_tag = type_tag(&node);
        let item_index = collected.iter().position(|c| type_tag(&c.node) == node_tag);

        if let Some(idx) = item_index {
            // node.value === 0 → return (don't add).
            if value_of(&node) == 0.0 {
                return Ok(());
            }

            // convertNodesUnits(otherValueNode, node, precision)
            let (mut reduced_node, current) = convert_nodes_units(
                std::mem::replace(&mut collected[idx].node, CalcNode::Number { value: 0.0 }),
                node,
                precision,
            )?;

            // If the existing pre-operator was '-', flip its sign and reset to '+'.
            if collected[idx].pre_operator == '-' {
                collected[idx].pre_operator = '+';
                value_of_mut(&mut reduced_node, |v| *v *= -1.0);
            }
            // Apply current value with proper sign.
            let current_v = value_of(&current);
            value_of_mut(&mut reduced_node, |v| {
                if pre_operator == '+' { *v += current_v; }
                else { *v -= current_v; }
            });

            // Make sure reducedNode.value >= 0.
            let v = value_of(&reduced_node);
            if v >= 0.0 {
                collected[idx] = Collectible { node: reduced_node, pre_operator: '+' };
            } else {
                value_of_mut(&mut reduced_node, |v| *v *= -1.0);
                collected[idx] = Collectible { node: reduced_node, pre_operator: '-' };
            }
        } else {
            // No matching index. Make sure node.value >= 0 before pushing.
            let v = value_of(&node);
            if v >= 0.0 {
                collected.push(Collectible { node, pre_operator });
            } else {
                let mut node = node;
                value_of_mut(&mut node, |v| *v *= -1.0);
                collected.push(Collectible { node, pre_operator: flip(pre_operator) });
            }
        }
        return Ok(());
    }

    if let CalcNode::MathExpression { operator, left, right } = node {
        if is_add_sub_operator(&operator) {
            collect_add_sub_items(pre_operator, *left, collected, precision)?;
            // collectRightOperator = preOperator === '-' ? flip(node.operator) : node.operator;
            let op_char = operator.chars().next().unwrap();
            let collect_right_operator = if pre_operator == '-' { flip(op_char) } else { op_char };
            collect_add_sub_items(collect_right_operator, *right, collected, precision)?;
        } else {
            // * or /: reduce first.
            let reduced = reduce(
                CalcNode::MathExpression {
                    operator: operator.clone(),
                    left,
                    right,
                },
                precision,
            )?;
            // prevent infinite recursive call
            match &reduced {
                CalcNode::MathExpression { operator: rop, .. } if !is_add_sub_operator(rop) => {
                    collected.push(Collectible { node: reduced, pre_operator });
                }
                _ => {
                    collect_add_sub_items(pre_operator, reduced, collected, precision)?;
                }
            }
        }
        return Ok(());
    }

    if let CalcNode::ParenthesizedExpression { content } = node {
        collect_add_sub_items(pre_operator, *content, collected, precision)?;
        return Ok(());
    }

    // Function or unknown.
    collected.push(Collectible { node, pre_operator });
    Ok(())
}

/// `reduceAddSubExpression(node, precision)` upstream (lines 133-178).
fn reduce_add_sub_expression(
    node: CalcNode,
    precision: Precision,
) -> Result<CalcNode, ReduceError> {
    let mut collected: Vec<Collectible> = Vec::new();
    collect_add_sub_items('+', node, &mut collected, precision)?;

    // withoutZeroItem = collected.filter(item => !(isValueType(item.node) && item.node.value === 0))
    let mut without_zero: Vec<Collectible> = collected
        .iter()
        .filter(|c| !(is_value_type(&c.node) && value_of(&c.node) == 0.0))
        .cloned()
        .collect();

    // First non-zero item.
    let first_non_zero = without_zero.first().cloned();

    // prevent producing "calc(-var(--a))" or "calc()" — re-insert a zero item
    // when first is missing OR first is a non-value-type with '-' preOp.
    let needs_zero_prefix = match &first_non_zero {
        None => true,
        Some(item) => item.pre_operator == '-' && !is_value_type(&item.node),
    };
    if needs_zero_prefix {
        if let Some(first_zero) = collected
            .iter()
            .find(|c| is_value_type(&c.node) && value_of(&c.node) == 0.0)
        {
            without_zero.insert(0, first_zero.clone());
        }
    }

    // Make sure the preOperator of the first item is '+'.
    if !without_zero.is_empty() {
        if without_zero[0].pre_operator == '-' && is_value_type(&without_zero[0].node) {
            value_of_mut(&mut without_zero[0].node, |v| *v *= -1.0);
            without_zero[0].pre_operator = '+';
        }
    }

    // Build the result tree left-to-right.
    if without_zero.is_empty() {
        // collected was empty AND no zero items existed. Per upstream this
        // would be a runtime error (`without_zero[0].node` undefined access).
        // In practice unreachable for well-formed inputs since collected has
        // at least one element if we got into this function. Defensive
        // fallback: return Number(0).
        return Ok(CalcNode::Number { value: 0.0 });
    }
    let mut iter = without_zero.into_iter();
    let mut root = iter.next().unwrap().node;
    for item in iter {
        root = CalcNode::MathExpression {
            operator: item.pre_operator.to_string(),
            left: Box::new(root),
            right: Box::new(item.node),
        };
    }
    Ok(root)
}

/// `reduceDivisionExpression(node)` upstream (lines 182-192).
fn reduce_division_expression(node: CalcNode) -> Result<CalcNode, ReduceError> {
    if let CalcNode::MathExpression { operator: _, left, right } = node {
        if !is_value_type(&right) {
            // Reconstruct and return.
            return Ok(CalcNode::MathExpression {
                operator: "/".to_string(),
                left,
                right,
            });
        }
        match right.as_ref() {
            CalcNode::Number { value: divisor } => {
                apply_number_division(*left, *divisor)
            }
            _ => {
                // upstream: throw new Error(`Cannot divide by "${node.right.unit}", number expected`);
                let unit = match right.as_ref() {
                    CalcNode::Dimension { unit, .. } => unit.clone(),
                    _ => String::new(),
                };
                Err(ReduceError(format!(
                    "Cannot divide by \"{}\", number expected",
                    unit
                )))
            }
        }
    } else {
        unreachable!("reduce_division_expression called with non-MathExpression");
    }
}

/// `applyNumberDivision(node, divisor)` upstream (lines 201-233).
fn apply_number_division(node: CalcNode, divisor: f64) -> Result<CalcNode, ReduceError> {
    if divisor == 0.0 {
        return Err(ReduceError("Cannot divide by zero".to_string()));
    }
    if is_value_type(&node) {
        let mut node = node;
        value_of_mut(&mut node, |v| *v /= divisor);
        return Ok(node);
    }
    if let CalcNode::MathExpression { operator, left, right } = node {
        if is_add_sub_operator(&operator) {
            // Distribute: (a +/- b) / num → a/num +/- b/num.
            let l = apply_number_division(*left, divisor)?;
            let r = apply_number_division(*right, divisor)?;
            return Ok(CalcNode::MathExpression {
                operator,
                left: Box::new(l),
                right: Box::new(r),
            });
        }
        // Otherwise — preserve the / so the browser handles it.
        return Ok(CalcNode::MathExpression {
            operator: "/".to_string(),
            left: Box::new(CalcNode::MathExpression { operator, left, right }),
            right: Box::new(CalcNode::Number { value: divisor }),
        });
    }
    // Fallback: preserve as `/`.
    Ok(CalcNode::MathExpression {
        operator: "/".to_string(),
        left: Box::new(node),
        right: Box::new(CalcNode::Number { value: divisor }),
    })
}

/// `reduceMultiplicationExpression(node)` upstream (lines 237-247).
fn reduce_multiplication_expression(node: CalcNode) -> CalcNode {
    if let CalcNode::MathExpression { operator: _, left, right } = node {
        // (expr) * number
        if let CalcNode::Number { value: v } = right.as_ref() {
            return apply_number_multiplication(*left, *v);
        }
        // number * (expr)
        if let CalcNode::Number { value: v } = left.as_ref() {
            return apply_number_multiplication(*right, *v);
        }
        return CalcNode::MathExpression {
            operator: "*".to_string(),
            left,
            right,
        };
    }
    node
}

/// `applyNumberMultiplication(node, multiplier)` upstream (lines 255-284).
fn apply_number_multiplication(node: CalcNode, multiplier: f64) -> CalcNode {
    if is_value_type(&node) {
        let mut node = node;
        value_of_mut(&mut node, |v| *v *= multiplier);
        return node;
    }
    if let CalcNode::MathExpression { operator, left, right } = node {
        if is_add_sub_operator(&operator) {
            // Distribute: (a +/- b) * num → a*num +/- b*num.
            let l = apply_number_multiplication(*left, multiplier);
            let r = apply_number_multiplication(*right, multiplier);
            return CalcNode::MathExpression {
                operator,
                left: Box::new(l),
                right: Box::new(r),
            };
        }
        return CalcNode::MathExpression {
            operator: "*".to_string(),
            left: Box::new(CalcNode::MathExpression { operator, left, right }),
            right: Box::new(CalcNode::Number { value: multiplier }),
        };
    }
    // Function / Parenthesized — preserve as multiplication.
    CalcNode::MathExpression {
        operator: "*".to_string(),
        left: Box::new(node),
        right: Box::new(CalcNode::Number { value: multiplier }),
    }
}

/// `convertNodesUnits(left, right, precision)` upstream (lines 291-317).
fn convert_nodes_units(
    left: CalcNode,
    right: CalcNode,
    precision: Precision,
) -> Result<(CalcNode, CalcNode), ReduceError> {
    // Only convert for length/angle/time/freq/res when both sides have units
    // and dimension kinds match.
    if let CalcNode::Dimension { kind: lk, value: _, unit: lu } = &left {
        if matches!(
            lk,
            DimensionKind::Length
                | DimensionKind::Angle
                | DimensionKind::Time
                | DimensionKind::Frequency
                | DimensionKind::Resolution
        ) {
            if let CalcNode::Dimension { kind: rk, value: rv, unit: ru } = &right {
                if rk == lk && !ru.is_empty() && !lu.is_empty() {
                    let converted = convert_unit(*rv, ru, lu, precision)?;
                    return Ok((
                        left.clone(),
                        CalcNode::Dimension {
                            kind: *lk,
                            value: converted,
                            unit: lu.clone(),
                        },
                    ));
                }
            }
        }
    }
    Ok((left, right))
}

/// `includesNoCssProperties(node)` upstream (lines 322-329).
fn includes_no_css_properties(content: &CalcNode) -> bool {
    if matches!(content, CalcNode::Function { .. }) { return false; }
    if let CalcNode::MathExpression { left, right, .. } = content {
        if matches!(left.as_ref(), CalcNode::Function { .. }) { return false; }
        if matches!(right.as_ref(), CalcNode::Function { .. }) { return false; }
    }
    true
}

/// `reduce(node, precision)` upstream (lines 335-360).
pub fn reduce(node: CalcNode, precision: Precision) -> Result<CalcNode, ReduceError> {
    if let CalcNode::MathExpression { operator, left, right } = node {
        if is_add_sub_operator(&operator) {
            return reduce_add_sub_expression(
                CalcNode::MathExpression { operator, left, right },
                precision,
            );
        }
        // node.left = reduce(node.left, precision); node.right = reduce(node.right, precision);
        let l = reduce(*left, precision)?;
        let r = reduce(*right, precision)?;
        let rebuilt = CalcNode::MathExpression {
            operator: operator.clone(),
            left: Box::new(l),
            right: Box::new(r),
        };
        return match operator.as_str() {
            "/" => reduce_division_expression(rebuilt),
            "*" => Ok(reduce_multiplication_expression(rebuilt)),
            _ => Ok(rebuilt),
        };
    }

    if let CalcNode::ParenthesizedExpression { content } = node {
        if includes_no_css_properties(&content) {
            return reduce(*content, precision);
        }
        return Ok(CalcNode::ParenthesizedExpression { content });
    }

    Ok(node)
}

// --------------------------------------------------------------------------
// Helpers — typed value-of accessors. These exist because the JS code
// directly mutates `node.value` on whatever value-type node it finds.
// --------------------------------------------------------------------------

fn value_of(node: &CalcNode) -> f64 {
    match node {
        CalcNode::Number { value } => *value,
        CalcNode::Dimension { value, .. } => *value,
        _ => 0.0,
    }
}

fn value_of_mut(node: &mut CalcNode, f: impl FnOnce(&mut f64)) {
    match node {
        CalcNode::Number { value } => f(value),
        CalcNode::Dimension { value, .. } => f(value),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::lib::stringifier::stringify;

    fn pipe(input: &str, prec: Precision) -> String {
        let ast = parse(input).expect("parse");
        let red = reduce(ast, prec).expect("reduce");
        stringify(&red, prec)
    }

    #[test]
    fn simple_add() {
        assert_eq!(pipe("1px + 1px", Precision::At(5.0)), "2px");
    }

    #[test]
    fn simple_sub() {
        assert_eq!(pipe("3em - 1em", Precision::At(5.0)), "2em");
    }

    #[test]
    fn distribute_mul() {
        // (1px + 2px) * 3 → 3px + 6px → 9px
        assert_eq!(pipe("(1px + 2px) * 3", Precision::At(5.0)), "9px");
    }

    #[test]
    fn distribute_div() {
        // (10px + 5px) / 5 → 2px + 1px → 3px
        assert_eq!(pipe("(10px + 5px) / 5", Precision::At(5.0)), "3px");
    }

    #[test]
    fn divide_by_zero_errors() {
        let ast = parse("500px/0").unwrap();
        let err = reduce(ast, Precision::At(5.0)).unwrap_err();
        assert_eq!(err.0, "Cannot divide by zero");
    }

    #[test]
    fn divide_by_unit_errors() {
        let ast = parse("500px/2px").unwrap();
        let err = reduce(ast, Precision::At(5.0)).unwrap_err();
        assert_eq!(err.0, "Cannot divide by \"px\", number expected");
    }

    #[test]
    fn vendor_calc_collapses() {
        assert_eq!(pipe("-webkit-calc(1px + 1px)", Precision::At(5.0)), "2px");
    }

    #[test]
    fn unit_conversion_cm_px() {
        // 1cm + 1px → 1.02646cm
        assert_eq!(pipe("1cm + 1px", Precision::At(5.0)), "1.02646cm");
    }

    #[test]
    fn css_var_preserves_calc() {
        // var(--a) is a Function — preserved.
        let ast = parse("var(--a) * 2").unwrap();
        let red = reduce(ast, Precision::At(5.0)).unwrap();
        assert_eq!(stringify(&red, Precision::At(5.0)), "var(--a)*2");
    }

    #[test]
    fn mixed_units_pass_through() {
        // 100% + 1px — incompatible kinds, kept as expression.
        assert_eq!(pipe("100% + 1px", Precision::At(5.0)), "100% + 1px");
    }

    #[test]
    fn unitless_with_unit_passes_through() {
        // 1px + 1 — Number doesn't share kind with Length.
        assert_eq!(pipe("1px + 1", Precision::At(5.0)), "1px + 1");
    }

    #[test]
    fn zero_drops() {
        assert_eq!(pipe("100vw / 2 - 6px + 0px", Precision::At(5.0)), "50vw - 6px");
    }
}
