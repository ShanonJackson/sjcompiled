//! Port of `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`.
//!
//! Phase 7 — body intentionally stubbed. ~1100 LOC of static data table.
//! Will be regenerated from the upstream JS via a build script in a
//! later sub-task (see plan: task #3).

use indexmap::IndexMap;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Default)]
pub struct PrefixEntry {
    pub browsers: Vec<String>,
    pub mistakes: Vec<String>,
    pub feature: Option<String>,
    pub props: Vec<String>,
    pub transition: bool,
    pub selector: bool,
}

pub static PREFIXES: Lazy<IndexMap<&'static str, PrefixEntry>> =
    Lazy::new(IndexMap::new);
