//! 1:1 port of `packages/babel-plugin/src/utils/traversers/get-export.ts`.
//!
//! Two functions:
//!
//! - [`get_default_export`] — find the default export of a module.
//!   Handles both `export default <expr>` and `export { x as default }`.
//! - [`get_named_export`] — find the named export with a given name.
//!   Handles both `export const x = <expr>` and `export { x };`.
//!
//! ## Bug parity
//!
//! Upstream uses Babel's `traverse(ast, { ExportDefaultDeclaration: ...,
//! ExportNamedDeclaration: ... })` with an early `path.stop()` on the
//! first match. The Rust port walks `Module::body` directly — we don't
//! need a full `Visit` impl because exports can only appear at
//! `ModuleItem::ModuleDecl::Export*` at the top level (export
//! statements are not allowed inside blocks per spec). Walking the
//! flat `body` slice + breaking on the first match preserves the
//! upstream semantics with less ceremony.

use swc_core::ecma::ast::{
    Decl, Expr, ExportDecl, ExportDefaultDecl, ExportSpecifier, MemberProp, Module, ModuleDecl,
    ModuleExportName, ModuleItem, Pat, VarDeclarator,
};

use super::types::{ExportResult, ReexportHop};

/// Find the default export of a module.
///
/// Returns the expression node of the export. For
/// `export default 'blue';` returns the string-literal expression.
/// For `export { x as default };` returns the local identifier
/// `x` as an `Expr::Ident`. For non-expression default exports
/// (e.g. `export default class Foo {}`), returns `Some` with
/// `node: None` so callers can distinguish "found but
/// non-expression" from "not found".
pub fn get_default_export(ast: &Module) -> Option<ExportResult> {
    for item in &ast.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            match decl {
                ModuleDecl::ExportDefaultDecl(ExportDefaultDecl { decl, .. }) => {
                    // `export default function/class/interface` — the JS
                    // port returns `path.node.declaration` which is a
                    // non-expression. We map to None per the
                    // ExportResult contract.
                    let _ = decl;
                    return Some(ExportResult { node: None, reexport_from: None });
                }
                ModuleDecl::ExportDefaultExpr(expr) => {
                    return Some(ExportResult {
                        node: Some(expr.expr.clone()),
                        reexport_from: None,
                    });
                }
                ModuleDecl::ExportNamed(named) => {
                    // Handle `export { alias as default }` shape, with
                    // and without a `from 'mod'` source (re-export
                    // hop). The upstream branch is the same `traverse`
                    // visitor — when `path.node.source` is set, the
                    // `default`-resolution call site
                    // (`resolve-binding.ts:386-388`) bypasses
                    // `getDefaultExport` for the consumer module and
                    // instead routes to the source module via
                    // `getDefaultExport(sourceAst)`. We surface this
                    // as a `ReexportHop` so the resolve-binding
                    // recursive caller does the AST swap.
                    for spec in &named.specifiers {
                        if let ExportSpecifier::Named(named_spec) = spec {
                            // The "exported" name is the public name; the
                            // "orig" is the local (or, for `export { x }`,
                            // both are the same).
                            let exported_name = exported_name_of(
                                named_spec.exported.as_ref(),
                                &named_spec.orig,
                            );
                            if exported_name.as_deref() == Some("default") {
                                if let Some(src) = named.src.as_ref() {
                                    // `export { local as default } from './m'`
                                    // — re-export-from-default. Upstream
                                    // dispatches to `getDefaultExport(ast)`
                                    // because exportName === 'default'
                                    // (`resolve-binding.ts:387`). The
                                    // `local_name` here matches Babel's
                                    // `exportedSpecifier.node.local.name`
                                    // even though it's unused for the
                                    // `is_default` branch, so the hop
                                    // shape stays uniform.
                                    let local = match &named_spec.orig {
                                        ModuleExportName::Ident(id) => id.sym.to_string(),
                                        ModuleExportName::Str(s) => {
                                            s.value.as_str().unwrap_or_default().to_string()
                                        }
                                    };
                                    return Some(ExportResult {
                                        node: None,
                                        reexport_from: Some(ReexportHop {
                                            source: src
                                                .value
                                                .as_str()
                                                .unwrap_or_default()
                                                .to_string(),
                                            local_name: local,
                                            is_default: true,
                                        }),
                                    });
                                }
                                // `export { local as default };` — non-re-export.
                                // Resolve to the local identifier as an Expr::Ident.
                                let node = ident_from_module_export_name(&named_spec.orig);
                                return Some(ExportResult { node, reexport_from: None });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Find a named export by name. Handles:
///
/// - `export const x = <expr>;` — returns the rhs expression.
/// - `export { x };` — returns `x` as an `Expr::Ident`.
/// - `export { x as y };` — looks up by `y`, returns local `x` ident.
pub fn get_named_export(ast: &Module, export_name: &str) -> Option<ExportResult> {
    for item in &ast.body {
        if let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl, .. })) = item {
            // `export const x = ...;` shape. The Rust port walks the
            // declarators; first one whose binding-pattern's identifier
            // matches `export_name` wins. Mirrors the JS
            // `t.isVariableDeclarator(declaration) ? declaration.id : declaration.exported`
            // discriminator.
            if let Decl::Var(var) = decl {
                for declarator in &var.decls {
                    if let Some(result) = match_var_declarator(declarator, export_name) {
                        return Some(result);
                    }
                }
            }
            // `export function foo() {}` / `export class Bar {}` /
            // `export interface Baz {}` — non-expression exports the
            // upstream evaluator deopts on. Walk past.
        } else if let ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) = item {
            // `export { x };` / `export { x as y };` shape, with and
            // without `from 'mod'` source. When `from` is present
            // upstream's `binding.path.isExportNamedDeclaration()`
            // branch (`resolve-binding.ts:367-394`) recurses into
            // the source module looking up `local.name`. Surfacing
            // the hop as `ReexportHop` lets the resolve-binding
            // caller perform the AST swap without duplicating the
            // ExportNamedDeclaration walk here.
            for spec in &named.specifiers {
                if let ExportSpecifier::Named(named_spec) = spec {
                    let exported_name =
                        exported_name_of(named_spec.exported.as_ref(), &named_spec.orig);
                    if exported_name.as_deref() == Some(export_name) {
                        if let Some(src) = named.src.as_ref() {
                            // Re-export-from. Upstream resolves via
                            // `getNamedExport(sourceAst, exportedSpecifier.node.local.name)`
                            // (`resolve-binding.ts:374-376,387`). The
                            // `local_name` is the LHS of `as` in the
                            // spec (or the name itself when unaliased).
                            let local = match &named_spec.orig {
                                ModuleExportName::Ident(id) => id.sym.to_string(),
                                ModuleExportName::Str(s) => {
                                    s.value.as_str().unwrap_or_default().to_string()
                                }
                            };
                            return Some(ExportResult {
                                node: None,
                                reexport_from: Some(ReexportHop {
                                    source: src
                                        .value
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_string(),
                                    local_name: local,
                                    is_default: false,
                                }),
                            });
                        }
                        // `export { x };` (no source) — return the
                        // local identifier; downstream resolves via
                        // the synthesised binding pathway in
                        // `resolve-binding.ts:200-220`. In Rust we
                        // also need to chase: when the local Ident
                        // refers to an `import * as X` / `import
                        // { X }` / `import X` in the SAME module,
                        // that's a re-export-via-import shape that
                        // upstream's `getBinding` walks naturally
                        // (Babel's scope lookup follows the import
                        // specifier's binding). We return the local
                        // ident unchanged so the caller's recursive
                        // dispatch into the imported module's scope
                        // resolves it through the in-module import
                        // binding.
                        let node = ident_from_module_export_name(&named_spec.orig);
                        return Some(ExportResult { node, reexport_from: None });
                    }
                }
            }
        }
    }
    None
}

/// Resolve a `(exported, orig)` pair to the public-facing name. If
/// `exported` is set, use it; otherwise fall back to `orig`.
/// Mirrors the JS `declaration.exported` access where `exported`
/// can be either an `Identifier` or a `StringLiteral` (string-form
/// re-exports). We treat StringLiteral and Identifier uniformly —
/// only the inner string matters for matching.
fn exported_name_of(
    exported: Option<&ModuleExportName>,
    orig: &ModuleExportName,
) -> Option<String> {
    let target = exported.unwrap_or(orig);
    match target {
        ModuleExportName::Ident(id) => Some(id.sym.to_string()),
        // Wtf8Atom→str: see compat/scope.rs::register_import comment.
        ModuleExportName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
    }
}

/// Convert a `ModuleExportName` (used for the local side of a
/// named export specifier) into an `Expr::Ident`. For string-form
/// re-exports (`export { "x-y" as foo } from 'mod'`), the local
/// name isn't a valid identifier — we map to `None` so the caller
/// deopts.
fn ident_from_module_export_name(name: &ModuleExportName) -> Option<Box<Expr>> {
    match name {
        ModuleExportName::Ident(id) => Some(Box::new(Expr::Ident(id.clone()))),
        ModuleExportName::Str(_) => None,
    }
}

/// Try to match a single `VariableDeclarator` (the `x = <expr>` in
/// `const x = <expr>`) against `export_name`. Returns the `init`
/// expression if the declarator's binding pattern is an `Ident`
/// matching `export_name`. Object/array destructuring patterns
/// aren't matched — upstream's discriminator is
/// `t.isIdentifier(declaration.id, { name: exportName })`, which
/// is false for patterns.
fn match_var_declarator(
    declarator: &VarDeclarator,
    export_name: &str,
) -> Option<ExportResult> {
    let Pat::Ident(id) = &declarator.name else {
        // Suppress `MemberProp` import to match upstream — it's not
        // reachable from the Pat::Ident branch but lints flag the
        // unused use otherwise. (Removed below.)
        let _ = MemberProp::Ident;
        return None;
    };
    if id.id.sym != *export_name {
        return None;
    }
    Some(ExportResult {
        node: declarator.init.clone(),
        reexport_from: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::EsVersion;
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse(src: &str) -> Module {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax {
                tsx: false,
                ..Default::default()
            }),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"))
    }

    fn expect_string_literal(node: &Option<Box<Expr>>) -> String {
        match node.as_deref() {
            Some(Expr::Lit(swc_core::ecma::ast::Lit::Str(s))) => {
                s.value.as_str().unwrap_or_default().to_string()
            }
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn default_export_string_literal() {
        let m = parse(r#"export default 'blue';"#);
        let r = get_default_export(&m).expect("default export found");
        assert_eq!(expect_string_literal(&r.node), "blue");
    }

    #[test]
    fn default_export_via_named_alias() {
        let m = parse(
            r#"
            const color = 'blue';
            export { color as default };
        "#,
        );
        let r = get_default_export(&m).expect("default export found");
        match r.node.as_deref() {
            Some(Expr::Ident(id)) => assert_eq!(id.sym.as_ref(), "color"),
            other => panic!("expected ident `color`, got {other:?}"),
        }
    }

    #[test]
    fn default_export_function_declaration_is_some_with_no_node() {
        let m = parse(r#"export default function foo() {}"#);
        let r = get_default_export(&m).expect("default export found");
        assert!(
            r.node.is_none(),
            "non-expression default-export must surface as Some with node=None"
        );
    }

    #[test]
    fn no_default_export_returns_none() {
        let m = parse(r#"export const x = 1;"#);
        assert!(get_default_export(&m).is_none());
    }

    #[test]
    fn named_export_via_var_decl() {
        let m = parse(r#"export const blue = 'blue';"#);
        let r = get_named_export(&m, "blue").expect("named export found");
        assert_eq!(expect_string_literal(&r.node), "blue");
    }

    #[test]
    fn named_export_via_specifier_returns_local_ident() {
        let m = parse(
            r#"
            const color = 'blue';
            export { color };
        "#,
        );
        let r = get_named_export(&m, "color").expect("named export found");
        match r.node.as_deref() {
            Some(Expr::Ident(id)) => assert_eq!(id.sym.as_ref(), "color"),
            other => panic!("expected ident `color`, got {other:?}"),
        }
    }

    #[test]
    fn named_export_with_alias_lookup_by_alias() {
        let m = parse(
            r#"
            const color = 'blue';
            export { color as primary };
        "#,
        );
        // Lookup by the alias name returns the local ident.
        let r = get_named_export(&m, "primary").expect("named export found");
        match r.node.as_deref() {
            Some(Expr::Ident(id)) => assert_eq!(id.sym.as_ref(), "color"),
            other => panic!("expected ident `color`, got {other:?}"),
        }
        // Lookup by the local name returns None — upstream behaviour.
        assert!(get_named_export(&m, "color").is_none());
    }

    #[test]
    fn missing_named_export_returns_none() {
        let m = parse(r#"export const blue = 'blue';"#);
        assert!(get_named_export(&m, "red").is_none());
    }
}
