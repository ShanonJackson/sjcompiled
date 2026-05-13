//! crates/cssnano-postcss-normalize-positions
//! Byte-for-byte Rust port of `postcss-normalize-positions@5.1.1`.
//!
//! Folder/file mapping (1:1 with upstream):
//!   - `src/index.js` -> `src/lib.rs` (this file).
//!
//! All bugs of upstream 5.1.1 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! `css.walkDecls(/^(background(-position)?|(-\w+-)?perspective-origin)$/i, cb)`.
//! For each match, `decl.value = transform(decl.value)` with a per-call cache
//! keyed on the original string.
//!
//! `transform(value)` parses the value via `postcss-value-parser`, sweeps the
//! top-level `nodes` array to identify position-keyword "ranges" (per
//! comma-separated background entry, terminated on `/`), and rewrites each
//! range in place per the upstream keyword/2-keyword rules. `var()`/`env()`/
//! `constant()` short-circuits the current entry.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, parse_unit, stringify as vp_stringify};

// ---------------------------------------------------------------------------
// Constants — mirror upstream module-level Sets/Maps.
// ---------------------------------------------------------------------------

const CENTER: &str = "50%";

fn horizontal_lookup(k: &str) -> Option<&'static str> {
    match k {
        "right" => Some("100%"),
        "left" => Some("0"),
        _ => None,
    }
}

fn vertical_lookup(k: &str) -> Option<&'static str> {
    match k {
        "bottom" => Some("100%"),
        "top" => Some("0"),
        _ => None,
    }
}

fn is_direction_keyword(s: &str) -> bool {
    matches!(s, "top" | "right" | "bottom" | "left" | "center")
}

fn is_math_function_name(s: &str) -> bool {
    matches!(s, "calc" | "min" | "max" | "clamp")
}

fn is_variable_function_name(s: &str) -> bool {
    matches!(s, "var" | "env" | "constant")
}

// ---------------------------------------------------------------------------
// Node predicates — 1:1 with upstream helpers.
// ---------------------------------------------------------------------------

fn is_comma_node(node: &VNode) -> bool {
    node.kind == VKind::Div && node.value == ","
}

fn is_variable_function_node(node: &VNode) -> bool {
    if node.kind != VKind::Function {
        return false;
    }
    is_variable_function_name(&node.value.to_lowercase())
}

fn is_math_function_node(node: &VNode) -> bool {
    if node.kind != VKind::Function {
        return false;
    }
    is_math_function_name(&node.value.to_lowercase())
}

/// Upstream: `parseFloat(node.value); !isNaN(value)`. The JS `parseFloat`
/// returns non-NaN whenever the string has a valid numeric prefix — which
/// includes the literal tokens `Infinity`, `+Infinity`, `-Infinity` (case
/// sensitive). The CSS-syntax `like_number` check used by `parse_unit`
/// rejects all three because they don't begin with a digit, sign+digit,
/// or `.digit`. Mirror JS verbatim so that e.g. `Infinity right` doesn't
/// get its trailing `right` rewritten to `100%` (Rust used to skip the
/// `Infinity` slot for range tracking and then map a single `right` keyword;
/// JS marked the whole range and dropped it because `Infinity`/`right`
/// aren't both in the horizontal/vertical lookup tables).
fn js_parse_float_is_number(s: &str) -> bool {
    if parse_unit(s).is_some() {
        return true;
    }
    let bytes = s.as_bytes();
    let start = if !bytes.is_empty() && (bytes[0] == b'+' || bytes[0] == b'-') { 1 } else { 0 };
    s.get(start..).map(|rest| rest.starts_with("Infinity")).unwrap_or(false)
}

fn is_number_node(node: &VNode) -> bool {
    if node.kind != VKind::Word {
        return false;
    }
    js_parse_float_is_number(&node.value)
}

