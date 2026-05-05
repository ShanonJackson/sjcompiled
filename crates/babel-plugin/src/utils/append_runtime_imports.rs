//! 1:1 port of `packages/babel-plugin/src/utils/append-runtime-imports.ts`.
//!
//! Appends the runtime entrypoint import (`@compiled/react/runtime`) to
//! the module. If a sibling import for that source already exists, new
//! specifiers are pushed onto it; otherwise a fresh `ImportDeclaration`
//! is unshifted to the front of `module.body`.
//!
//! Drift watch points vs upstream:
//!
//! * Specifier set is hard-coded to match upstream's two arrays
//!   verbatim. `WITH_COMPRESSION` (`ac, ix, CC, CS`) fires when
//!   `state.opts.classNameCompressionMap` is set; `WITHOUT_COMPRESSION`
//!   (`ax, ix, CC, CS`) is the default. The runtime function `ac` is
//!   less performant than `ax` so it's only imported when the
//!   compression map is provided.
//! * "Already imported" check uses LOCAL specifier names (not the
//!   imported name) — handles `import { CC as CompiledRoot, CC, CS }`
//!   where `CC` is bound under multiple aliases. Mirrors upstream
//!   lines 49–55 verbatim.
//! * Fresh specifier shape: `local == imported` (no rename); built via
//!   `ImportSpecifier::Named` with `imported = None` per SWC's
//!   "imported defaults to local" convention. Matches Babel's
//!   `t.importSpecifier(t.identifier(name), t.identifier(name))` shape
//!   on the wire (the SWC printer emits `name` not `name as name`).

use swc_core::common::{SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    Ident, ImportDecl, ImportNamedSpecifier, ImportPhase, ImportSpecifier, Module, ModuleDecl,
    ModuleItem, Str,
};

use crate::state::State;

const COMPILED_RUNTIME_IMPORTS_WITH_COMPRESSION: [&str; 4] = ["ac", "ix", "CC", "CS"];
const COMPILED_RUNTIME_IMPORTS_WITHOUT_COMPRESSION: [&str; 4] = ["ax", "ix", "CC", "CS"];
pub const COMPILED_RUNTIME_MODULE: &str = "@compiled/react/runtime";

/// Build a `{ name }` named specifier (no rename: imported defaults to
/// local). Mirrors upstream `importSpecifier(name)` with `localName`
/// omitted.
fn make_named_specifier(name: &str) -> ImportSpecifier {
    ImportSpecifier::Named(ImportNamedSpecifier {
        span: DUMMY_SP,
        local: Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()),
        imported: None,
        is_type_only: false,
    })
}

