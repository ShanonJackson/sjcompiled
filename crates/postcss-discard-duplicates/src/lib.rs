//! crates/postcss-discard-duplicates
//! Byte-for-byte Rust port of `postcss-discard-duplicates@6.0.0`.
//!
//! Used by `packages/css/src/sort.ts:2` (the `sort()` entry point).
//! Distinct from:
//! - `postcss-discard-duplicates@5.1.0` — pulled in transitively by
//!   `cssnano-preset-default@5.2.14` but filtered out before execution
//!   (see `PARITY_VERSIONS.md` Anomaly #5).
//! - The LOCAL `discard-duplicates` plugin
//!   (`crates/compiled-css/src/plugins/discard_duplicates.rs`) which
//!   only dedupes top-level decls.
//!
//! ## Equality semantics
//!
//! `equals(a, b)` upstream walks the AST recursively:
//! - same `type`
//! - same `important`
//! - per kind:
//!   - rule: `selector` matches.
//!   - atrule: `name` AND `params` match, plus `trim(raws.before)` and
//!     `trim(raws.afterName)` match.
//!   - decl: `prop` AND `value` match, plus `trim(raws.before)` matches.
//! - if both have `nodes`, recursively equal.
//!
//! ## Two dedupe variants
//!
//! - `dedupeRule(last, nodes)` — when two RULES at the same depth share
//!   a selector, decls inside the LATER rule scan back through earlier
//!   rules with the same selector; matching decls are REMOVED from the
//!   earlier rule. Earlier rules with no non-comment children left are
//!   removed entirely.
//! - `dedupeNode(last, nodes)` — for atrules and top-level decls: any
//!   earlier sibling that `equals` `last` is removed.

use postcss_core::container::remove_at;
use postcss_core::{Node, NodeKind, PluginResult, Root};

pub fn postcss_discard_duplicates(root: &mut Root) -> PluginResult {
    dedupe(&mut root.root);
    Ok(())
}

/// Mirrors upstream `dedupe(root)`. Iterates children right-to-left;
/// recursive descent first, then per-kind dedupe.
fn dedupe(parent: &mut Node) {
    if parent.nodes().is_none() {
        return;
    }
    let mut index: isize = parent.nodes().map(|n| n.len() as isize - 1).unwrap_or(-1);
    while index >= 0 {
        let i = index as usize;
        if i >= parent.nodes().map(|n| n.len()).unwrap_or(0) {
            index -= 1;
            continue;
        }
        // Recurse into the child first.
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            dedupe(child);
        }
        let kind = match parent.nodes().unwrap().get(i) {
            Some(n) => kind_tag(n),
            None => { index -= 1; continue; }
        };
        match kind {
            KindTag::Rule => dedupe_rule(parent, i),
            KindTag::AtRule | KindTag::Decl => dedupe_node(parent, i),
            _ => {}
        }
        index -= 1;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KindTag { Root, Rule, AtRule, Decl, Comment }

fn kind_tag(n: &Node) -> KindTag {
    match &n.kind {
        NodeKind::Root(_) => KindTag::Root,
        NodeKind::Rule(_) => KindTag::Rule,
        NodeKind::AtRule(_) => KindTag::AtRule,
        NodeKind::Declaration(_) => KindTag::Decl,
        NodeKind::Comment(_) => KindTag::Comment,
    }
}

/// `dedupeRule(last, nodes)` upstream.
fn dedupe_rule(parent: &mut Node, last_idx: usize) {
    // Snapshot last's selector + decl children for the comparison loop.
    let (last_selector, last_decls) = {
        let last = match parent.nodes().and_then(|n| n.get(last_idx)) {
            Some(n) => n,
            None => return,
        };
        let last_rule = match &last.kind {
            NodeKind::Rule(r) => r,
            _ => return,
        };
        let decls: Vec<Node> = last_rule.nodes
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::Declaration(_)))
            .cloned()
            .collect();
        (last_rule.selector.clone(), decls)
    };

    let mut index: isize = last_idx as isize - 1;
    while index >= 0 {
        let i = index as usize;
        let same_selector = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => matches!(&n.kind, NodeKind::Rule(r) if r.selector == last_selector),
            None => false,
        };
        if !same_selector {
            index -= 1;
            continue;
        }

        // For each decl in `last`, walk earlier-rule's body right-to-left.
        for last_decl in &last_decls {
            let earlier_body_len = parent
                .nodes()
                .and_then(|n| n.get(i))
                .and_then(|e| e.nodes())
                .map(|b| b.len())
                .unwrap_or(0);
            let mut j: isize = earlier_body_len as isize - 1;
            while j >= 0 {
                let jj = j as usize;
                let matched = {
                    let earlier_node = parent.nodes().and_then(|n| n.get(i));
                    let earlier_body = earlier_node.and_then(|e| e.nodes());
                    match earlier_body.and_then(|b| b.get(jj)) {
                        Some(child) => matches!(&child.kind, NodeKind::Declaration(_)) && nodes_equal(child, last_decl),
                        None => false,
                    }
                };
                if matched {
                    if let Some(earlier_mut) = parent.nodes_mut().and_then(|n| n.get_mut(i)) {
                        if let Some(body) = earlier_mut.nodes_mut() {
                            body.remove(jj);
                        }
                    }
                }
                j -= 1;
            }
        }

        // If earlier rule's body is now "empty" (only comments or zero
        // children), remove the earlier rule.
        let earlier_now_empty = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => n
                .nodes()
                .map(|body| body.iter().all(|c| matches!(c.kind, NodeKind::Comment(_))))
                .unwrap_or(false),
            None => false,
        };
        if earlier_now_empty {
            // Use `remove_at` so the Root-specific raws-transfer
            // (`postcss/lib/root.js::removeChild`) fires when removing
            // the first child of root.
            remove_at(parent, i);
        }
        index -= 1;
    }
}

