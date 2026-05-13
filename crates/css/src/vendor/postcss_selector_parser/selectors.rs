//! Stringifier for the selector AST.
//!
//! Each Node knows how to serialize itself. When a Node carries a
//! `raw_value` (set by the parser to original source bytes) and is
//! un-mutated, we emit it verbatim — that's the byte-identity guarantee.
//!
//! When a plugin mutates a Node it must clear `raw_value` (use
//! [`crate::nodes::Node::set_value`]) so the stringifier walks the typed
//! shape instead.

use super::nodes::{Node, NodeKind};

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
            out.push_str(&node.spaces.before);
            for child in &node.nodes { write_node(child, out); }
            out.push_str(&node.spaces.after);
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
            // `value` carries the `:foo` / `::foo` prefix only. Parens
            // are rebuilt from `nodes` (parsed inner Selectors) so plugin
            // mutations to the inner subtree flow through to output.
            // Bare `,` join matches upstream `pseudo.js::toString`'s
            // `this.map(String).join(',')`. Whitespace between selectors
            // lives on each child Selector's first child as `spaces.before`.
            out.push_str(&node.value);
            if !node.nodes.is_empty() {
                out.push('(');
                for (i, child) in node.nodes.iter().enumerate() {
                    if i > 0 { out.push(','); }
                    write_node(child, out);
                }
                out.push(')');
            }
            out.push_str(&node.spaces.after);
        }
        NodeKind::Attribute => {
            out.push_str(&node.spaces.before);
            // Payload-aware branch: when a plugin (e.g. cssnano-postcss-
            // minify-selectors) mutates `quote_mark`, `operator`, the
            // attribute name, value, or any of the four `attribute_spaces`
            // sub-pairs, it sets `payload.dirty = true` and we rebuild
            // the bracket form from the typed payload — mirroring upstream
            // `attribute.js::toString` (lines 289-306 in 6.1.2). For
            // un-mutated nodes we keep emitting `node.value` (raw bracket
            // text) so byte-identity round-trip is preserved.
            let dirty = node.attribute.as_ref().map_or(false, |p| p.dirty);
            if dirty {
                let payload = node.attribute.as_ref().unwrap();
                let attr_spaces = node.attribute_spaces.clone().unwrap_or_default();
                out.push('[');
                out.push_str(&attr_spaces.attribute.before);
                if let Some(ns) = &payload.namespace {
                    out.push_str(ns);
                    out.push('|');
                }
                out.push_str(&payload.attribute);
                out.push_str(&attr_spaces.attribute.after);
                if let Some(op) = &payload.operator {
                    out.push_str(&attr_spaces.operator.before);
                    out.push_str(op);
                    out.push_str(&attr_spaces.operator.after);
                    out.push_str(&attr_spaces.value.before);
                    if let Some(v) = &payload.value {
                        match payload.quote_mark {
                            Some(q) => {
                                out.push(q);
                                out.push_str(v);
                                out.push(q);
                            }
                            None => out.push_str(v),
                        }
                    }
                    out.push_str(&attr_spaces.value.after);
                    if payload.case_insensitive {
                        // Upstream defaultAttrConcat injects a single
                        // leading space when value is non-empty, the
                        // value is unquoted, and `attr_spaces.before`
                        // is empty (attribute.js:296-300). Plugins that
                        // explicitly clear all spaces and run with
                        // `insensitive: true` rely on
                        // `attr_spaces.value.after = " "` to provide
                        // that gap — so we emit no extra space here;
                        // the consumer controls separation via spaces.
                        out.push_str(&attr_spaces.insensitive.before);
                        out.push('i');
                        out.push_str(&attr_spaces.insensitive.after);
                    }
                }
                out.push(']');
            } else {
                out.push_str(&node.value);
            }
            out.push_str(&node.spaces.after);
        }
    }
}
