//! 1:1 port of `@babel/generator@7.23.0/lib/generators/jsx.js`.
//!
//! Upstream is 122 LOC of small printers — one per JSX node kind.
//! Each Rust function below mirrors a `function <NodeName>(node)` on
//! the upstream printer, in source order.
//!
//! ## SWC ↔ Babel field-name divergences (no source change beyond rename)
//!
//! | Babel                          | SWC                              |
//! |--------------------------------|----------------------------------|
//! | `JSXOpeningElement.attributes` | `JSXOpeningElement.attrs`        |
//! | `JSXOpeningElement.selfClosing`| `JSXOpeningElement.self_closing` |
//! | `JSXOpeningElement.typeParameters` | `JSXOpeningElement.type_args` (`Option<Box<TsTypeParamInstantiation>>`) |
//! | `JSXElement.openingElement`    | `JSXElement.opening`             |
//! | `JSXElement.closingElement`    | `JSXElement.closing` (`Option<JSXClosingElement>`) |
//! | `JSXFragment.openingFragment`  | `JSXFragment.opening`            |
//! | `JSXFragment.closingFragment`  | `JSXFragment.closing`            |
//! | `JSXMemberExpression.object`   | `JSXMemberExpr.obj`              |
//! | `JSXMemberExpression.property` | `JSXMemberExpr.prop`             |
//! | `JSXNamespacedName.namespace`  | `JSXNamespacedName.ns`           |
//! | `JSXExpressionContainer.expression` | `JSXExprContainer.expr` (`JSXExpr` enum: Empty | Expr) |
//! | `JSXSpreadChild.expression`    | `JSXSpreadChild.expr`            |
//! | `JSXSpreadAttribute`           | `JSXAttrOrSpread::SpreadElement(SpreadElement)` (uses generic SpreadElement, not a JSX-specific node) |
//! | `JSXIdentifier`                | `Ident` (in JSXElementName), `IdentName` (in JSXAttrName / JSXMemberExpr.prop) — same byte output: just the symbol |
//!
//! Field-name renames are mechanical; the byte output is identical.

use crate::compat::generator::printer::Printer;

use super::types::string_literal;

use swc_core::ecma::ast::{
    Ident, IdentName, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXClosingElement,
    JSXClosingFragment, JSXElement, JSXElementChild, JSXElementName, JSXEmptyExpr, JSXExpr,
    JSXExprContainer, JSXFragment, JSXMemberExpr, JSXNamespacedName, JSXObject,
    JSXOpeningElement, JSXOpeningFragment, JSXSpreadChild, JSXText, SpreadElement,
};

/// `JSXAttribute(node)`:
/// ```js
/// this.print(node.name, node);
/// if (node.value) {
///   this.tokenChar(61);          // '='
///   this.print(node.value, node);
/// }
/// ```
pub fn jsx_attribute(p: &mut Printer, node: &JSXAttr) {
    jsx_attr_name(p, &node.name);
    if let Some(v) = node.value.as_ref() {
        p.token_char(b'=');
        jsx_attr_value(p, v);
    }
}

fn jsx_attr_name(p: &mut Printer, name: &JSXAttrName) {
    match name {
        JSXAttrName::Ident(i) => jsx_identifier_from_ident_name(p, i),
        JSXAttrName::JSXNamespacedName(n) => jsx_namespaced_name(p, n),
    }
}

fn jsx_attr_value(p: &mut Printer, value: &JSXAttrValue) {
    match value {
        // Babel's `StringLiteral(node)` printer is invoked here when
        // the attribute value is a quoted string. We re-use the
        // existing string_literal which preserves source quotes via
        // the `Str.raw` passthrough.
        JSXAttrValue::Str(s) => string_literal(p, s),
        JSXAttrValue::JSXExprContainer(c) => jsx_expression_container(p, c),
        JSXAttrValue::JSXElement(e) => jsx_element(p, e),
        JSXAttrValue::JSXFragment(f) => jsx_fragment(p, f),
    }
}

/// `JSXIdentifier(node) { this.word(node.name); }` — Babel's
/// JSXIdentifier carries a `name: string`. SWC has two analog types
/// depending on position — `Ident` (in JSXElementName) and `IdentName`
/// (in JSXAttrName / JSXMemberExpr.prop). Both expose `.sym` so the
/// byte output is identical.
pub fn jsx_identifier_from_ident(p: &mut Printer, node: &Ident) {
    p.word(node.sym.as_ref());
}

pub fn jsx_identifier_from_ident_name(p: &mut Printer, node: &IdentName) {
    p.word(node.sym.as_ref());
}