/// `dedupeNode(last, nodes)` — remove any earlier sibling that
/// `equals` last.
fn dedupe_node(parent: &mut Node, last_idx: usize) {
    let last = match parent.nodes().and_then(|n| n.get(last_idx)).cloned() {
        Some(n) => n,
        None => return,
    };
    let mut index: isize = last_idx as isize - 1;
    while index >= 0 {
        let i = index as usize;
        let candidate_matches = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => nodes_equal(n, &last),
            None => false,
        };
        if candidate_matches {
            // Use `remove_at` for the Root.removeChild override.
            remove_at(parent, i);
        }
        index -= 1;
    }
}

/// `equals(a, b)` upstream — deep recursive equality.
fn nodes_equal(a: &Node, b: &Node) -> bool {
    if kind_tag(a) != kind_tag(b) {
        return false;
    }
    let a_important = matches!(&a.kind, NodeKind::Declaration(d) if d.important);
    let b_important = matches!(&b.kind, NodeKind::Declaration(d) if d.important);
    if a_important != b_important {
        return false;
    }

    match (&a.kind, &b.kind) {
        (NodeKind::Rule(ra), NodeKind::Rule(rb)) => {
            if ra.selector != rb.selector {
                return false;
            }
        }
        (NodeKind::AtRule(aa), NodeKind::AtRule(ab)) => {
            if aa.name != ab.name || aa.params != ab.params {
                return false;
            }
            if trim_str(a.raws.before.as_deref()) != trim_str(b.raws.before.as_deref()) {
                return false;
            }
            if trim_str(a.raws.after_name.as_deref()) != trim_str(b.raws.after_name.as_deref()) {
                return false;
            }
        }
        (NodeKind::Declaration(da), NodeKind::Declaration(db)) => {
            if da.prop != db.prop || da.value != db.value {
                return false;
            }
            if trim_str(a.raws.before.as_deref()) != trim_str(b.raws.before.as_deref()) {
                return false;
            }
        }
        (NodeKind::Comment(ca), NodeKind::Comment(cb)) => {
            if ca.text != cb.text {
                return false;
            }
        }
        _ => {}
    }

    let a_nodes = a.nodes();
    let b_nodes = b.nodes();
    if let (Some(an), Some(bn)) = (a_nodes, b_nodes) {
        if an.len() != bn.len() {
            return false;
        }
        for (ca, cb) in an.iter().zip(bn.iter()) {
            if !nodes_equal(ca, cb) {
                return false;
            }
        }
    }
    true
}

/// `trimValue(value)` upstream — `value ? value.trim() : value`.
fn trim_str(s: Option<&str>) -> Option<String> {
    s.map(|v| v.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_discard_duplicates(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn drops_duplicate_top_level_decl() {
        let out = run("color: red; color: red;");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn keeps_distinct_top_level_decls() {
        let css = "color: red;\nbackground: blue;";
        assert_eq!(run(css), css);
    }

    #[test]
    fn drops_duplicate_at_rule() {
        let out = run(
            "@media (max-width: 100px) { a { color: red; } }\n@media (max-width: 100px) { a { color: red; } }",
        );
        assert_eq!(out.matches("@media (max-width: 100px)").count(), 1);
    }

    #[test]
    fn keeps_distinct_at_rules() {
        let css = "@media (max-width: 100px) { a {} }\n@media (max-width: 200px) { a {} }";
        let out = run(css);
        assert_eq!(out.matches("@media").count(), 2);
    }

    #[test]
    fn rule_merge_dedupes_overlapping_decls() {
        let out = run("a { color: red; background: blue; } a { color: red; }");
        assert!(out.contains("background: blue"));
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn rule_emptied_after_decl_dedupe_is_removed() {
        let out = run("a { color: red; } a { color: red; }");
        assert_eq!(out.matches("a {").count(), 1, "got: {out:?}");
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn no_op_when_no_duplicates() {
        let css = "a { color: red; }\nb { color: blue; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn nested_dedupe_recurses() {
        let out = run("@media (min-width: 100px) { color: red; color: red; }");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn dedupe_three_consecutive_decls() {
        let out = run("color: red; color: red; color: red;");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }
}
