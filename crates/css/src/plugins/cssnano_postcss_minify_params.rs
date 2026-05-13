//! crates/cssnano-postcss-minify-params
//! Byte-for-byte Rust port of `postcss-minify-params@5.1.4`.
//!
//! Folder/file mapping (1:1 with upstream `node_modules/postcss-minify-params/`):
//!   - `src/index.js` -> `src/lib.rs` (this file).
//!
//! See `crates/_vendor/POSTCSS_MINIFY_PARAMS_5.1.4_REAUDIT.md` for the
//! full per-line audit and `crates/PARITY_VERSIONS.md` for the cardinal
//! rules. Bug-for-bug parity items called out in the audit (params.nodes
//! root-reads from inside function recursion, positional `-aspect-ratio`
//! match, `getArguments` leading-space-on-second-arg, ASCII-only sort,
//! NaN propagation in non-integer aspect-ratio inputs) are intentionally
//! preserved.
//!
//! `OnceExit`-only plugin: `prepare(result)` upstream resolves browserslist
//! once at instantiation; we resolve once per `postcss_minify_params` call
//! and walk every at-rule.

use postcss_core::container::{walk_at_rules_mut, Mutation};
use postcss_core::js_number_to_string;
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VpNode, NodeKind as VpKind};
use postcss_value_parser::{parse as value_parse, stringify as value_stringify};

/// Upstream `allBugBrowers = new Set(['ie 10', 'ie 11'])` (index.js:124).
const ALL_BUG_BROWSERS: &[&str] = &["ie 10", "ie 11"];

/// Plugin entry. Default options pass-through; AFM consumer
/// (`cssnano-preset-default@5.2.14`) calls `creator()` with no opts.
pub fn postcss_minify_params(root: &mut Root) -> PluginResult {
    // `pluginCreator(options = {})`: resolve browserslist once. AFM
    // consumer never sets `path`/`stats`/`env`, so default-query path.
    let browsers = browserslist_shim::resolve("", true);
    process_with_browsers(root, &browsers)
}

/// Snapshot-aware variant. When `snapshot` is `Some`, the
/// `has_all_bug` (legacy IE 10/11 detection) decision drives off the
/// snapshot's host-resolved list. When `None`, byte-equivalent to
/// [`postcss_minify_params`].
pub fn postcss_minify_params_with_snapshot(
    root: &mut Root,
    snapshot: Option<&::cssnano_browserslist_snapshot::PrecomputedBrowserslist>,
) -> PluginResult {
    match snapshot {
        Some(snap) => process_with_browsers(root, snap.selected.as_slice()),
        None => postcss_minify_params(root),
    }
}

fn process_with_browsers(root: &mut Root, browsers: &[String]) -> PluginResult {
    let has_all_bug = browsers
        .iter()
        .any(|b| ALL_BUG_BROWSERS.contains(&b.as_str()));

    walk_at_rules_mut(&mut root.root, &mut |node, _ctx| {
        transform(has_all_bug, node);
        Mutation::Keep
    });

    Ok(())
}

/// `transform(legacy, rule)` — index.js:71.
fn transform(legacy: bool, node: &mut postcss_core::Node) {
    let (orig_name, orig_params) = match &node.kind {
        NodeKind::AtRule(a) => (a.name.clone(), a.params.clone()),
        _ => return,
    };
    if orig_params.is_empty() {
        return;
    }
    let rule_name_lower = orig_name.to_ascii_lowercase();
    if rule_name_lower != "media" && rule_name_lower != "supports" {
        return;
    }

    let mut nodes = value_parse(&orig_params);

    // `params.walk(cb, true)` — bubble-mode walk over the value-parser tree.
    walk_bubble_root(&mut nodes, legacy, &rule_name_lower);

    // `getArguments(params).map(split)`.
    let groups = split_arguments(&nodes);
    let stringified: Vec<String> = groups.iter().map(|g| value_stringify(g)).collect();

    // `sortAndDedupe(items)`.
    let new_params = sort_and_dedupe(stringified);

    let is_empty = new_params.is_empty();
    if let NodeKind::AtRule(a) = &mut node.kind {
        a.params = new_params;
    }
    // `if (!rule.params.length) rule.raws.afterName = '';`
    if is_empty {
        node.raws.after_name = Some(String::new());
    }
}

/// Bubble walk entry — visits every node in `root_nodes` post-order
/// (function children fire before their function-node), invoking the
/// per-node callback. The callback's `else` branch reads ROOT slots
/// even when fired from inside a function recursion (upstream bug,
/// preserved 1:1) — we thread a path stack so the callback can reach
/// either the current frame or the root.
fn walk_bubble_root(root_nodes: &mut Vec<VpNode>, legacy: bool, rule_name_lower: &str) {
    let mut path: Vec<usize> = Vec::new();
    walk_frame(root_nodes, &mut path, legacy, rule_name_lower);
}

