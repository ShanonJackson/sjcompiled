//! Byte-for-byte Rust port of `cssnano-preset-default@5.2.14`.
//!
//! Folder/file mapping (1:1 with `crates/_vendor/cssnano-preset-default-5.2.14/src/`):
//!   - `index.js` -> `src/lib.rs` (this file — single-file upstream)
//!
//! Upstream is a **tuple-list factory** — it does not invoke any plugins.
//! It returns `{ plugins: [[creator, options], ...] }` in a fixed source
//! order. The consumer (`packages/css/src/plugins/normalize-css.ts`)
//! filters the list by `plugin.postcssPlugin` against
//! `BASE_PLUGINS ∪ PROD_PLUGINS` and applies the survivors **in
//! preset source order** (Anomaly #7 in `PARITY_VERSIONS.md`).
//!
//! Per Anomaly #8: `normalize-css.ts:69` calls `creator()` with **no
//! arguments**, so the second tuple slot (`options.X`) is *dropped*
//! before reaching each plugin on AFM's hashing path. We model both
//! anyway for 1:1 fidelity — `PluginEntry::name` covers the filter,
//! `PluginEntry::apply` covers the no-args invocation.
//!
//! Of the 29 entries, 14 are on AFM's hashing path. The remaining 15
//! survive `normalize-css.ts`'s filter only if a future change adds
//! them to `BASE_PLUGINS`/`PROD_PLUGINS`; today they're dead weight.
//! For those, `apply` returns `Err("not on AFM hashing path")` so any
//! drift in the consumer filter that suddenly reaches one fails loud.

use postcss_core::{PluginError, PluginResult, Root};

use cssnano_postcss_colormin::postcss_colormin;
use cssnano_postcss_convert_values::{postcss_convert_values, ConvertValuesOpts};
use cssnano_postcss_discard_comments::{postcss_discard_comments, DiscardCommentsOpts};
use cssnano_postcss_minify_gradients::postcss_minify_gradients;
use cssnano_postcss_minify_params::postcss_minify_params;
use cssnano_postcss_minify_selectors::postcss_minify_selectors;
use cssnano_postcss_normalize_positions::postcss_normalize_positions;
use cssnano_postcss_normalize_string::{postcss_normalize_string, NormalizeStringOpts};
use cssnano_postcss_normalize_timing_functions::postcss_normalize_timing_functions;
use cssnano_postcss_normalize_unicode::postcss_normalize_unicode;
use cssnano_postcss_normalize_url::{postcss_normalize_url, NormalizeUrlOpts};
use cssnano_postcss_ordered_values::postcss_ordered_values;
use cssnano_postcss_reduce_initial::{postcss_reduce_initial, PostcssReduceInitialOpts};
use postcss_calc::{postcss_calc, Options as CalcOptions};

/// Apply function signature — calls the plugin with **default options**
/// (per Anomaly #8: `creator()` from `normalize-css.ts:69`).
pub type PluginApply = fn(&mut Root) -> PluginResult;

/// One entry in the preset's plugin tuple list. Mirrors
/// `[creator, options]` from upstream `defaultPreset()` return value,
/// but elides the `options` slot (dropped by the AFM consumer per
/// Anomaly #8). `name` is `creator().postcssPlugin` — the string the
/// `normalize-css.ts` filter compares against.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// Matches `plugin.postcssPlugin` from upstream invocation.
    pub name: &'static str,
    /// Applies the plugin to a `Root` with default options. For
    /// plugins not on AFM's hashing path, returns `Err`.
    pub apply: PluginApply,
    /// `true` if this plugin is in `BASE_PLUGINS ∪ PROD_PLUGINS` per
    /// `packages/css/src/plugins/normalize-css.ts:13-50`. Plugins
    /// with `false` have `apply: apply_filtered_out`.
    pub on_afm_hashing_path: bool,
}

/// Return value of `default_preset()`. Mirrors upstream
/// `{ plugins: [...] }`.
#[derive(Debug, Clone)]
pub struct Preset {
    pub plugins: Vec<PluginEntry>,
}

