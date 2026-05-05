//! Byte-for-byte port of `@babel/generator@7.23.0`.
//!
//! Pinned in `crates/PARITY_VERSIONS.md` (AFM resolution under
//! `@compiled/babel-plugin@0.36.1`, commit `16a62b8`).
//! Upstream source: `node_modules/@babel/generator/lib/`.
//!
//! ## Module layout (1:1 with upstream)
//!
//! ```text
//! upstream                                 -> rust
//! lib/index.js              (entry)        -> mod.rs (this file)
//! lib/buffer.js                            -> buffer.rs
//! lib/printer.js                           -> printer.rs
//! lib/node/index.js         (needsParens)  -> printer.rs::needs_parens_for
//! lib/node/parentheses.js                  -> node/parentheses.rs
//! lib/generators/expressions.js            -> generators/expressions.rs
//! lib/generators/types.js                  -> generators/types.rs
//! lib/generators/template-literals.js      -> generators/template_literals.rs
//! lib/generators/jsx.js                    -> generators/jsx.rs
//! lib/generators/{base,classes,methods,modules,statements,flow,typescript}.js
//!     -> intentionally NOT ported. The 5 call sites in
//!        packages/babel-plugin/src/utils/ never feed those node
//!        kinds into generate(). Per CLAUDE.md "1:1 with what's
//!        reachable, not future-proofing" + "no half-baked compat
//!        shims": port the file when a real consumer-monorepo
//!        fixture surfaces the gap, never speculatively.
//! ```
//!
//! ## Why this exists (recap)
//!
//! `swc_ecma_codegen` is NOT byte-equivalent to `@babel/generator`.
//! Concretely it diverges on whitespace, paren policy, quote
//! preservation, trailing-comma policy, property-shorthand collapsing,
//! and comment attachment around ternary branches / eslint-disable
//! / PURE annotations. Output bytes from this generator feed
//! `compiled-utils::hash` for keyframe class-name computation
//! (`packages/babel-plugin/src/utils/css-builders.ts:464`), so a
//! one-byte difference renames every keyframe class in production.
//!
//! See `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` for the
//! per-call-site coverage manifest and the corpus at
//! `crates/babel-plugin/tests/compat_generator_corpus.json`.

pub mod buffer;
pub mod generators;
pub mod node;
pub mod printer;

use swc_core::common::comments::Comments;
use swc_core::ecma::ast::{Expr, JSXAttr, Stmt};

use printer::Printer;

/// `generate(ast, opts)` — upstream's `lib/index.js::generate`. Our
/// signature takes a single `Expr` because the 5 upstream call sites
/// always feed a parsed Expression node. When a future call site
/// hands a Statement / Declaration, extend this to accept `&Module`
/// (or split into `generate_expr` / `generate_module`).
///
/// Output bytes match `@babel/generator(swcExprToBabelAst(expr)).code`
/// for the AST shapes covered by the §4.2 fixtures. Coverage gaps
/// (JSXAttribute, comment attachment around statements, etc.) emit a
/// `/*UNHANDLED-*/` marker so the byte-parity gate fails with a
/// clear pointer.
pub fn generate(expr: &Expr) -> String {
    let mut p = Printer::new();
    p.print(expr, None);
    p.finish()
}

/// Same as `generate`, but threads a SWC `Comments` store. Babel's
/// generator reads `node.leadingComments` / `node.trailingComments`
/// directly off the AST; SWC stores them out-of-band keyed by
/// `BytePos`, so the printer queries the store at every node
/// boundary. Without comments threaded the comment-axis fixtures
/// from §4.2 (eslint-disable, ternary-inner blocks, PURE annotations)
/// fail byte-parity — comments get silently dropped.
pub fn generate_with_comments(expr: &Expr, comments: &dyn Comments) -> String {
    let mut p = Printer::with_comments(Some(comments));
    p.print(expr, None);
    p.finish()
}

/// `generate(jsxAttribute)` — the JSX-key call site at
/// `packages/babel-plugin/src/utils/build-compiled-component.ts:30`
/// hands a `JSXAttribute` (NOT an Expression) to `@babel/generator`.
/// Babel's `generate(node)` dispatches on `node.type` so it picks the
/// `JSXAttribute(node)` printer regardless of where the entry point
/// landed. We can't overload `generate(&Expr)` because the SWC type
/// for a JSX attribute is `JSXAttr`, not `Expr` — so we expose a
/// sibling entry point with the same byte contract.
///
/// Used by the §4.2 corpus's `jsx-key-attribute` axis (5 fixtures).
/// Inner `JSXExpressionContainer` values dispatch back through
/// `Printer::print(&Expr, _)` so the existing precedence /
/// quote-preservation / comment-threading paths apply.
pub fn generate_jsx_attribute(attr: &JSXAttr) -> String {
    let mut p = Printer::new();
    generators::jsx::jsx_attribute(&mut p, attr);
    p.finish()
}

/// Same as `generate_jsx_attribute`, threaded with a SWC comment
/// store so future fixtures with comments around the attribute /
/// inside the expression container land correctly.
pub fn generate_jsx_attribute_with_comments(
    attr: &JSXAttr,
    comments: &dyn Comments,
) -> String {
    let mut p = Printer::with_comments(Some(comments));
    generators::jsx::jsx_attribute(&mut p, attr);
    p.finish()
}

/// `generate(Stmt)` — surfaced by §6.8e (styled/behaviour cluster). Block-
/// body arrows (`props => { return X; }`) feed their `BlockStmt` body
/// through the arrow printer, which recursed into stmt-level printing.
/// Used directly by the unit tests in
/// `generators/statements.rs`.
pub fn generate_stmt(stmt: &Stmt) -> String {
    let mut p = Printer::new();
    generators::statements::print_statement(&mut p, stmt);
    p.finish()
}
