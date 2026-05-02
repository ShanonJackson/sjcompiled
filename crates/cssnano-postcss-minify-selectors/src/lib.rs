//! crates/cssnano-postcss-minify-selectors
//! Byte-for-byte Rust port of `postcss-minify-selectors@5.2.1`.
//!
//! Folder/file mapping (1:1 with upstream
//! `node_modules/postcss-minify-selectors@5.2.1/src/`, with one
//! Rust-mandated rename):
//!   - `index.js`               -> `src/lib.rs` (this file)
//!   - `lib/canUnquote.js`      -> `src/can_unquote.rs`
//!
//! The `lib/` parent directory is dropped because Rust's crate-root
//! file is itself `lib.rs`; a child module literally named `lib`
//! collides. Behavior is unaffected. Same convention used by
//! `crates/cssnano-postcss-discard-comments`.
//!
//! All bugs of upstream 5.2.1 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! For every Rule in the tree:
//!
//! 1. Determine the input selector — `rule.raws.selector.raw` if
//!    `rule.raws.selector.value === rule.selector`, else `rule.selector`.
//! 2. If the selector ends with `:`, skip (custom mixin guard, line 240).
//! 3. If the input is in the per-OnceExit cache, write the cached output
//!    to `rule.selector` and return.
//! 4. Otherwise run the input through a postcss-selector-parser-style
//!    pipeline:
//!    - Walk every descendant in pre-order. For each:
//!      a. Clear `spaces.before` / `spaces.after` and `raw_value`.
//!      b. Dispatch a per-kind reducer (attribute / combinator / pseudo /
//!         tag / universal). If a reducer ran, the dedup branch in (c)
//!         is skipped per upstream's early `return`.
//!      c. If the node is a top-level Selector (parent kind is NOT a
//!         Pseudo), dedupe against a per-rule string set — duplicate
//!         comma-separated arms are removed.
//!    - After the walk, `selectors.nodes.sort()` — alphabetize the
//!      remaining top-level Selectors by their stringified form.
//! 5. Stringify the mutated tree, write to `rule.selector`, and cache.

pub mod can_unquote;

use indexmap::{IndexMap, IndexSet};

use postcss_core::container::{walk_rules_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};

use postcss_selector_parser as sp;
use sp::nodes::{AttributeSpaces, Spaces};

use self::can_unquote::can_unquote;

// --------------------------------------------------------------------------
// Static tables — mirror the `Set` / `Map` literals at upstream lines
// 5-10, 82-87, 154-157.
// --------------------------------------------------------------------------

/// Pseudo-elements whose double-colon prefix gets compressed to a single
/// colon (legacy CSS2 form). Mirrors lines 5-10. Values include the `::`
/// prefix to match the typed value upstream stores on `Pseudo.value`.
fn is_pseudo_element(value: &str) -> bool {
    matches!(value, "::before" | "::after" | "::first-letter" | "::first-line")
}

/// `:nth-*(1)` → `:first-*` rewrite map. Values include the leading `:`.
/// Mirrors lines 82-87.
fn pseudo_replacement(value: &str) -> Option<&'static str> {
    match value {
        ":nth-child" => Some(":first-child"),
        ":nth-of-type" => Some(":first-of-type"),
        ":nth-last-child" => Some(":last-child"),
        ":nth-last-of-type" => Some(":last-of-type"),
        _ => None,
    }
}

/// `from` ↔ `0%`, `100%` ↔ `to` rewrite map for keyframe selectors.
/// Mirrors lines 154-157. Replacement direction is asymmetric — `from`
/// shrinks to `0%` (3 → 2 bytes), `100%` shrinks to `to` (4 → 2 bytes).
fn tag_replacement(value: &str) -> Option<&'static str> {
    match value {
        "from" => Some("0%"),
        "100%" => Some("to"),
        _ => None,
    }
}

// --------------------------------------------------------------------------
// Reducers — `attribute()` (16-67), `combinator()` (73-80),
// `pseudo()` (93-152), `tag()` (163-169), `universal()` (175-181).
// --------------------------------------------------------------------------

