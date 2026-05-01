//! Port of `packages/css/src/plugins/normalize-css.ts`.
//!
//! Wrapper around `cssnano-preset-default@5.2.14` plus the local
//! `normalize-current-color` plugin. Filter-then-execute order matches the
//! cssnano-preset-default source order — see `PARITY_VERSIONS.md` Anomaly #7.

use postcss_core::Root;

#[derive(Debug, Clone, Default)]
pub struct NormalizeCssOpts {
    pub optimize_css: Option<bool>,
}

pub fn normalize_css(_root: &mut Root, _opts: &NormalizeCssOpts) {
    unimplemented!("Phase 6 — port normalize-css.ts (cssnano-preset-default subset)");
}
