//! Port of `packages/css/src/sort.ts`.
//!
//! Second hashing entry point per `PARITY_VERSIONS.md`. Pipeline:
//!
//! ```text
//! postcss-discard-duplicates@6.0.0   (Phase 5c)
//! mergeDuplicateAtRules (local)      (Phase 4c)
//! sortAtomicStyleSheet (local)       (Phase 4c)
//! ```
//!
//! All three components are byte-clean (verified by parity-runner). This
//! function composes them. The error path mirrors upstream postcss: if
//! parsing fails, JS would throw — we propagate the parse error string.

use serde::{Deserialize, Serialize};

use compiled_css::plugins::merge_duplicate_at_rules::{finalize, visit};
use compiled_css::plugins::sort_atomic_style_sheet::{
    sort_atomic_style_sheet, SortAtomicStyleSheetOpts,
};
use postcss_core::{parse, stringify};
use postcss_discard_duplicates::postcss_discard_duplicates;

/// Mirrors upstream sort options shape (line 18-26 of `sort.ts`). The
/// upstream `undefined` defaults are *intentional* — they must propagate
/// down to the plugin so its own defaults take effect (see comment on the
/// JS file).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SortOpts {
    #[serde(rename = "sortAtRulesEnabled", default)]
    pub sort_at_rules_enabled: Option<bool>,
    #[serde(rename = "sortShorthandEnabled", default)]
    pub sort_shorthand_enabled: Option<bool>,
}

/// `sort(stylesheet, opts)` — packages/css/src/sort.ts:13.
///
/// **Plugin execution order matches postcss's lifecycle, not the array
/// order in `sort.ts`.** Postcss runs all `Once` hooks first (in plugin
/// array order), then per-node visitors (depth-first walk), then all
/// `OnceExit` hooks. The three plugins in the array are:
///
/// | Position | Plugin                       | Hooks                       |
/// |----------|------------------------------|-----------------------------|
/// | 0        | postcss-discard-duplicates@6 | OnceExit only               |
/// | 1        | mergeDuplicateAtRules        | AtRule visitor + OnceExit   |
/// | 2        | sortAtomicStyleSheet         | Once only                   |
///
/// So the actual lifecycle order is:
///
/// 1. `sortAtomicStyleSheet` Once
/// 2. `mergeDuplicateAtRules` AtRule visitor (= [`visit`])
/// 3. `postcss-discard-duplicates` OnceExit
/// 4. `mergeDuplicateAtRules` OnceExit (= [`finalize`])
///
/// The naive "call them in array order" approach silently produces a
/// different tree because `sortAtomicStyleSheet` reorders top-level
/// nodes, which changes which nodes occupy index 0 of root and therefore
/// changes which `Root.removeChild` raws-transfers fire later in the
/// pipeline. See `crates/STATUS.md` for the divergent-byte trace that
/// uncovered this.
pub fn sort(stylesheet: &str, opts: &SortOpts) -> Result<String, String> {
    let mut root = parse(stylesheet).map_err(|e| format!("parse error: {e}"))?;

    // 1. sortAtomicStyleSheet.Once
    let plugin_opts = SortAtomicStyleSheetOpts {
        sort_at_rules_enabled: opts.sort_at_rules_enabled,
        sort_shorthand_enabled: opts.sort_shorthand_enabled,
    };
    sort_atomic_style_sheet(&mut root, &plugin_opts)
        .map_err(|e| format!("sort-atomic-style-sheet: {e:?}"))?;

    // 2. mergeDuplicateAtRules.AtRule visitor pass.
    let merge_store = visit(&mut root);

    // 3. postcss-discard-duplicates.OnceExit (plugin index 0 — fires before
    //    plugin index 1's OnceExit).
    postcss_discard_duplicates(&mut root)
        .map_err(|e| format!("postcss-discard-duplicates: {e:?}"))?;

    // 4. mergeDuplicateAtRules.OnceExit.
    finalize(&mut root, merge_store);

    Ok(stringify(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_simple_input() {
        let css = "a { color: red; }";
        let out = sort(css, &SortOpts::default()).unwrap();
        assert!(out.contains("color: red"));
    }

    #[test]
    fn drops_top_level_dup_decls() {
        // postcss-discard-duplicates@6 dedupes top-level decls.
        let out = sort("color: red; color: red;", &SortOpts::default()).unwrap();
        assert_eq!(out.matches("color: red").count(), 1, "got: {out:?}");
    }

    #[test]
    fn merges_dup_at_rules() {
        let css = "@media (max-width: 100px) { a { color: red; } }\n@media (max-width: 100px) { a { color: blue; } }";
        let out = sort(css, &SortOpts::default()).unwrap();
        assert_eq!(out.matches("@media (max-width: 100px)").count(), 1, "got: {out:?}");
    }
}
