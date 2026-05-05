//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/evaluate-path/namespace-import.ts`.
//!
//! ```ts
//! export const evaluateNamespaceImportPath = (
//!   expression: t.Expression,
//!   file: t.File,
//!   meta: Metadata,
//!   exportName: string
//! ): ReturnType<typeof createResultPair> => {
//!   const result =
//!     exportName === 'default' ? getDefaultExport(file) : getNamedExport(file, exportName);
//!
//!   if (result) {
//!     const { node, path } = result;
//!     const updatedMeta = { ...meta, parentPath: path, ownPath: meta.parentPath };
//!     const { parentPath } = updatedMeta;
//!
//!     if (exportName === 'default' && !parentPath.scope.getOwnBinding('default')) {
//!       parentPath.scope.push({
//!         id: t.identifier('default'),
//!         init: node as t.Expression,
//!         kind: 'const',
//!       });
//!     }
//!
//!     return createResultPair(node as t.Expression, updatedMeta);
//! ```
//!
//! ## Status (post-§5.6 wiring)
//!
//! **Body landed and reachable.** Uses
//! [`PartialBindingWithMeta::imported_module`] (post-§5.4e drift-fix)
//! to dispatch into [`crate::utils::traversers::get_default_export`]
//! / [`crate::utils::traversers::get_named_export`] and synthesise
//! the 'default' binding via
//! [`crate::compat::scope::ScopeIndex::register_synthetic_binding`]
//! against a fresh [`crate::compat::scope::ScopeIndex::build`] over
//! the imported module.
//!
//! **Caller wired at §5.6.** The §5.6 evaluator
//! (`utils::evaluate_expression::dispatch_evaluate`) handles the
//! MemberExpression branch with a namespace-import preflight.
//! When the bottom binding identifier of a member chain resolves to
//! a namespace import (`source == Import &&
//! imported_module.is_some() && node.is_none()`) AND the chain has
//! a non-empty access path, the dispatcher routes the FIRST
//! access-path element through this function and continues the
//! remaining chain against the imported scope. The standard
//! `evaluate_path::evaluate_path` dispatch's
//! "ImportNamespaceSpecifier unreachable" caveat is therefore
//! sidestepped by routing AT THE MEMBER-EXPRESSION ENTRY rather
//! than mid-chain.
//!
//! ## SWC mapping
//!
//! - JS `t.identifier('default')` synthetic binding → SWC
//!   `ScopeIndex::register_synthetic_binding` against the fresh
//!   imported-module scope index.
//! - JS `parentPath.scope.push({ id, init, kind: 'const' })` —
//!   binding-table insertion only (no AST mutation needed because
//!   the imported module's AST is opaque to subsequent visitor
//!   passes — `Arc<Module>` is a side-table, not part of the
//!   transform-target tree).
//! - JS `meta` shape change (`parentPath: path; ownPath:
//!   meta.parentPath`) doesn't have a Rust analog because Rust
//!   `Metadata` doesn't carry path refs. The §5.6 caller will
//!   thread the imported scope's `parent_scope` / `own_scope` as
//!   explicit parameters when it routes here, mirroring the
//!   `resolve_binding` convention.

use std::sync::Arc;

use swc_core::ecma::ast::{Expr, Module};

