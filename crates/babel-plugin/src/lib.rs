//! crates/babel-plugin
//! Byte-for-byte port of `packages/babel-plugin/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 2 §2.3 status: dispatcher SKELETON landed. The visitor
//! recognises Compiled imports and populates `state.compiledImports`,
//! but does NOT mutate the AST. Output stays pass-through, so the
//! §2.3 verification gate ("byte-equal output through the prettier
//! oracle for every fixture, no handler logic yet") holds.
//!
//! What's still to land:
//! - JSX-pragma scan in `Program::enter` (regexes already in
//!   `sjcompiled_utils::jsx`; classic-pragma `path.remove()` gated
//!   until §6.5 css-prop).
//! - `Program::exit` `appendRuntimeImports` + banner + cleanup loop
//!   (Phase 6, alongside the first real handler).
//! - `ImportDeclaration` specifier removal (§2.4 MutationRecorder).
//! - Per-API stub handlers — placeholder bodies live in
//!   `babel_plugin.rs`.

pub mod babel_plugin;
pub mod constants;
pub mod types;
pub mod utils;

use serde::Deserialize;
use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::VisitMutWith;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::TransformPluginProgramMetadata;

use crate::babel_plugin::BabelPluginVisitor;
use crate::types::PluginOptions;

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let opts: PluginOptions = meta
        .get_transform_plugin_config()
        .as_deref()
        .and_then(|s| PluginOptions::deserialize(&mut serde_json::Deserializer::from_str(s)).ok())
        .unwrap_or_default();

    let mut visitor = BabelPluginVisitor::new(opts);
    let mut p = program;
    p.visit_mut_with(&mut visitor);
    p
}

/// In-process entry for workspace integration tests. Drives the
/// dispatcher without going through the SWC plugin transport so we
/// can inspect `state` after the run.
pub fn run_dispatcher(program: &mut Program, opts: PluginOptions) -> BabelPluginVisitor {
    let mut visitor = BabelPluginVisitor::new(opts);
    program.visit_mut_with(&mut visitor);
    visitor
}
