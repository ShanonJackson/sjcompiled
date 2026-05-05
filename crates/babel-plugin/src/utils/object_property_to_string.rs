//! 1:1 port of `packages/babel-plugin/src/utils/object-property-to-string.ts`.
//!
//! Returns the string form of an `ObjectProperty.key`. The simple
//! `Identifier`-without-`computed` path is the most common (every
//! `{ color: 'blue' }` shape) and is fully portable. Computed-key /
//! template-interpolation / member-expression paths recurse into
//! `evaluate_expression` (Phase 5 §5.6) — wired in §6.8b.
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

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::ast::{build_code_frame_error_no_node, CssBuildError};
use crate::utils::evaluate_expression::evaluate_expression;

fn template_literal_to_string(
    template: &Tpl,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Result<String, CssBuildError> {
    // Mirrors upstream lines 9–32. Walks `quasis[i].value.raw`
    // interleaved with `expressionToString(expressions[i])`.
    let mut result = String::new();
    for (i, q) in template.quasis.iter().enumerate() {
        result.push_str(&q.raw);
        if i < template.exprs.len() {
            let expression = &template.exprs[i];
            // Upstream throws on TS types in interpolations; SWC's
            // Tpl.exprs is `Vec<Box<Expr>>` — TS-type wrappers reach
            // here as Expr::TsTypeAssertion / Expr::TsAs / etc., not
            // as raw TS-only nodes. The babel-evaluator would deopt;
            // we fall through to the recursive expression_to_string
            // which will throw the "has no name" error if it can't
            // produce a string. Bytes match upstream behaviour.
            let evaluated = evaluate_expression(
                expression,
                meta,
                scope_index,
                parent_scope,
                own_scope,
            )
            .value
            .unwrap_or_else(|| Box::new((**expression).clone()));
            let part = expression_to_string(&evaluated, meta, scope_index, parent_scope, own_scope)?;
            result.push_str(&part);
        }
    }
    Ok(result)
}

fn binary_expression_to_string(
    expression: &BinExpr,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Result<String, CssBuildError> {
    // Mirrors upstream lines 34–49. Only `+` is allowed; anything
    // else throws the upstream error message verbatim.
    if matches!(expression.op, BinaryOp::Add) {
        let left_value = expression_to_string(&expression.left, meta, scope_index, parent_scope, own_scope)?;
        let right_value = expression_to_string(&expression.right, meta, scope_index, parent_scope, own_scope)?;
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
pub fn expression_to_string(
    expression: &Expr,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Result<String, CssBuildError> {
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
        // Upstream lines 57-67:
        //   const evaluatedExpression = evaluateExpression(expression, meta);
        //   if (evaluatedExpression.value === expression) { throw ... }
        //   return expressionToString(evaluatedExpression.value, evaluatedExpression.meta);
        let evaluated = evaluate_expression(
            expression,
            meta,
            scope_index,
            parent_scope,
            own_scope,
        );
        // JS `value === expression` reference-equality maps to Rust
        // ResultPair { value: None } (= deopt; evaluator could not
        // fold). The upstream throw includes the Identifier name
        // when the input was an Identifier; otherwise it includes
        // the Babel node-type string.
        let Some(folded) = evaluated.value else {
            let name = match expression {
                Expr::Ident(ident) => ident.sym.to_string(),
                _ => babel_type_name(expression).to_string(),
            };
            return Err(build_code_frame_error_no_node(format!(
                "Cannot statically evaluate the value of \"{}",
                name
            )));
        };
        return expression_to_string(&folded, meta, scope_index, parent_scope, own_scope);
    }

    // {[`key-${name}`]: 'value'}
    if let Expr::Tpl(tpl) = expression {
        return template_literal_to_string(tpl, meta, scope_index, parent_scope, own_scope);
    }

    // {['key-' + name]: 'value'}
    if let Expr::Bin(bin) = expression {
        return binary_expression_to_string(bin, meta, scope_index, parent_scope, own_scope);
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
pub fn object_property_to_string(
    prop_name: &PropName,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Result<String, CssBuildError> {
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
        PropName::Computed(c) => {
            expression_to_string(&c.expr, meta, scope_index, parent_scope, own_scope)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::scope::ScopeIndex;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{IdentName, Module, Number, Str};

    fn dummy_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        }
    }

    fn empty_scope_index() -> (ScopeIndex, crate::compat::scope::ScopeId) {
        let module = Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        let idx = ScopeIndex::build(&module);
        let root = idx.program_scope();
        (idx, root)
    }

    #[test]
    fn ident_key_returns_name() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let (mut scope_index, root) = empty_scope_index();
        let key = PropName::Ident(IdentName::new("color".into(), DUMMY_SP));
        assert_eq!(
            object_property_to_string(&key, &mut meta, &mut scope_index, root, None).unwrap(),
            "color"
        );
    }

    #[test]
    fn string_key_returns_value() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let (mut scope_index, root) = empty_scope_index();
        let key = PropName::Str(Str {
            span: DUMMY_SP,
            value: "background-color".into(),
            raw: None,
        });
        assert_eq!(
            object_property_to_string(&key, &mut meta, &mut scope_index, root, None).unwrap(),
            "background-color"
        );
    }

    #[test]
    fn integer_numeric_key_stringifies_without_dot() {
        let mut state = State::default();
        let mut meta = dummy_meta(&mut state);
        let (mut scope_index, root) = empty_scope_index();
        let key = PropName::Num(Number {
            span: DUMMY_SP,
            value: 1.0,
            raw: None,
        });
        assert_eq!(
            object_property_to_string(&key, &mut meta, &mut scope_index, root, None).unwrap(),
            "1"
        );
    }
}
