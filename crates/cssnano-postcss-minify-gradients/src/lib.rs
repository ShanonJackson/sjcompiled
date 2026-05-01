//! Byte-for-byte Rust port of `postcss-minify-gradients@5.1.1`.
//! Phase 6g — full port pending. Uses `colord` + `cssnano-utils`.
//! Reduces gradient definitions (`linear-gradient` / `radial-gradient`).

use postcss_core::{PluginResult, Root};

pub fn postcss_minify_gradients(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6g — port postcss-minify-gradients@5.1.1")
}
