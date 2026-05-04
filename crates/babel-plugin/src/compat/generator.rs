//! Byte-for-byte port of `@babel/generator@7.23.0`.
//! Pinned in `crates/PARITY_VERSIONS.md` (AFM resolution under
//! `@compiled/babel-plugin@0.36.1`, commit `16a62b8`).
//! Upstream source: `node_modules/@babel/generator/lib/`.
//!
//! ## Why this exists
//!
//! `swc_ecma_codegen` is NOT byte-equivalent to `@babel/generator`.
//! Concretely (from the §4.2 hand-off), the two diverge on:
//! - Whitespace around binary operators (`a+b` vs `a + b`).
//! - Paren policy (precedence-driven vs always-explicit).
//! - Default quote style (heuristic vs always-`"double"`).
//! - Trailing-comma policy in arrays / object literals.
//! - Property-shorthand collapsing (`{ a: a }` vs `{ a }`).
//! - Semicolon-after-class-body and `do {} while ()`.
//! - Comment attachment around ternary branches, eslint-disable
//!   directives, and JSDoc — which Babel preserves in attached
//!   positions and SWC emits via separate leading/trailing slots.
//!
//! `packages/babel-plugin/src/utils/css-builders.ts:464` calls
//! `hash(generate(expression).code)` to compute keyframe class
//! names. Any divergence in those bytes renames the class in
//! production. Same hazard applies to `:280` and `:298`
//! (`generate(node).code` → `variableName` → `hash(variableName)`
//! at line 639). Sites in `build-compiled-component.ts:30` and
//! `build-styled-component.ts:133` emit into source that prettier
//! round-trips, but we lock byte-exactness identically per the
//! Phase 4 hand-off contract — drift today = drift tomorrow.
//!
//! ## Status
//!
//! §4.2 (this checkpoint) ships:
//! - The coverage manifest at
//!   `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md`.
//! - The JS oracle and corpus shape at
//!   `parity-harness/compat-generator/{oracle.mjs,fixtures.json}` →
//!   `crates/babel-plugin/tests/compat_generator_corpus.json`.
//! - The integration-test scaffold at
//!   `crates/babel-plugin/tests/compat_generator_integration.rs`
//!   with corpus shape + version-pin assertions live, and the
//!   byte-parity assertion gated `#[ignore]` until §4.3 lands.
//!
//! §4.3 ports the actual logic line-by-line against
//! `node_modules/@babel/generator/lib/`. Until then, calling
//! `generate(...)` panics — by design, so callers know §4.3 is
//! a hard prerequisite.

use swc_core::ecma::ast::Expr;

/// Produce the `@babel/generator@7.23.0` source-text representation
/// of `expr`. Output bytes must match `generate(<babel ast of same
/// source>).code` exactly — see the parity gate at
/// `crates/babel-plugin/tests/compat_generator_integration.rs`.
///
/// The Phase 4 §4.3 port lands the actual implementation; the
/// `#[ignore]` byte-parity test there flips on once this returns
/// real bytes.
pub fn generate(_expr: &Expr) -> String {
    unimplemented!(
        "compat::generator::generate — Phase 4 §4.3 not yet ported. \
         See crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md for the \
         coverage manifest and crates/babel-plugin/tests/\
         compat_generator_integration.rs for the parity gate."
    )
}
