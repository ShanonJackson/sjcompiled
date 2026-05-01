//! Byte-for-byte Rust port of `postcss-normalize-timing-functions@5.1.0`.
//! Phase 6b — full port pending. Normalizes `transition-timing-function`
//! / `animation-timing-function` keywords (`cubic-bezier(0.25, 0.1, 0.25, 1)` → `ease`, etc.).

use postcss_core::{PluginResult, Root};

pub fn postcss_normalize_timing_functions(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6b — port postcss-normalize-timing-functions@5.1.0")
}
