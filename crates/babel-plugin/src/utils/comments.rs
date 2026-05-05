//! 1:1 port of `packages/babel-plugin/src/utils/comments.ts` —
//! INCOMPLETE per §6.5 closure note.
//!
//! Upstream `getNodeComments(path, meta)` walks
//! `meta.state.file.ast.comments` filtering by line-number against
//! `path.node.loc.start.line` and `lineNumber - 1`. It returns
//! `{before, current}` pairs of `CommentLine`s.
//!
//! ### SourceMap dependency
//!
//! Resolving SWC's `BytePos` → line number requires a SourceMap proxy
//! that the plugin runtime exposes via `meta.source_map` on
//! `TransformPluginProgramMetadata`. The visitor doesn't currently
//! thread that proxy through — see §6.5 closure note in
//! `plugins/STATUS.md` for the deferral rationale.
//!
//! ### Stub semantics
//!
//! [`is_css_prop_disabled_via_comment_store`] returns:
//! * `false` when the file's comment store has no `@compiled-disable*`
//!   line comments — transform proceeds normally.
//! * `true` when ANY `@compiled-disable*` directive is present in the
//!   file's comment store — transform bails conservatively (no css
//!   prop transform anywhere in the file).
//!
//! This is a per-file gate, NOT the per-line gate upstream implements.
//! It biases toward upstream's "directive disables" semantics — when
//! a directive is present, behaviour is more restrictive than upstream
//! (no transform vs. line-scoped no transform). Per the cardinal "BUGS
//! in OLD = BUGS in NEW" rule, this is documented divergence; the
//! follow-up checkpoint that wires SourceMap into the visitor will
//! restore line-scope precision.
//!
//! **Reachability:** today the function returns `false` because the
//! visitor doesn't expose its file-comment store at the call site
//! (`State` doesn't carry the comment store; the `Comments` proxy is
//! held by the visitor). The conservative bail-out is gated on a
//! future `state.disable_directive_seen` flag the SourceMap-thread
//! follow-up will populate. Until then `is_css_prop_disabled_via_comment_store`
//! is a stub returning `false` — matching upstream's "no-directive"
//! fast path for ALL inputs.

use crate::state::State;

/// Stub for the SourceMap-based per-line filter. See module doc.
pub fn is_css_prop_disabled_via_comment_store(_state: &State) -> bool {
    // §6.5 deferred: SourceMap-based per-line filter lands when the
    // visitor threads the SWC plugin metadata's `source_map` proxy
    // into the dispatch context. Until then no `@compiled-disable*`
    // directive disables any css prop. See module doc for rationale.
    false
}
