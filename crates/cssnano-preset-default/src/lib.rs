//! Byte-for-byte Rust port of `cssnano-preset-default@5.2.14`.
//! Phase 6h — full port pending. Replicates `node_modules/cssnano-preset-default@5.2.14/src/index.js`'s
//! plugin tuple list AND order. Filtered downstream by
//! `packages/css/src/plugins/normalize-css.ts` against `BASE_PLUGINS` /
//! `PROD_PLUGINS` arrays — the EXECUTION ORDER is THIS file's source
//! order, not normalize-css.ts's array order (Anomaly #7 in
//! `PARITY_VERSIONS.md`).

use postcss_core::{PluginResult, Root};

#[derive(Debug, Clone, Default)]
pub struct PresetOpts {
    /// `optimizeCss: true` includes the PROD_PLUGINS subset.
    pub optimize_css: bool,
}

pub fn cssnano_preset_default(_root: &mut Root, _opts: &PresetOpts) -> PluginResult {
    unimplemented!("Phase 6h — port cssnano-preset-default@5.2.14 orchestrator")
}
