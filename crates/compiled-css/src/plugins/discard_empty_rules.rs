//! Port of `packages/css/src/plugins/discard-empty-rules.ts`.
//!
//! Upstream JS:
//! ```ts
//! const isValueEmpty = (value: string): boolean =>
//!   value === 'undefined' || value === 'null' || value.trim() === '';
//!
//! Declaration(node) {
//!   if (isValueEmpty(node.value)) {
//!     const { parent } = node;
//!     node.remove();
//!     if (parent?.type === 'rule' && parent.nodes.length === 0) {
//!       parent.remove();
//!     }
//!   }
//! }
//! ```
//!
//! Two parity points worth flagging:
//! - The parent-removal branch only fires when `parent.type === 'rule'`.
//!   A Rule that becomes empty as a side-effect of removing its decls is
//!   removed; an AtRule that becomes empty is left in place. An *already*
//!   empty Rule (one that came in with zero children) is also left in place
//!   because the visitor never fires on it.
//! - `value.trim() === ''` uses JavaScript's `String.prototype.trim`, which
//!   strips ASCII whitespace + Unicode `Space_Separator` + line terminators
//!   + ZWNBSP (U+FEFF). Rust's `str::trim` is based on the Unicode
//!   `White_Space` property and does NOT include U+FEFF. We bridge the gap
//!   in [`is_value_empty`] so a value of just `\u{FEFF}` is treated as
//!   empty, matching JS.

use postcss_core::container::remove_at;
use postcss_core::{Node, NodeKind, PluginResult, Root};

pub fn discard_empty_rules(root: &mut Root) -> PluginResult {
    process_container(&mut root.root);
    Ok(())
}

/// Walk `parent`'s direct children left-to-right, removing empty
/// Declarations and recursing into containers. Returns `true` if at
/// least one Declaration was removed from `parent`'s direct child list
/// — the caller uses that to decide whether to remove `parent` itself
/// when it's a Rule that became empty.
fn process_container(parent: &mut Node) -> bool {
    if parent.nodes().is_none() {
        return false;
    }

    let mut removed_decl_here = false;
    let mut i = 0usize;
    loop {
        let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }

        // Inspect kind without holding a borrow across mutation.
        let (is_container, is_empty_decl) = {
            let child = &parent.nodes().unwrap()[i];
            let cont = matches!(child.kind, NodeKind::Rule(_) | NodeKind::AtRule(_));
            let empty_decl = if let NodeKind::Declaration(d) = &child.kind {
                is_value_empty(&d.value)
            } else {
                false
            };
            (cont, empty_decl)
        };

        if is_container {
            // Recurse first so nested empty decls get cleaned up before we
            // decide whether the child itself is now an empty Rule.
            let child_lost_decl = {
                let child = &mut parent.nodes_mut().unwrap()[i];
                process_container(child)
            };

            // Upstream's parent-removal branch: only Rule (not AtRule) gets
            // removed, and only if removing a Declaration is what emptied
            // it (already-empty Rules are left alone — the upstream
            // Declaration visitor never fires on them).
            let should_drop_child = {
                let child = &parent.nodes().unwrap()[i];
                child_lost_decl
                    && matches!(child.kind, NodeKind::Rule(_))
                    && child.nodes().map_or(false, |n| n.is_empty())
            };
            if should_drop_child {
                remove_at(parent, i);
                continue; // cursor stays on this index, now points at the next sibling
            }
            i += 1;
        } else if is_empty_decl {
            remove_at(parent, i);
            removed_decl_here = true;
            // cursor stays — next sibling slid down to this index
        } else {
            i += 1;
        }
    }

    removed_decl_here
}

/// Mirrors upstream's `isValueEmpty`. The `'undefined' / 'null'` literal
/// checks are byte-exact; the `value.trim() === ''` branch uses a
/// JS-equivalent whitespace set (Rust `is_whitespace` + U+FEFF).
fn is_value_empty(value: &str) -> bool {
    if value == "undefined" || value == "null" {
        return true;
    }
    js_trim_is_empty(value)
}

fn js_trim_is_empty(s: &str) -> bool {
    s.chars().all(is_js_whitespace)
}

