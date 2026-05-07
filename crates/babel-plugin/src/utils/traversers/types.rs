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
    /// **Re-export-from** chain hop. Populated when the matched
    /// export is an `ExportNamedDeclaration` whose `source` is set
    /// (i.e. `export { x } from './y'` or `export { x as default }
    /// from './y'`). Mirrors upstream's `binding.path.isExportNamedDeclaration()`
    /// branch in `resolve-binding.ts:367-394`: when a re-export hop
    /// is detected, upstream parses the source module and looks up
    /// either the default export (if the spec was `as default`) or
    /// the local-side name (`exportedSpecifier.node.local.name`).
    /// `resolve_binding` consumes this to recurse one hop deeper.
    /// `None` for non-re-export shapes.
    pub reexport_from: Option<ReexportHop>,
}

/// One hop of an `export { x [as y] } from './source'` chain.
#[derive(Debug, Clone)]
pub struct ReexportHop {
    /// Module specifier as written in source (e.g. `'./tokens'`).
    pub source: String,
    /// Name to look up in the source module — `local.name` upstream.
    /// For `export { local as exported } from './m'` the
    /// `exportedSpecifier.node.local.name` is `local`. For
    /// `export { x as default } from './m'` resolved via the
    /// default-export caller, this is `"default"` and the caller
    /// dispatches to `get_default_export` rather than
    /// `get_named_export`.
    pub local_name: String,
    /// `true` when the re-export's exported name is `default`. The
    /// upstream branch dispatches to `getDefaultExport` in this
    /// case (`resolve-binding.ts:386-388`).
    pub is_default: bool,
}
