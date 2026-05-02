//! crates/postcss-discard-duplicates
//! Byte-for-byte Rust port of `postcss-discard-duplicates@6.0.0`.
//!
//! Used by `packages/css/src/sort.ts:2` (the `sort()` entry point).
//! Distinct from:
//! - `postcss-discard-duplicates@5.1.0` — pulled in transitively by
//!   `cssnano-preset-default@5.2.14` but filtered out before execution
//!   (see `PARITY_VERSIONS.md` Anomaly #5).
//! - The LOCAL `discard-duplicates` plugin
//!   (`crates/compiled-css/src/plugins/discard_duplicates.rs`) which
//!   only dedupes top-level decls.
//!
//! ## Equality semantics
//!
//! `equals(a, b)` upstream walks the AST recursively:
//! - same `type`
//! - same `important`
//! - per kind:
//!   - rule: `selector` matches.
//!   - atrule: `name` AND `params` match, plus `trim(raws.before)` and
//!     `trim(raws.afterName)` match.
//!   - decl: `prop` AND `value` match, plus `trim(raws.before)` matches.
//! - if both have `nodes`, recursively equal.
//!
//! ## Two dedupe variants
//!
//! - `dedupeRule(last, nodes)` — when two RULES at the same depth share
//!   a selector, decls inside the LATER rule scan back through earlier
//!   rules with the same selector; matching decls are REMOVED from the
//!   earlier rule. Earlier rules with no non-comment children left are
//!   removed entirely.
//! - `dedupeNode(last, nodes)` — for atrules and top-level decls: any
//!   earlier sibling that `equals` `last` is removed.

use postcss_core::container::remove_at;
use postcss_core::{Node, NodeKind, PluginResult, Root};

pub fn postcss_discard_duplicates(root: &mut Root) -> PluginResult {
    dedupe(&mut root.root);
    Ok(())
}