/// Append (or merge into existing) the `@compiled/react/runtime`
/// import. The runtime entrypoint set depends on
/// `state.opts.class_name_compression_map`.
///
/// Mirrors upstream `appendRuntimeImports(path, state)` (lines 30–76).
pub fn append_runtime_imports(module: &mut Module, state: &State) {
    let runtime_imports: &[&str] = if state.opts().class_name_compression_map.is_some() {
        &COMPILED_RUNTIME_IMPORTS_WITH_COMPRESSION
    } else {
        &COMPILED_RUNTIME_IMPORTS_WITHOUT_COMPRESSION
    };

    // Find existing `import ... from '@compiled/react/runtime'`.
    // Upstream uses `path.get('body').find(isImportDeclaration && source.value === MODULE)`.
    let existing_idx = module.body.iter().position(|item| match item {
        ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) => {
            decl.src.value.to_atom_lossy().as_str() == COMPILED_RUNTIME_MODULE
        }
        _ => false,
    });

    if let Some(idx) = existing_idx {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = &mut module.body[idx] else {
            unreachable!("position() above filtered to ImportDecl")
        };

        // Local-name lookup per upstream comment: handles
        // `import { CC as Foo, CC, CS }` aliasing (the same imported
        // identifier may bind multiple locals).
        let existing_locals: Vec<String> = decl
            .specifiers
            .iter()
            .filter_map(|s| match s {
                ImportSpecifier::Named(named) => Some(named.local.sym.as_ref().to_string()),
                ImportSpecifier::Default(d) => Some(d.local.sym.as_ref().to_string()),
                ImportSpecifier::Namespace(n) => Some(n.local.sym.as_ref().to_string()),
            })
            .collect();

        for name in runtime_imports {
            if !existing_locals.iter().any(|n| n == name) {
                decl.specifiers.push(make_named_specifier(name));
            }
        }
    } else {
        // No existing declaration — prepend a fresh one.
        let new_decl = ImportDecl {
            span: DUMMY_SP,
            specifiers: runtime_imports
                .iter()
                .map(|n| make_named_specifier(n))
                .collect(),
            src: Box::new(Str {
                span: DUMMY_SP,
                value: COMPILED_RUNTIME_MODULE.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: ImportPhase::Evaluation,
        };
        module
            .body
            .insert(0, ModuleItem::ModuleDecl(ModuleDecl::Import(new_decl)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use swc_core::ecma::ast::ModuleExportName;

    use crate::types::PluginOptions;

    fn empty_module() -> Module {
        Module {
            span: DUMMY_SP,
            body: Vec::new(),
            shebang: None,
        }
    }

    fn module_with_one_import(source: &str, locals: &[&str]) -> Module {
        let specifiers = locals.iter().map(|l| make_named_specifier(l)).collect();
        let import = ImportDecl {
            span: DUMMY_SP,
            specifiers,
            src: Box::new(Str {
                span: DUMMY_SP,
                value: source.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: ImportPhase::Evaluation,
        };
        Module {
            span: DUMMY_SP,
            body: vec![ModuleItem::ModuleDecl(ModuleDecl::Import(import))],
            shebang: None,
        }
    }

    fn state_with_opts(opts: PluginOptions) -> State {
        let mut s = State::default();
        // SAFETY: `set_opts` is `pub(crate)` and tests are in-crate.
        s.set_opts(opts);
        s
    }

    fn import_specifier_locals(item: &ModuleItem) -> Vec<String> {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = item else {
            panic!("expected import")
        };
        decl.specifiers
            .iter()
            .filter_map(|s| match s {
                ImportSpecifier::Named(n) => Some(n.local.sym.as_ref().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fresh_module_gets_runtime_import_unshifted() {
        let mut module = empty_module();
        let state = State::default();
        append_runtime_imports(&mut module, &state);
        assert_eq!(module.body.len(), 1);
        let locals = import_specifier_locals(&module.body[0]);
        assert_eq!(locals, vec!["ax", "ix", "CC", "CS"]);
    }

    #[test]
    fn class_name_compression_uses_ac_set() {
        let mut module = empty_module();
        let opts = PluginOptions {
            class_name_compression_map: Some(IndexMap::from([("a".to_string(), "_a".to_string())])),
            ..Default::default()
        };
        let state = state_with_opts(opts);
        append_runtime_imports(&mut module, &state);
        let locals = import_specifier_locals(&module.body[0]);
        assert_eq!(locals, vec!["ac", "ix", "CC", "CS"]);
    }

    #[test]
    fn merges_into_existing_runtime_declaration() {
        // User pre-imported one of our names; we add only the missing.
        let mut module = module_with_one_import(COMPILED_RUNTIME_MODULE, &["ax"]);
        let state = State::default();
        append_runtime_imports(&mut module, &state);
        assert_eq!(module.body.len(), 1, "should not unshift a second import");
        let locals = import_specifier_locals(&module.body[0]);
        assert_eq!(locals, vec!["ax", "ix", "CC", "CS"]);
    }

    #[test]
    fn fully_pre_imported_module_unchanged() {
        let mut module =
            module_with_one_import(COMPILED_RUNTIME_MODULE, &["ax", "ix", "CC", "CS"]);
        let state = State::default();
        append_runtime_imports(&mut module, &state);
        assert_eq!(module.body.len(), 1);
        let locals = import_specifier_locals(&module.body[0]);
        assert_eq!(locals, vec!["ax", "ix", "CC", "CS"]);
    }

    #[test]
    fn local_name_lookup_handles_renamed_specifiers() {
        // Upstream lines 49–55: matches against LOCAL names. So
        // `import { CC as CompiledRoot } from '...'` does NOT count
        // as having `CC` imported — we still push `CC` to the
        // specifier list.
        let import = ImportDecl {
            span: DUMMY_SP,
            specifiers: vec![ImportSpecifier::Named(ImportNamedSpecifier {
                span: DUMMY_SP,
                local: Ident::new(
                    "CompiledRoot".into(),
                    DUMMY_SP,
                    SyntaxContext::empty(),
                ),
                imported: Some(ModuleExportName::Ident(Ident::new(
                    "CC".into(),
                    DUMMY_SP,
                    SyntaxContext::empty(),
                ))),
                is_type_only: false,
            })],
            src: Box::new(Str {
                span: DUMMY_SP,
                value: COMPILED_RUNTIME_MODULE.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: ImportPhase::Evaluation,
        };
        let mut module = Module {
            span: DUMMY_SP,
            body: vec![ModuleItem::ModuleDecl(ModuleDecl::Import(import))],
            shebang: None,
        };
        let state = State::default();
        append_runtime_imports(&mut module, &state);
        // After: original `CC as CompiledRoot` plus four pushed
        // specifiers (`ax`, `ix`, `CC`, `CS`) — `CC` IS pushed
        // because the local name was `CompiledRoot`, not `CC`.
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = &module.body[0] else {
            unreachable!()
        };
        assert_eq!(decl.specifiers.len(), 5);
        let locals: Vec<String> = decl
            .specifiers
            .iter()
            .filter_map(|s| match s {
                ImportSpecifier::Named(n) => Some(n.local.sym.as_ref().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            locals,
            vec!["CompiledRoot", "ax", "ix", "CC", "CS"]
        );
    }

    #[test]
    fn unrelated_imports_stay_intact_and_runtime_unshifted() {
        // Pre-existing import for a different module — we don't
        // touch it; we unshift a fresh `@compiled/react/runtime`
        // import to the front.
        let mut module = module_with_one_import("react", &["useState"]);
        let state = State::default();
        append_runtime_imports(&mut module, &state);
        assert_eq!(module.body.len(), 2);
        // Item 0 is the new runtime import (unshifted).
        let runtime_locals = import_specifier_locals(&module.body[0]);
        assert_eq!(runtime_locals, vec!["ax", "ix", "CC", "CS"]);
        // Item 1 is the original react import.
        let react_locals = import_specifier_locals(&module.body[1]);
        assert_eq!(react_locals, vec!["useState"]);
    }
}
