//! Port of `packages/css/src/plugins/normalize-current-color.ts`.
//!
//! Upstream JS:
//! ```ts
//! Declaration(declaration) {
//!   const lowerValue = declaration.value.toLowerCase();
//!   if (lowerValue === 'currentcolor' || lowerValue === 'current-color') {
//!     declaration.value = 'currentColor';
//!   }
//! }
//! ```
//!
//! Single-pass Declaration visitor: any decl whose value (case-insensitive)
//! equals `"currentcolor"` or `"current-color"` is rewritten to the
//! canonical `"currentColor"`. Anything else passes through unchanged.
//!
//! ## Why ASCII lowercase, not Unicode?
//! `String.prototype.toLowerCase()` in JS folds Unicode case (e.g.
//! `Σ → σ`). We use `to_ascii_lowercase` here because the comparison
//! targets are pure ASCII (`currentcolor`, `current-color`) and Unicode
//! folding could mis-match exotic inputs. If a real-world value contains
//! non-ASCII characters that fold to ASCII letters (rare for CSS values),
//! we'd diverge from upstream — extend the comparison if a corpus
//! reveals it.

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::{NodeKind, PluginResult, Root};

pub fn normalize_current_color(root: &mut Root) -> PluginResult {
    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        if let NodeKind::Declaration(d) = &mut node.kind {
            let lower = d.value.to_ascii_lowercase();
            if lower == "currentcolor" || lower == "current-color" {
                d.value = "currentColor".to_string();
                // Drop the cached raw value so the stringifier re-emits
                // the new value rather than the original bytes.
                node.raws.value = None;
            }
        }
        Mutation::Keep
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        normalize_current_color(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn rewrites_lowercase_currentcolor() {
        let out = run("a { color: currentcolor; }");
        assert!(out.contains("currentColor"), "got: {out:?}");
        assert!(!out.contains("currentcolor"));
    }

    #[test]
    fn rewrites_kebab_current_color() {
        let out = run("a { color: current-color; }");
        assert!(out.contains("currentColor"), "got: {out:?}");
    }

    #[test]
    fn rewrites_mixed_case_currentcolor() {
        let out = run("a { color: CurrentColor; }");
        assert!(out.contains("currentColor"));
    }

    #[test]
    fn leaves_canonical_currentcolor_alone() {
        // `to_ascii_lowercase("currentColor") == "currentcolor"` triggers
        // the first branch; the value is overwritten with the canonical
        // form. After overwrite, bytes match the original input so the
        // round-tripped output equals input.
        let css = "a { color: currentColor; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn leaves_unrelated_values_alone() {
        let css = "a { color: red; background: currentColors; }";
        let out = run(css);
        assert_eq!(out, css);
    }

    #[test]
    fn rewrites_inside_rule() {
        let out = run(".x { background: CURRENTCOLOR; }");
        assert!(out.contains("currentColor"));
    }

    #[test]
    fn rewrites_inside_atrule() {
        let out = run("@media { .x { color: CURRENTCOLOR; } }");
        assert!(out.contains("currentColor"));
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