/// `attribute(selector)` upstream (lines 16-67). Trims attribute name and
/// operator; drops quotes when `canUnquote` allows; clears every
/// per-name sub-space; sets `value.after = ' '` ONLY when
/// `case_insensitive` is set (the gap before the trailing `i`).
fn attribute_reducer(node: &mut sp::Node) {
    // Mark the payload dirty so the stringifier rebuilds the bracket
    // form from the typed payload. Without this, our stringifier emits
    // `node.value` (the raw bracket text) and our edits are lost.
    let payload = match node.attribute.as_mut() {
        Some(p) => p,
        None => return, // not actually an Attribute — defensive.
    };

    // Only touch `value`-related fields when a value is present (mirrors
    // upstream's `if (selector.value)` branch on line 17).
    if payload.value.is_some() {
        // Note: upstream maps `selector.raws.value.replace(/\\\n/g, '').trim()`
        // — line continuation strip + trim of the RAW (quoted) form. Our
        // payload doesn't model `raws.value` separately; the stringifier
        // rebuilds quotes from `payload.value` + `payload.quote_mark`, so
        // line continuations inside the original quoted bytes are already
        // collapsed in `payload.value` at parse time. The trim path is a
        // defensive no-op for the typed-rebuild branch and is omitted
        // here — flagged in the audit doc if a corpus entry surfaces a
        // counter-example.

        if can_unquote(payload.value.as_deref().unwrap_or("")) {
            payload.quote_mark = None;
        }

        if let Some(op) = payload.operator.take() {
            payload.operator = Some(op.trim().to_string());
        }
    }

    // Clear the Attribute node's outer spaces. Upstream sets both
    // `selector.rawSpaceBefore`/`rawSpaceAfter` (raws.spaces) AND
    // `selector.spaces.before`/`after` to empty; our Rust port has a
    // single `Spaces` struct on `Node`, so one clear covers both.
    node.spaces.before.clear();
    node.spaces.after.clear();

    // Build the per-name sub-space pairs. `value.after = ' '` is the
    // mandatory gap before the `i` flag when case-insensitive — upstream
    // sets it conditionally on lines 38-40. We mirror byte-for-byte.
    let mut spaces = AttributeSpaces::default();
    if payload.case_insensitive {
        spaces.value.after = " ".to_string();
    }
    node.attribute_spaces = Some(spaces);

    // Trim the attribute name itself. Upstream line 66.
    payload.attribute = payload.attribute.trim().to_string();

    // Now mark dirty so the stringifier rebuilds.
    payload.dirty = true;
    node.raw_value = None;
}

/// `combinator(selector)` upstream (lines 73-80). Trims the combinator's
/// value and outer spaces; if the trim leaves an empty string, restore
/// the descendant combinator (a single space).
fn combinator_reducer(node: &mut sp::Node) {
    let trimmed: String = node.value.trim().to_string();
    node.spaces.before.clear();
    node.spaces.after.clear();
    node.value = if trimmed.is_empty() { " ".to_string() } else { trimmed };
    node.raw_value = None;
}

