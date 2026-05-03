//! Port of `packages/css/src/plugins/sort-atomic-style-sheet.ts`.
//!
//! Upstream JS does (in `Once`):
//!
//! ```text
//! 1. Bucket each top-level node into one of three lists:
//!      catchAll  — comments, declarations
//!      rules     — Rule whose first child is NOT an AtRule
//!      atRules   — AtRule (or Rule whose first child IS an AtRule)
//! 2. If sortShorthandEnabled (default true):
//!      sortShorthandDeclarations(catchAll)
//!      sortShorthandDeclarations(rules)
//!      sortShorthandDeclarations(atRules.map(.node))
//! 3. sortPseudoSelectors(rules)              // LVFHA stable sort
//! 4. If sortAtRulesEnabled (default true):
//!      atRules.sort(sortAtRules)            // by name + parsed media-query
//! 5. For each AtRule node: sortAtRulePseudoSelectors(node)  // recursive
//! 6. root.nodes = [...catchAll, ...rules, ...atRules.map(.node)]
//! ```
//!
//! The recursive `sortAtRulePseudoSelectors` walks an at-rule body:
//! - For each AtRule child: recurse.
//! - For each Rule child: clone, push to local rules list, remove original.
//! - After all children visited: `sortPseudoSelectors(rules)` then
//!   re-append each rule to the at-rule.
//!
//! ## Stable sort guarantees
//! Both `sort_pseudo_selectors` and `sort_shorthand_declarations` are
//! stable. The bucket order at step 6 (catchAll → rules → atRules) is
//! the explicit ordering — bucketing is iteration-order preserving, so
//! within each bucket the original sequence is kept until the
//! sort_by call rearranges it.

use postcss_core::{Node, NodeKind, PluginResult, Root};

use super::at_rules::parse_at_rule::parse_at_rule;
use super::at_rules::sort_at_rules::sort_at_rules;
use super::at_rules::types::{AtRuleInfo, ParsedAtRule};
use super::sort_shorthand_declarations::sort_shorthand_declarations;
use crate::utils::sort_pseudo_selectors::sort_pseudo_selectors;

#[derive(Debug, Clone, Default)]
pub struct SortAtomicStyleSheetOpts {
    /// `undefined` upstream means "use plugin default" — we mirror with
    /// `Option<bool>`. Default values live in the plugin port itself, not
    /// at the call site (matches upstream comment in `sort.ts:18-26`).
    pub sort_at_rules_enabled: Option<bool>,
    pub sort_shorthand_enabled: Option<bool>,
}

