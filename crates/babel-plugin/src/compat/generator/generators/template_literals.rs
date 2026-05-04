//! 1:1 port of `@babel/generator@7.23.0/lib/generators/template-literals.js`.

use crate::compat::generator::printer::Printer;

use swc_core::ecma::ast::{Expr, TaggedTpl, Tpl};

/// `TaggedTemplateExpression(node)`.
pub fn tagged_tpl(p: &mut Printer, node: &TaggedTpl, parent_expr: &Expr) {
    p.print(&node.tag, Some(parent_expr));
    // Type parameters (TS) — skipped; not in the corpus's JS subset.
    tpl(p, &node.tpl, parent_expr);
}

/// `TemplateLiteral(node)`.
pub fn tpl(p: &mut Printer, node: &Tpl, parent_expr: &Expr) {
    p.token_char(b'`');
    let quasis = &node.quasis;
    let exprs = &node.exprs;
    // Babel iterates `quasi` and `expression` interleaved: quasi 0,
    // expr 0, quasi 1, expr 1, ..., quasi N. SWC stores quasis with
    // `tail: bool` flag where the last quasi has `tail = true`.
    for (i, quasi) in quasis.iter().enumerate() {
        // `TemplateElement` value: prefer `raw` (source-anchored) over
        // `cooked` (escape-resolved) — matches Babel's behaviour
        // (`packages/babel-types/src/utilities/cleanJSXElementLiteralChild.js`-
        //  style raw passthrough on TemplateElement).
        let raw = quasi.raw.as_ref();
        p.buf.append(raw);
        // After a quasi, ends-with state must be reset for the
        // following `${` token.
        p.ends_with_word = false;
        p.ends_with_integer = false;

        if !quasi.tail {
            p.token("${");
            if let Some(expr) = exprs.get(i) {
                p.print(expr, Some(parent_expr));
            }
            p.token_char(b'}');
        }
    }
    p.token_char(b'`');
}