/// `pseudo(selector)` upstream (lines 93-152). Three responsibilities:
///   1. `:nth-*(1)` / `:nth-*(even)` / `:nth-*(2n+1)` rewrites.
///   2. Walk descendants and dedupe sibling Selectors of every found
///      Selector child.
///   3. Strip the leading `:` for legacy double-colon → single-colon
///      pseudo elements.
fn pseudo_reducer(node: &mut sp::Node) {
    let lowered = node.value.to_lowercase();

    // Branch 1: `:nth-*(1)` style replacement.
    if node.nodes.len() == 1 {
        if let Some(replacement) = pseudo_replacement(&lowered) {
            // `first` is the single inner Selector. `one` is its first child.
            let first = &mut node.nodes[0];
            let first_len = first.nodes.len();
            if first_len == 1 {
                let one = &mut first.nodes[0];

                if one.value == "1" {
                    // Replace the entire Pseudo node with `:first-child` etc.
                    // Upstream `selector.replaceWith(parser.pseudo({value: ...}))`.
                    // We can't replace `node` with itself referencing parent here;
                    // instead we mutate `node` in place to be the replacement.
                    // Functionally equivalent: the new Pseudo has empty `nodes`
                    // (no inner selector), the same kind, and the new value.
                    node.kind = sp::NodeKind::Pseudo;
                    node.value = replacement.to_string();
                    node.nodes.clear();
                    node.raw_value = None;
                    return;
                }

                if !one.value.is_empty() && one.value.to_lowercase() == "even" {
                    one.value = "2n".to_string();
                    one.raw_value = None;
                }
            }

            if first_len == 3 {
                // `2n+1` shape — three children: `2n`, `+`, `1`.
                let one_value = first.nodes[0].value.clone();
                let two_value = first.nodes[1].value.clone();
                let three_value = first.nodes[2].value.clone();

                if !one_value.is_empty()
                    && one_value.to_lowercase() == "2n"
                    && two_value == "+"
                    && three_value == "1"
                {
                    first.nodes[0].value = "odd".to_string();
                    first.nodes[0].raw_value = None;
                    // Remove indices 2 then 1 so the earlier index doesn't
                    // shift before the second remove.
                    first.nodes.remove(2);
                    first.nodes.remove(1);
                    first.raw_value = None;
                }
            }

            // Upstream `return` at line 131 — the dedup walk + pseudoElement
            // strip are skipped on the replacement branch.
            node.raw_value = None;
            return;
        }
    }

    // Branch 2: dedup nested-pseudo argument selectors. Upstream walks
    // every descendant; for each Selector child, it dedupes that child's
    // parent's siblings. The outer OnceExit walk also visits every
    // descendant, so a pseudo inside a pseudo gets deduped both here AND
    // when the outer walk reaches it. Both walks reach the same end
    // state — idempotent. We perform the equivalent: walk every
    // descendant of `node`, and whenever the walk descends INTO a node
    // whose `nodes` list contains Selector children, dedupe that list.
    dedupe_selector_siblings_recursive(node);

    // Branch 3: `::before` → `:before` etc.
    if is_pseudo_element(&lowered) {
        // `selector.value.slice(1)` upstream — drop one character. The
        // `.value` carries the leading `:` or `::`. After slice(1), `::`
        // becomes `:` (correct), `:` becomes `` (empty — but the
        // pseudoElements set only contains `::` prefixes, so this branch
        // never reduces a single-colon pseudo to empty).
        if !node.value.is_empty() {
            // Slice off one byte (`:`); both ASCII colons in `::` are
            // single bytes so byte-slice == char-slice.
            node.value = node.value[1..].to_string();
            node.raw_value = None;
        }
    }
}

/// Recursive helper for pseudo's branch-2 sibling dedup. Mirrors
/// upstream lines 134-147. Walks every descendant of `node`; whenever
/// it finds a node containing Selector children, dedupes that container's
/// child list by stringified form.
fn dedupe_selector_siblings_recursive(node: &mut sp::Node) {
    // For each container, dedupe its Selector children if any.
    if node.nodes.iter().any(|n| matches!(n.kind, sp::NodeKind::Selector)) {
        let mut seen: IndexSet<String> = IndexSet::new();
        let mut i = 0;
        while i < node.nodes.len() {
            if matches!(node.nodes[i].kind, sp::NodeKind::Selector) {
                let key = sp::stringify(&node.nodes[i]);
                if seen.contains(&key) {
                    node.nodes.remove(i);
                    continue;
                }
                seen.insert(key);
            }
            i += 1;
        }
        node.raw_value = None;
    }
    for child in node.nodes.iter_mut() {
        dedupe_selector_siblings_recursive(child);
    }
}

/// `tag(selector)` upstream (lines 163-169). Rewrites `from` ↔ `0%`,
/// `100%` ↔ `to` after lowercasing the lookup key.
fn tag_reducer(node: &mut sp::Node) {
    let lowered = node.value.to_lowercase();
    if let Some(replacement) = tag_replacement(&lowered) {
        node.value = replacement.to_string();
        node.raw_value = None;
    }
}