/// Mirrors upstream `dedupe(root)`. Iterates children right-to-left;
/// recursive descent first, then per-kind dedupe.
fn dedupe(parent: &mut Node) {
    if parent.nodes().is_none() {
        return;
    }
    let mut index: isize = parent.nodes().map(|n| n.len() as isize - 1).unwrap_or(-1);
    while index >= 0 {
        let i = index as usize;
        let cur_len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= cur_len {
            index -= 1;
            continue;
        }
        // Snapshot the identity of `parent.nodes[i]` before recursing /
        // dispatching. Upstream's `if (!last || !last.parent) continue;`
        // (`src/index.js:141`) skips the iteration when `last` was
        // detached during a sibling's processing. Today neither
        // `dedupe_rule` nor `dedupe_node` removes `parent.nodes[last_idx]`
        // (only earlier siblings), so this never trips — but a
        // `debug_assert!` makes the invariant fail loudly if a future
        // edit (or a refactor of `remove_at`'s call sites) breaks it,
        // instead of silently dispatching against a stale `i`.
        #[cfg(debug_assertions)]
        let snapshot_ptr = parent.nodes().unwrap().get(i).map(|n| n as *const Node);
        // Recurse into the child first.
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            dedupe(child);
        }
        #[cfg(debug_assertions)]
        {
            let after_ptr = parent.nodes().and_then(|n| n.get(i)).map(|n| n as *const Node);
            debug_assert_eq!(
                snapshot_ptr, after_ptr,
                "dedupe: parent.nodes[{i}] mutated during recursive descent — \
                 upstream's `!last.parent` guard would skip this iteration."
            );
        }
        let kind = match parent.nodes().unwrap().get(i) {
            Some(n) => kind_tag(n),
            None => { index -= 1; continue; }
        };
        match kind {
            KindTag::Rule => dedupe_rule(parent, i),
            KindTag::AtRule | KindTag::Decl => dedupe_node(parent, i),
            _ => {}
        }
        index -= 1;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KindTag { Root, Rule, AtRule, Decl, Comment }

fn kind_tag(n: &Node) -> KindTag {
    match &n.kind {
        NodeKind::Root(_) => KindTag::Root,
        NodeKind::Rule(_) => KindTag::Rule,
        NodeKind::AtRule(_) => KindTag::AtRule,
        NodeKind::Declaration(_) => KindTag::Decl,
        NodeKind::Comment(_) => KindTag::Comment,
    }
}

/// `dedupeRule(last, nodes)` upstream.
fn dedupe_rule(parent: &mut Node, last_idx: usize) {
    // Snapshot last's selector + decl children for the comparison loop.
    let (last_selector, last_decls) = {
        let last = match parent.nodes().and_then(|n| n.get(last_idx)) {
            Some(n) => n,
            None => return,
        };
        let last_rule = match &last.kind {
            NodeKind::Rule(r) => r,
            _ => return,
        };
        let decls: Vec<Node> = last_rule.nodes
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::Declaration(_)))
            .cloned()
            .collect();
        (last_rule.selector.clone(), decls)
    };

    let mut index: isize = last_idx as isize - 1;
    while index >= 0 {
        let i = index as usize;
        let same_selector = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => matches!(&n.kind, NodeKind::Rule(r) if r.selector == last_selector),
            None => false,
        };
        if !same_selector {
            index -= 1;
            continue;
        }

        // JS `last.each((child) => …)` (`src/index.js:95-99`) iterates
        // `last.nodes` LIVE; mid-iteration mutation of `last.nodes` would
        // shift `each`'s index and visit a different set than the snapshot
        // we built above. The current call graph never mutates `last.nodes`
        // (the inner `dedupeNode(child, node.nodes)` only touches the
        // EARLIER rule's body), so snapshot vs live agree. The
        // `debug_assert_eq!` below traps any future plugin reordering or
        // call-graph change that violates this invariant in dev/CI.
        #[cfg(debug_assertions)]
        let last_nodes_len_before: usize = match parent.nodes().and_then(|n| n.get(last_idx)) {
            Some(n) => match &n.kind {
                NodeKind::Rule(r) => r.nodes.len(),
                _ => 0,
            },
            None => 0,
        };

        // For each decl in `last`, walk earlier-rule's body right-to-left.
        for last_decl in &last_decls {
            let earlier_body_len = parent
                .nodes()
                .and_then(|n| n.get(i))
                .and_then(|e| e.nodes())
                .map(|b| b.len())
                .unwrap_or(0);
            let mut j: isize = earlier_body_len as isize - 1;
            while j >= 0 {
                let jj = j as usize;
                let matched = {
                    let earlier_node = parent.nodes().and_then(|n| n.get(i));
                    let earlier_body = earlier_node.and_then(|e| e.nodes());
                    match earlier_body.and_then(|b| b.get(jj)) {
                        Some(child) => matches!(&child.kind, NodeKind::Declaration(_)) && nodes_equal(child, last_decl),
                        None => false,
                    }
                };
                if matched {
                    if let Some(earlier_mut) = parent.nodes_mut().and_then(|n| n.get_mut(i)) {
                        if let Some(body) = earlier_mut.nodes_mut() {
                            body.remove(jj);
                        }
                    }
                }
                j -= 1;
            }
        }

        #[cfg(debug_assertions)]
        {
            let last_nodes_len_after: usize = match parent.nodes().and_then(|n| n.get(last_idx)) {
                Some(n) => match &n.kind {
                    NodeKind::Rule(r) => r.nodes.len(),
                    _ => 0,
                },
                None => 0,
            };
            debug_assert_eq!(
                last_nodes_len_before, last_nodes_len_after,
                "dedupe_rule: last.nodes mutated during inner loop — \
                 the `last_decls` snapshot would diverge from JS `last.each`'s live iteration."
            );
        }

        // If earlier rule's body is now "empty" (only comments or zero
        // children), remove the earlier rule.
        //
        // Mirrors JS `if (empty(node)) node.remove();` (`src/index.js:101-103`)
        // where `empty(node) = !node.nodes.filter(c => c.type !== 'comment').length`
        // throws on `node.nodes.filter` if `node.nodes === undefined`. We
        // already gated on `Rule` via `same_selector`, so `nodes()` is
        // structurally `Some`. `.expect()` documents the invariant —
        // dispatching to `unwrap_or(false)` would silently treat a
        // malformed node as "non-empty" instead of mirroring JS's crash.
        let earlier_now_empty = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => {
                let body = n.nodes().expect(
                    "postcss-discard-duplicates: dedupe_rule — earlier sibling \
                     identified as Rule by same_selector check must have nodes(). \
                     Upstream JS would TypeError on `empty(node)` here.",
                );
                body.iter().all(|c| matches!(c.kind, NodeKind::Comment(_)))
            }
            None => false,
        };
        if earlier_now_empty {
            // Use `remove_at` so the Root-specific raws-transfer
            // (`postcss/lib/root.js::removeChild`) fires when removing
            // the first child of root.
            remove_at(parent, i);
        }
        index -= 1;
    }
}

