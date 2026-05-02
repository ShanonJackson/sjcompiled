//! crates/postcss-normalize-whitespace
//! Byte-for-byte Rust port of `postcss-normalize-whitespace@5.1.1`.
//! See `crates/PARITY_VERSIONS.md` Anomaly #2 — version pinned to 5.1.1.
//!
//! Folder/file mapping:
//!   - `node_modules/postcss-normalize-whitespace@5.1.1/src/index.js`
//!     -> `crates/postcss-normalize-whitespace/src/lib.rs` (this file).
//!
//! All bugs of upstream 5.1.1 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! 1. Walk every descendant of `css` (NOT `css` itself — postcss
//!    `walk(cb)` is `each(child, child.walk(cb))`).
//! 2. For decl / rule / atrule with truthy `raws.before`: strip every
//!    JS-`\s` whitespace character from `raws.before`.
//! 3. For decls only:
//!    - If `important`, set `raws.important = "!important"`.
//!    - First-occurrence-only regex `/\s*(\\9)\s*/` on the value (no
//!      `g` flag — single replace).
//!    - Cache-keyed `valueParser(value).walk(reduceWhitespaces).toString()`
//!      to collapse internal whitespace inside funcs / divs / spaces.
//!      Cache key is the post-IE9 value.
//!    - Custom property (`--*`) with empty post-walk value -> single space.
//!    - If `prev` sibling exists AND prev is NOT a rule, strip every `;`
//!      from `raws.before`.
//!    - Set `raws.between = ":"` and `raws.semicolon = false`.
//! 4. For rules / atrules: clear `raws.between`, `raws.after`, set
//!    `raws.semicolon = false`.
//! 5. Final `css.raws.after = ""` to drop the trailing newline.
//!
//! ## Single OnceExit hook
//!
//! Upstream registers ONLY an `OnceExit` hook. In a single-plugin
//! pipeline this is equivalent to running the function once on the
//! parsed root after every Once / per-node visitor / OnceExit-of-other-
//! plugins step has fired (none here). The parity-runner stage and the
//! NAPI bridge can call this directly on the post-parse root.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::node::{Node, NodeKind};
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, stringify as vp_stringify, walk as vp_walk};

/// Function names whose `before`/`after` raws are NOT collapsed.
/// Mirrors `variableFunctions = new Set(['var', 'env', 'constant'])`.
const VARIABLE_FUNCTION_NAMES: &[&str] = &["var", "env", "constant"];

fn is_variable_function(lower_name: &str) -> bool {
    VARIABLE_FUNCTION_NAMES.iter().any(|&v| v == lower_name)
}

/// `reduceCalcWhitespaces(node)` upstream.
fn reduce_calc_whitespaces(node: &mut VNode, _i: usize) -> Option<bool> {
    if node.kind == VKind::Space {
        node.value = " ".to_string();
    } else if node.kind == VKind::Function {
        let lower = node.value.to_lowercase();
        if !is_variable_function(&lower) {
            node.before.clear();
            node.after.clear();
        }
    }
    None
}

/// `reduceWhitespaces(node)` upstream.
///
/// Returns `Some(false)` for `calc(...)` to skip the default function-body
/// recursion — upstream's `valueParser.walk(node.nodes, reduceCalcWhitespaces)`
/// fires explicitly inside the `calc` branch instead.
fn reduce_whitespaces(node: &mut VNode, _i: usize) -> Option<bool> {
    if node.kind == VKind::Space {
        node.value = " ".to_string();
    } else if node.kind == VKind::Div {
        node.before.clear();
        node.after.clear();
    } else if node.kind == VKind::Function {
        let lower = node.value.to_lowercase();
        if !is_variable_function(&lower) {
            node.before.clear();
            node.after.clear();
        }
        if lower == "calc" {
            vp_walk(&mut node.nodes, reduce_calc_whitespaces, false);
            return Some(false);
        }
    }
    None
}

