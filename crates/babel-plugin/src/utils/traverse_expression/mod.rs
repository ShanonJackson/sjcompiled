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
//! ## Phase 5 §5.5 status — ☑ CLOSURE COMPLETE (claude-2026-05-05)
//!
//! All 14 leaves are real 1:1 ports landed across three passes:
//!
//! - **Pass 1 (parallel with §5.4a–e):** the three
//!   resolve-binding-INDEPENDENT files —
//!   `traverse_binary_expression`, `traverse_unary_expression`,
//!   `traverse_function`.
//! - **Pass 2 (post-§5.4e):** eight resolve-binding-DEPENDENT
//!   files — `traverse_identifier`, plus the entire
//!   `traverse_member_expression/**` subtree (8 files including
//!   `traverse_access_path/{evaluate_path,resolve_expression}/**`).
//! - **Pass 3 (closure complete; §5.0d absorbed):** the two
//!   previously-stubbed leaves landed real bodies —
//!   `traverse_call_expression` (using
//!   [`crate::compat::scope::ScopeIndex::register_new_scope`] for
//!   the IIFE arrow's transient `ScopeId`,
//!   `register_synthetic_binding` for `(param := evaluatedArg)`
//!   pairs, and the
//!   [`crate::types::Metadata::own_scope_override`] channel for
//!   the recursive evaluator call) and
//!   `traverse_access_path::evaluate_path::namespace_import`
//!   (using `PartialBindingWithMeta::imported_module: Arc<Module>`
//!   from the §5.4e drift-fix +
//!   `register_synthetic_binding` for the 'default' synthesis on a
//!   fresh imported `ScopeIndex`).
//!
//! ## §5.6 wiring contract
//!
//! The §5.6 evaluator (`evaluate_expression.rs`) inherits two
//! channels installed by §5.5 closure:
//!
//! 1. `Metadata::own_scope_override` — the dispatcher reads it
//!    per-call to honour `traverse_call_expression`'s IIFE-recursion
//!    own_scope swap. `traverse_call_expression` sets it before
//!    the recursive call and restores afterward; the evaluator's
//!    closure consumes it on each invocation.
//! 2. The namespace-import dispatch route — `evaluate_identifier`
//!    detects `source == Import && imported_module.is_some() &&
//!    node.is_none()` and calls `evaluate_namespace_import_path`
//!    directly with the upcoming `pathName` from the access-path
//!    chain. The body is real and unit-tested but unreachable from
//!    the standard `evaluate_path` dispatcher (SWC's
//!    `ImportNamespaceSpecifier` isn't an `Expr` variant).
//!
//! ## Bug-parity flag (documented; not patched)
//!
//! `traverse_call_expression` does NOT persist the IIFE wrap into
//! the AST (Babel uses `replaceWith`; Rust uses transient
//! `ScopeId`). May affect runtime-CSS-fallback emission on the
//! deopt path. If a fixture surfaces byte-divergence there, the
//! fix is at §5.6's evaluator boundary (decide which expression
//! flows to the runtime fallback), NOT in
//! `traverse_call_expression`. See that file's module docs.

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
