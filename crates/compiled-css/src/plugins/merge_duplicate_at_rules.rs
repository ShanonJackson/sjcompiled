//! Port of `packages/css/src/plugins/merge-duplicate-at-rules.ts`.
//!
//! Upstream JS:
//! ```ts
//! prepare() {
//!   const atRuleStore = {};
//!   return {
//!     AtRule(atRule) {
//!       const name = atRule.name + atRule.params;
//!       if (!atRuleStore[name]) {
//!         atRuleStore[name] = { node: atRule, children: {} };
//!       }
//!       atRule.each((node) => {
//!         const stringifiedNode = node.toString();
//!         if (!atRuleStore[name].children[stringifiedNode]) {
//!           atRuleStore[name].children[stringifiedNode] = node;
//!         }
//!       });
//!       atRule.remove();
//!     },
//!     OnceExit(root) {
//!       for (const key in atRuleStore) {
//!         const { node, children } = atRuleStore[key];
//!         node.nodes = Object.values(children);
//!         root.append(node);
//!       }
//!     },
//!   };
//! }
//! ```
//!
//! ## Behavior
//! - Visit every at-rule in document order. The visitor uses
//!   `atRule.name + atRule.params` as the merge key.
//! - For each at-rule's direct children, dedupe by `node.toString()` —
//!   first occurrence wins, insertion order preserved (`IndexMap`).
//! - The at-rule is removed from the tree on visit.
//! - On exit, each merged at-rule is appended to root in store
//!   insertion order.
//!
//! ## Documented upstream limitation: nested at-rules
//! Upstream's docstring says "Currently does not handle nested at-rules."
//! The visitor *does* descend into removed at-rules (postcss's `walk`
//! caches references), so nested at-rules become separate store entries
//! and the cleanup at OnceExit produces an unintuitive tree (the outer
//! ends up empty because the inner is reparented to root via re-append
//! using shared object identity).
//!
//! For the actual `sort.ts` pipeline this never matters: by the time
//! `merge-duplicate-at-rules` runs, the input is flat atomic CSS — no
//! nested at-rules. We mirror upstream's top-level merge exactly; the
//! nested-edge-case bytes are intentionally NOT replicated, because
//! doing so would require shared object-identity semantics that don't
//! map cleanly to Rust ownership. Any production input that hits nested
//! at-rules at this stage is already pathological.
//!
//! ## Use-site
//! This plugin runs in `packages/css/src/sort.ts` as the second stage
//! (after `postcss-discard-duplicates@6.0.0`, before
//! `sort-atomic-style-sheet`).

use indexmap::IndexMap;
use postcss_core::container::{append, remove_at};
use postcss_core::{stringify_node, Node, NodeKind, PluginResult, Root};

/// One entry in the at-rule merge store.
pub struct MergedAtRule {
    /// First-encountered AtRule node; its raws (between, params, etc.) are
    /// kept verbatim. Children are replaced with the deduped set at exit.
    node: Node,
    /// Insertion-ordered children, keyed by `stringify_node(child)` —
    /// matches upstream's plain JS object iteration order.
    children: IndexMap<String, Node>,
}

/// State shared between the visitor pass and the OnceExit pass — mirrors
/// the closure-captured `atRuleStore` in upstream `prepare()`.
pub type MergeStore = IndexMap<String, MergedAtRule>;

