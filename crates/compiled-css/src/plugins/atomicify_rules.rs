//! Port of `packages/css/src/plugins/atomicify-rules.ts`.
//!
//! **CRITICAL** plugin per `crates/EXECUTION_PLAN.md` 4d — this is the one
//! whose hash output becomes class names. The hash function lives in
//! `@sjcompiled/utils` and must be ported with bit-identical output.

use indexmap::IndexMap;
use postcss_core::Root;

#[derive(Debug, Clone, Default)]
pub struct AtomicifyRulesOpts {
    /// Maps long class hashes to short identifiers. Iteration order matters;
    /// upstream uses Object insertion order, which we preserve via `IndexMap`.
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    pub class_hash_prefix: Option<String>,
    /// Class names produced by the run are pushed here in order.
    pub class_names: Vec<String>,
}

/// `atomicifyRules(opts)` factory — the postcss plugin entrypoint.
/// Body intentionally unimplemented; will be ported in Phase 4d.
pub fn atomicify_rules(_root: &mut Root, _opts: &mut AtomicifyRulesOpts) {
    unimplemented!("Phase 4d — port atomicify-rules.ts");
}
