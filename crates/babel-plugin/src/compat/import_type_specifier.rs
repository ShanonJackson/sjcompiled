//! Babel-pipeline parity: drop TypeScript `type` specifiers from every
//! `ImportDeclaration` so the AST matches what `@compiled/babel-plugin`
//! sees in production under `@babel/preset-typescript`.
//!
//! Why this exists
//! ---------------
//! Compiled's Babel pipeline runs under
//! `@babel/preset-typescript { onlyRemoveTypeImports: true }`. Plugins
//! run BEFORE presets, but `transform-typescript` registers a
//! `Program::enter` pass that strips type-only specifiers (and entire
//! `import type {…}` declarations) from the AST before the visitor
//! body sees them. By the time `compiled-babel-plugin`'s
//! `ImportDeclaration` visitor fires, an input like
//!
//! ```ts
//! import { css, styled, jsx, type CssFunction } from '@compiled/react';
//! ```
//!
//! has already been narrowed to `[css, styled, jsx]` — the `type`
//! specifier is gone. After Compiled then strips its known APIs and
//! removes empty Compiled imports
//! (`if (specifiers.length === 0) path.remove()`), the entire
//! declaration disappears.
//!
//! In SWC's pipeline the equivalent strip is done by the built-in
//! TypeScript transform, which `experimental.runPluginFirst: true`
//! deliberately schedules AFTER our plugin (so we see TS-cast wrappers
//! the way Babel does — see `babel-plugin.rs:233` and
//! `fixtures/ct-ts-as-cast`). The side effect is that our plugin
//! sees the type-only specifier still attached. With `verbatimModuleSyntax: true`
//! SWC additionally refuses to elide the now-empty import shell at
//! codegen time, so the user-source `import { css, styled, jsx, type CssFunction }`
//! ends up as a side-effect-only `import "@compiled/react";` in the
//! output, while Babel's pipeline drops the import entirely.
//!
//! Where the fix lives
//! -------------------
//! Single one-shot pre-pass over the SWC `Program` immediately after
//! parse, before any plugin visitor (and before
//! `record_compiled_import` / `remove_empty_compiled_imports`) runs.
//! Mutating the AST in place produces the same shape Babel's
//! pipeline hands to `@compiled/babel-plugin`, so the existing
//! "strip then drop empty" flow lands the same way on both sides.
//!
//! Two mutations, both mirroring `transform-typescript`'s
//! `onlyRemoveTypeImports: true` behaviour:
//!
//! 1. **Whole-decl drop** — `import type { … } from '…';`
//!    (`decl.type_only == true`) → remove the entire `ModuleItem`.
//! 2. **Per-specifier drop** — keep the decl, retain only specifiers
//!    whose `is_type_only` flag is `false` (and `Default` /
//!    `Namespace`, which can't be type-only here).
//!
//! After this pass, the user input
//!
//! ```ts
//! import { css, styled, jsx, type CssFunction } from '@compiled/react';
//! ```
//!
//! becomes the same shape Babel's plugin sees:
//!
//! ```ts
//! import { css, styled, jsx } from '@compiled/react';
//! ```
//!
//! Per CLAUDE.md drift policy: this is not a behavioural fix to
//! upstream Babel — Babel's pipeline already strips these specifiers
//! before the plugin runs. We're aligning the SWC AST to the Babel AST
//! at the same point the plugin would observe it. Filed under
//! `compat/*` per the same precedent as `compat/template_literal_raw`,
//! `compat/paren`, etc.

use swc_core::ecma::ast::{ImportSpecifier, ModuleDecl, ModuleItem, Program};

/// Strip TypeScript `type` specifiers from every `ImportDeclaration`
/// in `program`. See module docs for the full rationale.
pub fn strip_type_only_import_specifiers(program: &mut Program) {
    let body = match program {
        Program::Module(m) => &mut m.body,
        Program::Script(_) => return,
    };

    body.retain_mut(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = item else {
            return true;
        };

        // (1) `import type { … } from '…';` — drop the whole decl.
        if decl.type_only {
            return false;
        }

        // (2) Otherwise drop only the type-only specifiers.
        decl.specifiers.retain(|spec| match spec {
            ImportSpecifier::Named(named) => !named.is_type_only,
            ImportSpecifier::Default(_) | ImportSpecifier::Namespace(_) => true,
        });

        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use swc_core::common::comments::SingleThreadedComments;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::EsVersion;
    use swc_core::ecma::parser::{parse_file_as_program, Syntax, TsSyntax};

    fn parse(src: &str) -> Program {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Arc::new(FileName::Real("input.tsx".into())),
            src.to_string(),
        );
        let comments = SingleThreadedComments::default();
        let mut errs = vec![];
        parse_file_as_program(
            &fm,
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            EsVersion::Es2022,
            Some(&comments),
            &mut errs,
        )
        .expect("parse ok")
    }

    fn body(p: &Program) -> &[ModuleItem] {
        match p {
            Program::Module(m) => &m.body,
            _ => panic!("expected module"),
        }
    }

    #[test]
    fn drops_type_only_specifier_keeps_value_specifiers() {
        let mut p = parse("import { css, styled, jsx, type CssFunction } from '@compiled/react';");
        strip_type_only_import_specifiers(&mut p);
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = &body(&p)[0] else {
            panic!()
        };
        let names: Vec<_> = decl
            .specifiers
            .iter()
            .filter_map(|s| match s {
                ImportSpecifier::Named(n) => Some(n.local.sym.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["css", "styled", "jsx"]);
    }

    #[test]
    fn drops_whole_import_type_decl() {
        let mut p = parse(
            "import type { CssFunction } from '@compiled/react';\nexport const x = 1;\n",
        );
        strip_type_only_import_specifiers(&mut p);
        // Only the `export const x = 1` ModuleItem remains; the
        // `import type` declaration is removed wholesale.
        assert_eq!(body(&p).len(), 1);
        assert!(matches!(
            &body(&p)[0],
            ModuleItem::Stmt(_) | ModuleItem::ModuleDecl(_)
        ));
        assert!(!matches!(
            &body(&p)[0],
            ModuleItem::ModuleDecl(ModuleDecl::Import(_))
        ));
    }

    #[test]
    fn leaves_value_only_imports_alone() {
        let mut p = parse("import { css, styled } from '@compiled/react';");
        strip_type_only_import_specifiers(&mut p);
        let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = &body(&p)[0] else {
            panic!()
        };
        assert_eq!(decl.specifiers.len(), 2);
    }

    #[test]
    fn leaves_default_and_namespace_imports_alone() {
        let mut p = parse(
            "import React from 'react';\nimport * as ns from 'mod';\nexport const x = 1;\n",
        );
        strip_type_only_import_specifiers(&mut p);
        // Both decls retained; specifiers untouched.
        assert_eq!(body(&p).len(), 3);
    }
}
