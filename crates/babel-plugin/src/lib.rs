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
pub mod class_names;
pub mod compat;
pub mod constants;
pub mod css;
pub mod css_map;
pub mod css_prop;
pub mod keyframes;
pub mod mutation_recorder;
pub mod resolver;
pub mod state;
pub mod styled;
pub mod types;
pub mod utils;
pub mod xcss_prop;

use std::sync::Arc;

use serde::Deserialize;
use swc_core::common::comments::Comments;
use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::VisitMutWith;
use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::{PluginCommentsProxy, TransformPluginProgramMetadata};

use crate::babel_plugin::BabelPluginVisitor;
use crate::resolver::build_default;
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

    // §4.6 bridge: SWC exposes the absolute source filename via the
    // metadata context. `resolve_binding.rs` reads
    // `meta.state.filename()` to anchor cross-file resolution; without
    // injection the cross-file branch silently no-ops. Empty string
    // when the host omits the context — `resolve_binding` treats
    // `Some("")` the same as `None` because the upstream JS plugin
    // also bails on missing filename.
    let filename: String = meta
        .get_context(&TransformPluginMetadataContextKind::Filename)
        .unwrap_or_default();

    // §4.6 bridge: build the default Compiled resolver and stash it
    // on `state` so `resolve_binding::resolve_request` can reach it.
    // `opts.extensions` honours `DEFAULT_CODE_EXTENSIONS` when unset
    // (per `build_default` contract).
    let resolver = Arc::new(build_default(opts.extensions.as_deref()));

    let mut visitor = BabelPluginVisitor::new(opts, comments);
    if !filename.is_empty() {
        visitor.state.set_filename(filename);
    }
    visitor.state.set_resolver(resolver);
    // §6.8i — bridge SWC's `unresolved_mark` from plugin metadata into
    // the visitor so the `Program::exit` React-import injection can
    // colour its local Ident with the same hygiene context downstream
    // free references (e.g. the react-classic JSX transform's
    // `React.createElement(...)` Idents) carry. Without this, fixtures
    // with no top-level user bindings fall back to an empty
    // `SyntaxContext` and SWC's hygiene pass renames our import to
    // `React1`. See `babel_plugin.rs::build_react_namespace_import`.
    visitor.unresolved_mark = Some(meta.unresolved_mark);
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
