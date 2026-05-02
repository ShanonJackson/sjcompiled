//! 1:1 port of `packages/babel-plugin-strip-runtime/src/utils/is-automatic-runtime.ts`.
//!
//! ```ts
//! export const isAutomaticRuntime = (
//!   node: t.Node,
//!   func: 'jsx' | 'jsxs'
//! ): node is t.CallExpression => {
//!   if (t.isCallExpression(node) && t.isIdentifier(node.callee) && node.callee.name === `_${func}`) {
//!     return true;
//!   }
//!   if (
//!     t.isCallExpression(node) &&
//!     t.isSequenceExpression(node.callee) &&
//!     t.isMemberExpression(node.callee.expressions[1]) &&
//!     t.isIdentifier(node.callee.expressions[1].property) &&
//!     node.callee.expressions[1].property.name === func
//!   ) {
//!     return true;
//!   }
//!   return false;
//! };
//! ```

use swc_core::ecma::ast::{Callee, Expr, MemberProp};

/// `'jsx' | 'jsxs'` discriminator from upstream. Two arms only — keep
/// the type-level guarantee so the dispatcher can't pass an unknown
/// runtime func name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsxFunc {
    Jsx,
    Jsxs,
}

impl JsxFunc {
    /// Bare name (`"jsx"` / `"jsxs"`) — what appears as a member name
    /// in a CommonJS-interop call like `(0, _jsxRuntime.jsx)(...)`.
    pub fn name(self) -> &'static str {
        match self {
            JsxFunc::Jsx => "jsx",
            JsxFunc::Jsxs => "jsxs",
        }
    }

    /// Underscore-prefixed name (`"_jsx"` / `"_jsxs"`) — what `preset-react`
    /// emits as the bare-identifier callee in ESM output.
    pub fn underscored(self) -> &'static str {
        match self {
            JsxFunc::Jsx => "_jsx",
            JsxFunc::Jsxs => "_jsxs",
        }
    }
}

/// Returns `true` if `node` looks like a `_jsx(...)` / `_jsxs(...)` call,
/// either as the bare identifier (ESM) or as the second member of a
/// CommonJS interop sequence expression like `(0, _jsxRuntime.jsx)(...)`.
///
/// Note: SWC's parser may keep an `Expr::Paren` around the sequence
/// expression (Babel's parser does not). We unwrap it before
/// inspecting so source like `(0, _jsxRuntime.jsx)(...)` matches the
/// upstream behaviour.
pub fn is_automatic_runtime(node: &Expr, func: JsxFunc) -> bool {
    let Expr::Call(call) = node else {
        return false;
    };
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };

    let callee = unwrap_paren(callee.as_ref());

    // (1) callee is a plain identifier `_jsx` / `_jsxs`.
    if let Expr::Ident(id) = callee {
        if id.sym == *func.underscored() {
            return true;
        }
    }

    // (2) callee is a sequence expression whose [1]th expression is a
    // MemberExpression with `.property.name === func`. Index 0 is the
    // throwaway `0` from CommonJS interop; index 1 is the real callee.
    if let Expr::Seq(seq) = callee {
        if let Some(second) = seq.exprs.get(1) {
            if let Expr::Member(member) = unwrap_paren(second.as_ref()) {
                if let MemberProp::Ident(id) = &member.prop {
                    if id.sym == *func.name() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn unwrap_paren(mut e: &Expr) -> &Expr {
    while let Expr::Paren(p) = e {
        e = p.expr.as_ref();
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        CallExpr, ExprOrSpread, Ident, IdentName, Lit, MemberExpr, Number, SeqExpr,
    };

    fn ident_expr(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            SyntaxContext::empty(),
        )))
    }

    fn member(obj: Box<Expr>, prop: &str) -> Box<Expr> {
        Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj,
            prop: MemberProp::Ident(IdentName::new(prop.into(), DUMMY_SP)),
        }))
    }

    fn call(callee: Box<Expr>) -> Expr {
        Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(callee),
            args: vec![],
            type_args: None,
        })
    }

    /// `(0, _jsxRuntime.<func>)` — the typical CommonJS interop callee.
    fn cjs_interop_callee(member_obj: &str, func: &str) -> Box<Expr> {
        let zero = Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: 0.0,
            raw: None,
        })));
        let m = member(ident_expr(member_obj), func);
        Box::new(Expr::Seq(SeqExpr {
            span: DUMMY_SP,
            exprs: vec![zero, m],
        }))
    }

    #[test]
    fn matches_underscore_jsx_call() {
        let expr = call(ident_expr("_jsx"));
        assert!(is_automatic_runtime(&expr, JsxFunc::Jsx));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsxs));
    }

    #[test]
    fn matches_underscore_jsxs_call() {
        let expr = call(ident_expr("_jsxs"));
        assert!(is_automatic_runtime(&expr, JsxFunc::Jsxs));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_underscore_mismatch() {
        // Bare `jsx` (no underscore) — preset-react wouldn't emit this.
        let expr = call(ident_expr("jsx"));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn matches_cjs_interop_jsx() {
        let expr = call(cjs_interop_callee("_jsxRuntime", "jsx"));
        assert!(is_automatic_runtime(&expr, JsxFunc::Jsx));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsxs));
    }

    #[test]
    fn matches_cjs_interop_jsxs() {
        let expr = call(cjs_interop_callee("_jsxRuntime", "jsxs"));
        assert!(is_automatic_runtime(&expr, JsxFunc::Jsxs));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_non_call_expression() {
        let id = *ident_expr("_jsx");
        assert!(!is_automatic_runtime(&id, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_seq_with_non_member_at_index_1() {
        // `(0, _jsx)(...)` — second expression is an identifier, not a
        // MemberExpression. Per the upstream check, this is REJECTED.
        let zero = Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: 0.0,
            raw: None,
        })));
        let callee = Box::new(Expr::Seq(SeqExpr {
            span: DUMMY_SP,
            exprs: vec![zero, ident_expr("_jsx")],
        }));
        let expr = call(callee);
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_member_expr_callee_without_seq() {
        // `_jsxRuntime.jsx(...)` (no surrounding sequence) — upstream
        // requires the `(0, X)` shape, so this is REJECTED.
        let expr = call(member(ident_expr("_jsxRuntime"), "jsx"));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_seq_with_wrong_member_name() {
        let expr = call(cjs_interop_callee("_jsxRuntime", "createElement"));
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
    }

    #[test]
    fn rejects_callee_super() {
        // `super(...)` — Callee::Super, not Callee::Expr.
        let expr = Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Super(swc_core::ecma::ast::Super { span: DUMMY_SP }),
            args: vec![],
            type_args: None,
        });
        assert!(!is_automatic_runtime(&expr, JsxFunc::Jsx));
        let _ = ExprOrSpread {
            spread: None,
            expr: ident_expr("x"),
        };
    }
}