/// Upstream `Options` — per-plugin disable/exclude flags. AFM does
/// not pass any of these (`normalize-css.ts:61` calls `cssnano()`
/// bare), so we keep this minimal. Extend if a future consumer
/// needs them.
#[derive(Debug, Clone, Default)]
pub struct PresetOpts {}

// -----------------------------------------------------------------
// Apply helpers — one per plugin on AFM's hashing path. Each calls
// its Rust port with `Default::default()` opts to mirror upstream
// `creator()` (no-args invocation).
// -----------------------------------------------------------------

fn apply_postcss_discard_comments(root: &mut Root) -> PluginResult {
    postcss_discard_comments(root, &DiscardCommentsOpts::default())
}

fn apply_postcss_minify_gradients(root: &mut Root) -> PluginResult {
    postcss_minify_gradients(root)
}

fn apply_postcss_reduce_initial(root: &mut Root) -> PluginResult {
    postcss_reduce_initial(root, &PostcssReduceInitialOpts::default())
}

fn apply_postcss_colormin(root: &mut Root) -> PluginResult {
    postcss_colormin(root)
}

fn apply_postcss_normalize_timing_functions(root: &mut Root) -> PluginResult {
    postcss_normalize_timing_functions(root)
}

fn apply_postcss_calc(root: &mut Root) -> PluginResult {
    postcss_calc(root, &CalcOptions::default())
}

fn apply_postcss_convert_values(root: &mut Root) -> PluginResult {
    postcss_convert_values(root, &ConvertValuesOpts::default())
}

fn apply_postcss_ordered_values(root: &mut Root) -> PluginResult {
    postcss_ordered_values(root)
}

fn apply_postcss_minify_selectors(root: &mut Root) -> PluginResult {
    postcss_minify_selectors(root)
}

fn apply_postcss_minify_params(root: &mut Root) -> PluginResult {
    postcss_minify_params(root)
}

fn apply_postcss_normalize_string(root: &mut Root) -> PluginResult {
    postcss_normalize_string(root, &NormalizeStringOpts::default())
}

fn apply_postcss_normalize_unicode(root: &mut Root) -> PluginResult {
    postcss_normalize_unicode(root)
}

fn apply_postcss_normalize_url(root: &mut Root) -> PluginResult {
    postcss_normalize_url(root, &NormalizeUrlOpts::default())
}

fn apply_postcss_normalize_positions(root: &mut Root) -> PluginResult {
    postcss_normalize_positions(root)
}

/// Apply for plugins not on AFM's hashing path. `normalize-css.ts`
/// filters them out before invocation — if one ever reaches this
/// function, the consumer filter has drifted.
fn apply_filtered_out(_root: &mut Root) -> PluginResult {
    Err(PluginError::generic(
        "cssnano-preset-default",
        "plugin not on AFM hashing path (filtered by normalize-css.ts) — \
         drift detected if invoked",
    ))
}

