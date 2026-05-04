//! crates/babel-plugin
//! Byte-for-byte port of `packages/babel-plugin/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 2 §2.3 status:
//!   * Skeleton (prior session): Compiled-import recognition. Output
//!     stays pass-through.
//!   * §2.3(a) (this checkpoint): JSX-pragma recognition (classic
//!     `import { jsx }` site + `@jsx` / `@jsxImportSource` comment
//!     scan). Recognition only — no AST mutations, no comment store
//!     mutations. State writes are allowed.
//!
//! What's still to land:
//! - §2.3(b): the deferred mutations paired with §2.4 MutationRecorder
//!   — `path.remove()` of the classic-pragma `jsx` specifier;
//!   filtering the matched JSX-pragma comment from the comment store
//!   so `@babel/plugin-transform-react-jsx`'s SWC analog ignores it.
//! - §2.4: state encapsulation + `MutationRecorder::apply` as the only
//!   mutator (per `STATE_MUTATIONS.md` / PLAN.md §3.9.8).
//! - `Program::exit` `appendRuntimeImports` + banner + cleanup loop
//!   (Phase 6, alongside the first real handler).
//! - `ImportDeclaration` specifier removal (§2.4 MutationRecorder).
//! - Per-API stub handlers — placeholder bodies live in
//!   `babel_plugin.rs`.

pub mod babel_plugin;
pub mod cache_schema;
pub mod compat;
pub mod constants;
pub mod mutation_recorder;
pub mod resolver;
pub mod state;
pub mod types;
pub mod utils;

use serde::Deserialize;
use swc_core::common::comments::Comments;
use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::VisitMutWith;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::{PluginCommentsProxy, TransformPluginProgramMetadata};

use crate::babel_plugin::BabelPluginVisitor;
use crate::types::PluginOptions;

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let opts: PluginOptions = meta
        .get_transform_plugin_config()
        .as_deref()
        .and_then(|s| PluginOptions::deserialize(&mut serde_json::Deserializer::from_str(s)).ok())
        .unwrap_or_default();

    // Mirror `babel-plugin-strip-runtime`'s comment-proxy wiring: real
    // proxy in production, fallback unit-struct (no-op outside the
    // plugin runtime). Keeps a single SWC-comment idiom across both
    // plugins.
    let comments: PluginCommentsProxy = meta.comments.clone().unwrap_or(PluginCommentsProxy);

    let mut visitor = BabelPluginVisitor::new(opts, comments);
    let mut p = program;
    p.visit_mut_with(&mut visitor);
    p
}

/// In-process entry for workspace integration tests. Drives the
/// dispatcher without going through the SWC plugin transport so we
/// can inspect `state` after the run.
///
/// Generic over `C: Comments` — tests typically pass
/// `swc_common::comments::SingleThreadedComments::default()` (an
/// in-process empty store) so the dispatcher's pragma scan reads
/// safely without the SWC plugin runtime's thread-locals being
/// initialised. Production paths go through `process` above.
pub fn run_dispatcher<C: Comments>(
    program: &mut Program,
    opts: PluginOptions,
    comments: C,
) -> BabelPluginVisitor<C> {
    let mut visitor = BabelPluginVisitor::new(opts, comments);
    program.visit_mut_with(&mut visitor);
    visitor
}
