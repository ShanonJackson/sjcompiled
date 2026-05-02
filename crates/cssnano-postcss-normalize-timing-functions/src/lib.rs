//! crates/cssnano-postcss-normalize-timing-functions
//! Byte-for-byte Rust port of `postcss-normalize-timing-functions@5.1.0`.
//!
//! Folder/file mapping (1:1 with upstream):
//!   - `src/index.js` -> `src/lib.rs` (this file).
//!
//! All bugs of upstream 5.1.0 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! `css.walkDecls(/^(-\w+-)?(animation|transition)(-timing-function)?$/i, cb)`.
//! For each match, `decl.value = transform(decl.value)` with a per-call cache
//! keyed on the original string.
//!
//! `transform(value)` runs `postcss-value-parser`'s `walk(reduce)`:
//!   - Non-Function nodes return `false` (don't descend).
//!   - `steps(1, start | jump-start)` → word `step-start`.
//!   - `steps(1, end | jump-end)`     → word `step-end`.
//!   - `steps(_, end | jump-end)`     → strip the trailing `, end` so the
//!     remaining function is `steps(N)` (browser default).
//!   - `cubic-bezier(a, b, c, d)`     → if `[a,b,c,d].toString()` matches
//!     a known easing keyword (ease/linear/ease-in/ease-out/ease-in-out),
//!     replace the function with that word.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::js_number_to_string;
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, parse_unit, stringify as vp_stringify, walk as vp_walk};

// ---------------------------------------------------------------------------
// JS parseFloat parity.
// ---------------------------------------------------------------------------

/// Mirrors `parseFloat(s)` — returns `Some(f64)` if `s` has a valid leading
/// numeric prefix (potentially followed by junk like `"px"`), else `None`
/// (i.e. JS `NaN`).
fn js_parse_float(s: &str) -> Option<f64> {
    let pu = parse_unit(s)?;
    pu.number.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// cubic-bezier conversion table.
// ---------------------------------------------------------------------------

/// Mirrors upstream `[a,b,c,d].toString()`. JS Array.toString joins via `,`
/// with each element via `String(num)`.
fn key_for(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| js_number_to_string(*v))
        .collect::<Vec<_>>()
        .join(",")
}

fn conversion_for(key: &str) -> Option<&'static str> {
    match key {
        "0.25,0.1,0.25,1" => Some("ease"),
        "0,0,1,1" => Some("linear"),
        "0.42,0,1,1" => Some("ease-in"),
        "0,0,0.58,1" => Some("ease-out"),
        "0.42,0,0.58,1" => Some("ease-in-out"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// reduce(node) — value-parser walk callback. 1:1 with upstream.
// ---------------------------------------------------------------------------

fn reduce(node: &mut VNode, _i: usize) -> Option<bool> {
    if node.kind != VKind::Function {
        return Some(false);
    }
    if node.value.is_empty() {
        return None;
    }

    let lower = node.value.to_lowercase();

    if lower == "steps" {
        // step-start: `steps(1, start | jump-start)`.
        if !node.nodes.is_empty()
            && node.nodes[0].kind == VKind::Word
            && js_parse_float(&node.nodes[0].value) == Some(1.0)
            && node.nodes.len() > 2
            && node.nodes[2].kind == VKind::Word
        {
            let third = node.nodes[2].value.to_lowercase();
            if third == "start" || third == "jump-start" {
                node.kind = VKind::Word;
                node.value = "step-start".to_string();
                node.nodes.clear();
                return None;
            }
        }
        // step-end: `steps(1, end | jump-end)`.
        if !node.nodes.is_empty()
            && node.nodes[0].kind == VKind::Word
            && js_parse_float(&node.nodes[0].value) == Some(1.0)
            && node.nodes.len() > 2
            && node.nodes[2].kind == VKind::Word
        {
            let third = node.nodes[2].value.to_lowercase();
            if third == "end" || third == "jump-end" {
                node.kind = VKind::Word;
                node.value = "step-end".to_string();
                node.nodes.clear();
                return None;
            }
        }
        // Strip trailing `, end | jump-end` (browser default).
        if node.nodes.len() > 2 && node.nodes[2].kind == VKind::Word {
            let third = node.nodes[2].value.to_lowercase();
            if third == "end" || third == "jump-end" {
                let first = node.nodes[0].clone();
                node.nodes = vec![first];
                return None;
            }
        }
        return Some(false);
    }

    if lower == "cubic-bezier" {
        // Even-indexed children (0, 2, 4, 6) are the numeric values; odd
        // indices are `,` Div tokens.
        let values: Vec<f64> = node
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .filter_map(|(_, n)| js_parse_float(&n.value))
            .collect();

        if values.len() != 4 {
            return None;
        }

        let key = key_for(&values);
        if let Some(name) = conversion_for(&key) {
            node.kind = VKind::Word;
            node.value = name.to_string();
            node.nodes.clear();
            return None;
        }
    }

    None
}

// ---------------------------------------------------------------------------
// transform(value) — 1:1 with upstream.
// ---------------------------------------------------------------------------

fn transform(value: &str) -> String {
    let mut parsed = vp_parse(value);
    vp_walk(&mut parsed, reduce, false);
    vp_stringify(&parsed)
}

// ---------------------------------------------------------------------------
// Plugin entry — mirrors upstream `pluginCreator().OnceExit(css)`.
// ---------------------------------------------------------------------------

static TIMING_PROP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(-\w+-)?(animation|transition)(-timing-function)?$").unwrap()
});

