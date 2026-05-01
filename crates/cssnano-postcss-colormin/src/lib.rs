//! Byte-for-byte Rust port of `postcss-colormin@5.3.1`.
//! Phase 6g — full port pending. **Highest-risk cssnano plugin** per
//! `crates/EXECUTION_PLAN.md` 6g — color downgrade decisions depend on
//! caniuse, colord rounding, and original-vs-minified byte-length
//! comparison. Budget multi-week iteration.

use postcss_core::{PluginResult, Root};

pub fn postcss_colormin(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 6g — port postcss-colormin@5.3.1")
}
