//! 1:1 port of `packages/babel-plugin/src/utils/object-property-to-string.ts`.
//!
//! Returns the string form of an `ObjectProperty.key`. The simple
//! `Identifier`-without-`computed` path is the most common (every
//! `{ color: 'blue' }` shape) and is fully portable. Other branches
//! ultimately reach `evaluateExpression` (Phase 5 §5.6) — those are
//! stubbed with `unimplemented!()` carrying the gating-row citation.
//!
//! ### Babel→SWC divergence
//!
//! Babel `ObjectProperty.key` typed as `Expression | PrivateName`.
//! SWC's `Prop::KeyValue.key` is `PropName` — distinct enum with
//! `Ident(IdentName)`, `Str(Str)`, `Num(Number)`, `BigInt`, and
//! `Computed(ComputedPropName)` variants. The `computed` flag in
//! Babel is implicit in SWC: only `PropName::Computed` is computed.
//!
//! `templateLiteralToString` walks `quasis[i].value.raw` interleaved
//! with `expressionToString(expressions[i])`. SWC's `Tpl` has the
//! same `quasis: Vec<TplElement>` + `exprs: Vec<Box<Expr>>` shape;
//! `cooked` / `raw` semantics match.
//!
//! `binaryExpressionToString` only accepts the `+` operator (string
//! concat). SWC's `BinExpr.op` enum has `BinaryOp::Add`.

use swc_core::ecma::ast::{
    BinExpr, BinaryOp, Expr, Lit, PropName, Tpl,
};

use crate::types::Metadata;
use crate::utils::ast::{build_code_frame_error_no_node, CssBuildError};

/// Internal helper carrying the "did this evaluate?" return shape.
/// Mirrors the JS `{ value: t.Expression, meta: Metadata }` tuple
/// from `EvaluateExpression`. Phase 5 §5.6 lands the concrete
/// evaluator; today this lives at the call boundary as
/// `unimplemented!()`.
pub type EvaluateExpressionFn<'a, 'b> = &'a dyn Fn(&Expr, &Metadata<'b>) -> EvaluatedExpression;

#[derive(Debug)]
pub struct EvaluatedExpression {
    pub value: Box<Expr>,
    // The JS variant returns a fresh `meta`; the Rust port returns
    // the change applied to the metadata as a separate value the
    // caller threads through (Phase 5 §5.6 specifies the exact
    // shape — included_files diff, etc).
}