fn is_js_whitespace(c: char) -> bool {
    // Rust `char::is_whitespace` covers ASCII whitespace, NBSP, NEL,
    // Space_Separator, line/paragraph separators (LS/PS) — i.e. the Unicode
    // `White_Space` property. JS `String.prototype.trim` strips the same set
    // PLUS U+FEFF (ZWNBSP / BOM), which Rust deliberately excludes.
    c.is_whitespace() || c == '\u{FEFF}'
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        discard_empty_rules(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn drops_undefined_value() {
        let out = run("a { color: undefined; background: blue; }");
        assert!(!out.contains("undefined"), "got: {out:?}");
        assert!(out.contains("background: blue"));
    }

    #[test]
    fn drops_null_value() {
        let out = run("a { color: null; background: blue; }");
        assert!(!out.contains("null"), "got: {out:?}");
        assert!(out.contains("background: blue"));
    }

    #[test]
    fn drops_empty_value() {
        let out = run("a { color: ; background: blue; }");
        assert!(!out.contains("color:"), "got: {out:?}");
        assert!(out.contains("background: blue"));
    }

    #[test]
    fn drops_whitespace_only_value() {
        let out = run("a { color:    ; background: blue; }");
        assert!(!out.contains("color:"), "got: {out:?}");
    }

    #[test]
    fn removes_rule_when_only_decl_was_empty() {
        let out = run(":hover { display: undefined; }");
        assert!(!out.contains(":hover"), "rule should be gone, got: {out:?}");
    }

    #[test]
    fn keeps_rule_when_at_least_one_decl_survives() {
        let out = run(":hover { display: undefined; color: red; }");
        assert!(out.contains(":hover"));
        assert!(out.contains("color: red"));
        assert!(!out.contains("display"));
    }

    #[test]
    fn keeps_already_empty_rule() {
        // Upstream's Declaration visitor never fires on an already-empty
        // rule, so it stays.
        let out = run("a {}\nb { color: red; }");
        assert!(out.contains("a {}"), "already-empty rule should remain: {out:?}");
        assert!(out.contains("color: red"));
    }

    #[test]
    fn keeps_word_undefined_inside_value() {
        let out = run("a { font-family: undefined-font; color: red; }");
        assert!(out.contains("undefined-font"), "got: {out:?}");
    }

    #[test]
    fn keeps_url_with_undefined() {
        let out = run("a { background: url(undefined); color: red; }");
        assert!(out.contains("url(undefined)"), "got: {out:?}");
    }

    #[test]
    fn drops_important_with_undefined() {
        let out = run("a { color: undefined !important; background: blue; }");
        assert!(!out.contains("undefined"));
        assert!(out.contains("background: blue"));
    }

    #[test]
    fn does_not_drop_at_rule_when_inner_decls_emptied() {
        // @media is an AtRule, not a Rule — upstream leaves it even if all
        // its children get emptied.
        let css = "@media (max-width: 100px) { display: undefined; }";
        let out = run(css);
        assert!(out.contains("@media"), "AtRule must remain: {out:?}");
    }

    #[test]
    fn drops_inner_rule_inside_at_rule() {
        let css = "@media (max-width: 100px) { a { color: undefined; } b { color: red; } }";
        let out = run(css);
        assert!(out.contains("@media"));
        assert!(!out.contains("a {"), "inner empty rule should be gone: {out:?}");
        assert!(out.contains("b { color: red; }"));
    }

    #[test]
    fn handles_multiple_empties_in_one_rule() {
        let css = "a { color: undefined; font-size: null; background: ; border: 1px solid red; }";
        let out = run(css);
        assert!(out.contains("border: 1px solid red"));
        assert!(!out.contains("color:"));
        assert!(!out.contains("font-size"));
        assert!(!out.contains("background"));
    }

    #[test]
    fn no_op_on_blank_input() {
        let out = run("");
        assert_eq!(out, "");
    }

    #[test]
    fn no_op_on_clean_input() {
        let css = "a { color: red; }\nb { font-size: 12px; background: blue; }";
        let out = run(css);
        assert_eq!(out, css, "clean input must round-trip identically");
    }

    #[test]
    fn is_value_empty_matches_upstream_branches() {
        assert!(is_value_empty("undefined"));
        assert!(is_value_empty("null"));
        assert!(is_value_empty(""));
        assert!(is_value_empty("   "));
        assert!(is_value_empty("\t\n  "));
        // Word "undefined" embedded — NOT empty.
        assert!(!is_value_empty("undefined-font"));
        assert!(!is_value_empty(" undefined "));   // surrounding ws but non-empty content
        assert!(!is_value_empty("url(undefined)"));
        assert!(!is_value_empty("0"));
        // BOM-only — JS trims it, Rust must too.
        assert!(is_value_empty("\u{FEFF}"));
    }
}
