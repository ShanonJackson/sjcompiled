//! Port of `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`.
//!
//! This file exposes a single static `PREFIXES` table keyed by the property,
//! value, or selector name that autoprefixer recognises. The table itself is
//! codegen'd at build time by `build.rs`, which evaluates the upstream JS via
//! `bun -e` and emits a sequence of `m.insert(...)` statements. This avoids
//! the silent transcription drift risk of typing 183 entries by hand.
//!
//! Pre-condition: `bun install` must have populated `node_modules/caniuse-lite`
//! at version 1.0.30001690 (pinned via root `package.json` `overrides`). See
//! `crates/PARITY_VERSIONS.md` Anomaly #3.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// One entry from `data/prefixes.js`. Mirrors the upstream JS object shape:
/// `{ browsers, feature, mistakes?, props?, selector?, transition? }`.
///
/// Optional JS fields map to defaulted Rust fields:
/// - `mistakes`/`props`/`browsers` absent → empty `Vec`
/// - `selector`/`transition` absent → `false`
/// - `feature` absent → `None` (kept defensive — every observed entry in
///   v10.4.14 sets `feature`; absence would indicate caniuse-lite drift)
///
/// `transition` is unused in the v10.4.14 / caniuse-lite 1.0.30001690 data
/// snapshot but kept on the struct for forward-compat with future caniuse-lite
/// versions. If a future snapshot emits it, deserialization continues to work.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrefixEntry {
    #[serde(default)]
    pub browsers: Vec<String>,
    /// JS omits this field when empty/undefined; the parity gate in
    /// `tests/data_parity.rs` requires us to do the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mistakes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<String>,
    /// Unused in the v10.4.14 / caniuse-lite 1.0.30001690 snapshot but
    /// kept for forward-compat. JS only sets this when truthy.
    #[serde(default, skip_serializing_if = "is_false")]
    pub transition: bool,
    /// JS sets `selector: true` for ~10 entries; absent otherwise.
    #[serde(default, skip_serializing_if = "is_false")]
    pub selector: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// All prefix entries from `data/prefixes.js`, in source-defined order.
/// `IndexMap` is load-bearing — JS `Object.keys` iteration order reaches
/// downstream prefix decisions, so a `HashMap` would silently drift.
pub static PREFIXES: Lazy<IndexMap<&'static str, PrefixEntry>> = Lazy::new(|| {
    let mut m: IndexMap<&'static str, PrefixEntry> = IndexMap::new();
    // The included file is a single block expression containing
    // `m.insert(...)` calls — see build.rs.
    include!(concat!(env!("OUT_DIR"), "/prefixes_table.rs"));
    m
});
