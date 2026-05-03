//! 1:1 port of `packages/babel-plugin/src/babel-plugin.ts` — DISPATCHER.
//!
//! Phase 2 §2.3 status: SKELETON ONLY. The dispatcher walks the
//! module, recognises Compiled imports, and populates
//! `state.compiledImports` — but it does NOT mutate the AST. Pass-
//! through output is preserved so the §2.3 verification gate
//! ("byte-equal output through the prettier oracle for every fixture,
//! no handler logic yet") holds trivially.
//!
//! Stubs that NEXT-SESSION work fills in:
//!
//!   * `pre()` analog — global cache initialisation, `pragma` reset,
//!     `pathsToCleanup` reset. Today the visitor allocates fresh
//!     state per `process(...)` call which matches Babel's "per-file"
//!     pre() semantics. Cache wiring is Phase 5 §5.3.
//!
//!   * `Program::enter` JSX-pragma scan (`findClassicJsxPragmaImport`,
//!     `JSX_ANNOTATION_REGEX`, `JSX_SOURCE_ANNOTATION_REGEX`). The
//!     regexes are ported in `sjcompiled-utils::jsx`; the
//!     classic-pragma `path.remove()` mutation is gated until §6.5
//!     (css-prop, the only consumer of `pragma.classicJsxPragmaIsCompiled`).
//!
//!   * `Program::exit` `appendRuntimeImports` + banner comment +
//!     `pathsToCleanup.forEach(...)`. Mutating exit lands with the
//!     first real handler in Phase 6.
//!
//!   * `ImportDeclaration` specifier removal (`specifier.remove()`,
//!     `path.remove()` when the source is fully drained). Mutation
//!     lands with §2.4 (state-encapsulation pre-req — the
//!     MutationRecorder is what owns deferred specifier deletes).
//!
//!   * `'TaggedTemplateExpression|CallExpression'` and `JSXElement`/
//!     `JSXOpeningElement` handlers — stubbed as no-ops here. The
//!     `is_compiled.rs` predicates land per Phase 6 sub-checkpoint
//!     (one per API: keyframes, css, cssMap, xcss-prop, css-prop,
//!     ClassNames, styled).
//!
//! Drift discipline: every divergence from upstream `babel-plugin.ts`
//! is documented inline. When upstream changes, this file MUST be
//! re-audited line-for-line — the 370-LOC source is checked in at
//! the pinned commits noted in `CLAUDE.md`.

