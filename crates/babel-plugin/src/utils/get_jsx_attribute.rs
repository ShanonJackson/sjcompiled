//! 1:1 port of `packages/babel-plugin/src/utils/get-jsx-attribute.ts`.
//!
//! Returns `(attribute, index)` for the first JSXAttribute on a
//! JSXElement whose `name.name == name`. Mirrors upstream's
//! `[t.JSXAttribute | undefined, number]` tuple shape exactly:
//! `(None, -1)` for "not found" or "not a JSXElement".
//!
//! Babel→SWC field-name divergences:
//! * `JSXOpeningElement.attributes` → `JSXOpeningElement.attrs`.
//! * `JSXAttribute` → `JSXAttr` (the spread variant is `SpreadElement`
//!   inside `JSXAttrOrSpread`).
//! * `attribute.name.name` (Babel: `JSXIdentifier.name`) →
//!   SWC `JSXAttrName::Ident(IdentName).sym` for the bare identifier;
//!   `JSXAttrName::JSXNamespacedName.name.sym` for the namespaced form.
//!   Both expose `.sym`.

use swc_core::ecma::ast::{Expr, JSXAttr, JSXAttrName, JSXAttrOrSpread};

/// Find the first `JSXAttr` on `expr`'s opening element with
/// `name.name == name`. Returns `(None, -1)` when `expr` isn't a
/// `JSXElement` or no matching attribute exists.
///
/// `expr` is `&Expr` because the upstream call sites pass `node`
/// (which is `t.Expression` in their types) — a JSXElement is one
/// `Expr` variant. Mirrors the JS `t.isJSXElement(node)` early-bail.
pub fn get_jsx_attribute<'a>(expr: &'a Expr, name: &str) -> (Option<&'a JSXAttr>, isize) {
    let Expr::JSXElement(elem) = expr else {
        return (None, -1);
    };

    for (idx, attr) in elem.opening.attrs.iter().enumerate() {
        if let JSXAttrOrSpread::JSXAttr(jsx_attr) = attr {
            if jsx_attr_matches_name(&jsx_attr.name, name) {
                return (Some(jsx_attr), idx as isize);
            }
        }
    }

    (None, -1)
}

fn jsx_attr_matches_name(attr_name: &JSXAttrName, name: &str) -> bool {
    match attr_name {
        // Bare identifier: `<div className="x">` → "className".
        JSXAttrName::Ident(id) => id.sym.as_str() == name,
        // Namespaced (`xmlns:foo="..."`): JS compares against the
        // *full* `name.name` which Babel renders as `<ns>:<name>`.
        // Upstream call sites only ever ask for bare attribute names
        // like `key`, `className`, `style` — namespaced never matches.
        // We mirror exactly: compare the local `name` segment, since
        // Babel's `JSXIdentifier.name` for a namespaced attr is just
        // the post-colon segment in the Babel AST.
        JSXAttrName::JSXNamespacedName(ns) => ns.name.sym.as_str() == name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{
        Ident, IdentName, JSXClosingElement, JSXElement, JSXElementName, JSXOpeningElement, Lit,
        Str,
    };

    fn make_jsx_element(attrs: Vec<JSXAttrOrSpread>) -> Expr {
        let opening = JSXOpeningElement {
            span: DUMMY_SP,
            name: JSXElementName::Ident(Ident::new("div".into(), DUMMY_SP, Default::default())),
            attrs,
            self_closing: false,
            type_args: None,
        };
        let closing = JSXClosingElement {
            span: DUMMY_SP,
            name: JSXElementName::Ident(Ident::new("div".into(), DUMMY_SP, Default::default())),
        };
        Expr::JSXElement(Box::new(JSXElement {
            span: DUMMY_SP,
            opening,
            children: vec![],
            closing: Some(closing),
        }))
    }

    fn make_string_attr(name: &str, value: &str) -> JSXAttrOrSpread {
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::Ident(IdentName::new(name.into(), DUMMY_SP)),
            value: Some(swc_core::ecma::ast::JSXAttrValue::Str(Str {
                span: DUMMY_SP,
                value: value.into(),
                raw: None,
            })),
        })
    }

    #[test]
    fn returns_none_when_node_is_not_jsx_element() {
        let expr = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: "not jsx".into(),
            raw: None,
        }));
        let (attr, idx) = get_jsx_attribute(&expr, "className");
        assert!(attr.is_none());
        assert_eq!(idx, -1);
    }

    #[test]
    fn returns_none_when_attribute_missing() {
        let expr = make_jsx_element(vec![make_string_attr("id", "x")]);
        let (attr, idx) = get_jsx_attribute(&expr, "className");
        assert!(attr.is_none());
        assert_eq!(idx, -1);
    }

    #[test]
    fn returns_first_match_with_correct_index() {
        let expr = make_jsx_element(vec![
            make_string_attr("id", "x"),
            make_string_attr("className", "btn"),
            make_string_attr("style", "color: red"),
        ]);
        let (attr, idx) = get_jsx_attribute(&expr, "className");
        assert!(attr.is_some());
        assert_eq!(idx, 1);
    }

    #[test]
    fn skips_spread_attributes() {
        // `<div {...rest} className="btn">` — spread at index 0
        // should not match; className is at index 1.
        let spread = JSXAttrOrSpread::SpreadElement(swc_core::ecma::ast::SpreadElement {
            dot3_token: DUMMY_SP,
            expr: Box::new(Expr::Ident(Ident::new(
                "rest".into(),
                DUMMY_SP,
                Default::default(),
            ))),
        });
        let expr = make_jsx_element(vec![spread, make_string_attr("className", "btn")]);
        let (attr, idx) = get_jsx_attribute(&expr, "className");
        assert!(attr.is_some());
        assert_eq!(idx, 1);
    }
}