pub fn sort_atomic_style_sheet(root: &mut Root, opts: &SortAtomicStyleSheetOpts) -> PluginResult {
    let sort_at_rules_enabled = opts.sort_at_rules_enabled.unwrap_or(true);
    let sort_shorthand_enabled = opts.sort_shorthand_enabled.unwrap_or(true);

    // Drain root children for bucketing — we'll rebuild the list at the
    // end to match upstream's `root.nodes = [...catchAll, ...rules, ...]`.
    let nodes = std::mem::take(root.root.nodes_mut().unwrap());

    let mut catch_all: Vec<Node> = Vec::new();
    let mut rules: Vec<Node> = Vec::new();
    let mut at_rules: Vec<AtRuleInfo> = Vec::new();

    for node in nodes {
        match &node.kind {
            NodeKind::Rule(_) => {
                // Special-case rules whose FIRST child is an AtRule —
                // treat the whole node as if it were the at-rule for
                // sorting purposes. Mirrors upstream
                // `node.first?.type === 'atrule'`.
                let first_kind = node.nodes().and_then(|c| c.first()).map(|c| c.kind.clone());
                if let Some(NodeKind::AtRule(at)) = first_kind {
                    // 0.19.0: parseAtRule runs on ANY at-rule when
                    // sortAtRulesEnabled — not gated by name == "media".
                    let parsed = if sort_at_rules_enabled {
                        parse_at_rule(&at.params)
                    } else {
                        Vec::<ParsedAtRule>::new()
                    };
                    at_rules.push(AtRuleInfo {
                        parsed,
                        at_rule_name: at.name.clone(),
                        query: at.params.clone(),
                        node,
                    });
                } else {
                    rules.push(node);
                }
            }
            NodeKind::AtRule(at) => {
                let parsed = if sort_at_rules_enabled {
                    parse_at_rule(&at.params)
                } else {
                    Vec::<ParsedAtRule>::new()
                };
                let name = at.name.clone();
                let query = at.params.clone();
                at_rules.push(AtRuleInfo {
                    parsed,
                    at_rule_name: name,
                    query,
                    node,
                });
            }
            // Decl, Comment, Root (shouldn't appear at root level) → catchAll.
            _ => catch_all.push(node),
        }
    }

    // Step 2 — shorthand sort within each bucket.
    if sort_shorthand_enabled {
        sort_shorthand_declarations(&mut catch_all);
        sort_shorthand_declarations(&mut rules);
        // For at-rules we sort the wrapper Vec<Node> (not the AtRuleInfo),
        // matching upstream `atRules.map((atRule) => atRule.node)`. We
        // need to extract → sort → reinject without losing the parsed
        // data carried by AtRuleInfo. Pull out node refs, sort by index.
        let mut at_rule_nodes: Vec<Node> = at_rules.iter().map(|a| a.node.clone()).collect();
        sort_shorthand_declarations(&mut at_rule_nodes);
        // Rebuild `at_rules` in the new node order. We need to match
        // each sorted Node back to its original AtRuleInfo.
        let mut new_at_rules: Vec<AtRuleInfo> = Vec::with_capacity(at_rules.len());
        for sorted in at_rule_nodes {
            // Linear search is fine — atomic CSS at-rule lists are small
            // (dozens, not thousands), and equality is by stringified
            // body which we already use as a stable identity in
            // merge-duplicate-at-rules.
            let pos = at_rules
                .iter()
                .position(|info| postcss_core::stringify_node(&info.node)
                    == postcss_core::stringify_node(&sorted))
                .unwrap_or(0);
            let info = at_rules.remove(pos);
            // Replace the cloned-then-sorted body back into the AtRuleInfo
            // (so any inner-mutation by sort_shorthand_declarations
            // survives).
            new_at_rules.push(AtRuleInfo { node: sorted, ..info });
        }
        at_rules = new_at_rules;
    }

    // Step 3 — pseudo-selector sort on regular rules.
    sort_pseudo_selectors(&mut rules);

    // Step 4 — sort at-rules by parsed media query / name.
    if sort_at_rules_enabled {
        at_rules.sort_by(sort_at_rules);
    }

    // Step 5 — recurse into each true AtRule to sort inner rules.
    for info in at_rules.iter_mut() {
        if matches!(info.node.kind, NodeKind::AtRule(_)) {
            sort_at_rule_pseudo_selectors(&mut info.node);
        }
    }

    // Step 6 — reassemble.
    let nodes_mut = root.root.nodes_mut().unwrap();
    nodes_mut.clear();
    nodes_mut.extend(catch_all);
    nodes_mut.extend(rules);
    for info in at_rules {
        nodes_mut.push(info.node);
    }

    Ok(())
}