/// Visitor-phase port of upstream `AtRule(atRule)`. Walks top-level
/// children of root, removes each at-rule (via `remove_at` so the
/// Root.removeChild raws-transfer fires), and snapshots its direct
/// children into the merge map.
///
/// Returns the populated store. The caller must run [`finalize`] later
/// to re-append the merged at-rules.
///
/// Splitting visit/finalize is required to reproduce postcss's plugin
/// lifecycle: when this plugin runs in a pipeline alongside an
/// `OnceExit`-only plugin (e.g. `postcss-discard-duplicates`), the
/// other plugin's OnceExit fires *between* this plugin's visitor and
/// its OnceExit. Calling them as a single combined function silently
/// changes the byte-equivalent tree state.
pub fn visit(root: &mut Root) -> MergeStore {
    let mut store: MergeStore = IndexMap::new();

    let mut i = 0usize;
    loop {
        let len = root.root.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len { break; }
        let is_at = matches!(root.root.nodes().unwrap()[i].kind, NodeKind::AtRule(_));
        if !is_at {
            i += 1;
            continue;
        }
        // Pull the at-rule out via `remove_at` so the Root.removeChild
        // override (`postcss/lib/root.js::removeChild`) fires — when the
        // first child of root is removed and at least one sibling remains,
        // the removed node's `raws.before` transfers onto the new first
        // child. Without this, our re-appended at-rules end up with the
        // wrong leading whitespace (a stray `\n`).
        let at_node = remove_at(&mut root.root, i).expect("at-rule node at index i");
        let key = match &at_node.kind {
            NodeKind::AtRule(a) => format!("{}{}", a.name, a.params),
            _ => unreachable!(),
        };
        let snapshotted_children: Vec<Node> = at_node
            .nodes()
            .map(|c| c.clone())
            .unwrap_or_default();

        let entry = store.entry(key).or_insert_with(|| MergedAtRule {
            node: at_node.clone(),
            children: IndexMap::new(),
        });
        for child in snapshotted_children {
            let key = stringify_node(&child);
            entry.children.entry(key).or_insert(child);
        }
    }

    store
}

/// OnceExit-phase port of upstream `OnceExit(root)`. Re-appends merged
/// at-rules to root in store insertion order, each with its deduped
/// child set substituted in for the original body.
///
/// Uses `container::append` (not raw `Vec::push`) so the Root.normalize
/// raws-transfer fires — when root already has ≥2 children at append
/// time, the appended at-rule's `raws.before` is overwritten with the
/// current last child's `raws.before`. Skipping this produces a stray
/// missing-newline at the boundary between the last rule and the
/// re-appended at-rule.
pub fn finalize(root: &mut Root, store: MergeStore) {
    for (_k, mut merged) in store {
        if let Some(body) = merged.node.nodes_mut() {
            *body = merged.children.into_values().collect();
        }
        append(&mut root.root, vec![merged.node]);
    }
}

/// Combined visit + finalize. Used by callers that don't need to
/// interleave another plugin's OnceExit between the two phases (e.g.
/// the `Stage::MergeDuplicateAtRules` parity stage that runs this
/// plugin in isolation).
pub fn merge_duplicate_at_rules(root: &mut Root) -> PluginResult {
    let store = visit(root);
    finalize(root, store);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        merge_duplicate_at_rules(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn merges_two_identical_at_rules() {
        let out = run(
            "@media (min-width:500px){.a{color:red}}\n@media (min-width:500px){.b{color:blue}}",
        );
        let media_count = out.matches("@media (min-width:500px)").count();
        assert_eq!(media_count, 1, "got: {out:?}");
        assert!(out.contains(".a{color:red}"));
        assert!(out.contains(".b{color:blue}"));
    }

    #[test]
    fn dedupes_identical_children() {
        let out = run(
            "@media (min-width:500px){.a{color:red}}\n@media (min-width:500px){.a{color:red}}",
        );
        assert_eq!(out.matches(".a{color:red}").count(), 1, "got: {out:?}");
    }

    #[test]
    fn keeps_distinct_at_rules_separate() {
        let out = run(
            "@media (min-width:500px){.a{color:red}}\n@supports (display:grid){.b{color:blue}}",
        );
        assert!(out.contains("@media (min-width:500px)"));
        assert!(out.contains("@supports (display:grid)"));
    }

    #[test]
    fn no_op_when_no_at_rules() {
        let css = ".a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn preserves_non_atrule_top_level_nodes() {
        let css =
            ".a{color:red}\n@media (min-width:500px){.b{color:blue}}\n@media (min-width:500px){.c{color:green}}";
        let out = run(css);
        assert!(out.contains(".a{color:red}"));
        assert_eq!(out.matches("@media (min-width:500px)").count(), 1);
        assert!(out.contains(".b{color:blue}"));
        assert!(out.contains(".c{color:green}"));
    }
}
