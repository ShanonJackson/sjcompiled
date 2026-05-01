//! Byte-for-byte Rust port of `postcss-ordered-values@5.1.3`.
//! Phase 6d — full port pending. Reorders multi-value properties
//! (`border` → width / style / color order, etc.) for shorthand
//! deduplication consistency.

use postcss_core::{PluginResult, Root};

pub fn postcss_ordered_values(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6d — port postcss-ordered-values@5.1.3")
}
