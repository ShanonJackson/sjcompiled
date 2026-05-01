//! Byte-for-byte Rust port of `postcss-reduce-initial@5.1.2`.
//! Phase 6e — full port pending. Browserslist-aware. Replaces
//! `initial` keyword with the property's actual initial value when
//! supported (gated via caniuse-api).

use postcss_core::{PluginResult, Root};

pub fn postcss_reduce_initial(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6e — port postcss-reduce-initial@5.1.2")
}