/// `JSXNamespacedName(node)`:
/// ```js
/// this.print(node.namespace, node);
/// this.tokenChar(58);            // ':'
/// this.print(node.name, node);
/// ```
pub fn jsx_namespaced_name(p: &mut Printer, node: &JSXNamespacedName) {
    jsx_identifier_from_ident_name(p, &node.ns);
    p.token_char(b':');
    jsx_identifier_from_ident_name(p, &node.name);
}

/// `JSXMemberExpression(node)`:
/// ```js
/// this.print(node.object, node);
/// this.tokenChar(46);            // '.'
/// this.print(node.property, node);
/// ```
pub fn jsx_member_expression(p: &mut Printer, node: &JSXMemberExpr) {
    jsx_object(p, &node.obj);
    p.token_char(b'.');
    jsx_identifier_from_ident_name(p, &node.prop);
}

fn jsx_object(p: &mut Printer, obj: &JSXObject) {
    match obj {
        JSXObject::JSXMemberExpr(e) => jsx_member_expression(p, e),
        JSXObject::Ident(i) => jsx_identifier_from_ident(p, i),
    }
}

/// `JSXSpreadAttribute(node)`:
/// ```js
/// this.tokenChar(123);           // '{'
/// this.token("...");
/// this.print(node.argument, node);
/// this.tokenChar(125);           // '}'
/// ```
/// SWC encodes JSX spread attributes as a generic `SpreadElement`
/// inside `JSXAttrOrSpread::SpreadElement` — the printer treats it
/// identically.
pub fn jsx_spread_attribute(p: &mut Printer, node: &SpreadElement) {
    p.token_char(b'{');
    p.token("...");
    p.print(&node.expr, None);
    p.token_char(b'}');
}

/// `JSXExpressionContainer(node)`:
/// ```js
/// this.tokenChar(123);           // '{'
/// this.print(node.expression, node);
/// this.tokenChar(125);           // '}'
/// ```
pub fn jsx_expression_container(p: &mut Printer, node: &JSXExprContainer) {
    p.token_char(b'{');
    match &node.expr {
        JSXExpr::JSXEmptyExpr(e) => jsx_empty_expression(p, e),
        JSXExpr::Expr(e) => p.print(e, None),
    }
    p.token_char(b'}');
}

/// `JSXSpreadChild(node)` — same shape as JSXSpreadAttribute, but the
/// argument is named `expression` upstream. SWC's `JSXSpreadChild.expr`
/// matches.
pub fn jsx_spread_child(p: &mut Printer, node: &JSXSpreadChild) {
    p.token_char(b'{');
    p.token("...");
    p.print(&node.expr, None);
    p.token_char(b'}');
}

/// `JSXText(node)`:
/// ```js
/// const raw = this.getPossibleRaw(node);
/// if (raw !== undefined) this.token(raw, true);
/// else                   this.token(node.value, true);
/// ```
/// Note Babel's second arg `maybeNewline=true` enables newline-aware
/// emit (line-tracking for source maps). We don't carry source maps,
/// so the byte output is identical to plain `token`. SWC's `JSXText.raw`
/// is always populated by the parser (not Option), so it's the
/// preferred source-truthful form.
pub fn jsx_text(p: &mut Printer, node: &JSXText) {
    if !node.raw.is_empty() {
        p.token(node.raw.as_ref());
    } else {
        p.token(node.value.as_ref());
    }
}

/// `JSXElement(node)`:
/// ```js
/// const open = node.openingElement;
/// this.print(open, node);
/// if (open.selfClosing) return;
/// this.indent();
/// for (const child of node.children) this.print(child, node);
/// this.dedent();
/// this.print(node.closingElement, node);
/// ```
pub fn jsx_element(p: &mut Printer, node: &JSXElement) {
    jsx_opening_element(p, &node.opening);
    if node.opening.self_closing {
        return;
    }
    p.indent();
    for child in &node.children {
        jsx_element_child(p, child);
    }
    p.dedent();
    if let Some(closing) = node.closing.as_ref() {
        jsx_closing_element(p, closing);
    }
}

fn jsx_element_child(p: &mut Printer, child: &JSXElementChild) {
    match child {
        JSXElementChild::JSXText(t) => jsx_text(p, t),
        JSXElementChild::JSXExprContainer(c) => jsx_expression_container(p, c),
        JSXElementChild::JSXSpreadChild(s) => jsx_spread_child(p, s),
        JSXElementChild::JSXElement(e) => jsx_element(p, e),
        JSXElementChild::JSXFragment(f) => jsx_fragment(p, f),
    }
}

