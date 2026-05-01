//! Byte-for-byte Rust port of `postcss-calc@8.2.4`.
//! Phase 6d — full port pending. Effectively a small expression
//! compiler (parses `calc()` operands, evaluates with unit awareness,
//! emits the simplified result). High float-math diff risk.

use postcss_core::{PluginResult, Root};

pub fn postcss_calc(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6d — port postcss-calc@8.2.4")
}