fn is_dimension_node(node: &VNode) -> bool {
    if node.kind != VKind::Word {
        return false;
    }
    match parse_unit(&node.value) {
        Some(p) => !p.unit.is_empty(),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// transform(value) — 1:1 with upstream `transform`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Range {
    start: Option<usize>,
    end: Option<usize>,
}

fn transform(value: &str) -> String {
    let mut parsed = vp_parse(value);

    // Mirrors upstream `ranges = []`. JS sparse arrays: forEach skips holes,
    // so `Vec<Option<Range>>` with `None` for holes preserves that.
    let mut ranges: Vec<Option<Range>> = Vec::new();
    let mut range_index: usize = 0;
    let mut should_continue = true;

    let len = parsed.len();
    for index in 0..len {
        // After comma (`,`) follows next background.
        if is_comma_node(&parsed[index]) {
            range_index += 1;
            should_continue = true;
            continue;
        }

        if !should_continue {
            continue;
        }

        // After separator (`/`) follows `background-size` values — avoid them.
        if parsed[index].kind == VKind::Div && parsed[index].value == "/" {
            should_continue = false;
            continue;
        }

        // `if (!ranges[rangeIndex]) ranges[rangeIndex] = {start:null,end:null}`.
        while ranges.len() <= range_index {
            ranges.push(None);
        }
        if ranges[range_index].is_none() {
            ranges[range_index] = Some(Range { start: None, end: None });
        }

        // Do not try to be processed `var` and `env` function inside background.
        if is_variable_function_node(&parsed[index]) {
            should_continue = false;
            let r = ranges[range_index].as_mut().unwrap();
            r.start = None;
            r.end = None;
            continue;
        }

        let is_position_keyword = (parsed[index].kind == VKind::Word
            && is_direction_keyword(&parsed[index].value.to_lowercase()))
            || is_dimension_node(&parsed[index])
            || is_number_node(&parsed[index])
            || is_math_function_node(&parsed[index]);

        let r = ranges[range_index].as_mut().unwrap();
        if r.start.is_none() && is_position_keyword {
            r.start = Some(index);
            r.end = Some(index);
            continue;
        }

        if r.start.is_some() {
            if parsed[index].kind == VKind::Space {
                continue;
            } else if is_position_keyword {
                r.end = Some(index);
                continue;
            }
            continue;
        }
    }

    // Apply ranges. JS `forEach` skips empty slots and entries whose start is
    // null — replicate via filter on `Some` + `start.is_some()`.
    let ranges_snapshot: Vec<Option<Range>> = ranges.clone();
    for range_opt in &ranges_snapshot {
        let range = match range_opt {
            Some(r) if r.start.is_some() => *r,
            _ => continue,
        };
        let start = range.start.unwrap();
        let end = range.end.unwrap();
        let count = end + 1 - start;
        if count > 3 {
            continue;
        }

        let first_node = parsed[start].value.to_lowercase();
        // Upstream: `nodes[2] && nodes[2].value ? ... : null`. nodes[2] is
        // parsed[start + 2]; "exists" means count >= 3, "truthy value" means
        // non-empty string.
        let second_node: Option<String> = if count >= 3 && !parsed[start + 2].value.is_empty() {
            Some(parsed[start + 2].value.to_lowercase())
        } else {
            None
        };

        if count == 1 || second_node.as_deref() == Some("center") {
            if second_node.is_some() {
                parsed[start + 2].value = String::new();
                parsed[start + 1].value = String::new();
            }

            // map = horizontal + ['center', center]
            let mapped = match first_node.as_str() {
                "right" => Some("100%"),
                "left" => Some("0"),
                "center" => Some(CENTER),
                _ => None,
            };
            if let Some(v) = mapped {
                parsed[start].value = v.to_string();
            }
            continue;
        }

        if let Some(second) = &second_node {
            if first_node == "center" && is_direction_keyword(second) {
                parsed[start].value = String::new();
                parsed[start + 1].value = String::new();
                if let Some(v) = horizontal_lookup(second) {
                    parsed[start + 2].value = v.to_string();
                }
                continue;
            }

            let h_first = horizontal_lookup(&first_node);
            let v_first = vertical_lookup(&first_node);
            let h_second = horizontal_lookup(second);
            let v_second = vertical_lookup(second);

            if h_first.is_some() && v_second.is_some() {
                parsed[start].value = h_first.unwrap().to_string();
                parsed[start + 2].value = v_second.unwrap().to_string();
                continue;
            } else if v_first.is_some() && h_second.is_some() {
                parsed[start].value = h_second.unwrap().to_string();
                parsed[start + 2].value = v_first.unwrap().to_string();
                continue;
            }
        }
    }

    vp_stringify(&parsed)
}

// ---------------------------------------------------------------------------
// Plugin entry — 1:1 with upstream `pluginCreator().OnceExit(css)`.
// ---------------------------------------------------------------------------

// Upstream regex: `/^(background(-position)?|(-\w+-)?perspective-origin)$/i`.
// JS `\w` (no `u` flag) is ASCII `[A-Za-z0-9_]`; Rust's default Unicode-aware
// `\w` would spuriously match e.g. `-übér-perspective-origin`. Scope the
// word class to ASCII via `(?-u:\w)` to mirror JS exactly.
static POSITION_PROP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(background(-position)?|(-(?-u:\w)+-)?perspective-origin)$").unwrap()
});