fn walk_frame(
    root: &mut Vec<VpNode>,
    path: &mut Vec<usize>,
    legacy: bool,
    rule_name_lower: &str,
) {
    let len = frame_len(root, path);
    for i in 0..len {
        let is_function = matches!(frame_node_kind(root, path, i), Some(VpKind::Function));
        if is_function {
            path.push(i);
            walk_frame(root, path, legacy, rule_name_lower);
            path.pop();
        }
        callback_at(root, path, i, legacy, rule_name_lower);
    }
}

fn frame_at<'a>(root: &'a Vec<VpNode>, path: &[usize]) -> &'a [VpNode] {
    let mut cur: &[VpNode] = root.as_slice();
    for &idx in path {
        cur = &cur[idx].nodes;
    }
    cur
}

fn frame_at_mut<'a>(root: &'a mut Vec<VpNode>, path: &[usize]) -> &'a mut Vec<VpNode> {
    let mut cur: &mut Vec<VpNode> = root;
    for &idx in path {
        cur = &mut cur[idx].nodes;
    }
    cur
}

fn frame_len(root: &Vec<VpNode>, path: &[usize]) -> usize {
    frame_at(root, path).len()
}

fn frame_node_kind(root: &Vec<VpNode>, path: &[usize], index: usize) -> Option<VpKind> {
    frame_at(root, path).get(index).map(|n| n.kind.clone())
}

