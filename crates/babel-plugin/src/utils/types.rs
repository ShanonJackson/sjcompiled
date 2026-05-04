//! 1:1 port of `packages/babel-plugin/src/utils/types.ts`.
//!
//! Pure type defs consumed by `utils/css_builders.rs` (and downstream
//! Phase 5/6 modules). No runtime logic; the only Babel→SWC divergences
//! here are the AST node types:
//!
//! * Babel `t.Expression` → SWC `Box<swc_core::ecma::ast::Expr>` (Expr
//!   is large; storing by box matches how SWC threads expressions).
//! * Babel `NodePath` → opaque recorder handle (`u32` id). Phase 5
//!   §5.4 (`resolve_binding`) is what populates this; today the field
//!   is a placeholder so `PartialBindingWithMeta` shape holds.
//! * Babel `'||' | '??' | '&&'` operator string →
//!   `swc_core::ecma::ast::BinaryOp::{LogicalOr, NullishCoalescing,
//!   LogicalAnd}`. Note that SWC unifies binary + logical ops in one
//!   `BinaryOp` enum; only the three logical variants reach this file.
//!
//! `EvaluateExpression` is intentionally a function-type alias on the
//! JS side; Rust uses a trait object / closure boundary at the
//! traverse-expression layer (Phase 5 §5.6). No analog needed here —
//! callers thread a closure / fn-pointer directly.

use swc_core::ecma::ast::Expr;

/// `{ type: 'unconditional', css: string }` — a static rule.
#[derive(Debug, Clone)]
pub struct UnconditionalCssItem {
    pub css: String,
}

/// `{ type: 'conditional', test, consequent, alternate }` — a
/// ternary-shaped CSS branch. Both branches recurse to `CssItem`.
#[derive(Debug, Clone)]
pub struct ConditionalCssItem {
    pub test: Box<Expr>,
    pub consequent: Box<CssItem>,
    pub alternate: Box<CssItem>,
}

/// `{ type: 'logical', expression, operator, css }` — a single-branch
/// guard (e.g. `props.isPrimary && { color: 'blue' }`).
#[derive(Debug, Clone)]
pub struct LogicalCssItem {
    pub expression: Box<Expr>,
    pub operator: LogicalOperator,
    pub css: String,
}

/// Pre-rendered stylesheet (`{ type: 'sheet', css }`). Cannot be
/// merged with adjacent unconditional items; promoted to the front of
/// the output by `mergeSubsequentUnconditionalCssItems`.
#[derive(Debug, Clone)]
pub struct SheetCssItem {
    pub css: String,
}

/// `{ type: 'map', name, expression, css }` — a `cssMap()`-bound
/// member expression. Resolved at the cssMap handler (Phase 6 §6.3).
#[derive(Debug, Clone)]
pub struct CssMapItem {
    pub name: String,
    pub expression: Box<Expr>,
    pub css: String,
}

/// Discriminated union mirroring upstream's `CssItem` type alias.
///
/// JS shape: `{ type: 'unconditional' | 'conditional' | 'logical' |
/// 'sheet' | 'map' }` with per-variant fields. Rust uses a tagged
/// enum — same variants, same discriminator.
#[derive(Debug, Clone)]
pub enum CssItem {
    Unconditional(UnconditionalCssItem),
    Conditional(ConditionalCssItem),
    Logical(LogicalCssItem),
    Sheet(SheetCssItem),
    Map(CssMapItem),
}

/// Babel uses `'&&' | '||' | '??'` as raw strings. SWC's `BinaryOp`
/// enum unifies binary + logical ops; only the three logical variants
/// reach this file. We model with a narrow enum so the call shape at
/// `getLogicalItemFromConditionalExpression` stays exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOperator {
    /// `&&`
    And,
    /// `||`
    Or,
    /// `??`
    NullishCoalescing,
}

/// `Variable` — one entry of `CSSOutput.variables`. Drives the inline
/// `style={{ '--_x': value }}` emit at the consumer site.
#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub expression: Box<Expr>,
    /// Optional CSS-syntax prefix (e.g. `-` for negative-value
    /// templates). Populated by `cssAffixInterpolation`.
    pub prefix: Option<String>,
    /// Optional CSS-syntax suffix (e.g. `px`). Populated by
    /// `cssAffixInterpolation`.
    pub suffix: Option<String>,
}

/// `CSSOutput` — the return shape for every `extract*` /
/// `buildCss` function in `css_builders.rs`.
#[derive(Debug, Clone, Default)]
pub struct CSSOutput {
    pub css: Vec<CssItem>,
    pub variables: Vec<Variable>,
}

/// `PartialBindingWithMeta` — what `resolveBinding` returns.
///
/// **§5.4e shape (locked at this checkpoint):** drops the `'a`
/// lifetime that the §4.4 placeholder carried (`meta: Metadata<'a>`
/// can't reference a different file's State than the caller's), and
/// drops the `path_id: u32` recorder-handle placeholder (no consumer
/// dereferences it — `compat::path::PathHandle` is the §5.5/§5.6
/// surface for path-shaped data).
///
/// The `node` is the resolved expression cloned out of the source
/// module, so the caller doesn't need to keep the imported AST
/// alive. `imported_filename` carries the resolved module's
/// absolute path so the §5.6 evaluator can include-files-track and
/// re-construct a `Metadata` pointed at the imported file when it
/// needs to fold deeper.
///
/// `node = None` is a meaningful "binding found but resolved to a
/// non-expression shape" — function/class/interface decls, etc.
/// Callers deopt.
#[derive(Debug)]
pub struct PartialBindingWithMeta {
    /// The resolved expression. `None` when the binding's resolved
    /// node isn't an `Expr` (declaration shape, namespace import).
    pub node: Option<Box<Expr>>,
    pub constant: bool,
    pub source: BindingSource,
    /// Absolute path of the imported module, when `source ==
    /// Import`. `None` for same-file (`source == Module`)
    /// resolutions and for namespace-import resolutions.
    pub imported_filename: Option<String>,
}


/// `'import' | 'module'` discriminator on `PartialBindingWithMeta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingSource {
    Import,
    Module,
}