/// Mirrors upstream `defaultPreset(opts)` from
/// `_vendor/cssnano-preset-default-5.2.14/src/index.js:92`.
///
/// Returns the 29-entry plugin tuple list in **source order**.
/// Argument is currently unused (AFM never passes preset options).
pub fn default_preset(_opts: &PresetOpts) -> Preset {
    Preset {
        // Source order matches upstream `src/index.js:96-126` exactly.
        // Anomaly #7: this order is the EXECUTION order, not the
        // declaration order in `normalize-css.ts`.
        plugins: vec![
            // 1.  [postcssDiscardComments,         options.discardComments]
            PluginEntry { name: "postcss-discard-comments",         apply: apply_postcss_discard_comments,         on_afm_hashing_path: true },
            // 2.  [postcssMinifyGradients,         options.minifyGradients]
            PluginEntry { name: "postcss-minify-gradients",         apply: apply_postcss_minify_gradients,         on_afm_hashing_path: true },
            // 3.  [postcssReduceInitial,           options.reduceInitial]
            PluginEntry { name: "postcss-reduce-initial",           apply: apply_postcss_reduce_initial,           on_afm_hashing_path: true },
            // 4.  [postcssSvgo,                    options.svgo]
            PluginEntry { name: "postcss-svgo",                     apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 5.  [postcssNormalizeDisplayValues,  options.normalizeDisplayValues]
            PluginEntry { name: "postcss-normalize-display-values", apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 6.  [postcssReduceTransforms,        options.reduceTransforms]
            PluginEntry { name: "postcss-reduce-transforms",        apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 7.  [postcssColormin,                options.colormin]
            PluginEntry { name: "postcss-colormin",                 apply: apply_postcss_colormin,                 on_afm_hashing_path: true },
            // 8.  [postcssNormalizeTimingFunctions, options.normalizeTimingFunctions]
            PluginEntry { name: "postcss-normalize-timing-functions", apply: apply_postcss_normalize_timing_functions, on_afm_hashing_path: true },
            // 9.  [postcssCalc,                    options.calc]
            PluginEntry { name: "postcss-calc",                     apply: apply_postcss_calc,                     on_afm_hashing_path: true },
            // 10. [postcssConvertValues,           options.convertValues]
            PluginEntry { name: "postcss-convert-values",           apply: apply_postcss_convert_values,           on_afm_hashing_path: true },
            // 11. [postcssOrderedValues,           options.orderedValues]
            PluginEntry { name: "postcss-ordered-values",           apply: apply_postcss_ordered_values,           on_afm_hashing_path: true },
            // 12. [postcssMinifySelectors,         options.minifySelectors]
            PluginEntry { name: "postcss-minify-selectors",         apply: apply_postcss_minify_selectors,         on_afm_hashing_path: true },
            // 13. [postcssMinifyParams,            options.minifyParams]
            PluginEntry { name: "postcss-minify-params",            apply: apply_postcss_minify_params,            on_afm_hashing_path: true },
            // 14. [postcssNormalizeCharset,        options.normalizeCharset]
            PluginEntry { name: "postcss-normalize-charset",        apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 15. [postcssDiscardOverridden,       options.discardOverridden]
            PluginEntry { name: "postcss-discard-overridden",       apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 16. [postcssNormalizeString,         options.normalizeString]
            PluginEntry { name: "postcss-normalize-string",         apply: apply_postcss_normalize_string,         on_afm_hashing_path: true },
            // 17. [postcssNormalizeUnicode,        options.normalizeUnicode]
            PluginEntry { name: "postcss-normalize-unicode",        apply: apply_postcss_normalize_unicode,        on_afm_hashing_path: true },
            // 18. [postcssMinifyFontValues,        options.minifyFontValues]
            PluginEntry { name: "postcss-minify-font-values",       apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 19. [postcssNormalizeUrl,            options.normalizeUrl]
            PluginEntry { name: "postcss-normalize-url",            apply: apply_postcss_normalize_url,            on_afm_hashing_path: true },
            // 20. [postcssNormalizeRepeatStyle,    options.normalizeRepeatStyle]
            PluginEntry { name: "postcss-normalize-repeat-style",   apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 21. [postcssNormalizePositions,      options.normalizePositions]
            PluginEntry { name: "postcss-normalize-positions",      apply: apply_postcss_normalize_positions,      on_afm_hashing_path: true },
            // 22. [postcssNormalizeWhitespace,     options.normalizeWhitespace]
            //     (filtered from preset; runs separately at end of transform.ts.)
            PluginEntry { name: "postcss-normalize-whitespace",     apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 23. [postcssMergeLonghand,           options.mergeLonghand]
            PluginEntry { name: "postcss-merge-longhand",           apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 24. [postcssDiscardDuplicates,       options.discardDuplicates]
            //     (Anomaly #5: this is v5.1.0; the v6.0.0 used by sort.ts
            //      is a different module.)
            PluginEntry { name: "postcss-discard-duplicates",       apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 25. [postcssMergeRules,              options.mergeRules]
            PluginEntry { name: "postcss-merge-rules",              apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 26. [postcssDiscardEmpty,            options.discardEmpty]
            PluginEntry { name: "postcss-discard-empty",            apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 27. [postcssUniqueSelectors,         options.uniqueSelectors]
            PluginEntry { name: "postcss-unique-selectors",         apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 28. [cssDeclarationSorter,           options.cssDeclarationSorter]
            PluginEntry { name: "css-declaration-sorter",           apply: apply_filtered_out,                     on_afm_hashing_path: false },
            // 29. [rawCache,                       options.rawCache]
            PluginEntry { name: "cssnano-util-raw-cache",           apply: apply_filtered_out,                     on_afm_hashing_path: false },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the manifest order against upstream
    /// `_vendor/cssnano-preset-default-5.2.14/src/index.js:96-126`.
    /// Names sourced from `creator().postcssPlugin` for the bundled
    /// plugin versions. Any drift here means upstream upgraded a
    /// plugin or we reordered — both are byte-affecting on AFM's
    /// hashing path.
    #[test]
    fn manifest_matches_upstream_source_order() {
        let preset = default_preset(&PresetOpts::default());
        let names: Vec<&str> = preset.plugins.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            vec![
                "postcss-discard-comments",
                "postcss-minify-gradients",
                "postcss-reduce-initial",
                "postcss-svgo",
                "postcss-normalize-display-values",
                "postcss-reduce-transforms",
                "postcss-colormin",
                "postcss-normalize-timing-functions",
                "postcss-calc",
                "postcss-convert-values",
                "postcss-ordered-values",
                "postcss-minify-selectors",
                "postcss-minify-params",
                "postcss-normalize-charset",
                "postcss-discard-overridden",
                "postcss-normalize-string",
                "postcss-normalize-unicode",
                "postcss-minify-font-values",
                "postcss-normalize-url",
                "postcss-normalize-repeat-style",
                "postcss-normalize-positions",
                "postcss-normalize-whitespace",
                "postcss-merge-longhand",
                "postcss-discard-duplicates",
                "postcss-merge-rules",
                "postcss-discard-empty",
                "postcss-unique-selectors",
                "css-declaration-sorter",
                "cssnano-util-raw-cache",
            ]
        );
        assert_eq!(preset.plugins.len(), 29);
    }

    /// Pins the AFM hashing-path subset. These 14 names are the union
    /// of `BASE_PLUGINS` (2) + `PROD_PLUGINS` (12) from
    /// `packages/css/src/plugins/normalize-css.ts:13-50`. Their entry
    /// must have `on_afm_hashing_path: true`.
    #[test]
    fn afm_hashing_path_subset_matches_normalize_css() {
        let preset = default_preset(&PresetOpts::default());
        let on_path: std::collections::HashSet<&str> = [
            "postcss-minify-selectors",
            "postcss-minify-params",
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
        ]
        .iter()
        .copied()
        .collect();

        for entry in &preset.plugins {
            let expected = on_path.contains(entry.name);
            assert_eq!(
                entry.on_afm_hashing_path, expected,
                "{}: on_afm_hashing_path={} but normalize-css.ts expects {}",
                entry.name, entry.on_afm_hashing_path, expected
            );
        }
        let count = preset
            .plugins
            .iter()
            .filter(|e| e.on_afm_hashing_path)
            .count();
        assert_eq!(count, 14);
    }

    /// `apply_filtered_out` returns a `PluginError` flagged as drift.
    /// Verifies the error wiring — if `normalize-css.ts`'s filter ever
    /// drifts and admits a stub-applied plugin, callers see the loud
    /// failure rather than silent byte divergence.
    #[test]
    fn filtered_out_apply_returns_drift_error() {
        let mut root = postcss_core::parse("a { color: red; }")
            .expect("parse fixture");
        let result = apply_filtered_out(&mut root);
        let err = result.expect_err("filtered-out plugin must error");
        assert_eq!(err.plugin, "cssnano-preset-default");
        assert!(
            err.message.contains("not on AFM hashing path"),
            "drift error message changed: {}",
            err.message
        );
    }
}
