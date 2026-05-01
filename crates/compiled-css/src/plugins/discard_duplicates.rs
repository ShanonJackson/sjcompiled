//! Port of `packages/css/src/plugins/discard-duplicates.ts`.
//!
//! NOTE: distinct from `postcss-discard-duplicates@6.0.0` (which lives in
//! `crates/postcss-discard-duplicates`). Per `PARITY_VERSIONS.md` Anomaly #9
//! these are different code paths and must not be conflated.

use postcss_core::Root;

pub fn discard_duplicates(_root: &mut Root) {
    unimplemented!("Phase 4a — port discard-duplicates.ts (local)");
}