/// JS regex `/\s*(\\9)\s*/` — match the literal two-byte sequence `\9`
/// optionally surrounded by whitespace. NO global flag: only the FIRST
/// match is replaced. The character class explicitly enumerates every
/// codepoint matched by ECMAScript `\s` (WhiteSpace + LineTerminator)
/// because Rust regex `\s` (Unicode mode) covers `\p{White_Space}`
/// which differs from ECMAScript at U+FEFF (in JS `\s`, not in
/// `White_Space`) and U+0085 (in `White_Space`, not in JS `\s`).
static IE9_HACK_REGEX: Lazy<Regex> = Lazy::new(|| {
    let ws_class = r"[\t\n\x0B\x0C\r \u{00A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}]";
    let pattern = format!(r"{ws_class}*(\\9){ws_class}*");
    Regex::new(&pattern).expect("IE9_HACK_REGEX must compile")
});

/// JS `\s` per ECMAScript spec — WhiteSpace + LineTerminator. Used
/// for the per-character `replace(/\s/g, '')` calls upstream. See
/// `IE9_HACK_REGEX` for why we hand-roll instead of relying on
/// regex `\s`.
fn is_es_whitespace(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' '
        | '\u{00A0}' | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
        | '\u{FEFF}'
    )
}

fn strip_es_whitespace(s: &str) -> String {
    s.chars().filter(|c| !is_es_whitespace(*c)).collect()
}

fn strip_semicolons(s: &str) -> String {
    s.chars().filter(|c| *c != ';').collect()
}

/// Plugin entrypoint — mirrors upstream's `OnceExit(css)` body.
pub fn postcss_normalize_whitespace(root: &mut Root) -> PluginResult {
    let mut cache: IndexMap<String, String> = IndexMap::new();
    walk_tree(&mut root.root, &mut cache);
    // Remove final newline.
    root.root.raws.after = Some(String::new());
    Ok(())
}

/// Pre-order DFS over `parent`'s descendants. Mirrors postcss
/// `Container.walk(cb)` — visits the child, then descends. Does NOT
/// visit `parent` itself. Captures the previous sibling's `is_rule`
/// flag *before* mutating the current child so the decl branch can
/// answer `node.prev() && prev.type !== 'rule'` without a second
/// borrow of `parent.nodes`.
fn walk_tree(parent: &mut Node, cache: &mut IndexMap<String, String>) {
    let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
    for i in 0..len {
        let prev_is_rule_opt = if i == 0 {
            None
        } else {
            Some(matches!(parent.nodes().unwrap()[i - 1].kind, NodeKind::Rule(_)))
        };
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            process_node(child, prev_is_rule_opt, cache);
        }
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            walk_tree(child, cache);
        }
    }
}