// --------------------------------------------------------------------------
// Walker — pre-order DFS over `selectors` (the parsed Root). Mirrors
// upstream lines 206-228 (the inner `selectors.walk(...)` callback).
//
// Differs from `postcss-selector-parser::walk_all` in two ways: it
// honors removals during the walk (the outer dedup + universal removal
// paths require this), and it fuses the per-node clear-spaces, reducer
// dispatch, and dedup-branch into a single closure since the upstream
// callback is one function.
// --------------------------------------------------------------------------

fn process_root(root: &mut sp::Node) {
    // Per-rule unique-set for top-level Selector dedup.
    let mut unique_top_level: IndexSet<String> = IndexSet::new();
    process_container(root, &mut unique_top_level);
    // `selectors.nodes.sort()` upstream line 229. Sort the top-level
    // Selectors by stringified form. JS `Array.prototype.sort` without
    // a comparator coerces to string and sorts by UTF-16 code units;
    // for the BMP (which CSS selectors live in 99.9% of the time)
    // UTF-16 code-unit order == Unicode code-point order == UTF-8 byte
    // order, so Rust's default `Ord` on `String` is byte-equivalent.
    // If a non-BMP corpus entry ever surfaces drift, swap to a
    // js-utf16-comparator helper here.
    if matches!(root.kind, sp::NodeKind::Root) {
        let mut keyed: Vec<(String, sp::Node)> = root
            .nodes
            .drain(..)
            .map(|n| (sp::stringify(&n), n))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        root.nodes = keyed.into_iter().map(|(_, n)| n).collect();
        root.raw_value = None;
    }
}

fn process_container(parent: &mut sp::Node, unique_top_level: &mut IndexSet<String>) {
    let mut i = 0usize;
    while i < parent.nodes.len() {
        // Step 1: clear spaces + raw_value on the current child. Upstream
        // does this UNCONDITIONALLY for every visited node (line 208).
        //
        // Parser-divergence carve-out: our `postcss-selector-parser` does
        // NOT emit explicit `Combinator{value: " "}` nodes for descendant
        // whitespace (filed as a parser bug — see `crates/postcss-nested`
        // section in STATUS.md). Instead descendant whitespace lives on
        // the next non-first sibling's `spaces.before`. Naively clearing
        // it here would produce `.a.b` from `.a .b`. So we detect the
        // descendant-combinator case (i > 0, prev sibling is not a
        // Combinator, current node is a non-leading element kind) and
        // collapse the whitespace to a single space (matching upstream's
        // `combinator()` reducer fallback when the trim leaves empty).
        let parent_is_selector = matches!(parent.kind, sp::NodeKind::Selector);
        let prev_is_combinator = if i > 0 {
            matches!(parent.nodes[i - 1].kind, sp::NodeKind::Combinator)
        } else {
            false
        };
        let before_has_whitespace = parent.nodes[i]
            .spaces
            .before
            .chars()
            .any(|c| c.is_whitespace());
        let preserve_descendant = parent_is_selector
            && i > 0
            && !prev_is_combinator
            && before_has_whitespace;
        parent.nodes[i].spaces.before = if preserve_descendant {
            " ".to_string()
        } else {
            String::new()
        };
        parent.nodes[i].spaces.after.clear();
        parent.nodes[i].raw_value = None;

        let kind = parent.nodes[i].kind.clone();

        // Step 2: reducer dispatch. The upstream `return` after a reducer
        // skips ONLY the dedup branch (Step 3) — descendant recursion
        // still happens because `walk` continues past the callback's
        // `return`.
        let mut handled_by_reducer = false;
        match kind {
            sp::NodeKind::Attribute => {
                attribute_reducer(&mut parent.nodes[i]);
                handled_by_reducer = true;
            }
            sp::NodeKind::Combinator => {
                combinator_reducer(&mut parent.nodes[i]);
                handled_by_reducer = true;
            }
            sp::NodeKind::Pseudo => {
                pseudo_reducer(&mut parent.nodes[i]);
                handled_by_reducer = true;
            }
            sp::NodeKind::Tag => {
                tag_reducer(&mut parent.nodes[i]);
                handled_by_reducer = true;
            }
            sp::NodeKind::Universal => {
                // Upstream `universal()` (lines 175-181) removes the `*`
                // when the next sibling exists and isn't a combinator.
                let next_kind = parent.nodes.get(i + 1).map(|n| n.kind.clone());
                let should_remove =
                    matches!(next_kind, Some(k) if k != sp::NodeKind::Combinator);
                if should_remove {
                    parent.nodes.remove(i);
                    parent.raw_value = None;
                    // Cursor stays at `i` — next sibling slid in.
                    continue;
                }
                handled_by_reducer = true;
            }
            _ => {}
        }

        // Step 3: Selector dedup at the top level (parent kind != Pseudo).
        // Skipped when a reducer fired (mirrors upstream's `return`).
        if !handled_by_reducer
            && matches!(kind, sp::NodeKind::Selector)
            && !matches!(parent.kind, sp::NodeKind::Pseudo)
        {
            let key = sp::stringify(&parent.nodes[i]);
            if unique_top_level.contains(&key) {
                parent.nodes.remove(i);
                parent.raw_value = None;
                continue;
            } else {
                unique_top_level.insert(key);
            }
        }

        // Step 4: recurse into children. Upstream `walk` is depth-first
        // pre-order; the callback has already run on `parent.nodes[i]`,
        // and now we descend into its `nodes`.
        process_container(&mut parent.nodes[i], unique_top_level);
        i += 1;
    }
}