pub fn postcss_normalize_positions(root: &mut Root) -> PluginResult {
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        let decl = match &mut node.kind {
            NodeKind::Declaration(d) => d,
            _ => return Mutation::Keep,
        };

        if !POSITION_PROP.is_match(&decl.prop) {
            return Mutation::Keep;
        }

        let value = decl.value.clone();
        if value.is_empty() {
            return Mutation::Keep;
        }

        // The postcss-core stringifier (`raw_value_str`) already mirrors
        // JS `lib/stringifier.js#rawValue`'s `raws.value.value === node.value
        // ? raws.value.raw : node.value` comparison, so the raws cache is
        // invalidated automatically when transform actually changes the
        // value, and preserved (correctly) when transform is a no-op.
        // Clearing raws here would lose source bytes (e.g. trailing
        // comments captured into `raws.value.raw`) on no-op transforms.
        if let Some(cached) = cache.get(&value).cloned() {
            decl.value = cached;
            return Mutation::Keep;
        }

        let result = transform(&value);
        decl.value = result.clone();
        cache.insert(value, result);
        Mutation::Keep
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — minimal coverage; corpus parity is the load-bearing gate.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_normalize_positions(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_blank() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn ignores_unrelated_decls() {
        let css = "a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn left_top_to_zeroes() {
        let out = run("a { background-position: left top; }");
        assert!(out.contains("0 0"), "got: {out:?}");
    }

    #[test]
    fn right_bottom_to_percent() {
        let out = run("a { background-position: right bottom; }");
        assert!(out.contains("100% 100%"), "got: {out:?}");
    }

    #[test]
    fn vertical_first_swaps_to_horizontal_first() {
        let out = run("a { background-position: top right; }");
        assert!(out.contains("100% 0"), "got: {out:?}");
    }

    #[test]
    fn single_left_to_zero() {
        let out = run("a { background-position: left; }");
        assert!(out.contains("0"), "got: {out:?}");
    }

    #[test]
    fn center_pair_collapses() {
        let out = run("a { background-position: right center; }");
        assert!(out.contains("100%"), "got: {out:?}");
        assert!(!out.contains("center"), "got: {out:?}");
    }

    #[test]
    fn var_function_short_circuits() {
        let css = "a { background-position: var(--p); }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn slash_protects_background_size() {
        // `right top / cover` — only the position part is rewritten.
        let out = run("a { background: red right top / cover; }");
        assert!(out.contains("100% 0"), "got: {out:?}");
        assert!(out.contains("/ cover"), "got: {out:?}");
    }

    #[test]
    fn comma_resets_range() {
        let out = run("a { background-position: left top, right bottom; }");
        assert!(out.contains("0 0"), "got: {out:?}");
        assert!(out.contains("100% 100%"), "got: {out:?}");
    }

    #[test]
    fn vendor_perspective_origin_matches() {
        let out = run("a { -webkit-perspective-origin: left top; }");
        assert!(out.contains("0 0"), "got: {out:?}");
    }

    #[test]
    fn three_value_form_is_skipped() {
        // count > 3 — upstream returns without transforming.
        let css = "a { background-position: left top right; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn preserves_raws_value_on_noop() {
        // Regression for the raws-clearing drift. The decl value has a
        // trailing comment captured into `raws.value.raw`; transform is a
        // no-op (`50% 50%` lookups all return None). Stringifier should
        // emit `raws.value.raw` (with the comment). Prior code cleared
        // `raws.value` → comment was lost. Same shape as the bug fixed
        // in `cssnano-postcss-normalize-timing-functions` /
        // `cssnano-postcss-normalize-string`.
        let css = "a { background-position: 50% 50% /* trailing */; }";
        let out = run(css);
        assert!(
            out.contains("/* trailing */"),
            "trailing comment must survive no-op normalization; got: {out:?}"
        );
    }

    #[test]
    fn preserves_raws_value_on_cache_hit_noop() {
        // Same as above but exercises the cache-hit path: two decls with
        // the same value in the same Root. The first populates the cache;
        // the second hits the cache. Both must preserve `raws.value.raw`.
        let css = "a { background-position: 50% 50% /* keep-1 */; }\n\
                   b { background-position: 50% 50% /* keep-2 */; }";
        let out = run(css);
        assert!(out.contains("/* keep-1 */"), "first decl comment lost; got: {out:?}");
        assert!(out.contains("/* keep-2 */"), "second decl comment lost; got: {out:?}");
    }

    #[test]
    fn unicode_prefix_property_does_not_match() {
        // JS regex `\w` (no `u` flag) is ASCII-only — `-übér-perspective-origin`
        // does NOT match upstream. With Rust's default Unicode-aware `\w`,
        // it WOULD match. We scope `\w` to ASCII via `(?-u:\w)` — verify
        // the property is left untouched.
        let css = "a { -übér-perspective-origin: left top; }";
        assert_eq!(run(css), css, "unicode-prefixed property must not match");
    }

    #[test]
    fn ascii_prefix_with_underscore_matches() {
        // Lock the inverse of the Unicode-prefix test: `_x_` is ASCII `\w+`,
        // so the regex must still match. (Underscore is in `[A-Za-z0-9_]`.)
        let out = run("a { -_x_-perspective-origin: left top; }");
        assert!(out.contains("0 0"), "ascii-underscore prefix should match; got: {out:?}");
    }

    #[test]
    fn infinity_first_with_keyword_does_not_substitute() {
        // Regression for the `is_number_node` Infinity drift.
        // JS `parseFloat("Infinity")` returns Infinity (NOT NaN), so
        // `isNumberNode` returns true → "Infinity" anchors the range and
        // "right" extends it (count=3). Apply step finds neither
        // `horizontal.has("infinity")` nor `verticalValue.has("infinity")`
        // so the range is left untouched. The prior Rust port treated
        // "Infinity" as a non-keyword (parse_unit's `like_number` rejects
        // it), so the range was anchored on `right` alone (count=1) and
        // the single-keyword branch rewrote `right` to `100%` — producing
        // `Infinity 100%` where JS produces `Infinity right`.
        let css = "a { background-position: Infinity right; }";
        let out = run(css);
        assert_eq!(out, css, "Infinity first must inhibit single-keyword rewrite");
    }

    #[test]
    fn negative_infinity_second_does_not_substitute() {
        // Mirror image: `-Infinity` second slot. JS treats it as a number
        // → end is extended to it. With the buggy Rust, end stays at the
        // first position keyword and the single-keyword rewrite fires.
        let css = "a { background-position: left -Infinity; }";
        let out = run(css);
        assert_eq!(out, css, "-Infinity second must inhibit single-keyword rewrite");
    }

    #[test]
    fn infinity_alone_left_alone() {
        // Single-token "Infinity" — JS parses it as a number, anchors a
        // range of count=1 with `firstNode = "infinity"` (lowercased).
        // The map only has horizontal/`center` keys, so "infinity" doesn't
        // match — no rewrite. Locks the no-match path on the lowercase
        // mapping; surfaces if a regression made the lookup case-sensitive
        // or accidentally added `infinity` as a key.
        let css = "a { background-position: Infinity; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn js_parse_float_is_number_handles_infinity() {
        // Direct unit test for the helper, since the integration cases
        // above only fire if the helper is correct AND the rest of the
        // range/apply logic is correct.
        assert!(super::js_parse_float_is_number("Infinity"));
        assert!(super::js_parse_float_is_number("+Infinity"));
        assert!(super::js_parse_float_is_number("-Infinity"));
        // Lowercase is NOT a number per JS parseFloat (case sensitive).
        assert!(!super::js_parse_float_is_number("infinity"));
        // NaN is NOT a number per parseFloat (parseFloat("NaN") === NaN).
        assert!(!super::js_parse_float_is_number("NaN"));
        // Sanity: ordinary numerics still work.
        assert!(super::js_parse_float_is_number("0"));
        assert!(super::js_parse_float_is_number("-0.5"));
        assert!(super::js_parse_float_is_number("1e10"));
        assert!(super::js_parse_float_is_number(".5"));
        // Non-numeric still rejected.
        assert!(!super::js_parse_float_is_number("abc"));
        assert!(!super::js_parse_float_is_number(""));
        assert!(!super::js_parse_float_is_number("+"));
        assert!(!super::js_parse_float_is_number("."));
    }
}
