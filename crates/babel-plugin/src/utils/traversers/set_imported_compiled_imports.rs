//! 1:1 port of
//! `packages/babel-plugin/src/utils/traversers/set-imported-compiled-imports.ts`.
//!
//! Walks an imported module's AST. If it imports `css` from any
//! origin, records the imported binding's local name into
//! `state.imported_compiled_imports.css`. Used by `resolve_binding`
//! when traversing into a module that itself uses Compiled APIs —
//! the `css` alias may differ from the entry-file's alias.
//!
//! ## Bug parity
//!
//! Upstream's `apiName = 'css'` is hardcoded — only `css` flows
//! through this trampoline (the other Compiled APIs don't have the
//! same cross-file aliasing concern). This port keeps the constant
//! verbatim. Adding more APIs requires bumping the cache schema
//! AND updating STATE_MUTATIONS.md per the encapsulation contract.
//!
//! The upstream walker's `path.stop()` on first match means at most
//! ONE css alias is recorded per imported module — the first
//! encountered ImportSpecifier with `imported.name === 'css'` wins.
//! Mirrored here.

use swc_core::ecma::ast::{ImportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem};

use crate::state::{ImportedCompiledImports, State};

const API_NAME: &str = "css";

/// Walk `ast`'s top-level imports. If any imports `css` from
/// somewhere, set `state.imported_compiled_imports.css` to the
/// local binding name and stop.
///
/// Idempotent: subsequent calls overwrite the field. Upstream
/// behaviour is "last visit wins" because the JS plugin re-walks
/// per cross-file resolution; the Rust port mirrors by allowing
/// overwrites.
pub fn set_imported_compiled_imports(ast: &Module, state: &mut State) {
    for item in &ast.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else {
            continue;
        };
        for spec in &import.specifiers {
            // Only `ImportSpecifier::Named` covers `import { css } from
            // 'mod'`. `ImportDefault` and `ImportNamespace` are
            // different shapes; upstream's `t.isImportSpecifier` is
            // false for those, so we mirror by not matching.
            let ImportSpecifier::Named(named) = spec else {
                continue;
            };
            // Resolve the imported name. For `import { css } from
            // 'mod'`, `imported` is None (shorthand) and `local.sym`
            // is the imported name. For `import { css as foo } from
            // 'mod'`, `imported` is Some(Ident("css")) and `local.sym`
            // is "foo".
            let imported_name = match named.imported.as_ref() {
                Some(ModuleExportName::Ident(id)) => id.sym.to_string(),
                // Wtf8Atom→str: see compat/scope.rs::register_import comment.
                Some(ModuleExportName::Str(s)) => {
                    s.value.as_str().unwrap_or_default().to_string()
                }
                None => named.local.sym.to_string(),
            };
            if imported_name == API_NAME {
                state.imported_compiled_imports = state
                    .imported_compiled_imports
                    .take()
                    .or_else(|| Some(ImportedCompiledImports::default()))
                    .map(|mut imports| {
                        imports.css = Some(named.local.sym.to_string());
                        imports
                    });
                // Mirror upstream's path.stop() — first match wins.
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
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

    #[test]
    fn records_local_name_for_imported_css() {
        let m = parse(r#"import { css } from '@compiled/react';"#);
        let mut state = State::default();
        set_imported_compiled_imports(&m, &mut state);
        assert_eq!(
            state
                .imported_compiled_imports()
                .and_then(|i| i.css.as_deref()),
            Some("css")
        );
    }

    #[test]
    fn records_alias_for_imported_css() {
        let m = parse(r#"import { css as styles } from '@compiled/react';"#);
        let mut state = State::default();
        set_imported_compiled_imports(&m, &mut state);
        assert_eq!(
            state
                .imported_compiled_imports()
                .and_then(|i| i.css.as_deref()),
            Some("styles")
        );
    }

    #[test]
    fn ignores_non_css_imports() {
        let m = parse(r#"import { styled } from '@compiled/react';"#);
        let mut state = State::default();
        set_imported_compiled_imports(&m, &mut state);
        assert!(state.imported_compiled_imports().is_none());
    }

    #[test]
    fn ignores_default_imports() {
        let m = parse(r#"import css from '@compiled/react';"#);
        let mut state = State::default();
        set_imported_compiled_imports(&m, &mut state);
        assert!(state.imported_compiled_imports().is_none());
    }

    #[test]
    fn first_match_wins_when_multiple_imports() {
        let m = parse(
            r#"
            import { css as first } from 'foo';
            import { css as second } from 'bar';
        "#,
        );
        let mut state = State::default();
        set_imported_compiled_imports(&m, &mut state);
        assert_eq!(
            state
                .imported_compiled_imports()
                .and_then(|i| i.css.as_deref()),
            Some("first"),
        );
    }
}
