//! 1:1 port of `packages/babel-plugin/src/utils/build-css-variables.ts`.
//!
//! Build the CSS variables prop placed as inline styles. Each unique
//! `Variable` becomes one ObjectProperty:
//!
//! ```text
//! '<name>': ix(<expression>[, <suffix>[, <prefix>]])
//! ```
//!
//! Bug-parity rule (from upstream): the prefix is ONLY emitted when
//! suffix is ALSO present. JS short-circuits
//! `(variable.suffix && variable.prefix && t.stringLiteral(variable.prefix))` —
//! if suffix is missing, the prefix is dropped regardless. The Rust
//! port preserves this verbatim. See
//! `packages/babel-plugin/src/utils/build-css-variables.ts:32`.
//!
//! Field-name divergences:
//! * Babel `t.objectProperty(key, value)` → SWC
//!   `Prop::KeyValue(KeyValueProp { key, value })` boxed inside
//!   `PropOrSpread::Prop`.
//! * Babel `t.stringLiteral(s)` → SWC `Lit::Str(Str { value, raw: None, .. })`.
//! * Babel `t.callExpression(callee, args)` → SWC `Expr::Call(CallExpr)`
//!   with `Callee::Expr` and `Vec<ExprOrSpread>` args.
//! * Babel `t.identifier('ix')` → SWC `Ident::new("ix".into(), DUMMY_SP, Default::default())`.

use compiled_utils::unique_by;
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, Ident, KeyValueProp, Lit, Prop, PropName, PropOrSpread,
    Str,
};

use crate::utils::types::Variable;