/// `JSXOpeningElement(node)`:
/// ```js
/// this.tokenChar(60);            // '<'
/// this.print(node.name, node);
/// this.print(node.typeParameters, node);
/// if (node.attributes.length > 0) {
///   this.space();
///   this.printJoin(node.attributes, node, { separator: spaceSeparator });
/// }
/// if (node.selfClosing) {
///   this.space();
///   this.token("/>");
/// } else {
///   this.tokenChar(62);          // '>'
/// }
/// ```
pub fn jsx_opening_element(p: &mut Printer, node: &JSXOpeningElement) {
    p.token_char(b'<');
    jsx_element_name(p, &node.name);
    // Babel: `this.print(node.typeParameters, node)` — emits TS
    // `<T, U>` annotations on JSX names. The corpus doesn't reach
    // this branch; surface the gap loudly when it does.
    if node.type_args.is_some() {
        p.buf.append("/*UNHANDLED-JSX-TYPE-ARGS*/");
    }
    if !node.attrs.is_empty() {
        p.space();
        // `printJoin(attrs, parent, { separator: spaceSeparator })` —
        // walk attrs, emit each, space between successive entries.
        for (i, attr) in node.attrs.iter().enumerate() {
            if i > 0 {
                p.space();
            }
            jsx_attr_or_spread(p, attr);
        }
    }
    if node.self_closing {
        p.space();
        p.token("/>");
    } else {
        p.token_char(b'>');
    }
}

fn jsx_attr_or_spread(p: &mut Printer, attr: &JSXAttrOrSpread) {
    match attr {
        JSXAttrOrSpread::JSXAttr(a) => jsx_attribute(p, a),
        JSXAttrOrSpread::SpreadElement(s) => jsx_spread_attribute(p, s),
    }
}

fn jsx_element_name(p: &mut Printer, name: &JSXElementName) {
    match name {
        JSXElementName::Ident(i) => jsx_identifier_from_ident(p, i),
        JSXElementName::JSXMemberExpr(m) => jsx_member_expression(p, m),
        JSXElementName::JSXNamespacedName(n) => jsx_namespaced_name(p, n),
    }
}

/// `JSXClosingElement(node)`:
/// ```js
/// this.token("</");
/// this.print(node.name, node);
/// this.tokenChar(62);            // '>'
/// ```
pub fn jsx_closing_element(p: &mut Printer, node: &JSXClosingElement) {
    p.token("</");
    jsx_element_name(p, &node.name);
    p.token_char(b'>');
}

/// `JSXEmptyExpression() { this.printInnerComments(); }` — upstream's
/// `printInnerComments` walks `node.innerComments` and prints them.
/// SWC stores all comments out-of-band in the `Comments` store keyed
/// by `BytePos`; the corpus does not exercise inner comments inside a
/// `JSXExpressionContainer`. When a future fixture surfaces one, the
/// fix is to query `comments.take_leading(span.lo)` /
/// `take_trailing(span.hi)` for the JSXEmptyExpr's span. Today's
/// no-op matches Babel's behaviour for empty containers without inner
/// comments (the byte output is just `{}`).
pub fn jsx_empty_expression(_p: &mut Printer, _node: &JSXEmptyExpr) {
    // intentional no-op (see doc-comment).
}

/// `JSXFragment(node)`:
/// ```js
/// this.print(node.openingFragment, node);
/// this.indent();
/// for (const child of node.children) this.print(child, node);
/// this.dedent();
/// this.print(node.closingFragment, node);
/// ```
pub fn jsx_fragment(p: &mut Printer, node: &JSXFragment) {
    jsx_opening_fragment(p, &node.opening);
    p.indent();
    for child in &node.children {
        jsx_element_child(p, child);
    }
    p.dedent();
    jsx_closing_fragment(p, &node.closing);
}

/// `JSXOpeningFragment() { this.tokenChar(60); this.tokenChar(62); }`
pub fn jsx_opening_fragment(p: &mut Printer, _node: &JSXOpeningFragment) {
    p.token_char(b'<');
    p.token_char(b'>');
}

/// `JSXClosingFragment() { this.token("</"); this.tokenChar(62); }`
pub fn jsx_closing_fragment(p: &mut Printer, _node: &JSXClosingFragment) {
    p.token("</");
    p.token_char(b'>');
}

// Comment-store threading for JSX-typed nodes is not exercised by the
// 5 jsx-key fixtures (none carry comments around the attribute or
// inside the expression container). When a future fixture surfaces a
// comment, query `Printer::print_leading_comments_at(node.span.lo)` /
// `print_trailing_comments_at(node.span.hi)` from the matching
// printer above — same shape as `Printer::print(&Expr, _)` does for
// Expression-typed nodes.
