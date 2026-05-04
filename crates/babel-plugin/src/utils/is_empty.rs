//! 1:1 port of `packages/babel-plugin/src/utils/is-empty.ts`.
//!
//! Three-way "is this expression a no-op CSS value?" check:
//! `undefined` identifier, `null` literal, or empty string literal.

use swc_core::ecma::ast::Expr;

/// Returns true when the expression is one of the three "treat as
/// missing" shapes Compiled recognises: `undefined`, `null`, or `''`.
///
/// Mirrors upstream:
/// ```ts
/// t.isIdentifier(expression, { name: 'undefined' }) ||
/// t.isNullLiteral(expression) ||
/// t.isStringLiteral(expression, { value: '' })
/// ```
pub fn is_empty_value(expression: &Expr) -> bool {
    match expression {
        Expr::Ident(ident) => &*ident.sym == "undefined",
        Expr::Lit(lit) => matches!(
            lit,
            swc_core::ecma::ast::Lit::Null(_)
                | swc_core::ecma::ast::Lit::Str(s) if s.value.is_empty()
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{Ident, IdentName, Lit, Null, Str};

    #[test]
    fn undefined_identifier_is_empty() {
        let e = Expr::Ident(Ident::new("undefined".into(), DUMMY_SP, Default::default()));
        assert!(is_empty_value(&e));
    }

    #[test]
    fn null_literal_is_empty() {
        let e = Expr::Lit(Lit::Null(Null { span: DUMMY_SP }));
        assert!(is_empty_value(&e));
    }

    #[test]
    fn empty_string_literal_is_empty() {
        let e = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: "".into(),
            raw: None,
        }));
        assert!(is_empty_value(&e));
    }

    #[test]
    fn non_empty_string_is_not_empty() {
        let e = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: "hello".into(),
            raw: None,
        }));
        assert!(!is_empty_value(&e));
    }

    #[test]
    fn other_identifier_is_not_empty() {
        let e = Expr::Ident(Ident::new("foo".into(), DUMMY_SP, Default::default()));
        assert!(!is_empty_value(&e));
    }

    #[test]
    fn ident_name_is_unused() {
        // IdentName is a separate SWC type used for member-prop / attr
        // positions. is_empty operates on Expr — IdentName never reaches it.
        let _ = IdentName::new("undefined".into(), DUMMY_SP);
    }
}
