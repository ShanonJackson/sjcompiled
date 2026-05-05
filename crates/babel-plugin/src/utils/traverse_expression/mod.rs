//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/`.
//!
//! Mirrors `traverse-expression/index.ts` — re-exports the leaf
//! traversers consumed by `utils/evaluate-expression.ts` (Phase 5
//! §5.6). Each leaf takes the recursive `evaluateExpression` callback
//! as a parameter; Babel needs this to break the circular module
//! dependency between `traverse-expression/*` and `evaluate-expression.ts`.
//! In Rust we mirror with a generic `F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair`
//! parameter on each function.
//!
//! ## Closure shape + scope-info threading (§5.5 closure)
//!
//! The recursive `evaluate_expression` closure keeps the JS-shaped
//! `(expr, meta) -> ResultPair` signature. Scope information
//! (`scope_index`, `parent_scope`, `own_scope`) is passed to each
//! leaf as EXPLICIT parameters because:
//!
//! 1. JS derives scope from `meta.parentPath.scope` (Babel `NodePath`).
//!    The Rust `Metadata` doesn't carry scope refs — adding a
//!    lifetime parameter to `Metadata<'a>` for `&'idx ScopeIndex`
//!    would touch the entire callgraph (css_builders, transform_css_items,
//!    etc.) and isn't justified by the §5.5 surface alone.
//! 2. The §5.4e port already uses this convention — `resolve_binding`
//!    takes `(reference_name, meta, scope_index, parent_scope, own_scope)`.
//!    The leaves match.
//! 3. The §5.6 evaluator (caller) will close over scope info for
//!    its dispatch, but the closure it passes to leaves stays
//!    `(expr, meta)`-shaped — consistent with JS's circularity-break
//!    contract.
//!
//! ## Phase 5 §5.5 status
//!
//! - **PARTIAL leaves (claude-2026-05-05, parallel with §5.4):** the
//!   three resolve-binding-INDEPENDENT files —
//!   `traverse_binary_expression`, `traverse_unary_expression`,
//!   `traverse_function`.
//! - **Closure leaves (claude-2026-05-05 + §5.4e closure):** the
//!   eight resolve-binding-DEPENDENT files —
//!   `traverse_identifier`, plus the entire
//!   `traverse_member_expression/**` subtree (8 files including
//!   `traverse_access_path/{evaluate_path,resolve_expression}/**`).
//! - **STUBS pending compat-layer work:**
//!   `traverse_call_expression` (IIFE wrap + new-scope registration,
//!   gated on §5.0d compat extension or §5.6 owner) and
//!   `traverse_access_path::evaluate_path::namespace_import`
//!   (cross-file ScopeIndex synthesis, gated on §5.6 cross-file
//!   scope management). Both files quote upstream verbatim and
//!   `unimplemented!()` in their bodies — see their module docs
//!   for the unblock checklist.
//!
//! ## Drift discipline
//!
//! Per CLAUDE.md DRIFT DETECTION, the two SHELL stubs above were
//! ESCALATED to the §5.4e owner / coordinator before landing — the
//! missing compat infra (IIFE CallExpr wrap, runtime scope
//! registration, cross-file ScopeIndex synthesis) is recorded in
//! both files' module docs so a future agent finds the unblock
//! checklist by greping `unimplemented!()` in the subtree.

pub mod traverse_binary_expression;
pub mod traverse_call_expression;
pub mod traverse_function;
pub mod traverse_identifier;
pub mod traverse_member_expression;
pub mod traverse_unary_expression;

pub use traverse_binary_expression::traverse_binary_expression;
pub use traverse_call_expression::traverse_call_expression;
pub use traverse_function::traverse_function;
pub use traverse_identifier::traverse_identifier;
pub use traverse_member_expression::traverse_member_expression;
pub use traverse_unary_expression::traverse_unary_expression;