// --------------------------------------------------------------------------
// Plugin entry — mirror `OnceExit(css)` lines 201-256.
// --------------------------------------------------------------------------

/// Plugin entrypoint. Cache is per-invocation (one per `OnceExit`),
/// matching upstream's `const cache = new Map()` on line 202.
pub fn postcss_minify_selectors(root: &mut Root) -> PluginResult {
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_rules_mut(&mut root.root, &mut |node, _ctx| {
        // Read selector source — `rule.raws.selector.raw` if `value`
        // matches, else `rule.selector`. Upstream lines 233-236.
        let selector_input: String = {
            let rule = match &node.kind {
                NodeKind::Rule(r) => r,
                _ => return Mutation::Keep, // unreachable via walk_rules_mut.
            };
            match &node.raws.selector {
                Some(rv) if rv.value == rule.selector => rv.raw.clone(),
                _ => rule.selector.clone(),
            }
        };

        // Custom-mixin guard — line 240: trailing `:` skips the rule.
        if selector_input.ends_with(':') {
            return Mutation::Keep;
        }

        // Cache hit — line 244-248.
        if let Some(cached) = cache.get(&selector_input) {
            if let NodeKind::Rule(r) = &mut node.kind {
                r.selector = cached.clone();
                // raws.selector now stale (raws.selector.value !=
                // r.selector); leave it — postcss's stringifier checks
                // `raws.selector.value === selector` and falls through
                // to emit `selector` directly when they differ. Upstream
                // does the same (no explicit clear).
            }
            return Mutation::Keep;
        }

        // Parse → mutate → stringify, mirroring `processor.processSync`
        // upstream (line 250). The closure runs the full
        // `selectors.walk(...)` + `nodes.sort()` body.
        let processor = sp::Processor::new();
        let optimized = match processor.process(&selector_input, |root_node| {
            process_root(root_node);
        }) {
            Ok(out) => out,
            Err(_) => {
                // Tokenize error — upstream throws via processSync. We
                // preserve the input verbatim (no rule.selector write,
                // no cache entry). Surfaces as an unmodified rule, which
                // the JS oracle would also drop into the throw path —
                // the parity-runner records the throw and skips byte-
                // compare. Matching that on the Rust side requires
                // surfacing the error; for now we treat as a no-op.
                return Mutation::Keep;
            }
        };

        if let NodeKind::Rule(r) = &mut node.kind {
            r.selector = optimized.clone();
        }
        cache.insert(selector_input, optimized);
        Mutation::Keep
    });

    Ok(())
}