/// `cb(node)` body upstream — handles one node based on its type.
/// `prev_is_rule_opt`:
///   - `None` — no previous sibling.
///   - `Some(true)` — prev sibling is a Rule.
///   - `Some(false)` — prev sibling exists and is NOT a Rule.
fn process_node(
    node: &mut Node,
    prev_is_rule_opt: Option<bool>,
    cache: &mut IndexMap<String, String>,
) {
    let is_decl = matches!(node.kind, NodeKind::Declaration(_));
    let is_rule = matches!(node.kind, NodeKind::Rule(_));
    let is_atrule = matches!(node.kind, NodeKind::AtRule(_));

    // Step 1: decl/rule/atrule with truthy raws.before -> strip \s.
    if is_decl || is_rule || is_atrule {
        let needs = matches!(&node.raws.before, Some(b) if !b.is_empty());
        if needs {
            let stripped = strip_es_whitespace(node.raws.before.as_deref().unwrap());
            node.raws.before = Some(stripped);
        }
    }

    if is_decl {
        // Step 2a: !important → "!important"
        let important = match &node.kind {
            NodeKind::Declaration(d) => d.important,
            _ => false,
        };
        if important {
            node.raws.important = Some("!important".to_string());
        }

        // Step 2b: IE9 hack regex (NO global flag — single replace).
        let original_value = match &node.kind {
            NodeKind::Declaration(d) => d.value.clone(),
            _ => String::new(),
        };
        let post_ie9: String = IE9_HACK_REGEX.replace(&original_value, "$1").into_owned();
        if let NodeKind::Declaration(d) = &mut node.kind {
            d.value = post_ie9.clone();
        }

        // Step 2c: cache-keyed value normalization. Key is post-IE9
        // value; cached result is the walked-then-stringified output.
        let normalized = if let Some(cached) = cache.get(&post_ie9) {
            cached.clone()
        } else {
            let mut parsed = vp_parse(&post_ie9);
            vp_walk(&mut parsed, reduce_whitespaces, false);
            let result = vp_stringify(&parsed);
            cache.insert(post_ie9.clone(), result.clone());
            result
        };

        // Step 2d: --* with empty value -> " ".
        let prop = match &node.kind {
            NodeKind::Declaration(d) => d.prop.clone(),
            _ => String::new(),
        };
        let mut final_value = normalized;
        if prop.starts_with("--") && final_value.is_empty() {
            final_value = " ".to_string();
        }
        if let NodeKind::Declaration(d) = &mut node.kind {
            d.value = final_value;
        }

        // Step 2e: strip semicolons from raws.before iff prev exists
        // and prev is not a rule. JS check `if (node.raws.before)`
        // also gates on truthy (post-Step-1 raws.before may be empty).
        if matches!(prev_is_rule_opt, Some(false)) {
            let needs = matches!(&node.raws.before, Some(b) if !b.is_empty());
            if needs {
                let before = node.raws.before.as_deref().unwrap();
                node.raws.before = Some(strip_semicolons(before));
            }
        }

        node.raws.between = Some(":".to_string());
        node.raws.semicolon = Some(false);
    } else if is_rule || is_atrule {
        node.raws.between = Some(String::new());
        node.raws.after = Some(String::new());
        node.raws.semicolon = Some(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_normalize_whitespace(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn collapses_decl_value_whitespace_inside_calc() {
        let out = run("a { width: calc(  1px +   2px  ); }");
        assert!(out.contains("calc(1px + 2px)"), "got: {out:?}");
    }

    #[test]
    fn preserves_var_function_before_and_after() {
        // var() Function's `before`/`after` are exempt (variableFunctions
        // set), so the leading and trailing space inside the parens is
        // preserved. Div nodes (commas) inside DO get their before/after
        // cleared by the default reducer recursion.
        let out = run("a { color: var( --x , red ); }");
        assert!(out.contains("var( --x,red )"), "got: {out:?}");
    }

    #[test]
    fn collapses_ie9_hack_whitespace() {
        let out = run("a { color: red \\9; }");
        assert!(out.contains("color:red\\9"), "got: {out:?}");
    }

    #[test]
    fn empty_custom_property_becomes_space() {
        let out = run("a { --x: ; }");
        assert!(out.contains("--x: "), "got: {out:?}");
    }

    #[test]
    fn important_collapsed() {
        let out = run("a { color: red  !important; }");
        assert!(out.contains("!important"), "got: {out:?}");
        assert!(!out.contains(" !important"), "got: {out:?}");
    }

    #[test]
    fn rule_between_and_after_cleared() {
        let out = run("a { color: red; }");
        // Selector then `{` directly (no space), no trailing newline before `}`.
        assert!(out.starts_with("a{"), "got: {out:?}");
        assert!(out.ends_with("}"), "got: {out:?}");
    }

    #[test]
    fn atrule_between_and_after_cleared() {
        let out = run("@media (min-width: 100px) { a { color: red } }");
        assert!(out.contains("@media (min-width: 100px){"), "got: {out:?}");
    }

    #[test]
    fn final_newline_removed() {
        let out = run("a { color: red; }\n");
        assert!(!out.ends_with('\n'), "got: {out:?}");
    }

    #[test]
    fn cache_hit_yields_same_value() {
        // Two decls with the same value — second hit goes through cache.
        let out = run("a { width: calc( 1px ); height: calc( 1px ); }");
        // Both should produce `calc(1px)`.
        assert_eq!(out.matches("calc(1px)").count(), 2, "got: {out:?}");
    }

    #[test]
    fn keeps_url_function_inner_unchanged_at_top() {
        // url() is not a variableFunction but its inner is parsed as a
        // single Word by value-parser when unquoted; before/after of url()
        // get cleared.
        let out = run("a { background: url( foo.png ); }");
        assert!(out.contains("url(foo.png)"), "got: {out:?}");
    }

    #[test]
    fn comments_between_decls_pass_through_raws_before_unchanged() {
        // Comment node — type !== decl/rule/atrule, so raws.before is NOT
        // stripped. (The comment text itself is kept.)
        let out = run("a { /* hi */ color: red; }");
        assert!(out.contains("/* hi */"), "got: {out:?}");
    }
}
