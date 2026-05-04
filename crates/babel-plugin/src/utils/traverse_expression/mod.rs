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
//! ## Phase 5 §5.5 PARTIAL — leaves landed in this checkpoint
//!
//! Per the §5.4 owner's parallel-work scope contract (recorded in
//! `plugins/STATUS.md` §5.5 row), the §5.5 owner is sequencing the
//! `traverse-expression/` subtree behind §5.4 (`resolve_binding.rs`).
//! THIS checkpoint lands the three leaves that DO NOT call into the
//! resolver/scope index:
//!
//! * `traverse_binary_expression.rs` — deopts via numeric-literal
//!   check; recurses through `evaluate_expression` only.
//! * `traverse_unary_expression.rs` — same shape as binary.
//! * `traverse_function.rs` — pure AST shape recognition; walks a
//!   `BlockStmt` body for the first `ReturnStatement` (mirroring
//!   Babel's `traverse(body, { ReturnStatement })` + `path.stop()`).
//!
//! The remaining 11 files in `packages/babel-plugin/src/utils/traverse-expression/`
//! (the resolve-binding-dependent half — `traverse-identifier`,
//! `traverse-call-expression`, and the entire
//! `traverse-member-expression/**` subtree including `resolve-expression/`
//! and `evaluate-path/`) wait on §5.4. The §5.5 closure agent picks
//! them up after `resolve_binding.rs` lands.
//!
//! ## Drift discipline
//!
//! Per CLAUDE.md and the §5.4 owner's parallel-work contract: if a
//! ported leaf in this checkpoint is found to reach into
//! `resolve_binding`, `meta.state.cache`, or `resolveRequest` —
//! STOP and escalate. Do not introduce a stub. The three leaves
//! landed here were verified clean by grep before porting.

pub mod traverse_binary_expression;
pub mod traverse_function;
pub mod traverse_unary_expression;

pub use traverse_binary_expression::traverse_binary_expression;
pub use traverse_function::traverse_function;
pub use traverse_unary_expression::traverse_unary_expression;
