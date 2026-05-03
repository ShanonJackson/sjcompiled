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
//!   how upstream's `return 0` interacts with V8's sort to keep
//!   AtRules/comment positions intact.
//! - Buckets default to a sentinel "infinity" (we use `i32::MAX`) when
//!   the prop isn't in the table. This pushes non-shorthand decls
//!   AFTER all shorthands at any given depth.
//!
//! ## V8-parity sort algorithm
//!
//! `Array.prototype.sort` in V8 uses TimSort, but for arrays smaller
//! than `kMinRunLength = 32` it uses a pure binary-insertion-sort. The
//! comparator above is **non-transitive** — it returns `0` whenever
//! a Comment (or any node without a child decl) is one side, but
//! returns a non-zero value for two Decls with different buckets. So
//! the SET of pairs that compare equal is not closed under transitivity:
//! `cmp(comment, color) = 0` and `cmp(comment, background) = 0` but
//! `cmp(color, background) ≠ 0`.
//!
//! Under a non-transitive comparator the result of any stable sort is
//! algorithm-defined. Rust's `slice::sort_by` uses a TimSort variant
//! whose insertion-sort phase is a *linear* (left-scan) insertion that
//! stops at the first `Less` (i.e. it's a lower-bound style scan). V8's
//! binary-insertion phase uses a *binary* search that uses upper-bound
//! semantics (equal elements go AFTER, so the scan continues into the
//! left half only on strict `Less`). The two algorithms produce
//! different observable orderings on the same non-transitive
//! comparator.
//!
//! Concretely, on the `[comment, color, comment, background, comment]`
//! catchAll bucket from `sortAtomicStyleSheet`, V8 rearranges to
//! `[comment, background, color, comment, comment]` — the two trailing
//! comments end up adjacent because the binary search for `comment@4`
//! settles to upper-bound (end-of-array). Rust's linear insertion
//! never moves either decl past a comment because it stops at the
//! first `Equal`. To match the JS oracle byte-for-byte we re-implement
//! V8's binary-insertion-sort here. See
//! `crates/PHASE_8B_NAPI_NOTES.md` "Drift detected §2" for the gate
//! that surfaced this.
//!
//! For arrays of length >= 32 V8 transitions to TimSort proper (run
//! detection + merging). Atomic-CSS top-level catchAll/rules/atRules
//! buckets are typically <10 entries; we have no fixture exercising
//! 32+ elements, and the recursive descent into rule/at-rule bodies
//! also stays small in practice. If a real-world input ever hits the
//! TimSort threshold here, the parity-runner corpus will surface it
//! as drift and we extend this helper to a full TimSort port. Until
//! then binary-insertion-sort covers every observed corpus input.

use std::cmp::Ordering;

use postcss_core::{Node, NodeKind};
use compiled_utils::shorthand_buckets;

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

/// `sortShorthandDeclarations(nodes)` — depth-first sort matching V8's
/// `Array.prototype.sort` behaviour (binary insertion sort).
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
    v8_binary_insertion_sort(nodes, cmp_nodes);
}

/// V8-parity binary insertion sort. Replicates the pre-TimSort branch
/// of V8's `Array.prototype.sort` used for arrays of length < 32, and
/// the per-run insertion-extension phase used inside TimSort proper.
///
/// For each `i` in `1..nodes.len()`, binary-searches the prefix
/// `nodes[0..i]` for the upper-bound insertion point of `nodes[i]`
/// (equal elements go AFTER), then rotates the element into place via
/// `slice::rotate_right(1)`. The upper-bound semantics — `lo = mid + 1`
/// on `Equal` or `Greater`, `hi = mid` only on `Less` — is what makes
/// the algorithm move comments past following decls in the
/// non-transitive comment-vs-decl comparator setup; see module docs.
fn v8_binary_insertion_sort<T, F>(nodes: &mut [T], mut cmpf: F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    let len = nodes.len();
    for i in 1..len {
        let mut lo = 0usize;
        let mut hi = i;
        while lo < hi {
            let mid = lo + ((hi - lo) >> 1);
            // Compare element-being-inserted (at index i) vs prefix[mid].
            // The prefix is fully sorted at this point so `nodes[mid]`
            // is the pivot and `nodes[i]` is the candidate. Match V8's
            // ArrayTimSortImpl.tq `BinarySearch` upper-bound semantics:
            // strict `Less` narrows right; `Equal`/`Greater` narrows
            // left half off (lo = mid + 1) so equal keys land AFTER.
            match cmpf(&nodes[i], &nodes[mid]) {
                Ordering::Less => hi = mid,
                Ordering::Equal | Ordering::Greater => lo = mid + 1,
            }
        }
        // Move `nodes[i]` to position `lo`, shifting `nodes[lo..i]`
        // one slot right.
        if lo < i {
            nodes[lo..=i].rotate_right(1);
        }
    }
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
