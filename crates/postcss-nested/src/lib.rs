//! crates/postcss-nested
//! Byte-for-byte Rust port of `postcss-nested@5.0.6`.
//! See `crates/PARITY_VERSIONS.md` Anomaly #1 — version pinned to 5.x; the
//! v5 → v6 rewrite changed selector merging semantics. Do NOT consult v6.
//!
//! Phase 5a — full port pending. Upstream source:
//! `node_modules/postcss-nested@5.0.6/index.js` (~250 LOC, complex
//! recursive selector merging with bubble/unwrap config). This is the
//! single hardest pre-autoprefixer port; budget multi-day effort.
//!
//! Configuration shape (per `packages/css/src/transform.ts:48-61`):
//! ```ts
//! nested({
//!   bubble: ['container', '-moz-document', 'layer', 'else', 'when',
//!            'starting-style'],
//!   unwrap: ['color-profile', 'counter-style', 'font-palette-values', 'page', 'property'],
//! })
//! ```

use postcss_core::{PluginResult, Root};

#[derive(Debug, Clone, Default)]
pub struct PostcssNestedOpts {
    /// At-rule names that bubble up (their bodies stay separate; the
    /// at-rule wraps each child rule).
    pub bubble: Vec<String>,
    /// At-rule names that unwrap (their bodies are flattened into the
    /// parent's children, the at-rule itself is removed).
    pub unwrap: Vec<String>,
}

pub fn postcss_nested(_root: &mut Root, _opts: &PostcssNestedOpts) -> PluginResult {
    unimplemented!("Phase 5a — port postcss-nested@5.0.6")
}