pub fn postcss_normalize_timing_functions(root: &mut Root) -> PluginResult {
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        let decl = match &mut node.kind {
            NodeKind::Declaration(d) => d,
            _ => return Mutation::Keep,
        };

        if !TIMING_PROP.is_match(&decl.prop) {
            return Mutation::Keep;
        }

        let value = decl.value.clone();
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
        postcss_normalize_timing_functions(&mut root).unwrap();
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
    fn cubic_bezier_to_ease() {
        let out = run("a { transition-timing-function: cubic-bezier(0.25, 0.1, 0.25, 1); }");
        assert!(out.contains("ease"), "got: {out:?}");
        assert!(!out.contains("cubic-bezier"), "got: {out:?}");
    }

    #[test]
    fn cubic_bezier_to_linear() {
        let out = run("a { transition-timing-function: cubic-bezier(0, 0, 1, 1); }");
        assert!(out.contains("linear"), "got: {out:?}");
    }

    #[test]
    fn cubic_bezier_to_ease_in() {
        let out = run("a { transition-timing-function: cubic-bezier(0.42, 0, 1, 1); }");
        assert!(out.contains("ease-in"), "got: {out:?}");
    }

    #[test]
    fn cubic_bezier_to_ease_out() {
        let out = run("a { transition-timing-function: cubic-bezier(0, 0, 0.58, 1); }");
        assert!(out.contains("ease-out"), "got: {out:?}");
    }

    #[test]
    fn cubic_bezier_to_ease_in_out() {
        let out = run("a { transition-timing-function: cubic-bezier(0.42, 0, 0.58, 1); }");
        assert!(out.contains("ease-in-out"), "got: {out:?}");
    }

    #[test]
    fn cubic_bezier_unknown_unchanged() {
        let css = "a { transition-timing-function: cubic-bezier(0.1, 0.2, 0.3, 0.4); }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn steps_one_start_to_step_start() {
        let out = run("a { transition-timing-function: steps(1, start); }");
        assert!(out.contains("step-start"), "got: {out:?}");
    }

    #[test]
    fn steps_one_jump_start_to_step_start() {
        let out = run("a { transition-timing-function: steps(1, jump-start); }");
        assert!(out.contains("step-start"), "got: {out:?}");
    }

    #[test]
    fn steps_one_end_to_step_end() {
        let out = run("a { transition-timing-function: steps(1, end); }");
        assert!(out.contains("step-end"), "got: {out:?}");
    }

    #[test]
    fn steps_n_end_strips_default() {
        // `steps(4, end)` → `steps(4)`.
        let out = run("a { transition-timing-function: steps(4, end); }");
        assert!(out.contains("steps(4)"), "got: {out:?}");
    }

    #[test]
    fn steps_n_jump_end_strips_default() {
        let out = run("a { transition-timing-function: steps(4, jump-end); }");
        assert!(out.contains("steps(4)"), "got: {out:?}");
    }

    #[test]
    fn comma_list_processed() {
        let out = run(
            "a { transition: 0.5s cubic-bezier(0.25, 0.1, 0.25, 1), 1s cubic-bezier(0, 0, 1, 1); }",
        );
        assert!(out.contains("ease"), "got: {out:?}");
        assert!(out.contains("linear"), "got: {out:?}");
    }

    #[test]
    fn vendor_prefix_matches() {
        let out = run("a { -webkit-transition-timing-function: cubic-bezier(0.25, 0.1, 0.25, 1); }");
        assert!(out.contains("ease"), "got: {out:?}");
    }
}
