//! Stringifier for the selector AST.
//!
//! Each Node knows how to serialize itself. When a Node carries a
//! `raw_value` (set by the parser to original source bytes) and is
//! un-mutated, we emit it verbatim — that's the byte-identity guarantee.
//!
//! When a plugin mutates a Node it must clear `raw_value` (use
//! [`crate::nodes::Node::set_value`]) so the stringifier walks the typed
//! shape instead.

use crate::nodes::{Node, NodeKind};

pub fn stringify(node: &Node) -> String {
    let mut out = String::new();
    write_node(node, &mut out);
    out
}

fn any_subtree_mutated(node: &Node) -> bool {
    if node.raw_value.is_none() { return true; }
    node.nodes.iter().any(any_subtree_mutated)
}

fn write_node(node: &Node, out: &mut String) {
    match node.kind {
        NodeKind::Root => {
            if !any_subtree_mutated(node) {
                if let Some(raw) = &node.raw_value { out.push_str(raw); return; }
            }
            for (i, child) in node.nodes.iter().enumerate() {
                if i > 0 { out.push(','); }
                write_node(child, out);
            }
        }
        NodeKind::Selector => {
            if !any_subtree_mutated(node) {
                if let Some(raw) = &node.raw_value { out.push_str(raw); return; }
            }
            for child in &node.nodes { write_node(child, out); }
        }
        NodeKind::ClassName => {
            out.push_str(&node.spaces.before);
            out.push('.');
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
        NodeKind::Identifier => {
            out.push_str(&node.spaces.before);
            out.push('#');
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
        NodeKind::Tag | NodeKind::Universal | NodeKind::Nesting | NodeKind::String => {
            out.push_str(&node.spaces.before);
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
        NodeKind::Combinator | NodeKind::Comment => {
            out.push_str(&node.spaces.before);
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
        NodeKind::Pseudo => {
            out.push_str(&node.spaces.before);
            // `value` already includes the `:` or `::` prefix and any
            // inline `(args)` body the parser captured.
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
        NodeKind::Attribute => {
            out.push_str(&node.spaces.before);
            out.push_str(&node.value);
            out.push_str(&node.spaces.after);
        }
    }
}