/// One bubble-callback firing — index.js:77-118.
///
/// Borrows are short-lived: we never hold a mutable borrow of a sub-frame
/// across an access to `root` (which would alias when path is empty). The
/// `else` branch accesses `root` (sibling reads/writes) — those are
/// scoped to their own sub-blocks so `frame_at_mut` is freshly re-taken
/// when needed.
fn callback_at(
    root: &mut Vec<VpNode>,
    path: &[usize],
    index: usize,
    legacy: bool,
    rule_name_lower: &str,
) {
    let kind = match frame_node_kind(root, path, index) {
        Some(k) => k,
        None => return,
    };

    match kind {
        VpKind::Div => {
            // `node.before = node.after = '';` (index.js:78-79).
            let frame = frame_at_mut(root, path);
            let n = &mut frame[index];
            n.before.clear();
            n.after.clear();
        }
        VpKind::Function => {
            // `node.before = '';` then conditional `node.after`, then
            // aspect-ratio reduction over `nodes[2]`/`nodes[4]`.
            let frame = frame_at_mut(root, path);
            let n = &mut frame[index];
            n.before.clear();

            let after_space = n
                .nodes
                .first()
                .map(|n0| {
                    matches!(n0.kind, VpKind::Word)
                        && n0.value.starts_with("--")
                        && n.nodes.get(2).is_none()
                })
                .unwrap_or(false);
            n.after = if after_space {
                " ".to_string()
            } else {
                String::new()
            };

            // `if (node.nodes[4] && node.nodes[0].value.toLowerCase()
            //       .indexOf('-aspect-ratio') === 3)` — aspect ratio.
            if n.nodes.get(4).is_some() {
                if let Some(n0) = n.nodes.first() {
                    let lower = n0.value.to_ascii_lowercase();
                    if lower.find("-aspect-ratio") == Some(3) {
                        let a_str = n.nodes[2].value.clone();
                        let b_str = n.nodes[4].value.clone();
                        let a_num = js_number_coerce(&a_str);
                        let b_num = js_number_coerce(&b_str);
                        let (a, b) = aspect_ratio(a_num, b_num);
                        n.nodes[2].value = js_number_to_string(a);
                        n.nodes[4].value = js_number_to_string(b);
                    }
                }
            }
        }
        VpKind::Space => {
            // `node.value = ' ';` (index.js:104-105).
            let frame = frame_at_mut(root, path);
            frame[index].value = " ".to_string();
        }
        _ => {
            // `else` branch — upstream reads `params.nodes[index ± k]`
            // unconditionally. `params.nodes` is the ROOT array, NOT
            // the current parent's `nodes`. When `index` is out of root
            // range, the read returns `undefined` — falsy — which keeps
            // most pathological branches from firing.
            let value_lower = {
                let frame = frame_at(root, path);
                frame[index].value.to_ascii_lowercase()
            };
            // `prevWord = params.nodes[index - 2]`. JS `arr[-1]` is
            // `undefined` (not negative-indexed). Mirror that with a
            // bounds check.
            let prev_word_exists = if index >= 2 {
                root.get(index - 2).is_some()
            } else {
                false
            };

            if value_lower == "all" && rule_name_lower == "media" && !prev_word_exists {
                // `nextWord = params.nodes[index + 2]`.
                let next_word_value: Option<String> = root
                    .get(index + 2)
                    .map(|n| n.value.to_ascii_lowercase());
                let has_next_word = next_word_value.is_some();

                // `if (!legacy || nextWord) removeNode(node);`
                if !legacy || has_next_word {
                    let frame = frame_at_mut(root, path);
                    let n = &mut frame[index];
                    n.value.clear();
                    n.kind = VpKind::Word;
                }

                // `if (nextWord && nextWord.value.toLowerCase() === 'and')`
                // — remove nextWord, the space at index+1, and the space
                // at index+3. All from ROOT (not frame).
                if let Some(nw_lower) = next_word_value {
                    if nw_lower == "and" {
                        for off in [2usize, 1, 3] {
                            if let Some(n) = root.get_mut(index + off) {
                                n.value.clear();
                                n.kind = VpKind::Word;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Recursive Euclid GCD on f64. `b ? gcd(b, a % b) : a` — JS modulo
/// semantics (`%` follows sign of dividend; for non-NaN integers this
/// matches Rust `%`). NaN-in-NaN-out: any NaN short-circuits to the
/// first arg because NaN is JS-falsy.
fn gcd(a: f64, b: f64) -> f64 {
    if b == 0.0 || b.is_nan() {
        a
    } else {
        gcd(b, a % b)
    }
}

fn aspect_ratio(a: f64, b: f64) -> (f64, f64) {
    let divisor = gcd(a, b);
    (a / divisor, b / divisor)
}

/// JS `Number(string)` coercion for the inputs reachable from
/// `node.nodes[2/4].value` (Word values from value-parser).
fn js_number_coerce(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if let Some(rest) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(rest, 16)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    if let Some(rest) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return u64::from_str_radix(rest, 8)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    if let Some(rest) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return u64::from_str_radix(rest, 2)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// Inline port of `cssnano-utils::getArguments(node)` for a flat slice.
/// Splits at top-level Div tokens. Always returns at least one group
/// (matches upstream's initial `[[]]`).
fn split_arguments(nodes: &[VpNode]) -> Vec<Vec<VpNode>> {
    let mut list: Vec<Vec<VpNode>> = vec![Vec::new()];
    for child in nodes {
        if !matches!(child.kind, VpKind::Div) {
            list.last_mut().unwrap().push(child.clone());
        } else {
            list.push(Vec::new());
        }
    }
    list
}

/// `[...new Set(items)].sort().join()` — JS dedupe (insertion-order
/// preserving) + default sort + comma-join.
///
/// `Array.prototype.sort()` without comparator orders strings by UTF-16
/// code units; Rust `sort()` orders by UTF-8 byte sequence. The two
/// coincide for ASCII (which covers media/supports conditions in
/// practice). Documented as a drift candidate alongside the
/// `sort_at_rules` UCA gap.
fn sort_and_dedupe(items: Vec<String>) -> String {
    let mut seen: Vec<String> = Vec::with_capacity(items.len());
    for it in items {
        if !seen.contains(&it) {
            seen.push(it);
        }
    }
    seen.sort();
    seen.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse as css_parse, stringify as css_stringify};

    fn run(input: &str) -> String {
        let mut root = css_parse(input).unwrap();
        postcss_minify_params(&mut root).unwrap();
        css_stringify(&root)
    }

    #[test]
    fn bare_at_media_all_collapses() {
        // `raws.afterName = ""` clears the space between `@media` and
        // (now-empty) params; the trailing space before `{` comes from
        // `raws.between` and is preserved verbatim. JS oracle confirmed.
        assert_eq!(
            run("@media all { a { color: red; } }"),
            "@media { a { color: red; } }"
        );
    }

    #[test]
    fn media_dedupe_and_sort_simple_args() {
        // `@media a, b, a` → dedupe `[a, b]` → sort → `"a,b"`.
        assert_eq!(
            run("@media a, b, a { x { color: red; } }"),
            "@media a,b { x { color: red; } }"
        );
    }

    #[test]
    fn at_media_all_and_drops_all_keyword() {
        let out = run("@media all and (min-width: 768px) { a { color: red; } }");
        assert_eq!(out, "@media (min-width:768px) { a { color: red; } }");
    }

    #[test]
    fn supports_normalizes_dimension_function_whitespace() {
        let out = run("@supports ( display : grid ) { a { color: red; } }");
        assert_eq!(out, "@supports (display:grid) { a { color: red; } }");
    }

    #[test]
    fn keyframes_untouched() {
        let css = "@keyframes  fade { from { opacity: 0; } to { opacity: 1; } }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn aspect_ratio_reduction() {
        let out = run("@media (min-aspect-ratio: 4/2) { a { color: red; } }");
        assert_eq!(out, "@media (min-aspect-ratio:2/1) { a { color: red; } }");
    }

    #[test]
    fn aspect_ratio_already_reduced() {
        let out = run("@media (min-aspect-ratio: 16/9) { a { color: red; } }");
        assert_eq!(out, "@media (min-aspect-ratio:16/9) { a { color: red; } }");
    }

    #[test]
    fn supports_empty_custom_property_keeps_space_after_colon() {
        let out = run("@supports (--foo:) { a { color: red; } }");
        assert_eq!(out, "@supports (--foo: ) { a { color: red; } }");
    }

    #[test]
    fn supports_populated_custom_property_no_extra_space() {
        let out = run("@supports (--foo: red) { a { color: red; } }");
        assert_eq!(out, "@supports (--foo:red) { a { color: red; } }");
    }

    #[test]
    fn at_import_untouched() {
        let css = "@import \"x.css\";\na { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn supports_and_keyword_at_root_preserved() {
        // `@supports (a) and (b)` — `and` at root, not after `all` —
        // must NOT be removed.
        let out = run("@supports (display: grid) and (color: red) { a { x: 1; } }");
        assert_eq!(
            out,
            "@supports (display:grid) and (color:red) { a { x: 1; } }"
        );
    }

    #[test]
    fn js_number_coerce_basics() {
        assert_eq!(js_number_coerce("16"), 16.0);
        assert_eq!(js_number_coerce(""), 0.0);
        assert_eq!(js_number_coerce("  4  "), 4.0);
        assert!(js_number_coerce("16px").is_nan());
        assert_eq!(js_number_coerce("0x10"), 16.0);
        assert_eq!(js_number_coerce("Infinity"), f64::INFINITY);
    }

    #[test]
    fn gcd_and_aspect_ratio() {
        assert_eq!(gcd(16.0, 9.0), 1.0);
        assert_eq!(gcd(120.0, 1080.0), 120.0);
        assert_eq!(aspect_ratio(1920.0, 1080.0), (16.0, 9.0));
        assert_eq!(aspect_ratio(4.0, 2.0), (2.0, 1.0));
    }

    #[test]
    fn sort_and_dedupe_orders_lexically() {
        assert_eq!(
            sort_and_dedupe(vec!["b".into(), "a".into(), "b".into()]),
            "a,b"
        );
        assert_eq!(sort_and_dedupe(vec![]), "");
        assert_eq!(sort_and_dedupe(vec!["".into()]), "");
    }

    // -------------------------------------------------------------------
    // Phase B / E5 — snapshot-aware entry-point parity tests.
    // -------------------------------------------------------------------

    use ::cssnano_browserslist_snapshot::{
        PrecomputedBrowserslist, PRECOMPUTED_FORMAT_VERSION,
    };

    fn snap(selected: &[&str]) -> PrecomputedBrowserslist {
        let owned: Vec<String> = selected.iter().map(|s| (*s).to_string()).collect();
        let joined = owned.join(", ");
        PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: owned,
            joined_query: joined,
        }
    }

    fn run_with_snap(css: &str, snapshot: Option<&PrecomputedBrowserslist>) -> String {
        let mut root = css_parse(css).unwrap();
        postcss_minify_params_with_snapshot(&mut root, snapshot).unwrap();
        css_stringify(&root)
    }

    /// E5.a — `None` snapshot byte-equivalent to default entry.
    #[test]
    fn snapshot_none_byte_equivalent_to_default_entry() {
        let cases = [
            "@media all { a { color: red; } }",
            "@media a, b, a { x { color: red; } }",
            "@media all and (min-width: 768px) { a { color: red; } }",
            "@supports ( display : grid ) { a { color: red; } }",
            "@import \"x.css\";\na { color: red; }",
        ];
        for src in cases {
            assert_eq!(
                run(src),
                run_with_snap(src, None),
                "snapshot=None drifted from default entry on input {src:?}",
            );
        }
    }

    /// E5.b — modern snapshot has no IE 10/11 → `has_all_bug = false` —
    /// `@media all` still collapses (collapse is unconditional, not
    /// gated by IE bug).
    #[test]
    fn snapshot_modern_collapses_at_media_all() {
        let modern = snap(&["chrome 144", "firefox 147"]);
        assert_eq!(
            run_with_snap("@media all { a { color: red; } }", Some(&modern)),
            "@media { a { color: red; } }",
        );
    }
}