use crate::compat::scope::{Binding, BindingKind, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::traversers::{get_default_export, get_named_export};

/// 1:1 port of `evaluateNamespaceImportPath`. Returns the resolved
/// export expression for `<namespace>.<exportName>` against the
/// imported module's parsed AST.
///
/// **Parameters:**
/// - `expression`: the input expression (returned unchanged on
///   miss, mirroring upstream's `createResultPair(expression, meta)`
///   fall-through).
/// - `imported_module`: the imported file's parsed AST. Sourced
///   from [`PartialBindingWithMeta::imported_module`] post-§5.4e
///   drift-fix.
/// - `imported_scope_index`: a fresh `ScopeIndex` built over the
///   imported module. The §5.6 caller is expected to construct this
///   via `ScopeIndex::build(&*imported_module)` once per resolution
///   AND THREAD IT BACK as `&mut` so the synthetic 'default'
///   binding sticks for subsequent calls. (The Arc-shared `Module`
///   is parsed once per `resolve_binding` call and shared; the
///   `ScopeIndex` is built once per cross-file fold boundary and
///   should likewise be shared via `&mut` from the caller.)
/// - `meta`: caller's `Metadata`. Threaded through unchanged for
///   the `_meta` parity-preserving slot of `create_result_pair`;
///   no fields are read in this function.
/// - `export_name`: the `pathName` from the access-path chain
///   (`<namespace>.<exportName>`).
pub fn evaluate_namespace_import_path<'a>(
    expression: &Expr,
    imported_module: &Arc<Module>,
    imported_scope_index: &mut ScopeIndex,
    meta: &mut Metadata<'a>,
    export_name: &str,
) -> ResultPair {
    let result = if export_name == "default" {
        get_default_export(imported_module)
    } else {
        get_named_export(imported_module, export_name)
    };

    if let Some(export) = result {
        // Synthetic 'default' binding side-effect (upstream behaviour
        // at `evaluate-path/namespace-import.ts:18-26`). Only fires
        // when:
        //   1. exportName === 'default' (the JS plugin synthesises
        //      ONLY the `default` shorthand binding so subsequent
        //      `getOwnBinding('default')` lookups against the
        //      imported scope find the synthetic).
        //   2. The imported scope doesn't already own a `default`
        //      binding (idempotency — JS uses
        //      `!parentPath.scope.getOwnBinding('default')` as the
        //      gate).
        if export_name == "default"
            && imported_scope_index
                .get_own_binding(imported_scope_index.program_scope(), "default")
                .is_none()
        {
            if let Some(node) = export.node.as_ref() {
                let synthetic = Binding {
                    kind: BindingKind::Const,
                    identifier_name: "default".to_string(),
                    constant: true,
                    constant_violations: Vec::new(),
                    reference_paths: Vec::new(),
                    binding_node_type: "VariableDeclarator",
                    parent_node_type: "VariableDeclaration",
                    binding_init_string: None,
                    init_expr: Some(node.clone()),
                    binding_id_type: Some("Identifier"),
                    scope: imported_scope_index.program_scope(),
                    span: swc_core::common::DUMMY_SP,
                    import_info: None,
                };
                let prog = imported_scope_index.program_scope();
                imported_scope_index.register_synthetic_binding(prog, "default", synthetic);
            }
        }

        // Return the resolved export expression. JS upstream returns
        // `node as t.Expression` even when `node` might not strictly
        // be an Expr — the §5.4e `ExportResult` shape already
        // pre-filtered to `Option<Box<Expr>>`, so we propagate
        // None unchanged (rare path: matching export but
        // non-expression resolved value, e.g. `export class X {}`).
        return create_result_pair(export.node, meta);
    }

    // Miss: no matching export found in the imported file. Mirrors
    // upstream's `createResultPair(expression, meta)` fall-through.
    create_result_pair(Some(Box::new(expression.clone())), meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use std::sync::Arc;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, Lit};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_module(src: &str) -> Arc<Module> {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"));
        Arc::new(module)
    }

    fn meta_for_test<'a>(state: &'a mut State) -> Metadata<'a> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        }
    }

    #[test]
    fn resolves_named_export() {
        let imported = parse_module("export const color = 'blue';");
        let mut imported_index = ScopeIndex::build(&imported);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let placeholder = Expr::Ident(swc_core::ecma::ast::Ident::new(
            "theme".into(),
            swc_core::common::DUMMY_SP,
            Default::default(),
        ));
        let pair = evaluate_namespace_import_path(
            &placeholder,
            &imported,
            &mut imported_index,
            &mut meta,
            "color",
        );
        let v = pair.value.expect("resolved export");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "blue"),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn default_export_synthesises_default_binding() {
        let imported = parse_module("export default 'red';");
        let mut imported_index = ScopeIndex::build(&imported);
        let prog = imported_index.program_scope();
        // No 'default' binding exists pre-call.
        assert!(imported_index.get_own_binding(prog, "default").is_none());
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let placeholder = Expr::Ident(swc_core::ecma::ast::Ident::new(
            "theme".into(),
            swc_core::common::DUMMY_SP,
            Default::default(),
        ));
        let pair = evaluate_namespace_import_path(
            &placeholder,
            &imported,
            &mut imported_index,
            &mut meta,
            "default",
        );
        // Returned node is the export's value.
        let v = pair.value.expect("resolved default");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "red"),
            other => panic!("expected string literal, got {other:?}"),
        }
        // Synthetic 'default' binding now visible.
        let synthetic = imported_index
            .get_own_binding(prog, "default")
            .expect("synthetic default binding");
        assert_eq!(synthetic.kind, BindingKind::Const);
        assert!(synthetic.constant);
        assert!(synthetic.init_expr.is_some());
    }

    #[test]
    fn default_export_synthetic_binding_is_idempotent() {
        // Second call doesn't overwrite the existing synthetic.
        let imported = parse_module("export default 'red';");
        let mut imported_index = ScopeIndex::build(&imported);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let placeholder = Expr::Ident(swc_core::ecma::ast::Ident::new(
            "theme".into(),
            swc_core::common::DUMMY_SP,
            Default::default(),
        ));
        let _ = evaluate_namespace_import_path(
            &placeholder,
            &imported,
            &mut imported_index,
            &mut meta,
            "default",
        );
        let _ = evaluate_namespace_import_path(
            &placeholder,
            &imported,
            &mut imported_index,
            &mut meta,
            "default",
        );
        let prog = imported_index.program_scope();
        assert!(imported_index.get_own_binding(prog, "default").is_some());
    }

    #[test]
    fn missing_export_falls_through_to_input_expression() {
        let imported = parse_module("export const color = 'blue';");
        let mut imported_index = ScopeIndex::build(&imported);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let placeholder = Expr::Ident(swc_core::ecma::ast::Ident::new(
            "theme".into(),
            swc_core::common::DUMMY_SP,
            Default::default(),
        ));
        let pair = evaluate_namespace_import_path(
            &placeholder,
            &imported,
            &mut imported_index,
            &mut meta,
            "missingExport",
        );
        // Falls through to `createResultPair(expression, meta)` —
        // returns the input expression unchanged.
        let v = pair.value.expect("fall-through value");
        match *v {
            Expr::Ident(id) => assert_eq!(id.sym.as_str(), "theme"),
            other => panic!("expected input identifier, got {other:?}"),
        }
    }
}
