//! Port of `packages/css/src/transform.ts`.
//!
//! Locks the public surface — every byte the parity-runner compares passes
//! through here. The body is filled in incrementally as plugins land:
//!
//!   Phase 4   — local plugins (`compiled-css::plugins::*`) are wired in
//!   Phase 5   — `postcss-nested@5.0.6`, `postcss-normalize-whitespace@5.1.1`
//!   Phase 6   — cssnano preset (via `compiled-css::plugins::normalize_css`)
//!   Phase 7   — `autoprefixer@10.4.14`
//!
//! The pipeline order from upstream `transform.ts` lines 44-78:
//!
//! ```text
//! discardDuplicates (local)
//! discardEmptyRules (local)
//! parentOrphanedPseudos (local)
//! postcss-nested@5.0.6 (with bubble/unwrap config below)
//! ...normalizeCSS(opts)            // cssnano preset subset
//! expandShorthands (local)
//! atomicifyRules (local)            // populates classNames
//! [increaseSpecificity] when opts.increaseSpecificity
//! sortAtomicStyleSheet (local)
//! [autoprefixer] unless AUTOPREFIXER=off
//! postcss-normalize-whitespace@5.1.1
//! extractStyleSheets (local)        // populates sheets
//! ```
//!
//! `nested({ bubble: ['container', '-moz-document', 'layer', 'else', 'when',
//! 'starting-style'], unwrap: ['color-profile', 'counter-style',
//! 'font-palette-values', 'page', 'property'] })` — these arrays are the
//! call-site config; the plugin's *interpretation* must match v5 exactly
//! (per Anomaly #1 in PARITY_VERSIONS.md).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use postcss_core::{parse, stringify};

/// Mirrors upstream `TransformOpts` (line 17 of `transform.ts`). Field-by-field:
///
/// | upstream                   | rust                          | notes |
/// |----------------------------|-------------------------------|-------|
/// | `optimizeCss?`             | `optimize_css`                | `Option<bool>` — `undefined` is meaningful |
/// | `classNameCompressionMap?` | `class_name_compression_map` | `IndexMap` to preserve insertion order |
/// | `increaseSpecificity?`     | `increase_specificity`        | |
/// | `sortAtRules?`             | `sort_at_rules`               | forwarded to `sortAtomicStyleSheet` |
/// | `sortShorthand?`           | `sort_shorthand`              | forwarded to `sortAtomicStyleSheet` |
/// | `classHashPrefix?`         | `class_hash_prefix`           | |
///
/// `flattenMultipleSelectors` was added in @compiled/css 0.20+ and is
/// **not** part of the AFM-pinned 0.19.0 surface (see PARITY_VERSIONS.md
/// "JS oracle source pin"). Do not re-add this field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformOpts {
    #[serde(rename = "optimizeCss", default)]
    pub optimize_css: Option<bool>,
    #[serde(rename = "classNameCompressionMap", default)]
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    #[serde(rename = "increaseSpecificity", default)]
    pub increase_specificity: Option<bool>,
    #[serde(rename = "sortAtRules", default)]
    pub sort_at_rules: Option<bool>,
    #[serde(rename = "sortShorthand", default)]
    pub sort_shorthand: Option<bool>,
    #[serde(rename = "classHashPrefix", default)]
    pub class_hash_prefix: Option<String>,
}

/// Mirrors upstream return shape: `{ sheets: string[]; classNames: string[] }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformResult {
    pub sheets: Vec<String>,
    #[serde(rename = "classNames")]
    pub class_names: Vec<String>,
}

/// `transformCss(css, opts)` — packages/css/src/transform.ts:33.
///
/// ## Pipeline status
///
/// **Identity passthrough today.** Until the plugin pipeline lands, this
/// function returns `{ sheets: [parse(css).toString()], classNames: [] }`.
/// That gives us a fixed entry point the parity-runner can drive end-to-end:
/// the JS counterpart for the Phase 0 harness is just `postcss.parse(css)
/// .toString()`, which the postcss-core round-trip already passes for any
/// byte-valid CSS input.
///
/// As plugins land they're wired in here in upstream order (see this
/// module's doc comment for the canonical sequence). Each insertion turns
/// on parity tests for an incrementally larger slice of the JS pipeline.
pub fn transform_css(css: &str, _opts: &TransformOpts) -> TransformResult {
    // TODO(phase 4..7): replace this with the full plugin pipeline.
    let root = match parse(css) {
        Ok(r) => r,
        Err(_) => return TransformResult { sheets: vec![css.to_string()], class_names: Vec::new() },
    };
    let out = stringify(&root);
    TransformResult { sheets: vec![out], class_names: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identity passthrough must round-trip — same exit gate as
    /// `postcss-core::roundtrip_tests` but reached through this crate's
    /// public surface. When Phase 4 lands these tests will broaden to cover
    /// real plugin output.
    fn assert_passthrough(css: &str) {
        let r = transform_css(css, &TransformOpts::default());
        assert_eq!(r.class_names, Vec::<String>::new());
        assert_eq!(r.sheets, vec![css.to_string()]);
    }

    #[test] fn passthrough_simple() { assert_passthrough("a { color: red; }"); }
    #[test] fn passthrough_nested() { assert_passthrough("@media (max-width: 100px) {\n  a { color: red; }\n}"); }
    #[test] fn passthrough_empty() { assert_passthrough(""); }
}
