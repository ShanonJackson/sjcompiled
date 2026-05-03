//! Port of `packages/css/src/plugins/increase-specificity.ts`.
//!
//! Upstream JS:
//! ```ts
//! import { INCREASE_SPECIFICITY_SELECTOR } from '@compiled/utils';
//! import { default as selectorParser, pseudo } from 'postcss-selector-parser';
//!
//! const parser = selectorParser((root) => {
//!   root.walkClasses((node) => {
//!     if (node.parent) {
//!       node.parent.insertAfter(node, pseudo({ value: INCREASE_SPECIFICITY_SELECTOR }));
//!     }
//!   });
//! });
//!
//! OnceExit(root) {
//!   root.walkRules((rule) => {
//!     rule.selectors = rule.selectors.map((selector) => {
//!       if (selector.includes('._')) {
//!         return parser.astSync(selector).toString();
//!       }
//!       return selector;
//!     });
//!   });
//! }
//! ```
//!
//! Behavior summary:
//! - Walks every Rule. For each comma-split selector, if the selector
//!   contains the substring `._` (Compiled-generated class marker), parse
//!   with selector-parser, walk every ClassName (recursively, including
//!   inside `:is(...)` etc.), and insert a new Pseudo carrying
//!   `:not(#\#)` immediately AFTER each. Otherwise leave the selector
//!   alone.
//! - Joins back via `rule.selectors = …` which preserves the original
//!   comma-separator pattern.
//!
//! ## "Bugs are features"
//! - The `selector.includes('._')` test is a substring match — a class
//!   like `.x._y` triggers it because `_y` is preceded by a `.`. This is
//!   intentional (matches all Compiled-generated atomic classes) but also
//!   incidentally matches user-authored classes that start with `_` after
//!   a `.`. We replicate the same matching.
//! - `walkClasses` is recursive — a class inside `:is(...)` gets the
//!   `:not(#\#)` inserted in the INNER Selector. That changes the
//!   inner-selector specificity in a way that may or may not be desired,
//!   but it's upstream's behavior.

use postcss_core::container::walk_rules_mut;
use postcss_core::{Mutation, NodeKind, PluginResult, Root};
use postcss_selector_parser as ssp;
use compiled_utils::INCREASE_SPECIFICITY_SELECTOR;

pub fn increase_specificity(root: &mut Root) -> PluginResult {
    walk_rules_mut(&mut root.root, &mut |node, _ctx| {
        if let NodeKind::Rule(rule) = &mut node.kind {
            let selectors = rule.get_selectors();
            let mapped: Vec<String> = selectors.into_iter().map(process_one_selector).collect();
            rule.set_selectors(&mapped);
            node.raws.selector = None;
        }
        Mutation::Keep
    });
    Ok(())
}

fn process_one_selector(selector: String) -> String {
    if !selector.contains("._") {
        return selector;
    }
    let proc = ssp::Processor::new();
    let mut parsed = match proc.ast_sync(&selector) {
        Ok(p) => p,
        Err(_) => return selector,
    };

    ssp::walk_classes(&mut parsed, |parent, idx| {
        let new_pseudo = ssp::Node::pseudo(INCREASE_SPECIFICITY_SELECTOR.to_string());
        // Insert AFTER the matched class. The walk_each cursor advances
        // past the matched node only; new siblings inserted here aren't
        // re-visited because `walk_each` only revisits when items are
        // inserted BEFORE.
        parent.nodes.insert(idx + 1, new_pseudo);
        parent.raw_value = None;
    });
    parsed.raw_value = None;

    ssp::stringify(&parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        increase_specificity(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn ignores_non_underscore_class() {
        let css = ".foo {}";
        assert_eq!(run(css), css);
    }

    #[test]
    fn appends_to_underscore_prefixed_class() {
        let out = run("._foo {}");
        assert!(out.contains(r"._foo:not(#\#)"), "got: {out:?}");
    }

    #[test]
    fn ignores_atrule_without_underscore_class_inside() {
        let css = "@media { .a {} }";
        let out = run(css);
        // `.a` doesn't have `._`, so unchanged.
        assert!(out.contains(".a {}"), "got: {out:?}");
    }

    #[test]
    fn rewrites_underscore_class_inside_atrule() {
        let out = run("@media { ._foo {} }");
        assert!(out.contains(r"._foo:not(#\#)"), "got: {out:?}");
    }

    #[test]
    fn ignores_root_and_html() {
        let css = "html {}\n:root {}";
        assert_eq!(run(css), css);
    }

    #[test]
    fn prepends_before_other_pseudos() {
        let out = run("._foo:hover { color: red; }");
        // Insert is AFTER the class but BEFORE the trailing :hover pseudo.
        assert!(out.contains(r"._foo:not(#\#):hover"), "got: {out:?}");
    }

    #[test]
    fn handles_pseudo_element_double_colon() {
        let out = run(r#"._baz::before { content: "bar"; }"#);
        assert!(out.contains(r"._baz:not(#\#)::before"), "got: {out:?}");
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
