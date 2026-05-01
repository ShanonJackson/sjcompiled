//! crates/postcss-normalize-whitespace
//! Byte-for-byte Rust port of `postcss-normalize-whitespace@5.1.1`.
//! See `crates/PARITY_VERSIONS.md` Anomaly #2 — version pinned to 5.1.1.
//!
//! Phase 5b — full port pending. Upstream source:
//! `node_modules/postcss-normalize-whitespace@5.1.1/src/index.js` (~80 LOC).
//! Walks the tree, normalizes:
//! - `node.raws.before` → strip whitespace.
//! - `decl.raws.important` → `'!important'` (collapse).
//! - decl value: regex `\s*(\\9)\s*` → `$1` (IE9 hack).
//! - decl value: parsed via postcss-value-parser, walked with
//!   `reduceWhitespaces` — collapses spaces to ` `, drops surrounding
//!   ws of `/` and `(...)` except for `var`/`env`/`constant`.
//! - decl raws.between=":", raws.semicolon=false.
//! - rule/atrule raws.between="", raws.after="", raws.semicolon=false.
//! - root.raws.after="".

use postcss_core::{PluginResult, Root};

pub fn postcss_normalize_whitespace(_root: &mut Root) -> PluginResult {
    unimplemented!("Phase 5b — port postcss-normalize-whitespace@5.1.1")
}
