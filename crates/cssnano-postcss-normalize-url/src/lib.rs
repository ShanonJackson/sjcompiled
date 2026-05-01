//! Byte-for-byte Rust port of `postcss-normalize-url@5.1.0`.
//! Phase 6b — full port pending. URL parsing edge cases — uses upstream
//! `normalize-url` package internals (relative paths, percent-encoding).

use postcss_core::{PluginResult, Root};

pub fn postcss_normalize_url(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6b — port postcss-normalize-url@5.1.0")
}
