//! Byte-for-byte Rust port of `postcss-discard-comments@5.1.2`.
//! Phase 6a — full port pending. Upstream:
//! `node_modules/postcss-discard-comments@5.1.2/src/index.js` (~100 LOC)
//! plus `lib/commentRemover.js` and `lib/commentParser.js`.
//!
//! Removes comments from the tree and from inline raws (between, etc.)
//! per a CommentRemover predicate (default: keep `/*!` important
//! comments, drop the rest).

use postcss_core::{PluginResult, Root};

#[derive(Debug, Clone, Default)]
pub struct DiscardCommentsOpts {
    /// Default `false`. When `true`, every comment is removed including
    /// `/*!` ones.
    pub remove_all: bool,
    /// Default `false`. When `true`, keep only the FIRST comment in
    /// document order.
    pub remove_all_but_first: bool,
}

pub fn postcss_discard_comments(_root: &mut Root, _opts: &DiscardCommentsOpts) -> PluginResult {
    unimplemented!("Phase 6a — port postcss-discard-comments@5.1.2")
}
