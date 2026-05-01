//! Port of `packages/css/src/plugins/flatten-multiple-selectors.ts`.
//!
//! Upstream JS:
//! ```ts
//! function flattenNode(node: Container) {
//!   node.each((child) => {
//!     if (!child.parent) return;
//!     if (child.type === 'atrule' && 'each' in child) flattenNode(child);
//!     if (child.type === 'rule' && child.parent) {
//!       const selectors: string[] = [];
//!       selectorParser((root) => {
//!         root.each((sel) => { selectors.push(sel.toString().trim()); });
//!       }).processSync((child as Rule).selector);
//!       if (selectors.length > 1) {
//!         selectors.forEach((selector) => {
//!           const rule = (child as Rule).clone();
//!           rule.selector = selector;
//!           child.parent?.insertBefore(child, rule);
//!         });
//!         child.parent?.removeChild(child);
//!       }
//!     }
//!   });
//! }
//!
//! OnceExit(root) { flattenNode(root); }
//! ```
//!
//! Behavior summary:
//! - Recurse into every AtRule with a body. Don't recurse into Rules
//!   (the upstream comment says "Preconditions: 1. No nested rules
//!   allowed" — by this stage `postcss-nested` has already flattened).
//! - For each Rule child, parse its selector with selector-parser, take
//!   `sel.toString().trim()` for each top-level Selector group.
//! - If more than one selector exists, clone the rule once per selector
//!   with the cloned rule's `selector` set to that single selector,
//!   insert each clone before the original, then remove the original.
//! - Each clone keeps the original's `raws` (including `before`), so
//!   downstream stringification preserves indentation.

use postcss_core::{Node, NodeKind, PluginResult, Root};
use postcss_selector_parser as ssp;

pub fn flatten_multiple_selectors(root: &mut Root) -> PluginResult {
    flatten_node(&mut root.root);
    Ok(())
}

fn flatten_node(parent: &mut Node) {
    // Manual cursor walk so we can control raws semantics around the
    // remove+insert sequence (matches upstream Root.normalize behavior:
    // when inserting before the first child of root, the original's
    // `raws.before` is deleted, so subsequent clones come out with
    // `raws.before == undefined` → the stringifier derives the default).
    let parent_is_root = matches!(parent.kind, NodeKind::Root(_));
    if parent.nodes().is_none() { return; }

    let mut i = 0usize;
    loop {
        let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len { break; }

        // Snapshot the kind to decide what to do, without holding a
        // borrow across mutation.
        let is_atrule_with_block = matches!(
            &parent.nodes().unwrap()[i].kind,
            NodeKind::AtRule(a) if a.has_block
        );
        let is_rule = matches!(parent.nodes().unwrap()[i].kind, NodeKind::Rule(_));

        if is_atrule_with_block {
            // Recurse into the at-rule body.
            let nodes = parent.nodes_mut().unwrap();
            flatten_node(&mut nodes[i]);
            i += 1;
            continue;
        }

        if !is_rule {
            i += 1;
            continue;
        }

        // It's a Rule — split its selector via selector-parser.
        let selectors = {
            let nodes = parent.nodes().unwrap();
            let r = match &nodes[i].kind { NodeKind::Rule(r) => r, _ => unreachable!() };
            split_via_selector_parser(&r.selector)
        };

        if selectors.len() < 2 {
            i += 1;
            continue;
        }

        // Build clones — one per selector. Each clone inherits all of
        // the original's raws by default.
        let mut clones: Vec<Node> = {
            let original = &parent.nodes().unwrap()[i];
            selectors.into_iter().map(|sel| {
                let mut clone = original.clone();
                if let NodeKind::Rule(r) = &mut clone.kind {
                    r.selector = sel;
                }
                clone.raws.selector = None;
                clone
            }).collect()
        };

        // Match upstream's Root vs container raws semantics:
        //
        // - At Root: the very first clone's `insertBefore(original, clone)`
        //   triggers `Root.normalize(clone, original, 'prepend')` which
        //   `delete`s the original's `raws.before`. Subsequent clones
        //   then inherit `raws.before = undefined` from the (mutated)
        //   sample. Final result: clone[0].raws.before = original's,
        //   clones[1..].raws.before = undefined.
        // - At AtRule (any non-Root container): no special override,
        //   each clone retains the original's `raws.before` verbatim.
        //
        // We replicate by explicitly clearing `raws.before` on
        // clones[1..] when the parent is Root.
        if parent_is_root {
            for clone in clones.iter_mut().skip(1) {
                clone.raws.before = None;
            }
        }

        // Apply: remove original, splice clones at i.
        let count = clones.len();
        {
            let nodes = parent.nodes_mut().unwrap();
            nodes.remove(i);
            for (offset, clone) in clones.into_iter().enumerate() {
                nodes.insert(i + offset, clone);
            }
        }

        // Advance past the inserts. We don't need to re-process the
        // single-selector clones (they have only one selector each).
        i += count;
    }
}

/// `selectorParser((root) => root.each(sel => list.push(sel.toString().trim())))`.
/// Returns the trimmed `.toString()` of each top-level Selector. On parse
/// failure, falls back to the input string as a single-element list (so
/// the caller doesn't try to flatten and the rule passes through).
fn split_via_selector_parser(selector: &str) -> Vec<String> {
    let proc = ssp::Processor::new();
    let parsed = match proc.ast_sync(selector) {
        Ok(p) => p,
        Err(_) => return vec![selector.to_string()],
    };
    parsed
        .nodes
        .iter()
        .map(|sel| ssp::stringify(sel).trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        flatten_multiple_selectors(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_on_single_selector() {
        let css = "a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn flattens_two_selector_list() {
        let out = run("a, b { color: red; }");
        assert_eq!(out.matches("color: red").count(), 2, "got: {out:?}");
        assert!(out.contains("a {"), "got: {out:?}");
        assert!(out.contains("b {"), "got: {out:?}");
    }

    #[test]
    fn flattens_three_element_selectors() {
        let out = run("div, span, li { color: red; }");
        assert_eq!(out.matches("color: red").count(), 3);
    }

    #[test]
    fn preserves_complex_selectors_inside_groups() {
        // Each selector group might contain `:is(.a, .b)` etc — must not
        // split inside parens (selector-parser handles that).
        let out = run(":is(.a, .b), .c { color: red; }");
        assert_eq!(out.matches("color: red").count(), 2);
        assert!(out.contains(":is(.a, .b)"), "got: {out:?}");
    }

    #[test]
    fn flattens_inside_at_rule() {
        let css = "@media (min-width: 100px) { a, b { color: red; } }";
        let out = run(css);
        assert_eq!(out.matches("color: red").count(), 2);
        assert!(out.contains("@media"));
    }

    #[test]
    fn does_not_flatten_attributes_with_comma_in_value() {
        let out = run(r#"[data-x="a,b"] { color: red; }"#);
        assert_eq!(out.matches("color: red").count(), 1);
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