use sjcompiled_utils::DEFAULT_IMPORT_SOURCES;
use swc_core::ecma::ast::{
    ImportSpecifier, ModuleDecl, ModuleExportName, ModuleItem, Program,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::types::{CompiledImports, PluginOptions, State};

/// The five Compiled APIs the dispatcher recognises by import name.
/// Mirrors upstream lines 275: `(['styled', 'ClassNames', 'css',
/// 'keyframes', 'cssMap'] as const)`. Order matters only for
/// readability — visitor logic iterates over each separately.
const COMPILED_API_NAMES: &[&str] = &["styled", "ClassNames", "css", "keyframes", "cssMap"];

/// Resolve the effective import-sources set: `DEFAULT_IMPORT_SOURCES`
/// ∪ user `opts.import_sources`. Mirrors upstream `pre()`'s
/// `this.importSources = [...DEFAULT_IMPORT_SOURCES, ...opts.importSources]`.
///
/// Relative-path resolution from upstream (`origin[0] === '.'` →
/// `join(rootPath, origin)`) is deferred to §5.4 — `rootPath` comes
/// from `state.opts.root ?? this.cwd`, and the cwd-anchored resolver
/// is Phase 5's `oxc_resolver` config. For §2.3 we just ingest user
/// values verbatim and let Phase 5's resolver handle path resolution.
pub fn resolve_import_sources(opts: &PluginOptions) -> Vec<String> {
    let mut out: Vec<String> = DEFAULT_IMPORT_SOURCES.iter().map(|s| s.to_string()).collect();
    if let Some(extra) = &opts.import_sources {
        for src in extra {
            out.push(src.clone());
        }
    }
    out
}

/// Match `userland_module_specifier` against `import_sources`.
/// Mirrors upstream lines 242–259 — exact match wins; fallback is a
/// relative-path comparison that resolves the user's `./foo` against
/// the file's dirname and compares against each compiled origin.
///
/// §2.3 implements only the EXACT-match path; the relative-path
/// comparison needs the file's dirname and the resolver, both of
/// which arrive in Phase 5. Documented here so the §5.4 wiring is a
/// localised diff.
pub fn is_compiled_module_source(userland: &str, import_sources: &[String]) -> bool {
    import_sources.iter().any(|src| src == userland)
}

/// `BabelPluginVisitor` — the top-level dispatcher.
///
/// Holds owned `State` (Babel's PluginPass analog). The `process(...)`
/// entry in `lib.rs` allocates this once per transform; SWC tears the
/// WASI instance down between transforms, so per-call state is the
/// only safe shape (PLAN.md cross-transform-caching constraint
/// re-confirmed in `plugins/STATUS.md`).
pub struct BabelPluginVisitor {
    pub state: State,
    /// Effective import-sources set (DEFAULT ∪ opts.importSources).
    /// Held alongside `state` because upstream stores it on `this`
    /// (the plugin instance), not on `state` — see lines 96–108.
    pub import_sources: Vec<String>,
    /// §2.3 stub log: every node the dispatcher would have handled
    /// gets a string here. The `lib.rs` `process(...)` entry can
    /// inspect this in tests to assert "the dispatcher saw what we
    /// expected" without requiring the AST mutations to land. NOT
    /// emitted in release builds — the production plugin is silent.
    #[cfg(debug_assertions)]
    pub stub_log: Vec<String>,
}

impl BabelPluginVisitor {
    pub fn new(opts: PluginOptions) -> Self {
        let import_sources = resolve_import_sources(&opts);
        let mut state = State::default();
        state.opts = opts;
        state.import_sources = import_sources.clone();
        // Mirror upstream `pre()` initialisation: `sheets`, `cssMap`,
        // `ignoreMemberExpressions`, `includedFiles`, `pathsToCleanup`,
        // `pragma`, `usesXcss=false`. The Default impl of `State`
        // already zeroes these; documenting here so a future audit
        // sees the parity intent.
        Self {
            state,
            import_sources,
            #[cfg(debug_assertions)]
            stub_log: Vec::new(),
        }
    }

    /// Recognise an `ImportDeclaration` and update `state.compiledImports`
    /// with each Compiled API's local name(s). Does NOT remove the
    /// specifier or the import — that's §2.4 / Phase 6 work. Output
    /// stays pass-through.
    fn record_compiled_import(&mut self, decl: &swc_core::ecma::ast::ImportDecl) {
        let userland_atom = decl.src.value.to_atom_lossy();
        let userland = userland_atom.as_str();
        if !is_compiled_module_source(userland, &self.import_sources) {
            return;
        }

        // Upstream: `state.compiledImports = state.compiledImports || {}`.
        // Empty struct means "Compiled module imported, but no API
        // names yet recorded" — used by the css-prop visitor (Phase 6
        // §6.5) as a "should we even look at css={...}" gate.
        if self.state.compiled_imports.is_none() {
            self.state.compiled_imports = Some(CompiledImports::default());
        }

        let imports = self.state.compiled_imports.as_mut().unwrap();
        for spec in &decl.specifiers {
            let ImportSpecifier::Named(named) = spec else { continue };
            // `imported` defaults to the local name when absent
            // (`import { foo }` vs `import { foo as bar }`).
            let imported_name = match &named.imported {
                Some(ModuleExportName::Ident(id)) => id.sym.as_ref().to_string(),
                Some(ModuleExportName::Str(s)) => s.value.to_atom_lossy().as_str().to_string(),
                None => named.local.sym.as_ref().to_string(),
            };
            for api in COMPILED_API_NAMES {
                if &imported_name == api {
                    let local = named.local.sym.as_ref().to_string();
                    push_api_local(imports, api, local);
                    break;
                }
            }
        }
    }
}

/// Tag-dispatch helper: append `local` to the right `CompiledImports`
/// field based on `api_name`. Inlined in upstream's `forEach` over
/// the API tuple; here we keep a single match so adding an API in
/// future is a localised diff.
fn push_api_local(imports: &mut CompiledImports, api_name: &str, local: String) {
    let slot = match api_name {
        "styled" => &mut imports.styled,
        "ClassNames" => &mut imports.class_names,
        "css" => &mut imports.css,
        "keyframes" => &mut imports.keyframes,
        "cssMap" => &mut imports.css_map,
        _ => return,
    };
    slot.get_or_insert_with(Vec::new).push(local);
}

impl VisitMut for BabelPluginVisitor {
    /// Dispatcher entry. The Rust port walks once (PLAN.md §3.5 —
    /// single-pass, no scan/apply); upstream Babel uses `pre()` +
    /// individual visitor entries. We collapse the ImportDeclaration
    /// recogniser here and recurse into children so nested handlers
    /// (Phase 6) see the populated state.
    fn visit_mut_program(&mut self, program: &mut Program) {
        // Upstream `Program::enter` JSX-pragma logic is deferred to
        // a follow-up §2.3 chunk (next session). The pragma scan
        // walks `file.ast.comments` for `@jsx`/`@jsxImportSource`
        // matches and primes `state.pragma`. Today, css-prop
        // / xcss-prop handlers don't exist yet, so the missing
        // pragma state has no observable effect.

        // First pass: ImportDeclaration recognition. Mirrors upstream
        // line 241's `ImportDeclaration` visitor, but RECOGNITION-ONLY
        // — no `specifier.remove()` / `path.remove()`.
        if let Program::Module(module) = program {
            for item in &module.body {
                if let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item {
                    self.record_compiled_import(import);
                }
            }
        }

        // Second pass: recurse into children. Stub handlers below
        // record what they would have handled but mutate nothing.
        program.visit_mut_children_with(self);

        // Upstream `Program::exit`: appendRuntimeImports, banner
        // comment, React/forwardRef injection, `pathsToCleanup`.
        // Deferred — needs §2.4 MutationRecorder to track what the
        // handlers want injected.
    }

    /// `'TaggedTemplateExpression|CallExpression'` upstream. The
    /// upstream visitor:
    ///   1. throws if `isTransformedJsxFunction(...)` — Phase 2
    ///      §2.3 stub: skip.
    ///   2. dispatches to css-map / styled / css/keyframes utility
    ///      branches. Stubbed.
    fn visit_mut_call_expr(&mut self, n: &mut swc_core::ecma::ast::CallExpr) {
        #[cfg(debug_assertions)]
        if self.state.compiled_imports.is_some() {
            self.stub_log.push("call_expr_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_tagged_tpl(&mut self, n: &mut swc_core::ecma::ast::TaggedTpl) {
        #[cfg(debug_assertions)]
        if self.state.compiled_imports.is_some() {
            self.stub_log.push("tagged_tpl_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_jsx_element(&mut self, n: &mut swc_core::ecma::ast::JSXElement) {
        #[cfg(debug_assertions)]
        if self
            .state
            .compiled_imports
            .as_ref()
            .and_then(|i| i.class_names.as_ref())
            .is_some()
        {
            self.stub_log.push("jsx_element_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_jsx_opening_element(&mut self, n: &mut swc_core::ecma::ast::JSXOpeningElement) {
        // Upstream: `processXcss = state.opts.processXcss ?? true`.
        // Stub: only log when the gate would have fired so the test
        // log isn't flooded.
        let _process_xcss = self.state.opts.process_xcss.unwrap_or(true);
        #[cfg(debug_assertions)]
        if self.state.compiled_imports.is_some() {
            self.stub_log.push("jsx_opening_element_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SourceMap, SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        Ident, ImportDecl, ImportNamedSpecifier, ImportPhase, Module, ModuleDecl, ModuleItem,
        Str,
    };
    use std::sync::Arc;

    fn named_specifier(local: &str, imported: Option<&str>) -> ImportSpecifier {
        ImportSpecifier::Named(ImportNamedSpecifier {
            span: DUMMY_SP,
            local: Ident::new(local.into(), DUMMY_SP, SyntaxContext::empty()),
            imported: imported.map(|n| {
                ModuleExportName::Ident(Ident::new(n.into(), DUMMY_SP, SyntaxContext::empty()))
            }),
            is_type_only: false,
        })
    }

    fn import_decl(source: &str, specs: Vec<ImportSpecifier>) -> ImportDecl {
        ImportDecl {
            span: DUMMY_SP,
            specifiers: specs,
            src: Box::new(Str {
                span: DUMMY_SP,
                value: source.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: ImportPhase::Evaluation,
        }
    }

    fn module_with_imports(imports: Vec<ImportDecl>) -> Module {
        Module {
            span: DUMMY_SP,
            body: imports
                .into_iter()
                .map(|d| ModuleItem::ModuleDecl(ModuleDecl::Import(d)))
                .collect(),
            shebang: None,
        }
    }

    #[test]
    fn resolve_import_sources_includes_defaults() {
        let opts = PluginOptions::default();
        let srcs = resolve_import_sources(&opts);
        assert!(srcs.iter().any(|s| s == "@sjcompiled/react"));
        assert!(srcs.iter().any(|s| s == "@atlaskit/css"));
    }

    #[test]
    fn resolve_import_sources_appends_user_extras() {
        let opts = PluginOptions {
            import_sources: Some(vec!["my-design-system".to_string()]),
            ..Default::default()
        };
        let srcs = resolve_import_sources(&opts);
        assert!(srcs.iter().any(|s| s == "my-design-system"));
    }

    #[test]
    fn is_compiled_module_source_exact_match() {
        let srcs = vec!["@sjcompiled/react".to_string(), "@atlaskit/css".to_string()];
        assert!(is_compiled_module_source("@sjcompiled/react", &srcs));
        assert!(is_compiled_module_source("@atlaskit/css", &srcs));
        assert!(!is_compiled_module_source("react", &srcs));
        assert!(!is_compiled_module_source("@emotion/react", &srcs));
    }

    #[test]
    fn record_styled_import_populates_state() {
        let mut v = BabelPluginVisitor::new(PluginOptions::default());
        let decl = import_decl(
            "@sjcompiled/react",
            vec![named_specifier("styled", None)],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports.expect("compiled_imports populated");
        let styled = imports.styled.expect("styled recorded");
        assert_eq!(styled, vec!["styled".to_string()]);
    }

    #[test]
    fn record_renamed_import_uses_local_name() {
        let mut v = BabelPluginVisitor::new(PluginOptions::default());
        let decl = import_decl(
            "@sjcompiled/react",
            vec![named_specifier("MyCss", Some("css"))],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports.unwrap();
        assert_eq!(imports.css.unwrap(), vec!["MyCss".to_string()]);
    }

    #[test]
    fn record_multiple_apis_in_one_import() {
        let mut v = BabelPluginVisitor::new(PluginOptions::default());
        let decl = import_decl(
            "@sjcompiled/react",
            vec![
                named_specifier("styled", None),
                named_specifier("css", None),
                named_specifier("keyframes", None),
            ],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports.unwrap();
        assert_eq!(imports.styled.unwrap(), vec!["styled".to_string()]);
        assert_eq!(imports.css.unwrap(), vec!["css".to_string()]);
        assert_eq!(imports.keyframes.unwrap(), vec!["keyframes".to_string()]);
    }

    #[test]
    fn record_ignores_non_compiled_source() {
        let mut v = BabelPluginVisitor::new(PluginOptions::default());
        let decl = import_decl(
            "@emotion/react",
            vec![named_specifier("css", None)],
        );
        v.record_compiled_import(&decl);
        // `state.compiled_imports` stays None — the visitor never
        // recognised the source as Compiled.
        assert!(v.state.compiled_imports.is_none());
    }

    #[test]
    fn record_compiled_imports_through_visit_mut_program() {
        // End-to-end: a module with a Compiled import + an
        // unrelated import. After `visit_mut_program`, only the
        // Compiled import is in state.
        let module = module_with_imports(vec![
            import_decl(
                "@sjcompiled/react",
                vec![named_specifier("styled", None), named_specifier("ClassNames", None)],
            ),
            import_decl("react", vec![named_specifier("useState", None)]),
        ]);
        let mut v = BabelPluginVisitor::new(PluginOptions::default());
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);
        let imports = v.state.compiled_imports.expect("compiled");
        assert_eq!(imports.styled.unwrap(), vec!["styled".to_string()]);
        assert_eq!(imports.class_names.unwrap(), vec!["ClassNames".to_string()]);
        // Verify NO mutation: original module body length preserved.
        if let Program::Module(m) = &program {
            assert_eq!(m.body.len(), 2);
            // Specifier counts unchanged — confirms we didn't drop
            // any import.
            if let ModuleItem::ModuleDecl(ModuleDecl::Import(im)) = &m.body[0] {
                assert_eq!(im.specifiers.len(), 2);
            }
        }
        // SourceMap is unused in this test but kept as a touchstone
        // for future tests that need to feed the visitor a parsed
        // program.
        let _ = Arc::new(SourceMap::default());
    }
}
