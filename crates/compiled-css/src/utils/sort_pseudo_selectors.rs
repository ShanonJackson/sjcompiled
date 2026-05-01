//! Port of `packages/css/src/utils/sort-pseudo-selectors.ts`.
//!
//! Upstream JS:
//! ```ts
//! const getPseudoSelectorScore = (selector: string) => {
//!   const index = styleOrder.findIndex((pseudoClass) =>
//!     selector.trim().endsWith(pseudoClass));
//!   return index + 1;
//! };
//!
//! export const sortPseudoSelectors = (rules: Rule[]): void => {
//!   rules.sort((rule1, rule2) => {
//!     const selector1 = rule1.selectors.length ? rule1.selectors[0] : rule1.selector;
//!     const selector2 = rule2.selectors.length ? rule2.selectors[0] : rule2.selector;
//!     return getPseudoSelectorScore(selector1) - getPseudoSelectorScore(selector2);
//!   });
//! };
//! ```
//!
//! Sorts a list of Rule nodes IN PLACE by trailing pseudo-selector,
//! using `STYLE_ORDER` as the priority list (LVFHA + focus-within/focus-visible).
//! Selectors that don't end in a known pseudo score `0` and sort first.
//!
//! ## Stable sort, JS parity
//! `Array.prototype.sort` has been stable since ES2019. Rust's
//! `sort_by` is also stable. For equal scores, the original order is
//! preserved — which matters because `sort-atomic-style-sheet`
//! interleaves `sortShorthandDeclarations` (also a stable sort) before
//! this; both must be stable and produce identical tie-break ordering.
//!
//! ## `selectors[0]` vs `selector`
//! Upstream picks the first comma-split selector when present, falls
//! back to the raw `selector` string otherwise. We mirror via
//! `Rule::get_selectors()`.

use postcss_core::{Node, NodeKind};

use super::style_ordering::STYLE_ORDER;

/// Returns `index + 1` for the first matching trailing pseudo, or `0`
/// when no pseudo in `STYLE_ORDER` matches.
fn pseudo_score(selector: &str) -> usize {
    let trimmed = selector.trim();
    for (i, pseudo) in STYLE_ORDER.iter().enumerate() {
        if trimmed.ends_with(pseudo) {
            return i + 1;
        }
    }
    0
}

/// Score for a Rule node: takes its first comma-split selector if any,
/// else the raw selector string.
fn rule_score(node: &Node) -> usize {
    match &node.kind {
        NodeKind::Rule(r) => {
            let parts = r.get_selectors();
            let first = parts.first().map(|s| s.as_str()).unwrap_or(&r.selector);
            pseudo_score(first)
        }
        // Non-Rule nodes shouldn't reach here in practice; treat as
        // unscored so they preserve their relative position.
        _ => 0,
    }
}

/// `sortPseudoSelectors(rules)` — stable, in-place sort.
pub fn sort_pseudo_selectors(rules: &mut [Node]) {
    rules.sort_by(|a, b| rule_score(a).cmp(&rule_score(b)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn collect_selectors(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match &n.kind {
                NodeKind::Rule(r) => r.selector.clone(),
                _ => String::new(),
            })
            .collect()
    }

    fn parse_rules(css: &str) -> Vec<Node> {
        let root = parse(css).unwrap();
        root.root
            .nodes()
            .map(|v| v.iter().filter(|n| matches!(n.kind, NodeKind::Rule(_))).cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn lvfha_ordering() {
        let mut rules = parse_rules(
            ".a:hover{}\n.b:visited{}\n.c:active{}\n.d:link{}\n.e:focus{}\n.f:focus-visible{}\n.g:focus-within{}",
        );
        sort_pseudo_selectors(&mut rules);
        let selectors = collect_selectors(&rules);
        // L V Fw F Fv H A
        assert_eq!(
            selectors,
            vec![
                ".d:link",
                ".b:visited",
                ".g:focus-within",
                ".e:focus",
                ".f:focus-visible",
                ".a:hover",
                ".c:active",
            ]
        );
    }

    #[test]
    fn unscored_first_then_pseudos() {
        let mut rules = parse_rules(".a:hover{}\n.b{}\n.c:active{}");
        sort_pseudo_selectors(&mut rules);
        let selectors = collect_selectors(&rules);
        // `.b` (score 0) first, then `.a:hover` (score 6), then `.c:active` (score 7).
        assert_eq!(selectors, vec![".b", ".a:hover", ".c:active"]);
    }

    #[test]
    fn ties_preserve_input_order() {
        let mut rules = parse_rules(".a:hover{}\n.b:hover{}\n.c:hover{}");
        sort_pseudo_selectors(&mut rules);
        let selectors = collect_selectors(&rules);
        assert_eq!(selectors, vec![".a:hover", ".b:hover", ".c:hover"]);
    }

    #[test]
    fn picks_first_comma_selector() {
        // `.a:hover, .b:active` — the first selector `.a:hover` scores 6.
        let mut rules = parse_rules(".a:hover, .b:active{}\n.c:active{}");
        sort_pseudo_selectors(&mut rules);
        let selectors = collect_selectors(&rules);
        assert_eq!(selectors[0], ".a:hover, .b:active");
        assert_eq!(selectors[1], ".c:active");
    }
}