/// `dedupeNode(last, nodes)` — remove any earlier sibling that
/// `equals` last.
fn dedupe_node(parent: &mut Node, last_idx: usize) {
    let last = match parent.nodes().and_then(|n| n.get(last_idx)).cloned() {
        Some(n) => n,
        None => return,
    };
    let mut index: isize = last_idx as isize - 1;
    while index >= 0 {
        let i = index as usize;
        let candidate_matches = match parent.nodes().and_then(|n| n.get(i)) {
            Some(n) => nodes_equal(n, &last),
            None => false,
        };
        if candidate_matches {
            // Use `remove_at` for the Root.removeChild override.
            remove_at(parent, i);
        }
        index -= 1;
    }
}

/// `equals(a, b)` upstream — deep recursive equality.
///
/// **Pure-AST invariant.** This function reads `kind`, `raws.before`,
/// `raws.after_name`, and `Declaration.important`/`prop`/`value`/
/// `Rule.selector`/`AtRule.name`/`AtRule.params` only. It does NOT
/// read `node.attrs` (the per-node attribute bag). Callers in this
/// module (`dedupe_rule`, `dedupe_node`) snapshot some operands by
/// `clone()` and compare the snapshot against live siblings; the
/// `attrs` field on the snapshot would be frozen relative to the
/// live one. Today this is safe because `nodes_equal` ignores `attrs`.
/// If a future change ever incorporates `attrs` into equality (e.g.
/// some plugin-specific tagging that affects dedupe), the
/// snapshot-and-compare pattern in `dedupe_rule` / `dedupe_node`
/// breaks: clones would compare frozen attrs against potentially-
/// mutated live attrs. The `debug_assert!` below traps that drift in
/// dev/CI.
fn nodes_equal(a: &Node, b: &Node) -> bool {
    // Load-bearing invariant: `attrs` MUST NOT influence equality, or
    // the clone+compare pattern in `dedupe_rule` / `dedupe_node`
    // becomes unsound. Guard fires only when `attrs` is non-empty AND
    // the rest of equality would otherwise return `true`.
    debug_assert!(
        a.attrs.is_empty() && b.attrs.is_empty(),
        "postcss-discard-duplicates::nodes_equal — `attrs` must not \
         participate in equality. dedupe_rule / dedupe_node clone operands \
         and compare against live siblings; if `attrs` ever becomes \
         load-bearing here, those snapshots will diverge from live state."
    );
    if kind_tag(a) != kind_tag(b) {
        return false;
    }
    // JS `a.important !== b.important` (`src/index.js:30-32`).
    //
    // Tristate collapse: JS `Declaration.important` can be `true`,
    // `false`, or `undefined`. Rust collapses to `bool`, mapping
    // `undefined` and `false` to the same value (`false`). The parser
    // only ever emits `true` (when `!important` is present) or
    // `undefined` (when absent) — verified by a node_modules-wide grep
    // for `.important = false` / `important: false` (zero hits across
    // every JS plugin AFM consumes). If a future plugin port ever sets
    // `Declaration.important = false` deliberately, this collapse
    // becomes load-bearing and the field must widen to `Option<bool>`.
    let a_important = matches!(&a.kind, NodeKind::Declaration(d) if d.important);
    let b_important = matches!(&b.kind, NodeKind::Declaration(d) if d.important);
    if a_important != b_important {
        return false;
    }

    match (&a.kind, &b.kind) {
        (NodeKind::Rule(ra), NodeKind::Rule(rb)) => {
            if ra.selector != rb.selector {
                return false;
            }
        }
        (NodeKind::AtRule(aa), NodeKind::AtRule(ab)) => {
            if aa.name != ab.name || aa.params != ab.params {
                return false;
            }
            if trim_str(a.raws.before.as_deref()) != trim_str(b.raws.before.as_deref()) {
                return false;
            }
            if trim_str(a.raws.after_name.as_deref()) != trim_str(b.raws.after_name.as_deref()) {
                return false;
            }
        }
        (NodeKind::Declaration(da), NodeKind::Declaration(db)) => {
            if da.prop != db.prop || da.value != db.value {
                return false;
            }
            if trim_str(a.raws.before.as_deref()) != trim_str(b.raws.before.as_deref()) {
                return false;
            }
        }
        // Upstream `equals()` (src/index.js:38-69) has NO `comment` case in
        // its switch — two comments are considered equal as long as their
        // type matches, regardless of `text`. Do NOT add a Comment branch.
        _ => {}
    }

    // Mirrors upstream `if (a.nodes) { … a.nodes.length !== b.nodes.length … }`
    // (`src/index.js:71-81`). Upstream only guards on `a.nodes`; if `b.nodes`
    // is undefined while `a.nodes` is defined, JS throws `TypeError` on
    // `b.nodes.length`. The asymmetric case is reachable in real CSS via two
    // atrules with the same `name` + `params` but different block-form
    // (e.g. `@foo bar { }` vs `@foo bar;`), which `Node::nodes()` reports
    // as `Some([…])` vs `None` because `AtRule.has_block` differs.
    //
    // Mirror JS verbatim: when `a.nodes()` is `Some` and `b.nodes()` is
    // `None`, panic instead of silently returning `true`. A silent
    // return would mis-dedupe the block atrule against the statement
    // atrule and produce divergent bytes vs JS (which would crash the
    // pipeline). Loud failure is the byte-equal mirror of JS's TypeError.
    let a_nodes = a.nodes();
    let b_nodes = b.nodes();
    if let Some(an) = a_nodes {
        let bn = b_nodes.expect(
            "postcss-discard-duplicates: equals(a, b) — a has nodes but b does not. \
             Upstream JS would throw TypeError on b.nodes.length here. \
             Most likely two atrules with same name+params but different has_block.",
        );
        if an.len() != bn.len() {
            return false;
        }
        for (ca, cb) in an.iter().zip(bn.iter()) {
            if !nodes_equal(ca, cb) {
                return false;
            }
        }
    }
    true
}

