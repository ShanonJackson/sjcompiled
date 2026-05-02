//! crates/babel-plugin
//! Byte-for-byte port of `packages/babel-plugin/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 0 status: scaffold only. Pass-through visitor (Program::enter +
//! Program::exit no-ops). The parity harness asserts byte-equality through
//! the prettier oracle for any input — at this phase, equality holds
//! trivially because we touch nothing.

use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::{visit_mut_pass, VisitMut};
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

// Re-export so workspace integration tests can drive the visitor without
// going through the SWC plugin transport.
pub fn run_passthrough(program: &mut Program) {
    program.visit_mut_with(&mut PassthroughVisitor);
}

// VisitMutWith trait import for the helper above.
use swc_core::ecma::visit::VisitMutWith;

// `visit_mut_pass` is unused at this scaffold stage but is the canonical
// entry SWC docs point to; keep the import suppressed to avoid breaking
// builds that promote unused warnings to errors.
#[allow(dead_code)]
fn _ensure_visit_mut_pass_visible() {
    let _ = visit_mut_pass(PassthroughVisitor::default());
}