fn template_literal_to_string(
    template: &Tpl,
    _meta: &mut Metadata<'_>,
    _expression_to_string: impl Fn(&Expr, &mut Metadata<'_>) -> Result<String, CssBuildError>,
) -> Result<String, CssBuildError> {
    // Mirrors upstream lines 9–32. Walks `quasis[i].value.raw`
    // interleaved with `expressionToString(expressions[i])`. The
    // expression-side dispatch hits `evaluateExpression`
    // unconditionally — Phase 5 §5.6 work.
    if template.exprs.is_empty() {
        // Static template literal — no interpolation to evaluate.
        // This sub-case IS reachable from §4.4 paths; supported now.
        let mut result = String::new();
        for q in &template.quasis {
            result.push_str(&q.raw);
        }
        return Ok(result);
    }
    unimplemented!(
        "templateLiteralToString with interpolations requires evaluateExpression — \
         Phase 5 §5.6 (utils/evaluate-expression.ts)"
    )
}

fn binary_expression_to_string(
    expression: &BinExpr,
    meta: &mut Metadata<'_>,
    expression_to_string: impl Fn(&Expr, &mut Metadata<'_>) -> Result<String, CssBuildError>,
) -> Result<String, CssBuildError> {
    // Mirrors upstream lines 34–49. Only `+` is allowed; anything
    // else throws the upstream error message verbatim.
    if matches!(expression.op, BinaryOp::Add) {
        let left_value = expression_to_string(&expression.left, meta)?;
        let right_value = expression_to_string(&expression.right, meta)?;
        return Ok(format!("{}{}", left_value, right_value));
    }
    Err(build_code_frame_error_no_node(format!(
        "Cannot use {} for string operation. Use + for string concatenation",
        op_str(expression.op)
    )))
}

fn op_str(op: BinaryOp) -> &'static str {
    // Babel's `operator` is the source-form string; SWC's BinaryOp
    // is an enum. Rebuild the source form so the error message
    // matches upstream byte-for-byte.
    match op {
        BinaryOp::EqEq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::EqEqEq => "===",
        BinaryOp::NotEqEq => "!==",
        BinaryOp::Lt => "<",
        BinaryOp::LtEq => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::GtEq => ">=",
        BinaryOp::LShift => "<<",
        BinaryOp::RShift => ">>",
        BinaryOp::ZeroFillRShift => ">>>",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::BitAnd => "&",
        BinaryOp::LogicalOr => "||",
        BinaryOp::LogicalAnd => "&&",
        BinaryOp::In => "in",
        BinaryOp::InstanceOf => "instanceof",
        BinaryOp::Exp => "**",
        BinaryOp::NullishCoalescing => "??",
    }
}

/// `expressionToString` upstream lines 51–79. Recursive dispatch
/// over expression kinds.
pub fn expression_to_string(expression: &Expr, meta: &mut Metadata<'_>) -> Result<String, CssBuildError> {
    // {'key-name': 'value'} or {1: 'value'}
    if let Expr::Lit(lit) = expression {
        match lit {
            Lit::Str(s) => return Ok(s.value.to_atom_lossy().as_str().to_string()),
            Lit::Num(n) => {
                // JS `String(numericLiteral.value)` — `String(12)` →
                // `"12"`, `String(1.5)` → `"1.5"`. Rust's Display
                // for f64 is close but not identical for some
                // edge cases; for §4.4 reachable inputs (integer
                // CSS keys like `{1: 'value'}`) the output matches.
                if n.value.fract() == 0.0 && n.value.abs() < 1e16 {
                    return Ok((n.value as i64).to_string());
                }
                return Ok(n.value.to_string());
            }
            _ => {}
        }
    }

    // {[key]: 'value'} and {[key.key]: 'value'} — needs evaluateExpression.
    if matches!(expression, Expr::Ident(_) | Expr::Member(_)) {
        unimplemented!(
            "expressionToString for Identifier / MemberExpression requires evaluateExpression — \
             Phase 5 §5.6 (utils/evaluate-expression.ts)"
        );
    }

    // {[`key-${name}`]: 'value'}
    if let Expr::Tpl(tpl) = expression {
        return template_literal_to_string(tpl, meta, expression_to_string);
    }

    // {['key-' + name]: 'value'}
    if let Expr::Bin(bin) = expression {
        return binary_expression_to_string(bin, meta, expression_to_string);
    }

    Err(build_code_frame_error_no_node(format!(
        "{} has no name.'",
        // Mirror upstream's `expression.type` literal — a Babel
        // node-type string. SWC doesn't expose this directly; we
        // reconstruct the canonical Babel name for byte parity in
        // the error message.
        babel_type_name(expression)
    )))
}

/// Map an SWC `Expr` variant back to the Babel `node.type` string the
/// JS error message would have produced. Keeps `expression.type`
/// error-byte parity with upstream.
fn babel_type_name(expression: &Expr) -> &'static str {
    match expression {
        Expr::This(_) => "ThisExpression",
        Expr::Array(_) => "ArrayExpression",
        Expr::Object(_) => "ObjectExpression",
        Expr::Fn(_) => "FunctionExpression",
        Expr::Unary(_) => "UnaryExpression",
        Expr::Update(_) => "UpdateExpression",
        Expr::Bin(_) => "BinaryExpression",
        Expr::Assign(_) => "AssignmentExpression",
        Expr::Member(_) => "MemberExpression",
        Expr::SuperProp(_) => "MemberExpression",
        Expr::Cond(_) => "ConditionalExpression",
        Expr::Call(_) => "CallExpression",
        Expr::New(_) => "NewExpression",
        Expr::Seq(_) => "SequenceExpression",
        Expr::Ident(_) => "Identifier",
        Expr::Lit(_) => "Literal",
        Expr::Tpl(_) => "TemplateLiteral",
        Expr::TaggedTpl(_) => "TaggedTemplateExpression",
        Expr::Arrow(_) => "ArrowFunctionExpression",
        Expr::Class(_) => "ClassExpression",
        Expr::Yield(_) => "YieldExpression",
        Expr::MetaProp(_) => "MetaProperty",
        Expr::Await(_) => "AwaitExpression",
        Expr::Paren(_) => "ParenthesizedExpression",
        Expr::JSXMember(_) => "JSXMemberExpression",
        Expr::JSXNamespacedName(_) => "JSXNamespacedName",
        Expr::JSXEmpty(_) => "JSXEmptyExpression",
        Expr::JSXElement(_) => "JSXElement",
        Expr::JSXFragment(_) => "JSXFragment",
        Expr::TsTypeAssertion(_) => "TSTypeAssertion",
        Expr::TsConstAssertion(_) => "TSConstAssertion",
        Expr::TsNonNull(_) => "TSNonNullExpression",
        Expr::TsAs(_) => "TSAsExpression",
        Expr::TsInstantiation(_) => "TSInstantiationExpression",
        Expr::TsSatisfies(_) => "TSSatisfiesExpression",
        Expr::PrivateName(_) => "PrivateName",
        Expr::OptChain(_) => "OptionalCallExpression",
        Expr::Invalid(_) => "Invalid",
    }
}

