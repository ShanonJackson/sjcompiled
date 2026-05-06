//! Babel↔SWC parser-shape shim: `ParenthesizedExpression` handling.
//!
//! Babel's parser strips parens by default — `@babel/parser` only
//! emits `ParenthesizedExpression` nodes when `createParenthesizedExpressions`
//! is set, and `@compiled/babel-plugin` (per its `babel.config` /
//! consumer toolchains) does NOT set it. Result: upstream's
//! `t.isObjectExpression(node)`, `t.isCallExpression(node)`,
//! `t.isArrowFunctionExpression(node)` etc. NEVER see a paren wrapper.
//!
//! SWC's parser keeps `Expr::Paren` unconditionally. So `() => ({x:1})`
//! parses with body `BlockStmtOrExpr::Expr(Paren(Object))` (SWC) vs
//! `ArrowFunctionExpression { body: ObjectExpression }` (Babel). Every
//! pattern-match site in the port that mirrors a `t.isX(node)` check
//! must unwrap `Expr::Paren` first to remain 1:1 with upstream.
//!
//! `Expr::TsAs` is handled the same way — Babel's `t.isTSAsExpression`
//! is checked explicitly at one site (`evaluate-expression.ts:132`),
//! and we treat it identically here for callers that want both
//! transparent unwraps.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::compat::paren::{unwrap_paren, unwrap_paren_and_ts_as};
//!
//! // For predicates mirroring `t.isCallExpression(node)`:
//! let Expr::Call(call) = unwrap_paren(expr) else { return false; };
//!
//! // For evaluate-expression's `targetExpression` normalisation
//! // (mirrors the JS `t.isTSAsExpression(expression) ? expression.expression : expression`
//! // PLUS the implicit Babel-strips-parens contract):
//! let target = unwrap_paren_and_ts_as(expression);
//! ```
//!
//! See `plugins/STATUS.md` "Standing bug-parity flags & known
//! divergences" for the wider §6.8b shim register.

use swc_core::ecma::ast::Expr;

/// Strip nested `Expr::Paren` wrappers, returning the inner expression.
/// Mirrors Babel's parser default of NOT producing
/// `ParenthesizedExpression` nodes — so any pattern-match in the port
/// that mirrors a `t.isX(node)` check should call this first.
///
/// Iterative loop handles `((x))` → `x` (rare; SWC may collapse some
/// of these during parse, but the loop is cheap and robust).
pub fn unwrap_paren(expr: &Expr) -> &Expr {
    let mut current = expr;
    while let Expr::Paren(p) = current {
        current = &*p.expr;
    }
    current
}

/// Combine `unwrap_paren` with the `Expr::TsAs` unwrap that
/// `evaluate-expression.ts:132` applies to its input. Used at the
/// `evaluateExpression(...)` entry point so downstream dispatch sees
/// the same expression Babel would have seen post-parse.
pub fn unwrap_paren_and_ts_as(expr: &Expr) -> &Expr {
    let mut current = expr;
    loop {
        match current {
            Expr::Paren(p) => current = &*p.expr,
            // Babel: `t.isTSAsExpression` covers `x as T` AND
            // `x as const` (the latter parses as TSAsExpression with
            // a TSTypeReference("const") annotation in @babel/parser).
            // SWC splits these into two AST variants — `Expr::TsAs`
            // and `Expr::TsConstAssertion`. Both are observationally
            // identical to Babel's TSAsExpression for evaluator
            // purposes (CSS-value extraction is type-agnostic), so
            // both unwrap to their inner expression.
            //
            // `TsTypeAssertion` (`<T>x`) and `TsSatisfies` (`x satisfies T`)
            // are NOT covered by upstream's `t.isTSAsExpression`, so
            // we leave them alone — a fixture that exercises one of
            // these will deopt the same way Babel's evaluator deopts
            // when it doesn't have an explicit unwrap, matching the
            // upstream behaviour bit-for-bit.
            Expr::TsAs(ts) => current = &*ts.expr,
            Expr::TsConstAssertion(ts) => current = &*ts.expr,
            _ => break current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{
        Lit, Number, ObjectLit, ParenExpr, Str, TsAsExpr, TsKeywordType, TsKeywordTypeKind, TsType,
    };

    fn num_lit(value: f64) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value,
            raw: None,
        })))
    }

    fn str_lit(value: &str) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        })))
    }

    fn paren(inner: Box<Expr>) -> Box<Expr> {
        Box::new(Expr::Paren(ParenExpr {
            span: DUMMY_SP,
            expr: inner,
        }))
    }

    fn ts_as_any(inner: Box<Expr>) -> Box<Expr> {
        Box::new(Expr::TsAs(TsAsExpr {
            span: DUMMY_SP,
            expr: inner,
            type_ann: Box::new(TsType::TsKeywordType(TsKeywordType {
                span: DUMMY_SP,
                kind: TsKeywordTypeKind::TsAnyKeyword,
            })),
        }))
    }

    #[test]
    fn unwrap_paren_strips_single_layer() {
        let inner = num_lit(42.0);
        let wrapped = paren(inner.clone());
        let result = unwrap_paren(&*wrapped);
        match result {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 42.0),
            _ => panic!("expected num lit"),
        }
    }

    #[test]
    fn unwrap_paren_strips_nested_layers() {
        let inner = str_lit("hello");
        let wrapped = paren(paren(paren(inner)));
        let result = unwrap_paren(&*wrapped);
        match result {
            Expr::Lit(Lit::Str(s)) => assert_eq!(&*s.value, "hello"),
            _ => panic!("expected string lit"),
        }
    }

    #[test]
    fn unwrap_paren_passes_non_paren_unchanged() {
        let object = Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        }));
        let result = unwrap_paren(&*object);
        assert!(matches!(result, Expr::Object(_)));
    }

    #[test]
    fn unwrap_paren_and_ts_as_strips_paren_only() {
        let inner = num_lit(7.0);
        let wrapped = paren(inner);
        let result = unwrap_paren_and_ts_as(&*wrapped);
        assert!(matches!(result, Expr::Lit(Lit::Num(_))));
    }

    #[test]
    fn unwrap_paren_and_ts_as_strips_ts_as_only() {
        let inner = num_lit(7.0);
        let wrapped = ts_as_any(inner);
        let result = unwrap_paren_and_ts_as(&*wrapped);
        assert!(matches!(result, Expr::Lit(Lit::Num(_))));
    }

    #[test]
    fn unwrap_paren_and_ts_as_strips_mixed_stack() {
        // (x as any) → ParenExpr(TsAs(...)). And ((x as any)) →
        // ParenExpr(ParenExpr(TsAs(...))). Verify the loop handles
        // both orderings.
        let inner = num_lit(7.0);
        let wrapped = paren(ts_as_any(paren(inner)));
        let result = unwrap_paren_and_ts_as(&*wrapped);
        assert!(matches!(result, Expr::Lit(Lit::Num(_))));
    }
}
