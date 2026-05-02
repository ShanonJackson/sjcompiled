//! crates/babel-plugin-strip-runtime
//! Byte-for-byte port of `packages/babel-plugin-strip-runtime/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 0 status: scaffold only. Pass-through visitor.

pub mod utils;

use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::TransformPluginProgramMetadata;

#[derive(Default)]
pub struct PassthroughVisitor;

impl VisitMut for PassthroughVisitor {}

#[plugin_transform]
pub fn process(program: Program, _meta: TransformPluginProgramMetadata) -> Program {
    let mut p = program;
    p.visit_mut_with(&mut PassthroughVisitor);
    p
}

pub fn run_passthrough(program: &mut Program) {
    program.visit_mut_with(&mut PassthroughVisitor);
}
