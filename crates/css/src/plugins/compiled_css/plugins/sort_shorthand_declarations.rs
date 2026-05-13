//! Port of `packages/css/src/plugins/sort-shorthand-declarations.ts`.
//!
//! Utility used by `sort-atomic-style-sheet.ts`. Sorts a list of nodes
//! by the first declaration's `prop` against `shorthand_buckets`. The
//! bucket integer determines order — `all` (0) < shorthand families
//! (1..=5) < non-shorthand (Infinity).
//!
//! Upstream JS (verbatim):
//! ```ts
//! const findDeclaration = (node) => {
//!   if (node.type === 'decl') return node;
//!   if ('nodes' in node) return node.nodes.find(nodeIsDeclaration);
//!   return undefined;
//! };
//!
//! const sortNodes = (a, b) => {
//!   const aDecl = findDeclaration(a);
//!   const bDecl = findDeclaration(b);
//!   if (!aDecl?.prop || !bDecl?.prop) return 0;
//!   const aShorthandBucket = shorthandBuckets[aDecl.prop] ?? Infinity;
//!   const bShorthandBucket = shorthandBuckets[bDecl.prop] ?? Infinity;
//!   return aShorthandBucket - bShorthandBucket;
//! };
//!
//! export const sortShorthandDeclarations = (nodes) => {
//!   if (!nodes?.length) return;
//!   nodes.forEach((node) => {
//!     if ('nodes' in node && node.nodes?.length) {
//!       sortShorthandDeclarations(node.nodes);
//!     }
//!   });
//!   nodes.sort(sortNodes);
//! };
//! ```
//!
//! ## Behaviour (1:1 with upstream)
//! - Recursive: descend into every container, sorting children at each
//!   depth before sorting at the current depth (matches upstream's
//!   `forEach(... sortShorthandDeclarations(node.nodes))` then
//!   `nodes.sort(...)`).
//! - "First decl" mirrors `findDeclaration`:
//!   - if the node is itself a Decl, return it,
//!   - else if it's a container, return the first direct-child Decl,
//!   - else `None` (upstream `undefined`).
//! - When either side has no decl, comparator returns `Equal`
//!   (upstream `return 0`) — V8's stable `Array.prototype.sort`
//!   preserves the relative order of equal elements, and so does
//!   Rust's `slice::sort_by`. AtRules / comments therefore stay where
//!   they were.
//! - Bucket defaults to `i32::MAX` when the prop is unknown
//!   (upstream `?? Infinity`). This pushes non-shorthand decls AFTER
//!   all shorthands at any given depth.
//!
//! ## Sort algorithm
//!
//! Upstream is a single line: `nodes.sort(sortNodes)`. The naive port
//! `nodes.sort_by(cmp_nodes)` is **NOT** byte-equivalent.
//!
//! `cmp_nodes` is non-transitive: it returns `Equal` whenever either
//! side has no first decl (a Comment, or a Rule whose first child is
//! itself a Rule). The set of pairs that compare equal is therefore
//! not closed under transitivity, e.g.
//! `cmp(comment, color) = Equal` and `cmp(comment, background) =
//! Equal` but `cmp(color, background) = Less`. Under such a
//! comparator, the result of any stable sort is algorithm-defined —
//! V8's PowerSort and Rust's slice::sort_by produce different
//! permutations on the same input.
//!
//! AFM production runs `transformCss` under node V8. The parity
//! oracle now does too (see `packages/css/scripts/parity-bridge-
//! ts-loader.mjs` for why). To produce byte-identical output we
//! delegate the sort itself to [`crate::compat::v8_array_sort::
//! v8_sort`] — a 1:1 port of V8's `Array.prototype.sort`
//! (PowerSort/TimSort variant, full algorithm including run
//! detection, min_run boost, and galloping merge). That shim
//! exists solely for this surface; ordinary Rust code uses
//! `slice::sort_by`.
//!
//! Concrete divergence that motivated this: AFM fixture 02508
//! (`crates/parity-runner/corpus/afm-transform-css/02508_*.css`)
//! had `Rust.sort_by` placing `&__table` after `&--dynamic` while
//! V8 placed `&__table` adjacent to `font` because both sides see
//! the same buckets ([Inf, Inf, 1, Inf, no-decl, 4, 1, 1, 1]) but
//! V8's run detector treats the first transition (Inf → 1 at index
//! 2) as a run break and merges the two halves under PowerSort's
//! galloping rules; Rust's TimSort variant detects different runs
//! and merges them differently.

use std::cmp::Ordering;

use postcss_core::{Node, NodeKind};
use compiled_utils::shorthand_buckets;

use super::super::compat::v8_array_sort::v8_sort;

/// Find the "first declaration" used as the sort key for `node`.
fn find_decl(node: &Node) -> Option<&postcss_core::Declaration> {
    if let NodeKind::Declaration(d) = &node.kind {
        return Some(d);
    }
    let children = node.nodes()?;
    children.iter().find_map(|c| {
        if let NodeKind::Declaration(d) = &c.kind { Some(d) } else { None }
    })
}