/// `sortAtRulePseudoSelectors(atRule)` upstream — recursive helper that
/// re-sorts inner rules of an at-rule. Mirrors:
/// ```ts
/// const sortAtRulePseudoSelectors = (atRule) => {
///   const rules = [];
///   atRule.each((childNode) => {
///     switch (childNode.type) {
///       case 'atrule': sortAtRulePseudoSelectors(childNode); break;
///       case 'rule':   rules.push(childNode.clone()); childNode.remove(); break;
///       default: break;
///     }
///   });
///   sortPseudoSelectors(rules);
///   rules.forEach((rule) => atRule.append(rule));
/// };
/// ```
fn sort_at_rule_pseudo_selectors(parent: &mut Node) {
    if parent.nodes().is_none() {
        return;
    }
    // Snapshot rule indices in iteration order; recurse into nested
    // at-rules in place.
    let mut rules: Vec<Node> = Vec::new();
    let mut i = 0usize;
    loop {
        let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }
        match &parent.nodes().unwrap()[i].kind {
            NodeKind::AtRule(_) => {
                let nodes_mut = parent.nodes_mut().unwrap();
                sort_at_rule_pseudo_selectors(&mut nodes_mut[i]);
                i += 1;
            }
            NodeKind::Rule(_) => {
                let cloned = parent.nodes().unwrap()[i].clone();
                rules.push(cloned);
                parent.nodes_mut().unwrap().remove(i);
                // Don't advance — next sibling slid down.
            }
            _ => {
                i += 1;
            }
        }
    }

    sort_pseudo_selectors(&mut rules);
    let body = parent.nodes_mut().unwrap();
    for rule in rules {
        body.push(rule);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        sort_atomic_style_sheet(
            &mut root,
            &SortAtomicStyleSheetOpts {
                sort_at_rules_enabled: None,
                sort_shorthand_enabled: None,
            },
        )
        .unwrap();
        stringify(&root)
    }

    #[test]
    fn at_rules_move_to_bottom() {
        let out = run(
            "@media screen { .x { color: red; } }\n.y { color: blue; }",
        );
        let media_pos = out.find("@media").unwrap();
        let y_pos = out.find(".y").unwrap();
        assert!(y_pos < media_pos, "expected `.y` before `@media`, got: {out:?}");
    }

    #[test]
    fn lvfha_ordering_top_level() {
        let out = run(
            ".a:hover { color: red; }\n\
             .b:focus { color: pink; }\n\
             .c:active { color: white; }\n\
             .d:link { color: purple; }\n\
             .e:visited { color: pink; }\n\
             .f:focus-visible { color: black; }\n\
             .g:focus-within { color: black; }\n\
             .h:first-child { color: grey; }\n\
             .i { color: blue; }",
        );
        // Expected order (top-level rules; LVFHA after unscored).
        let positions = [".h:first-child", ".i", ".d:link", ".e:visited", ".g:focus-within", ".b:focus", ".f:focus-visible", ".a:hover", ".c:active"];
        let mut last = 0usize;
        for sel in positions {
            let p = out.find(sel).unwrap_or_else(|| panic!("missing {sel} in {out}"));
            assert!(p >= last, "out-of-order at {sel}: {out:?}");
            last = p;
        }
    }

    #[test]
    fn shorthand_sorting_within_rules() {
        let out = run(
            ".a { outline-width: 1px; }\n\
             .b { all: unset; }\n\
             .c { border: none; }",
        );
        // `.b` (all=0) → `.c` (border=1) → `.a` (outline-width=Inf).
        let pa = out.find(".a").unwrap();
        let pb = out.find(".b").unwrap();
        let pc = out.find(".c").unwrap();
        assert!(pb < pc && pc < pa, "got: {out:?}");
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn preserves_simple_single_rule() {
        let css = ".a { color: red; }";
        // sort plugin with one rule, no at-rules, no shorthands → no change.
        assert_eq!(run(css), css);
    }

    /// Regression for Phase 8b NAPI drift §2 — when top-level Comment
    /// nodes interleave with top-level Decl nodes in the catchAll
    /// bucket, V8's `Array.prototype.sort` (binary-insertion-sort
    /// branch) reorders decls by shorthand bucket while ALSO moving
    /// trailing comments past the shorter shorthand-bucket decl. The
    /// JS oracle output for the failing fixture
    /// `crates/parity-runner/corpus/transform-css/22_comments_at_positions.css`
    /// is what we assert here verbatim.
    #[test]
    fn comment_interleave_with_top_level_decls() {
        let input =
            "/* leading */\ncolor: red;\n/* between */\nbackground: blue;\n/* trailing */\n";
        let expected =
            "/* leading */\nbackground: blue;\ncolor: red;\n/* between */\n/* trailing */\n";
        assert_eq!(run(input), expected, "actual: {:?}", run(input));
    }

    /// Tighter follow-up: every observed V8 small-array result for
    /// the catchAll permutations we trace in
    /// `PHASE_8B_NAPI_NOTES.md` § "Drift detected" §2. Locks in the
    /// V8-parity binary-insertion-sort behaviour at the
    /// sort_atomic_style_sheet level.
    #[test]
    fn comment_interleave_v8_parity_table() {
        // [c, color, bg, c]
        assert_eq!(
            run("/* a */\ncolor: red;\nbackground: blue;\n/* b */\n"),
            "/* a */\nbackground: blue;\ncolor: red;\n/* b */\n"
        );
        // [c, c, color, bg]  →  comments stay, decls reorder.
        assert_eq!(
            run("/* a */\n/* b */\ncolor: red;\nbackground: blue;\n"),
            "/* a */\n/* b */\nbackground: blue;\ncolor: red;\n"
        );
        // [c, color, c, bg, c, all]  →  all bucket-0 first, comments shoved to end.
        assert_eq!(
            run("/* a */\ncolor: red;\n/* b */\nbackground: blue;\n/* c */\nall: unset;\n"),
            "/* a */\nall: unset;\nbackground: blue;\ncolor: red;\n/* b */\n/* c */\n"
        );
    }
}
