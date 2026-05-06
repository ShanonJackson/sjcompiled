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

use std::cell::Cell;

use swc_core::common::comments::Comments;
use swc_core::ecma::ast::{Expr, JSXAttr, Stmt};

use printer::Printer;

thread_local! {
    /// Ambient comments handle for `generate(&Expr)` and friends.
    ///
    /// Babel's `@babel/generator` reads `node.leadingComments` /
    /// `trailingComments` directly off AST nodes. SWC stores comments
    /// out-of-band keyed by `BytePos`, so the printer needs a
    /// `Comments` trait reference to query.
    ///
    /// Threading `Comments` through every `generate()` call site in
    /// `utils/css_builders.rs` and `utils/normalize_props_usage.rs`
    /// (10+ sites) would touch every signature on the call-graph.
    /// Instead, the plugin entry (`lib.rs::process`) installs the
    /// host's `PluginCommentsProxy` here for the duration of
    /// `visit_mut_with`, and `generate()` reads from it.
    ///
    /// **Soundness:** the pointer is set via [`set_ambient_comments`]
    /// from a scope that owns the comments value (`process()` /
    /// `run_dispatcher()`); cleared via [`clear_ambient_comments`]
    /// before that scope exits. Plugin invocations are single-threaded
    /// (SWC runs each plugin call on its own WASI instance with no
    /// inter-call shared state), so the cross-plugin-call view is
    /// always `None` at entry. Within a call, every `generate()` runs
    /// on the same thread under the same scope.
    ///
    /// Stored as a fat pointer (`(*const T, *const VTable)`) — `Cell`
    /// is `Copy`-friendly so we use it without a `RefCell`.
    static AMBIENT_COMMENTS: Cell<Option<*const dyn Comments>> = const { Cell::new(None) };
}

/// Install a comments handle for the duration of the current plugin
/// invocation. SAFETY: `comments` MUST outlive every subsequent call
/// to `generate()` until [`clear_ambient_comments`] is called.
///
/// In production, `lib.rs::process` calls this with `&meta.comments`
/// before `program.visit_mut_with(&mut visitor)`, and clears
/// immediately after. Tests / `run_dispatcher` follow the same
/// scoping discipline (or skip the install — `generate()` falls back
/// to `Printer::new()` when no ambient is set, dropping comments —
/// matching the pre-fix behaviour).
pub fn set_ambient_comments<C: Comments>(comments: &C) {
    // Lifetime erasure: store as `*const dyn Comments` so the
    // thread-local can outlive the borrow's static-lifetime
    // requirement. `Cell` only stores by-value, and a non-`'static`
    // borrow inside a `Cell<...>` would fail the borrow checker
    // even with an explicit scope. SAFETY: see thread-local doc —
    // caller MUST `clear_ambient_comments` before `comments` drops.
    let trait_ref: &dyn Comments = comments;
    let ptr: *const dyn Comments = unsafe {
        // Coerce `&'_ dyn Comments` to `&'static dyn Comments` then
        // to `*const dyn Comments`. The unsafe block widens the
        // lifetime; we re-narrow it (back to "valid for current
        // scope") at every read site by ensuring `clear_ambient_comments`
        // runs before the source borrow ends.
        std::mem::transmute::<&dyn Comments, &'static dyn Comments>(trait_ref)
    };
    AMBIENT_COMMENTS.with(|c| c.set(Some(ptr)));
}

/// Clear the ambient comments handle. Call BEFORE the comments value
/// goes out of scope. Idempotent.
pub fn clear_ambient_comments() {
    AMBIENT_COMMENTS.with(|c| c.set(None));
}

/// Build a `Printer` that uses the ambient comments if installed.
/// SAFETY: any `*const dyn Comments` we read here was set via
/// `set_ambient_comments` from a scope that owns the comments value
/// and clears the cell before exit. The visitor pass is synchronous.
fn printer_with_ambient_comments<'a>() -> Printer<'a> {
    AMBIENT_COMMENTS.with(|c| match c.get() {
        // SAFETY: see thread-local doc — pointer is live for the
        // current visitor call.
        Some(ptr) => unsafe { Printer::with_comments(Some(&*ptr)) },
        None => Printer::new(),
    })
}

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
    let mut p = printer_with_ambient_comments();
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
    let mut p = printer_with_ambient_comments();
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
    let mut p = printer_with_ambient_comments();
    generators::statements::print_statement(&mut p, stmt);
    p.finish()
}