/// Build the `style={{ '--_x': ix(expr) }}` ObjectProperties from a
/// list of `Variable`s. The `transform` closure mirrors upstream's
/// optional second parameter (defaulting to identity); pass
/// `|expr| expr` for the default behaviour.
///
/// Caller-side default (matches the JS default arg):
/// `build_css_variables(&variables, |expr| expr)`.
pub fn build_css_variables<F>(variables: &[Variable], transform: F) -> Vec<PropOrSpread>
where
    F: Fn(Box<Expr>) -> Box<Expr>,
{
    // Upstream: `unique(variables, (v) => v.name)`.
    let deduped = unique_by(variables, |v: &Variable| v.name.clone());

    deduped
        .into_iter()
        .map(|variable| {
            // Replicate the JS `[transform(...), suffix && ..., suffix && prefix && ...].filter(Boolean)`
            // truthy semantics. `Option<String>::None` and `Some("")` are
            // both treated as falsy to match JS exactly.
            let suffix_truthy = variable
                .suffix
                .as_deref()
                .filter(|s| !s.is_empty());
            let prefix_truthy = variable
                .prefix
                .as_deref()
                .filter(|s| !s.is_empty());

            let mut args: Vec<ExprOrSpread> = Vec::with_capacity(3);
            // Mirror upstream's `[transform(variable.expression), …].filter(Boolean)`.
            // When `expression` is `None` (no-init IIFE-injected
            // declarator), drop the first arg entirely — produces
            // `ix()` instead of `ix(<param>)`.
            if let Some(expr) = variable.expression.clone() {
                args.push(ExprOrSpread {
                    spread: None,
                    expr: transform(expr),
                });
            }
            if let Some(suffix) = suffix_truthy {
                args.push(ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: suffix.into(),
                        raw: None,
                    }))),
                });
                // Prefix is gated on suffix-truthy AND prefix-truthy
                // (matches the JS `suffix && prefix && ...` short-circuit).
                if let Some(prefix) = prefix_truthy {
                    args.push(ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: prefix.into(),
                            raw: None,
                        }))),
                    });
                }
            }

            let call = Expr::Call(CallExpr {
                span: DUMMY_SP,
                callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                    "ix".into(),
                    DUMMY_SP,
                    Default::default(),
                )))),
                args,
                type_args: None,
                ctxt: Default::default(),
            });

            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Str(Str {
                    span: DUMMY_SP,
                    value: variable.name.clone().into(),
                    raw: None,
                }),
                value: Box::new(call),
            })))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::ecma::ast::Number;

    fn ident_expr(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            Default::default(),
        )))
    }

    fn num_expr(n: f64) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: n,
            raw: None,
        })))
    }

    fn make_var(name: &str, expr: Box<Expr>, prefix: Option<&str>, suffix: Option<&str>) -> Variable {
        Variable {
            name: name.to_string(),
            expression: Some(expr),
            prefix: prefix.map(String::from),
            suffix: suffix.map(String::from),
        }
    }

    fn extract_call_args(prop: &PropOrSpread) -> Vec<Box<Expr>> {
        let PropOrSpread::Prop(boxed) = prop else { panic!("not a Prop") };
        let Prop::KeyValue(kv) = &**boxed else { panic!("not KeyValue") };
        let Expr::Call(call) = &*kv.value else { panic!("value not Call") };
        call.args.iter().map(|a| a.expr.clone()).collect()
    }

    fn extract_key(prop: &PropOrSpread) -> String {
        let PropOrSpread::Prop(boxed) = prop else { panic!("not a Prop") };
        let Prop::KeyValue(kv) = &**boxed else { panic!("not KeyValue") };
        let PropName::Str(s) = &kv.key else { panic!("key not Str") };
        s.value.to_atom_lossy().as_str().to_string()
    }

    #[test]
    fn emits_single_arg_when_no_prefix_no_suffix() {
        let v = make_var("--_a", ident_expr("foo"), None, None);
        let out = build_css_variables(&[v], |e| e);
        assert_eq!(out.len(), 1);
        let args = extract_call_args(&out[0]);
        assert_eq!(args.len(), 1);
        assert_eq!(extract_key(&out[0]), "--_a");
    }

    #[test]
    fn emits_two_args_with_suffix_only() {
        let v = make_var("--_a", num_expr(8.0), None, Some("px"));
        let out = build_css_variables(&[v], |e| e);
        let args = extract_call_args(&out[0]);
        assert_eq!(args.len(), 2);
        let Expr::Lit(Lit::Str(s)) = &*args[1] else { panic!("arg 2 not Str") };
        assert_eq!(s.value.to_atom_lossy().as_str(), "px");
    }

    #[test]
    fn emits_three_args_when_both_present() {
        let v = make_var("--_a", num_expr(8.0), Some("-"), Some("px"));
        let out = build_css_variables(&[v], |e| e);
        let args = extract_call_args(&out[0]);
        assert_eq!(args.len(), 3);
        let Expr::Lit(Lit::Str(suf)) = &*args[1] else { panic!("arg 2 not Str") };
        let Expr::Lit(Lit::Str(pre)) = &*args[2] else { panic!("arg 3 not Str") };
        assert_eq!(suf.value.to_atom_lossy().as_str(), "px");
        assert_eq!(pre.value.to_atom_lossy().as_str(), "-");
    }

    #[test]
    fn drops_prefix_when_suffix_missing_bug_parity() {
        // Upstream BUG-PARITY: prefix-only is dropped because of the
        // `suffix && prefix && ...` short-circuit. Locked here so a
        // future "fix" that emits prefix-without-suffix breaks loudly.
        let v = make_var("--_a", num_expr(8.0), Some("-"), None);
        let out = build_css_variables(&[v], |e| e);
        let args = extract_call_args(&out[0]);
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn dedupes_by_name_preserving_first_insertion() {
        // The first `--_a` keeps its expression / affix; the second is dropped.
        let v1 = make_var("--_a", ident_expr("first"), None, Some("px"));
        let v2 = make_var("--_a", ident_expr("second"), None, Some("em"));
        let v3 = make_var("--_b", ident_expr("third"), None, None);
        let out = build_css_variables(&[v1, v2, v3], |e| e);
        assert_eq!(out.len(), 2);
        assert_eq!(extract_key(&out[0]), "--_a");
        assert_eq!(extract_key(&out[1]), "--_b");
        let args0 = extract_call_args(&out[0]);
        // First definition wins → expression "first" + suffix "px"
        let Expr::Ident(id) = &*args0[0] else { panic!("arg 0 not Ident") };
        assert_eq!(id.sym.as_str(), "first");
    }

    #[test]
    fn transform_callback_is_applied_to_expression() {
        let v = make_var("--_a", ident_expr("inner"), None, None);
        // Replace expression with the literal `42`.
        let out = build_css_variables(&[v], |_| num_expr(42.0));
        let args = extract_call_args(&out[0]);
        let Expr::Lit(Lit::Num(n)) = &*args[0] else { panic!("arg 0 not Num") };
        assert_eq!(n.value, 42.0);
    }

    #[test]
    fn empty_string_suffix_is_treated_as_falsy() {
        // JS truthy semantics: "" is falsy, so suffix="" suppresses
        // both suffix and prefix args.
        let v = make_var("--_a", num_expr(1.0), Some("-"), Some(""));
        let out = build_css_variables(&[v], |e| e);
        let args = extract_call_args(&out[0]);
        assert_eq!(args.len(), 1);
    }
}
