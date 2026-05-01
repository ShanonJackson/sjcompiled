//! Port of `packages/css/src/plugins/parent-orphaned-pseudos.ts`.
//!
//! Upstream JS:
//! ```ts
//! const prependNestingTypeToSelector = (selector) => {
//!   const { parent } = selector;
//!   if (parent) {
//!     const nesting = selectorParser.nesting();
//!     parent.insertBefore(selector, nesting);
//!   }
//! };
//!
//! Once(root) {
//!   root.walkRules((rule) => {
//!     const { selectors } = rule;
//!     rule.selectors = selectors.map((selector) => {
//!       if (!selector.startsWith(':')) return selector;
//!       const parser = selectorParser((root) => {
//!         root.walkPseudos((pseudoSelector) => {
//!           prependNestingTypeToSelector(pseudoSelector);
//!         });
//!       }).astSync(selector, { lossless: false });
//!       return parser.toString();
//!     });
//!   });
//! }
//! ```
//!
//! Behavior summary:
//! - Walks every Rule in the tree.
//! - Splits the selector via `rule.selectors` (top-level comma split, trimmed).
//! - For each individual selector that **starts with `:`**, parses with
//!   selector-parser, walks every Pseudo (recursively, including those
//!   inside `:not(...)`, `:is(...)`, etc.), and inserts a `&` Nesting node
//!   immediately before each.
//! - Joins back via `rule.selectors = …` which preserves the original
//!   comma-separator pattern.
//!
//! ## "Bugs are features"
//! - `walkPseudos` IS recursive — a pseudo nested inside another
//!   pseudo's argument list gets a `&` prepended in its INNER Selector.
//!   That can produce odd byte sequences (`&:not(&:focus)`) but matches
//!   upstream and is part of the parity contract.
//! - The `startsWith(':')` check is on the *trimmed* selector string
//!   from `list.comma`, so `  :hover` (leading whitespace) becomes
//!   `:hover` and gets processed. Matches upstream which also reads
//!   the trimmed form via `list.comma`.

use postcss_core::container::walk_rules_mut;
use postcss_core::{Mutation, NodeKind, PluginResult, Root};
use postcss_selector_parser as ssp;

pub fn parent_orphaned_pseudos(root: &mut Root) -> PluginResult {
    walk_rules_mut(&mut root.root, &mut |node, _ctx| {
        if let NodeKind::Rule(rule) = &mut node.kind {
            let selectors = rule.get_selectors();
            let mapped: Vec<String> = selectors.into_iter().map(process_one_selector).collect();
            rule.set_selectors(&mapped);
            // Drop the cached raw selector so the stringifier re-emits
            // from the (possibly-mutated) `rule.selector` string.
            node.raws.selector = None;
        }
        Mutation::Keep
    });
    Ok(())
}

fn process_one_selector(selector: String) -> String {
    if !selector.starts_with(':') {
        return selector;
    }
    let proc = ssp::Processor::new();
    let mut parsed = match proc.ast_sync(&selector) {
        Ok(p) => p,
        // Tokenization failure means the selector was unparseable; pass
        // through unchanged rather than panic. Upstream similarly returns
        // the input on parse failure (the plugin doesn't error-handle).
        Err(_) => return selector,
    };

    ssp::walk_pseudos(&mut parsed, |parent, idx| {
        let nesting = ssp::Node::nesting();
        parent.nodes.insert(idx, nesting);
        // Clearing the parent Selector's raw_value forces the stringifier
        // to re-emit the typed children (which now include the new `&`).
        parent.raw_value = None;
    });
    // Root's raw_value must also drop so the top-level walk emits typed.
    parsed.raw_value = None;

    ssp::stringify(&parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        parent_orphaned_pseudos(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn parents_orphaned_top_level_pseudo() {
        let out = run(":hover { display: block; }");
        assert!(out.contains("&:hover"), "got: {out:?}");
        // Selector body must start with `&` — the orphan must have been parented.
        assert!(out.starts_with("&:hover"), "got: {out:?}");
    }

    #[test]
    fn does_not_re_parent_nested_with_ampersand() {
        // `&:hover` doesn't start with `:` so it's left alone.
        let css = "div { &:hover { display: block; } }";
        let out = run(css);
        assert!(out.contains("&:hover"));
        assert!(!out.contains("&&:hover"), "must not double-prepend: {out:?}");
    }

    #[test]
    fn skips_when_combinator_precedes_pseudo() {
        // `div > :hover` doesn't start with `:` — left alone.
        let css = "div > :hover { display: block; }";
        let out = run(css);
        assert!(out.contains("div > :hover"));
    }

    #[test]
    fn parents_dangling_pseudo_with_following_nesting() {
        // `:first-child &` starts with `:` and gets `&` prepended →
        // `&:first-child &`.
        let out = run(":first-child & { color: hotpink; }");
        assert!(out.contains("&:first-child &"), "got: {out:?}");
    }

    #[test]
    fn skips_attribute_then_nesting() {
        let css = "[data-look='h100']& { display: block; }";
        let out = run(css);
        assert!(out.contains("[data-look='h100']&"));
    }

    #[test]
    fn parents_each_pseudo_in_comma_list() {
        let out = run(":hover, :active { display: block; }");
        assert!(out.contains("&:hover"));
        assert!(out.contains("&:active"));
        assert!(!out.contains(", :active"), "got: {out:?}");
    }

    #[test]
    fn comma_list_with_mixed_kinds_only_parents_pseudos() {
        // `div` doesn't start with `:`, only `:active` does.
        let out = run("div, :active { display: block; }");
        assert!(out.contains("div, &:active"), "got: {out:?}");
    }

    #[test]
    fn no_op_on_rule_without_pseudo() {
        let css = "a.b { color: red; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
