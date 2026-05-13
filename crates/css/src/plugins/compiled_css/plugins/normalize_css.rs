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
//! unicode`, `postcss-reduce-initial`); autoprefixer (a 6th consumer)
//! lives outside this preset but reads the same browserslist data via
//! `crates/css/src/transform.rs`'s autoprefixer step.
//!
//! ### Resolution path (post-`DEFINITIVE_BROWSERSLIST_PLAN.md`)
//!
//! Browserslist is resolved **on the host** (NAPI side) via
//! `cssnano_browserslist_snapshot::precompute_browserslist`, anchored
//! on `require.resolve('postcss-reduce-initial/package.json')` so the
//! upward `find_config` walk lands on the exact same `.browserslistrc`
//! the leaf plugin would find at runtime. The resolved
//! `PrecomputedBrowserslist` is postcard-encoded and threaded into
//! the preset via `PresetOpts::browserslist_snapshot`, which the 5
//! cssnano leaf plugins consume through their `*_with_snapshot`
//! entry points.
//!
//! Why a precomputed snapshot instead of in-plugin resolution: the
//! WASI sandbox the SWC plugin runs inside has no environment-
//! variable passthrough and only a `/cwd` preopen, so
//! `browserslist_shim::find_config_file`'s upward FS walk fails
//! silently for any project whose `.browserslistrc` lives outside
//! cwd (the AFM monorepo case). Hosting the resolution on the
//! Node side and shipping bytes across the boundary matches the
//! same pattern `autoprefixer::precomputed::PrecomputedPrefixes`
//! uses for the prefix tables.
//!
//! ### Delivery surfaces
//!
//! - `TransformOpts::precomputed_browserslist`: inline `Vec<u8>`,
//!   used by direct NAPI callers (round-trips a `Buffer`).
//! - `TransformOpts::precomputed_browserslist_path`: filesystem
//!   path, used by the SWC plugin path because plugin options
//!   serialize as JSON and arbitrary postcard bytes do not
//!   JSON-encode safely. Path is translated host→WASI via
//!   `babel-plugin/src/compat/wasi_path.rs::host_to_wasi`.
//!
//! Precedence: inline > path > slow build (live
//! `browserslist_shim::resolve`).
//!
//! Cross-pipeline byte-equality between the env-pinned resolution
//! and the snapshot resolution is gated by the Phase E7 test at
//! `crates/babel-plugin/tests/transform_css_browserslist_snapshot_integration.rs`.

use postcss_core::{PluginResult, Root};

use crate::plugins::cssnano_preset_default::{default_preset, PresetOpts};

use super::normalize_current_color::normalize_current_color;

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
    // PresetOpts is the threading mechanism for the host-resolved
    // browserslist snapshot (`PrecomputedBrowserslist`) — the
    // `compiled-css` orchestrator does NOT receive a snapshot
    // (consumed exclusively via `crates/css::transform_css`'s opts);
    // here we always pass `PresetOpts::default()`, which means
    // `browserslist_snapshot: None` and every leaf plugin falls back
    // to its in-process resolution. Byte-equivalent to the
    // pre-2026-05-08 signature for this call site.
    let preset_opts = PresetOpts::default();
    let preset = default_preset(&preset_opts);
    for entry in &preset.plugins {
        if plugins_to_include.contains(entry.name) {
            (entry.apply)(root, &preset_opts)?;
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
