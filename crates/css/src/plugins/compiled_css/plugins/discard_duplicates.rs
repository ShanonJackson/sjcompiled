//! Port of `packages/css/src/plugins/discard-duplicates.ts`.
//!
//! NOTE: distinct from `postcss-discard-duplicates@6.0.0` (which lives in
//! `crates/postcss-discard-duplicates`). Per `PARITY_VERSIONS.md` Anomaly #9
//! these are different code paths and must not be conflated.
//!
//! Upstream JS:
//! ```ts
//! Once(root) {
//!   const decls: Record<string, Declaration[]> = {};
//!   root.each((node) => {
//!     if (node.type === 'decl') {
//!       decls[node.prop] = decls[node.prop] || [];
//!       decls[node.prop].push(node);
//!     }
//!   });
//!   for (const key in decls) {
//!     const found = decls[key];
//!     for (let i = 0; i < found.length - 1; i++) {
//!       found[i].remove();
//!     }
//!   }
//! }
//! ```
//!
//! Top-level only (`root.each` is non-recursive). Groups direct-child
//! Declarations by `prop`, keeps the last in each group, removes the
//! rest. Insertion order of the prop-keyed map is the order props first
//! appear in document order — IndexMap preserves that.

use indexmap::IndexMap;
use postcss_core::container::remove_at;
use postcss_core::{NodeKind, PluginResult, Root};

pub fn discard_duplicates(root: &mut Root) -> PluginResult {
    // Phase 1: collect indices of every top-level Declaration grouped by prop.
    let mut groups: IndexMap<String, Vec<usize>> = IndexMap::new();
    if let Some(children) = root.root.nodes() {
        for (i, child) in children.iter().enumerate() {
            if let NodeKind::Declaration(d) = &child.kind {
                groups.entry(d.prop.clone()).or_default().push(i);
            }
        }
    }

    // Phase 2: collect every "all-but-last" original index that needs to go.
    // Sort ascending so we can apply with a running shift counter that
    // matches upstream's document-order remove sequence.
    let mut to_remove: Vec<usize> = Vec::new();
    for (_prop, indices) in &groups {
        if indices.len() > 1 {
            to_remove.extend(&indices[..indices.len() - 1]);
        }
    }
    to_remove.sort_unstable();

    // Phase 3: apply removals via remove_at so the Root-specific
    // `raws.before` transfer (postcss/lib/root.js::removeChild) fires on
    // every removal that lands at index 0 — matching upstream's cumulative
    // raws-cascade across the chain.
    let mut shift = 0usize;
    for orig in to_remove {
        let actual = orig - shift;
        remove_at(&mut root.root, actual);
        shift += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        discard_duplicates(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn keeps_last_of_duplicate_pair() {
        let out = run("display: block; display: flex;");
        assert!(!out.contains("display: block"), "got: {out:?}");
        assert!(out.contains("display: flex"));
    }

    #[test]
    fn keeps_last_of_three() {
        let out = run("color: red; color: blue; color: green;");
        assert!(!out.contains("red"));
        assert!(!out.contains("blue"));
        assert!(out.contains("green"));
    }

    #[test]
    fn no_op_when_no_duplicates() {
        let css = "color: red; background: blue; border: 0;";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn preserves_non_decl_top_level_nodes() {
        // `root.each` visits decls AND rules at top level; the plugin only
        // touches decls. Rules pass through untouched.
        let css = "color: red; color: blue; a { color: green; }";
        let out = run(css);
        assert!(!out.contains("red"));
        assert!(out.contains("color: blue"));
        assert!(out.contains("a { color: green; }"));
    }

    #[test]
    fn ignores_decls_nested_inside_rules() {
        // The decl inside the rule is NOT a top-level child of root,
        // so `root.each` doesn't see it. Both `color: red` decls survive.
        let css = "color: red; a { color: red; color: blue; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn interleaved_props_keep_last_each() {
        let out = run("color: red; background: x; color: blue; background: y;");
        assert!(!out.contains("color: red"));
        assert!(!out.contains("background: x"));
        assert!(out.contains("color: blue"));
        assert!(out.contains("background: y"));
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
