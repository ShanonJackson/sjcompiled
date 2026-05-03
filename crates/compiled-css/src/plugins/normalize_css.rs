//! Port of `packages/css/src/plugins/normalize-css.ts`.
//!
//! Wrapper around `cssnano-preset-default@5.2.14` plus the local
//! `normalize-current-color` plugin. Filter-then-execute order matches the
//! cssnano-preset-default source order — see `PARITY_VERSIONS.md` Anomaly #7.
//!
//! ## Lifecycle ordering — load-bearing
//!
//! Of the 14 AFM-hashing-path cssnano sub-plugins, **all 14 use only
//! `OnceExit`**. The custom `normalize-current-color` plugin uses a
//! `Declaration` visitor (no Once/OnceExit). Postcss's lifecycle is:
//!
//!   1. Once hooks (in plugin-array order) — none of the 15 plugins here.
//!   2. Per-node visitors (depth-first walk) — `normalize-current-color`'s
//!      `Declaration` visitor fires here.
//!   3. OnceExit hooks (in plugin-array order) — all 14 cssnano plugins.
//!
//! In JS, `normalize-css.ts` constructs the array as
//! `[…filtered cssnano plugins (preset source order), normalizeCurrentColor]`
//! when `optimizeCss` is true. `Array.filter` preserves the source order
//! of the preset, so the OnceExit firing order is preset source order.
//!
//! This Rust port reproduces both passes in that exact sequence: walk pass
//! first (`normalize_current_color`), then OnceExits in preset source order.
//! See `crates/css/src/sort.rs` "Lifecycle ordering — load-bearing" for the
//! same hazard surfaced in Phase 8a.
//!
//! ## Browserslist
//!
//! Five of the 14 plugins are browserslist-aware (`postcss-colormin`,
//! `postcss-convert-values`, `postcss-minify-params`, `postcss-normalize-
//! unicode`, `postcss-reduce-initial`). They each call
//! `browserslist_shim::resolve("", true)` internally, which honours the
//! `BROWSERSLIST` environment variable (mirroring upstream
//! `browserslist(null, ...)`). The parity gate sets BROWSERSLIST in both
//! Rust and JS processes so both engines resolve to the same query.

use postcss_core::{PluginResult, Root};

use cssnano_preset_default::{default_preset, PresetOpts};

use crate::plugins::normalize_current_color::normalize_current_color;

/// `BASE_PLUGINS` from `packages/css/src/plugins/normalize-css.ts:44-50`.
/// These run regardless of `optimizeCss`.
const BASE_PLUGINS: &[&str] = &[
    "postcss-minify-selectors",
    "postcss-minify-params",
];

/// `PROD_PLUGINS` from `packages/css/src/plugins/normalize-css.ts:13-39`.
/// These run only when `optimizeCss` is true.
const PROD_PLUGINS: &[&str] = &[
    "postcss-ordered-values",
    "postcss-reduce-initial",
    "postcss-convert-values",
    "postcss-colormin",
    "postcss-normalize-url",
    "postcss-normalize-unicode",
    "postcss-normalize-string",
    "postcss-normalize-positions",
    "postcss-normalize-timing-functions",
    "postcss-minify-gradients",
    "postcss-discard-comments",
    "postcss-calc",
];

#[derive(Debug, Clone, Default)]
pub struct NormalizeCssOpts {
    /// Mirrors `optimizeCss?` — `None` defaults to `true` per JS line 59.
    pub optimize_css: Option<bool>,
}

/// `normalizeCSS(opts)` from `packages/css/src/plugins/normalize-css.ts:58`,
/// applied to a parsed `Root`. Mirrors `postcss(normalizeCSS(opts)).process(css)`
/// in JS (which is how `transformCss` consumes it via spread), with the full
/// postcss lifecycle (walk → OnceExit) replayed inside this single function.
pub fn normalize_css(root: &mut Root, opts: &NormalizeCssOpts) -> PluginResult {
    let optimize_css = opts.optimize_css.unwrap_or(true);

    let plugins_to_include: std::collections::HashSet<&str> = if optimize_css {
        BASE_PLUGINS.iter().chain(PROD_PLUGINS.iter()).copied().collect()
    } else {
        BASE_PLUGINS.iter().copied().collect()
    };

    // Step 2 (walk pass): the only visitor in this band is
    // `normalize-current-color`'s `Declaration` visitor. It is appended to
    // the plugin array AFTER the cssnano filter (normalize-css.ts:76-78),
    // but its visitor still fires during the single walk pass that postcss
    // runs BEFORE all OnceExits. Skipped when `optimize_css` is false
    // (JS only pushes it inside `if (optimizeCss)`).
    if optimize_css {
        normalize_current_color(root)?;
    }

    // Step 3 (OnceExit): run each preset plugin in source order, restricted
    // to `plugins_to_include`. JS `Array.filter` preserves source order, and
    // postcss fires OnceExit hooks in array order — so the filtered preset
    // source order IS the OnceExit firing order.
    let preset = default_preset(&PresetOpts::default());
    for entry in &preset.plugins {
        if plugins_to_include.contains(entry.name) {
            (entry.apply)(root)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run_default(css: &str) -> String {
        let mut root = parse(css).unwrap();
        normalize_css(root_mut(&mut root), &NormalizeCssOpts::default()).unwrap();
        stringify(&root)
    }

    fn root_mut<'a>(r: &'a mut Root) -> &'a mut Root { r }

    #[test]
    fn passes_through_blank_input() {
        assert_eq!(run_default(""), "");
    }

    #[test]
    fn applies_normalize_current_color_under_default_optimize() {
        // optimize_css defaults to true → normalize-current-color runs.
        let out = run_default("a { color: currentcolor; }");
        assert!(out.contains("currentColor"), "got: {out:?}");
    }

    #[test]
    fn skips_normalize_current_color_when_optimize_off() {
        let mut root = parse("a { color: currentcolor; }").unwrap();
        normalize_css(
            &mut root,
            &NormalizeCssOpts { optimize_css: Some(false) },
        )
        .unwrap();
        let out = stringify(&root);
        // optimize_css=false → only BASE_PLUGINS (minify-selectors,
        // minify-params) run. normalize-current-color is gated on
        // optimize_css and is skipped, so the original `currentcolor` byte
        // sequence survives.
        assert!(out.contains("currentcolor"), "got: {out:?}");
    }

    #[test]
    fn discards_non_important_comments_under_default_optimize() {
        // postcss-discard-comments is in PROD_PLUGINS — runs at default.
        let out = run_default("/* dropped */ a { color: red; }");
        assert!(!out.contains("dropped"), "got: {out:?}");
    }

    #[test]
    fn keeps_important_comments_under_default_optimize() {
        let out = run_default("/*! kept */ a { color: red; }");
        assert!(out.contains("kept"), "got: {out:?}");
    }

    #[test]
    fn minify_selectors_runs_under_optimize_off() {
        // minify-selectors IS in BASE_PLUGINS, so it should run even with
        // optimize_css=false. Sortable selectors get reordered.
        let mut root = parse(".b, .a { color: red; }").unwrap();
        normalize_css(
            &mut root,
            &NormalizeCssOpts { optimize_css: Some(false) },
        )
        .unwrap();
        let out = stringify(&root);
        // After minify-selectors, lex-sorted: `.a, .b` (or with no space —
        // depends on minify-selectors output). Either way `.a` appears
        // before `.b`.
        let a = out.find(".a").expect("has .a");
        let b = out.find(".b").expect("has .b");
        assert!(a < b, "selectors not lex-sorted: {out:?}");
    }
}