fn bucket_for(prop: &str) -> i32 {
    // Sentinel for "not a shorthand" — `Infinity` upstream. Use
    // `i32::MAX` in our integer space; consistent across all sort
    // calls because the table is the only source of finite bucket
    // values (0..=5 in the current tables).
    shorthand_buckets()
        .get(prop)
        .copied()
        .map(|b| b as i32)
        .unwrap_or(i32::MAX)
}

fn cmp_nodes(a: &Node, b: &Node) -> Ordering {
    let a_decl = find_decl(a);
    let b_decl = find_decl(b);
    let (Some(ad), Some(bd)) = (a_decl, b_decl) else {
        // Upstream `return 0` — leave the order unchanged.
        return Ordering::Equal;
    };
    if ad.prop.is_empty() || bd.prop.is_empty() {
        return Ordering::Equal;
    }
    bucket_for(&ad.prop).cmp(&bucket_for(&bd.prop))
}

/// `sortShorthandDeclarations(nodes)` — depth-first stable sort,
/// 1:1 with upstream `nodes.forEach(...)` then `nodes.sort(...)`.
pub fn sort_shorthand_declarations(nodes: &mut [Node]) {
    if nodes.is_empty() {
        return;
    }
    // Recurse first — children at each depth get sorted before their
    // parent compares them. Matches upstream which calls into
    // `sortShorthandDeclarations(node.nodes)` BEFORE `nodes.sort(...)`.
    for n in nodes.iter_mut() {
        if let Some(child_nodes) = n.nodes_mut() {
            if !child_nodes.is_empty() {
                sort_shorthand_declarations(child_nodes.as_mut_slice());
            }
        }
    }
    v8_sort(nodes, cmp_nodes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn parse_top_level(css: &str) -> Vec<Node> {
        let root = parse(css).unwrap();
        root.root.nodes().cloned().unwrap_or_default()
    }

    fn extract_props(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match &n.kind {
                NodeKind::Rule(_) | NodeKind::AtRule(_) => {
                    if let Some(decl) = find_decl(n) { decl.prop.clone() } else { String::new() }
                }
                NodeKind::Declaration(d) => d.prop.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn sorts_decls_by_bucket() {
        // `all` (0) < `border` (1) < `border-color` (2) < `border-block` (3)
        // < `border-top` (4) < `border-block-start` (5) < `outline-width` (Inf).
        let mut nodes = parse_top_level(
            ".a { outline-width: 1px; }\n\
             .b { border-top: 1px; }\n\
             .c { all: unset; }\n\
             .d { border: none; }\n\
             .e { border-color: red; }",
        );
        sort_shorthand_declarations(&mut nodes);
        assert_eq!(
            extract_props(&nodes),
            vec!["all", "border", "border-color", "border-top", "outline-width"]
        );
    }

    #[test]
    fn unknown_prop_sorts_last() {
        let mut nodes = parse_top_level(
            ".a { color: red; }\n\
             .b { border: none; }\n\
             .c { all: unset; }",
        );
        sort_shorthand_declarations(&mut nodes);
        assert_eq!(extract_props(&nodes), vec!["all", "border", "color"]);
    }

    #[test]
    fn empty_input_no_panic() {
        let mut empty: Vec<Node> = Vec::new();
        sort_shorthand_declarations(&mut empty);
    }

    #[test]
    fn recurses_into_atrule_body() {
        // The decls inside an at-rule should also be sorted.
        let mut nodes = parse_top_level(
            "@media all {\n\
                .a { outline-width: 1px; }\n\
                .b { all: unset; }\n\
                .c { border: none; }\n\
             }",
        );
        sort_shorthand_declarations(&mut nodes);
        // Top-level: just one @media node, no top-level sort change.
        // Inside the @media, the rules should be sorted.
        let inner: Vec<String> = match &nodes[0].kind {
            NodeKind::AtRule(_) => nodes[0]
                .nodes()
                .map(|v| {
                    v.iter()
                        .map(|n| match &n.kind {
                            NodeKind::Rule(_) => find_decl(n).map(|d| d.prop.clone()).unwrap_or_default(),
                            _ => String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => panic!("expected atrule"),
        };
        assert_eq!(inner, vec!["all", "border", "outline-width"]);
    }

    #[test]
    fn ties_preserve_order() {
        let mut nodes = parse_top_level(
            ".a { color: red; }\n\
             .b { color: blue; }\n\
             .c { color: green; }",
        );
        sort_shorthand_declarations(&mut nodes);
        // All three share `color` (Infinity) — original order preserved.
        let selectors: Vec<String> = nodes
            .iter()
            .map(|n| if let NodeKind::Rule(r) = &n.kind { r.selector.clone() } else { String::new() })
            .collect();
        assert_eq!(selectors, vec![".a", ".b", ".c"]);
    }
}
