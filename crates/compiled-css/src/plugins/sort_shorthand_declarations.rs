//! Port of `packages/css/src/plugins/sort-shorthand-declarations.ts`.
//!
//! Utility used by `sort-atomic-style-sheet.ts`. Sorts a list of nodes
//! by the first declaration's `prop` against `shorthand_buckets`. The
//! bucket integer determines order — `all` (0) < shorthand families
//! (1..=5) < non-shorthand (Infinity).
//!
//! Upstream JS:
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
//!   if (!aDecl?.prop || !bDecl?.prop) return 0;  // ← no swap on missing decl
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
//! ## Behavior
//! - Recursive: descend into every container, sorting children at each
//!   depth before sorting at the current depth.
//! - "First decl" is found by:
//!   - if the node is itself a Decl, return it.
//!   - else if it's a container, return the first direct-child Decl.
//!   - else None (skip).
//! - When either side has no decl, return `Equal` (no swap) — this is
//!   how upstream's `return 0` interacts with stable sort to keep
//!   AtRules/comment positions intact.
//! - Buckets default to a sentinel "infinity" (we use `i32::MAX`) when
//!   the prop isn't in the table. This pushes non-shorthand decls
//!   AFTER all shorthands at any given depth.

use std::cmp::Ordering;

use postcss_core::{Node, NodeKind};
use sjcompiled_utils::shorthand_buckets;

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

/// `sortShorthandDeclarations(nodes)` — depth-first stable sort.
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
    nodes.sort_by(cmp_nodes);
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
