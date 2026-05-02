//! 1:1 port of `packages/babel-plugin-strip-runtime/src/utils/is-create-element.ts`.
//!
//! ```ts
//! export const isCreateElement = (node: t.Node): node is t.CallExpression => {
//!   return (
//!     t.isMemberExpression(node) &&
//!     t.isIdentifier(node.object) &&
//!     node.object.name === 'React' &&
//!     t.isIdentifier(node.property) &&
//!     node.property.name === 'createElement'
//!   );
//! };
//! ```
//!
//! Upstream's TypeScript predicate claims `node is t.CallExpression`
//! but the body checks for a `MemberExpression`. The function is
//! invoked on `CallExpression.callee` (which is itself a
//! MemberExpression), so the runtime behaviour is correct — only the
//! type predicate is misleading. We port the runtime behaviour
//! verbatim per the "bugs are features" rule (PLAN.md §1.6 / Cardinal
//! rules conformance).

use swc_core::ecma::ast::{Expr, MemberProp};

/// Returns `true` iff `node` is the `MemberExpression` `React.createElement`.
/// Despite the JS function name, this does NOT check that the
/// containing node is a CallExpression — callers (the visitor in
/// `index.ts`) pass `CallExpression.callee` directly.
pub fn is_create_element(node: &Expr) -> bool {
    let Expr::Member(member) = node else {
        return false;
    };
    let Expr::Ident(obj) = member.obj.as_ref() else {
        return false;
    };
    if obj.sym != *"React" {
        return false;
    }
    let MemberProp::Ident(prop) = &member.prop else {
        return false;
    };
    prop.sym == *"createElement"
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{Ident, IdentName, MemberExpr};

    fn ident_expr(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            SyntaxContext::empty(),
        )))
    }

    fn react_create_element() -> Expr {
        Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr("React"),
            prop: MemberProp::Ident(IdentName::new("createElement".into(), DUMMY_SP)),
        })
    }

    #[test]
    fn matches_react_create_element() {
        assert!(is_create_element(&react_create_element()));
    }

    #[test]
    fn rejects_lowercase_react() {
        let m = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr("react"),
            prop: MemberProp::Ident(IdentName::new("createElement".into(), DUMMY_SP)),
        });
        assert!(!is_create_element(&m));
    }

    #[test]
    fn rejects_non_create_element_property() {
        let m = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr("React"),
            prop: MemberProp::Ident(IdentName::new("Fragment".into(), DUMMY_SP)),
        });
        assert!(!is_create_element(&m));
    }

    #[test]
    fn rejects_chained_member_object() {
        // `React.something.createElement` — obj is itself a Member,
        // not an Ident. Upstream rejects this.
        let inner = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr("React"),
            prop: MemberProp::Ident(IdentName::new("something".into(), DUMMY_SP)),
        });
        let outer = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(inner),
            prop: MemberProp::Ident(IdentName::new("createElement".into(), DUMMY_SP)),
        });
        assert!(!is_create_element(&outer));
    }

    #[test]
    fn rejects_computed_property() {
        let m = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr("React"),
            prop: MemberProp::Computed(swc_core::ecma::ast::ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(swc_core::ecma::ast::Lit::Str(
                    swc_core::ecma::ast::Str {
                        span: DUMMY_SP,
                        value: "createElement".into(),
                        raw: None,
                    },
                ))),
            }),
        });
        assert!(!is_create_element(&m));
    }

    #[test]
    fn rejects_bare_identifier() {
        assert!(!is_create_element(&ident_expr("React")));
    }
}
