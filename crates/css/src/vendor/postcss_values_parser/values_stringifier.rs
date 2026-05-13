//! Port of `postcss-values-parser/lib/ValuesStringifier.js`.
//!
//! Two distinct surface entry-points mirror upstream:
//!
//! 1. [`ValuesStringifier::stringify`] — full Root walk used for
//!    parse → stringify round-trips. Emits each child with its
//!    `raws_before` and `raws_after` (this is the `body` context).
//! 2. [`stringify_standalone`] — port of upstream `node.toString()`
//!    for a single Node. Mirrors `ValuesStringifier::basic(node)`,
//!    which emits `value + raws_after` only (no `raws_before`).
//!    The `func` override does manually emit each child's `raws_before`
//!    inside the parens — `stringify_standalone` replicates that.
//!    Used by `expand-shorthands/*` to render extracted nodes.

use super::nodes::{Node, NodeKind, Root};

pub struct ValuesStringifier;

impl ValuesStringifier {
    pub fn stringify(root: &Root) -> String {
        if root.nodes.is_empty() {
            return root.raw_value.clone().unwrap_or_default();
        }
        let mut out = String::new();
        for n in &root.nodes { stringify_node(n, &mut out); }
        if let Some(rv) = &root.raw_value {
            // Trailing-whitespace-only roots store on raw_value.
            if root.nodes.is_empty() { out.push_str(rv); }
        }
        out
    }
}

/// `node.toString()` upstream — emits `value + raws_after` only,
/// SKIPPING the outer `raws_before`. Mirrors `ValuesStringifier::basic`'s
/// comment "before is handled by postcss in stringifier.body".
///
/// Funcs emit each child's `raws_before` manually inside the parens
/// (mirrors the upstream `func` override).
pub fn stringify_standalone(node: &Node) -> String {
    let mut out = String::new();
    write_standalone(node, &mut out);
    out
}

fn write_standalone(node: &Node, out: &mut String) {
    match &node.kind {
        NodeKind::Root => { /* unreachable for a single-node toString */ }
        NodeKind::AtWord(a) => {
            out.push('@');
            out.push_str(&a.name);
        }
        NodeKind::Comment(c) => {
            if c.inline {
                out.push_str("//");
                out.push_str(&c.text);
            } else {
                out.push_str("/*");
                out.push_str(&c.text);
                out.push_str("*/");
            }
        }
        NodeKind::Func(f) => {
            out.push_str(&f.name);
            out.push('(');
            for child in &f.nodes {
                out.push_str(&child.raws_before);
                write_standalone(child, out);
            }
            if !f.unclosed { out.push(')'); }
            out.push_str(&f.raws_after);
        }
        NodeKind::Interpolation(i) => {
            out.push_str(&i.prefix);
            out.push_str(&i.params);
        }
        NodeKind::Numeric(n) => {
            out.push_str(&n.common.value);
            out.push_str(&n.unit);
        }
        NodeKind::Operator(o) => out.push_str(&o.common.value),
        NodeKind::Punctuation(p) => out.push_str(&p.common.value),
        NodeKind::Quoted(q) => out.push_str(&q.common.value),
        NodeKind::UnicodeRange(u) => out.push_str(&u.common.value),
        NodeKind::Word(w) => out.push_str(&w.common.value),
    }
    out.push_str(&node.raws_after);
}

fn stringify_node(node: &Node, out: &mut String) {
    out.push_str(&node.raws_before);
    match &node.kind {
        NodeKind::Root => { /* unreachable */ }
        NodeKind::AtWord(a) => {
            out.push('@');
            out.push_str(&a.name);
        }
        NodeKind::Comment(c) => {
            if c.inline {
                out.push_str("//");
                out.push_str(&c.text);
            } else {
                out.push_str("/*");
                out.push_str(&c.text);
                out.push_str("*/");
            }
        }
        NodeKind::Func(f) => {
            out.push_str(&f.name);
            out.push('(');
            for child in &f.nodes { stringify_node(child, out); }
            if !f.unclosed { out.push(')'); }
            out.push_str(&f.raws_after);
        }
        NodeKind::Interpolation(i) => {
            out.push_str(&i.prefix);
            out.push_str(&i.params);
        }
        NodeKind::Numeric(n) => {
            out.push_str(&n.common.value);
            out.push_str(&n.unit);
        }
        NodeKind::Operator(o) => out.push_str(&o.common.value),
        NodeKind::Punctuation(p) => out.push_str(&p.common.value),
        NodeKind::Quoted(q) => out.push_str(&q.common.value),
        NodeKind::UnicodeRange(u) => out.push_str(&u.common.value),
        NodeKind::Word(w) => out.push_str(&w.common.value),
    }
    out.push_str(&node.raws_after);
}