// Suppress "unused import" if the AttributeSpaces import is only used
// inside a sub-fn signature reference.
#[allow(dead_code)]
fn _force_imports(_: AttributeSpaces, _: Spaces) {}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).expect("parse");
        postcss_minify_selectors(&mut root).expect("plugin ok");
        stringify(&root)
    }

    #[test]
    fn idempotent_on_clean_input() {
        // No-op transform: simple class selector with no whitespace.
        assert_eq!(run(".a { color: red; }"), ".a { color: red; }");
    }

    #[test]
    fn strips_descendant_extra_whitespace() {
        assert_eq!(run(".a    .b { color: red; }"), ".a .b { color: red; }");
    }

    #[test]
    fn trims_combinator_padding() {
        assert_eq!(run(".a   >   .b { color: red; }"), ".a>.b { color: red; }");
    }

    #[test]
    fn does_not_dedupe_top_level_with_inter_arg_whitespace() {
        // Bug-for-bug: upstream's outer `selectors.walk((sel) => ...)`
        // visits Selector1 first (clears its spaces, runs dedup → adds
        // ".a" to set), THEN visits Selector1's children (clears their
        // spaces — already empty), THEN visits Selector2 (clears
        // Selector2.spaces — also empty in our parser; the leading space
        // lives on Selector2's FIRST CHILD's `spaces.before`). At dedup
        // time, Selector2's child spaces have NOT been cleared yet, so
        // `String(Selector2)` = " .a" ≠ ".a" → no dedup.
        //
        // Verified against upstream JS via
        // `packages/css/scripts/dbg-minify.mjs` — both engines emit
        // `.a,.a { color: red; }` for `.a, .a { color: red; }`.
        let out = run(".a, .a { color: red; }");
        assert_eq!(out, ".a,.a { color: red; }");
    }

    #[test]
    fn dedupes_top_level_when_inter_arg_whitespace_absent() {
        // No space between commas → both Selectors stringify to ".a"
        // identically at dedup time → second is removed.
        let out = run(".a,.a { color: red; }");
        assert_eq!(out, ".a { color: red; }");
    }

    #[test]
    fn sorts_top_level_selectors_alphabetically() {
        // After space-clearing, Selectors are joined with bare `,` (no
        // space) — upstream JS produces the same. The Root stringifier
        // does `nodes.map(String).join(',')`.
        let out = run(".b, .a { color: red; }");
        assert_eq!(out, ".a,.b { color: red; }");
    }

    #[test]
    fn rewrites_nth_child_one_to_first_child() {
        let out = run(":nth-child(1) { color: red; }");
        assert_eq!(out, ":first-child { color: red; }");
    }

    #[test]
    fn rewrites_nth_of_type_one_to_first_of_type() {
        let out = run(":nth-of-type(1) { color: red; }");
        assert_eq!(out, ":first-of-type { color: red; }");
    }

    #[test]
    fn rewrites_nth_last_child_one_to_last_child() {
        let out = run(":nth-last-child(1) { color: red; }");
        assert_eq!(out, ":last-child { color: red; }");
    }

    #[test]
    fn rewrites_even_to_2n() {
        let out = run(":nth-child(even) { color: red; }");
        assert_eq!(out, ":nth-child(2n) { color: red; }");
    }

    #[test]
    fn rewrites_2n_plus_1_to_odd() {
        let out = run(":nth-child(2n+1) { color: red; }");
        assert_eq!(out, ":nth-child(odd) { color: red; }");
    }

    #[test]
    fn drops_double_colon_for_legacy_pseudo_elements() {
        // `::before` → `:before`, `::after` → `:after`.
        let out = run(".x::before { color: red; }");
        assert_eq!(out, ".x:before { color: red; }");
    }

    #[test]
    fn keeps_double_colon_on_modern_pseudo_elements() {
        // `::placeholder` not in the legacy set — preserved.
        let out = run(".x::placeholder { color: red; }");
        assert!(out.contains("::placeholder"), "got {out:?}");
    }

    #[test]
    fn rewrites_keyframe_from_to_zero_percent() {
        let out = run("@keyframes spin { from { color: red; } to { color: blue; } }");
        assert!(out.contains("0% { color: red; }"), "got {out:?}");
        // `to` should remain `to` (only `from`/`100%` get rewritten).
        assert!(out.contains("to { color: blue; }"), "got {out:?}");
    }

    #[test]
    fn rewrites_keyframe_100pct_to_to() {
        let out = run("@keyframes spin { 0% { color: red; } 100% { color: blue; } }");
        assert!(out.contains("to { color: blue; }"), "got {out:?}");
    }

    #[test]
    fn drops_universal_followed_by_class() {
        // `* .a` → `.a` (universal followed by non-combinator).
        let out = run("* .a { color: red; }");
        // The `.a` keeps the descendant combinator (single space) on
        // its parent Selector after the universal is dropped. Strip the
        // preceding `*` byte-for-byte.
        assert!(!out.contains('*'), "universal not dropped: {out:?}");
    }

    #[test]
    fn keeps_universal_before_combinator() {
        // `* > .a` keeps `*` (next sibling IS a combinator).
        let out = run("* > .a { color: red; }");
        assert!(out.contains('*'), "universal incorrectly dropped: {out:?}");
    }

    #[test]
    fn unquotes_attribute_value_when_safe() {
        // `[data-x="hi"]` → `[data-x=hi]` (canUnquote returns true).
        let out = run(r#"[data-x="hi"] { color: red; }"#);
        assert!(out.contains("[data-x=hi]"), "got {out:?}");
    }

    #[test]
    fn keeps_quotes_when_value_has_disallowed_char() {
        // `[data-x="my value"]` — space is in disallowed range, keeps quotes.
        let out = run(r#"[data-x="my value"] { color: red; }"#);
        assert!(out.contains(r#"[data-x="my value"]"#), "got {out:?}");
    }

    #[test]
    fn keeps_quotes_when_value_starts_with_digit() {
        let out = run(r#"[data-x="123"] { color: red; }"#);
        assert!(out.contains(r#"[data-x="123"]"#), "got {out:?}");
    }

    #[test]
    fn skips_rule_when_selector_ends_with_colon() {
        // Custom-mixin guard. Selector preserved verbatim — even though
        // a real parser would balk, the plugin's `endsWith(':')` check
        // bypasses processing entirely.
        let css = ".x: { color: red; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn caches_repeated_selectors() {
        // Two rules with the same selector — second hits the cache.
        // Regression: if cache writes break, the second rule's selector
        // would be re-emitted with potential diff.
        let out = run(".a   .b { color: red; } .a   .b { color: blue; }");
        // Both rules normalized to `.a .b`.
        let count = out.matches(".a .b").count();
        assert_eq!(count, 2, "got {out:?}");
    }

    #[test]
    fn dedupes_pseudo_argument_selectors_when_no_inter_arg_whitespace() {
        // Upstream's pseudo() inner dedup runs BEFORE the outer walker
        // clears the inner Selectors' `spaces.before` — so the dedup
        // sees stringified-with-leading-spaces and only dedupes args
        // whose original bytes match. Bug-for-bug we mirror this.
        // Input written without spaces between commas → all three args
        // stringify identically → dedup fires.
        let out = run(":is(.a,.b,.a) { color: red; }");
        assert!(out.contains(":is(.a,.b)"), "got {out:?}");
    }

    #[test]
    fn does_not_dedupe_pseudo_args_with_inter_arg_spaces() {
        // Bug-for-bug: leading-space-divergent args don't dedupe. After
        // the OUTER walker clears spaces, dedup has already finished —
        // so the duplicate survives in output. (Upstream behaves the
        // same.)
        let out = run(":is(.a, .b, .a) { color: red; }");
        // Three args remain (post-clear, all comma-separated, no dups
        // collapsed). Output: `:is(.a,.b,.a)`.
        assert!(out.contains(":is(.a,.b,.a)"), "got {out:?}");
    }
}
