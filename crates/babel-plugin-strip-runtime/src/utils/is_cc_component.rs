//! 1:1 port of `packages/babel-plugin-strip-runtime/src/utils/is-cc-component.ts`.
//!
//! ```ts
//! export const isCCComponent = (node: t.Node): boolean => {
//!   if (t.isIdentifier(node) && node.name === 'CC') {
//!     return true;
//!   }
//!   if (t.isMemberExpression(node) && t.isIdentifier(node.property) && node.property.name === 'CC') {
//!     return true;
//!   }
//!   return false;
//! };
//! ```

use swc_core::ecma::ast::{Expr, MemberProp};

/// Returns `true` if `node` is a `CC` identifier — either bare
/// (`CC`) or as a member access (`X.CC` / `X[CC]` where the property
/// is a non-private identifier).
pub fn is_cc_component(node: &Expr) -> bool {
    if let Expr::Ident(id) = node {
        if id.sym == *"CC" {
            return true;
        }
    }

    if let Expr::Member(member) = node {
        if let MemberProp::Ident(id) = &member.prop {
            if id.sym == *"CC" {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{Ident, IdentName, MemberExpr};

    fn ident(name: &str) -> Expr {
        Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            SyntaxContext::empty(),
        ))
    }

    fn member(obj_name: &str, prop_name: &str) -> Expr {
        Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(ident(obj_name)),
            prop: MemberProp::Ident(IdentName::new(prop_name.into(), DUMMY_SP)),
        })
    }

    #[test]
    fn matches_bare_cc_identifier() {
        assert!(is_cc_component(&ident("CC")));
    }

    #[test]
    fn rejects_other_identifiers() {
        assert!(!is_cc_component(&ident("CS")));
        assert!(!is_cc_component(&ident("cc")));
        assert!(!is_cc_component(&ident("Cc")));
        assert!(!is_cc_component(&ident("CCX")));
    }

    #[test]
    fn matches_member_with_cc_property() {
        // e.g. `_compiledRuntime.CC` from CommonJS interop.
        assert!(is_cc_component(&member("_compiledRuntime", "CC")));
    }

    #[test]
    fn rejects_member_with_non_cc_property() {
        assert!(!is_cc_component(&member("_compiledRuntime", "CS")));
        assert!(!is_cc_component(&member("CC", "Foo")));
    }

    #[test]
    fn rejects_non_ident_non_member_expressions() {
        let lit = Expr::Lit(swc_core::ecma::ast::Lit::Str(
            swc_core::ecma::ast::Str {
                span: DUMMY_SP,
                value: "CC".into(),
                raw: None,
            },
        ));
        assert!(!is_cc_component(&lit));
    }

    #[test]
    fn rejects_member_with_computed_property() {
        // `obj["CC"]` — MemberProp::Computed, not MemberProp::Ident.
        let computed = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(ident("obj")),
            prop: MemberProp::Computed(swc_core::ecma::ast::ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(swc_core::ecma::ast::Lit::Str(
                    swc_core::ecma::ast::Str {
                        span: DUMMY_SP,
                        value: "CC".into(),
                        raw: None,
                    },
                ))),
            }),
        });
        assert!(!is_cc_component(&computed));
    }
}
