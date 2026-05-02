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
/// returns non-NaN exactly when the string starts with a valid numeric
/// prefix — which is exactly what `parse_unit` checks via `like_number`.
fn is_number_node(node: &VNode) -> bool {
    if node.kind != VKind::Word {
        return false;
    }
    parse_unit(&node.value).is_some()
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

static POSITION_PROP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(background(-position)?|(-\w+-)?perspective-origin)$").unwrap()
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

        if let Some(cached) = cache.get(&value).cloned() {
            decl.value = cached;
            node.raws.value = None;
            return Mutation::Keep;
        }

        let result = transform(&value);
        decl.value = result.clone();
        node.raws.value = None;
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
}
