//! Port of `postcss-values-parser/lib/ValuesStringifier.js`.

use crate::nodes::{Node, NodeKind, Root};

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
