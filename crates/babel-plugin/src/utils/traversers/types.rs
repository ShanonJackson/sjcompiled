//! 1:1 port of `packages/babel-plugin/src/utils/traversers/types.ts`.
//!
//! Babel's `Result<T>` is `{ node: t.Node; path: NodePath<T> }`. The
//! Rust analog drops the `path` field — `resolve_binding.rs` only
//! reads `node` from the `Result` value, and `path` would require
//! threading a per-imported-file `ScopeIndex` we haven't built yet.
//! When the §5.6 evaluator surfaces a real `path`-identity need,
//! extend [`ExportResult`] with a richer handle then.

use swc_core::ecma::ast::Expr;

/// What `getDefaultExport` / `getNamedExport` returns when an
/// export is found.
///
/// `node` is the resolved expression — `'blue'` for
/// `export default 'blue';`, the rhs of `export const x = ...;`,
/// or a non-expression placeholder upstream Babel returns as a
/// `t.Node` (e.g. an `Identifier` for `export { color };`). When
/// the resolved node isn't an `Expr`, we set `node = None` so the
/// downstream evaluator deopts cleanly — preserves the upstream
/// behaviour where `path.evaluate()` returns `confident: false`
/// for non-expression-shaped exports.
#[derive(Debug, Clone)]
pub struct ExportResult {
    pub node: Option<Box<Expr>>,
}