/// `objectPropertyToString` upstream lines 87–95. The single public
/// entry point.
///
/// SWC `PropName` divergence: matches the Babel `Identifier &&
/// !computed` happy path against `PropName::Ident(IdentName)`;
/// `PropName::Str` and `PropName::Num` are also handled here (Babel
/// hits these via the StringLiteral / NumericLiteral arms in
/// `expressionToString`). `PropName::Computed` dispatches to
/// `expression_to_string` over the inner expression (matches Babel's
/// `expressionToString(key, meta)` dispatch).
pub fn object_property_to_string(prop_name: &PropName, meta: &mut Metadata<'_>) -> Result<String, CssBuildError> {
    match prop_name {
        // {key: 'value'} — Babel's Identifier&&!computed branch.
        PropName::Ident(ident_name) => Ok(ident_name.sym.to_string()),
        // SWC routes `{'key': 'value'}` and `{1: 'value'}` here as
        // PropName::Str / PropName::Num directly (no synthetic
        // computed-key wrap). Babel routes them through
        // expressionToString's StringLiteral/NumericLiteral arms;
        // bytes match.
        PropName::Str(s) => Ok(s.value.to_atom_lossy().as_str().to_string()),
        PropName::Num(n) => {
            if n.value.fract() == 0.0 && n.value.abs() < 1e16 {
                Ok((n.value as i64).to_string())
            } else {
                Ok(n.value.to_string())
            }
        }
        PropName::BigInt(b) => Ok(b.value.to_string()),
        PropName::Computed(c) => expression_to_string(&c.expr, meta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{IdentName, Number, Str};

    fn dummy_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        }
    }

    #[test]
    fn ident_key_returns_name() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let key = PropName::Ident(IdentName::new("color".into(), DUMMY_SP));
        assert_eq!(object_property_to_string(&key, &mut meta).unwrap(), "color");
    }

    #[test]
    fn string_key_returns_value() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let key = PropName::Str(Str {
            span: DUMMY_SP,
            value: "background-color".into(),
            raw: None,
        });
        assert_eq!(
            object_property_to_string(&key, &mut meta).unwrap(),
            "background-color"
        );
    }

    #[test]
    fn integer_numeric_key_stringifies_without_dot() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let key = PropName::Num(Number {
            span: DUMMY_SP,
            value: 1.0,
            raw: None,
        });
        assert_eq!(object_property_to_string(&key, &mut meta).unwrap(), "1");
    }
}