/// `trimValue(value)` upstream (`src/index.js:6-8`) — `value ? value.trim() : value`.
///
/// Uses ECMAScript `String.prototype.trim()` semantics (WhiteSpace +
/// LineTerminator per ECMA-262). Rust's `str::trim()` strips Unicode
/// `White_Space`, which DIFFERS at:
///   - U+0085 (NEL): in Rust `White_Space`, NOT in ECMA WhiteSpace.
///   - U+FEFF (BOM/ZWNBSP): in ECMA WhiteSpace, NOT in Rust `White_Space`.
/// Mismatch would let a `raws.before` containing either codepoint diverge
/// equality between JS and Rust. Use a hand-rolled predicate instead.
fn trim_str(s: Option<&str>) -> Option<String> {
    s.map(|v| v.trim_matches(is_ecma_whitespace).to_string())
}

fn is_ecma_whitespace(c: char) -> bool {
    matches!(
        c,
        // ECMA WhiteSpace
        '\u{0009}'    // TAB
        | '\u{000B}'  // VT
        | '\u{000C}'  // FF
        | '\u{0020}'  // SPACE
        | '\u{00A0}'  // NBSP
        | '\u{FEFF}'  // ZWNBSP / BOM
        // ECMA LineTerminator
        | '\u{000A}'  // LF
        | '\u{000D}'  // CR
        | '\u{2028}'  // LS
        | '\u{2029}'  // PS
        // Unicode general category Zs (Space_Separator)
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_discard_duplicates(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn drops_duplicate_top_level_decl() {
        let out = run("color: red; color: red;");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn keeps_distinct_top_level_decls() {
        let css = "color: red;\nbackground: blue;";
        assert_eq!(run(css), css);
    }

    #[test]
    fn drops_duplicate_at_rule() {
        let out = run(
            "@media (max-width: 100px) { a { color: red; } }\n@media (max-width: 100px) { a { color: red; } }",
        );
        assert_eq!(out.matches("@media (max-width: 100px)").count(), 1);
    }

    #[test]
    fn keeps_distinct_at_rules() {
        let css = "@media (max-width: 100px) { a {} }\n@media (max-width: 200px) { a {} }";
        let out = run(css);
        assert_eq!(out.matches("@media").count(), 2);
    }

    #[test]
    fn rule_merge_dedupes_overlapping_decls() {
        let out = run("a { color: red; background: blue; } a { color: red; }");
        assert!(out.contains("background: blue"));
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn rule_emptied_after_decl_dedupe_is_removed() {
        let out = run("a { color: red; } a { color: red; }");
        assert_eq!(out.matches("a {").count(), 1, "got: {out:?}");
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn no_op_when_no_duplicates() {
        let css = "a { color: red; }\nb { color: blue; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn nested_dedupe_recurses() {
        let out = run("@media (min-width: 100px) { color: red; color: red; }");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn dedupe_three_consecutive_decls() {
        let out = run("color: red; color: red; color: red;");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    // Upstream `equals()` does NOT compare comment text — see src/index.js:38-69
    // (no `comment` case in the switch). Two atrules whose only difference is
    // the body of an inner comment are considered equal by JS, so dedupe must
    // remove the earlier one. The Rust port previously diverged here.
    #[test]
    fn atrule_inner_comment_text_is_ignored_in_equality() {
        let out = run(
            "@media (min-width:100px){/* a */color:red}@media (min-width:100px){/* b */color:red}",
        );
        assert_eq!(out.matches("@media").count(), 1, "got: {out:?}");
    }

    #[test]
    fn rule_inner_comment_text_is_ignored_in_equality() {
        // dedupeRule snapshots `last`'s decls and runs dedupeNode against the
        // earlier rule's body for each. The earlier rule loses the matching
        // decl; if its only remaining children are comments, `empty(node)`
        // (src/index.js:14-16) is truthy and the rule is removed entirely.
        let out = run("a { /* keep */ color: red; } a { color: red; }");
        assert_eq!(out.matches("a {").count(), 1, "got: {out:?}");
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    // `trim_str` mirrors JS `String.prototype.trim()` (ECMA-262 WhiteSpace +
    // LineTerminator). U+FEFF (BOM) IS whitespace to JS but NOT to Rust's
    // default `is_whitespace`; U+0085 (NEL) is whitespace to Rust but NOT to
    // JS. Both must be handled identically here.
    #[test]
    fn js_trim_strips_bom_zwnbsp() {
        assert_eq!(trim_str(Some("\u{FEFF}foo\u{FEFF}")).unwrap(), "foo");
        assert_eq!(trim_str(Some("\u{FEFF}\u{FEFF}")).unwrap(), "");
    }

    #[test]
    fn js_trim_does_not_strip_nel_u0085() {
        // Rust's str::trim() WOULD strip U+0085. JS does NOT.
        let trimmed = trim_str(Some("\u{0085}foo\u{0085}")).unwrap();
        assert_eq!(trimmed, "\u{0085}foo\u{0085}");
    }

    #[test]
    fn js_trim_strips_zs_category() {
        // U+2003 is "EM SPACE" — Zs category. Both Rust and JS strip these.
        assert_eq!(trim_str(Some("\u{2003}foo\u{2003}")).unwrap(), "foo");
    }

    // Mirror JS `equals(a, b)` when `a.nodes` is defined and `b.nodes` is
    // not — JS would `b.nodes.length` → TypeError. The Rust port now panics
    // with a descriptive message instead of silently returning `true` (the
    // pre-fix behavior would have wrongly deduped a block atrule against
    // a statement atrule with the same name + params).
    #[test]
    #[should_panic(expected = "a has nodes but b does not")]
    fn equals_panics_on_asymmetric_nodes_a_block_b_statement() {
        use postcss_core::at_rule::AtRule;
        use postcss_core::node::{Node as PNode, NodeKind as PKind};

        let a = PNode::new(PKind::AtRule(AtRule {
            name: "foo".to_string(),
            params: "bar".to_string(),
            has_block: true, // block form: nodes() -> Some(_)
            nodes: vec![],
        }));
        let b = PNode::new(PKind::AtRule(AtRule {
            name: "foo".to_string(),
            params: "bar".to_string(),
            has_block: false, // statement form: nodes() -> None
            nodes: vec![],
        }));
        let _ = nodes_equal(&a, &b);
    }

    // Reverse direction: a.nodes is None, b.nodes is Some. JS skips the
    // recursion entirely (no crash) and returns true. Rust must do the
    // same — DO NOT panic here.
    #[test]
    fn equals_returns_true_on_asymmetric_nodes_a_statement_b_block() {
        use postcss_core::at_rule::AtRule;
        use postcss_core::node::{Node as PNode, NodeKind as PKind};

        let a = PNode::new(PKind::AtRule(AtRule {
            name: "foo".to_string(),
            params: "bar".to_string(),
            has_block: false,
            nodes: vec![],
        }));
        let b = PNode::new(PKind::AtRule(AtRule {
            name: "foo".to_string(),
            params: "bar".to_string(),
            has_block: true,
            nodes: vec![],
        }));
        // JS: `if (a.nodes)` is false (a.nodes undefined) → skip recursion → true.
        assert!(nodes_equal(&a, &b));
    }
}
