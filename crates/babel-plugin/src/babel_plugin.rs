//! 1:1 port of `packages/babel-plugin/src/babel-plugin.ts` — DISPATCHER.
//!
//! Phase 2 §2.3 status:
//!   * §2.3 skeleton (prior session): Compiled-import recognition into
//!     `state.compiled_imports`. Pass-through preserved.
//!   * §2.3(a) (this checkpoint): JSX-pragma recognition. Walks the
//!     classic-pragma `import { jsx }` site (recording
//!     `pragma.classic_jsx_pragma_is_compiled` /
//!     `classic_jsx_pragma_local_name`) and scans the canonical
//!     module-level leading-comment position for `@jsx` /
//!     `@jsxImportSource` pragmas (setting `pragma.jsx` /
//!     `pragma.jsx_import_source` and bootstrapping
//!     `state.compiled_imports = Some(default)`).
//!   * §2.3(a) is recognition-only — it WRITES TO `state` but does NOT
//!     mutate the AST or the comment store. The two AST mutations
//!     upstream performs in this region (`path.remove()` of the
//!     classic-pragma `jsx` specifier; `file.ast.comments` filter to
//!     hide the matched JSX-pragma comment from
//!     `@babel/plugin-transform-react-jsx`) are deferred to §2.3(b)
//!     once §2.4 ships the `MutationRecorder`. See the inline
//!     `// §2.3(b):` TODOs.
//!
//! Stubs that NEXT-SESSION (§2.3(b) / §2.4 / Phase 6) work fills in:
//!
//!   * `pre()` analog — global cache initialisation, `pragma` reset,
//!     `pathsToCleanup` reset. Today the visitor allocates fresh
//!     state per `process(...)` call which matches Babel's "per-file"
//!     pre() semantics. Cache wiring is Phase 5 §5.3.
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

use compiled_utils::jsx::{jsx_annotation_regex, jsx_source_annotation_regex};
use compiled_utils::DEFAULT_IMPORT_SOURCES;
use swc_core::common::comments::Comments;
use swc_core::common::Spanned;
use swc_core::ecma::ast::{
    ImportDecl, ImportSpecifier, ModuleDecl, ModuleExportName, ModuleItem, Program,
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

/// Read the imported name out of an `ImportSpecifier::Named`. Mirrors
/// Babel's `specifier.imported.name` (Identifier) /
/// `specifier.imported.value` (StringLiteral). When SWC's `imported`
/// is `None` (the `{ jsx }` shape, no rename), the imported name is
/// the local name — Babel's AST always populates the `imported`
/// field, so this branch is the SWC analog of Babel's "imported ===
/// local" identity case.
fn imported_name(spec: &swc_core::ecma::ast::ImportNamedSpecifier) -> String {
    match &spec.imported {
        Some(ModuleExportName::Ident(id)) => id.sym.as_ref().to_string(),
        Some(ModuleExportName::Str(s)) => s.value.to_atom_lossy().as_str().to_string(),
        None => spec.local.sym.as_ref().to_string(),
    }
}

/// `BabelPluginVisitor` — the top-level dispatcher.
///
/// Holds owned `State` (Babel's PluginPass analog). The `process(...)`
/// entry in `lib.rs` allocates this once per transform; SWC tears the
/// WASI instance down between transforms, so per-call state is the
/// only safe shape (PLAN.md cross-transform-caching constraint
/// re-confirmed in `plugins/STATUS.md`).
///
/// Generic over `C: Comments` so the SWC plugin entry can pass
/// `PluginCommentsProxy` (the host-channel proxy) and unit tests can
/// pass `SingleThreadedComments::default()` (an in-process empty
/// store). The generic is monomorphised, so there's no runtime cost.
pub struct BabelPluginVisitor<C: Comments> {
    pub state: State,
    /// Effective import-sources set (DEFAULT ∪ opts.importSources).
    /// Held alongside `state` because upstream stores it on `this`
    /// (the plugin instance), not on `state` — see lines 96–108.
    pub import_sources: Vec<String>,
    /// SWC stores comments in a side-channel keyed by `BytePos`; Babel
    /// stores them inline on `file.ast.comments`. The pragma scan in
    /// `Program::enter` reads through this proxy.
    ///
    /// Drift watch point: upstream walks the FLAT `file.ast.comments`
    /// list. The SWC analog requires an anchor — for module-level
    /// pragmas, that's the leading-comment position of the FIRST body
    /// item (the canonical attachment point for file-banner / pragma
    /// comments). This matches the routing pattern
    /// `babel-plugin-strip-runtime` already uses for banner comments
    /// (see `crates/babel-plugin-strip-runtime/src/lib.rs`'s banner
    /// span re-anchoring). Keeping ONE SWC-comment idiom across both
    /// plugins reduces maintainer cognitive load.
    pub comments: C,
    /// §2.3 stub log: every node the dispatcher would have handled
    /// gets a string here. The `lib.rs` `process(...)` entry can
    /// inspect this in tests to assert "the dispatcher saw what we
    /// expected" without requiring the AST mutations to land. NOT
    /// emitted in release builds — the production plugin is silent.
    #[cfg(debug_assertions)]
    pub stub_log: Vec<String>,
}

impl<C: Comments> BabelPluginVisitor<C> {
    pub fn new(opts: PluginOptions, comments: C) -> Self {
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
            comments,
            #[cfg(debug_assertions)]
            stub_log: Vec::new(),
        }
    }

    /// Recognise an `ImportDeclaration` and update `state.compiledImports`
    /// with each Compiled API's local name(s). Does NOT remove the
    /// specifier or the import — that's §2.4 / Phase 6 work. Output
    /// stays pass-through.
    fn record_compiled_import(&mut self, decl: &ImportDecl) {
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
            let name = imported_name(named);
            for api in COMPILED_API_NAMES {
                if &name == api {
                    let local = named.local.sym.as_ref().to_string();
                    push_api_local(imports, api, local);
                    break;
                }
            }
            // §2.3(b): MutationRecorder.queue_specifier_remove(specifier_id)
            // — upstream calls `specifier.remove()` here and, when the
            // specifier list empties, `path.remove()`. Deferred until
            // §2.4 ships the recorder.
        }
    }

    /// §2.3(a) — `findClassicJsxPragmaImport` analog. Walk the module
    /// body for `ImportDeclaration`s whose source is a Compiled origin
    /// and look for an `ImportSpecifier::Named` where the imported
    /// name (or local name when imported is None — the `{ jsx }`
    /// no-rename shape) is `"jsx"`. Records the local name and sets
    /// `pragma.classic_jsx_pragma_is_compiled = Some(true)`.
    ///
    /// Recognition only — does NOT call `path.remove()` on the
    /// specifier. The `// §2.3(b):` TODO inside marks where the
    /// MutationRecorder hook lands once §2.4 is in.
    ///
    /// Mirrors upstream `babel-plugin.ts` lines 43–66. Upstream uses
    /// `path.traverse(visitor, this)` over `ImportSpecifier`; we walk
    /// `module.body` directly because the dispatcher is a single-pass
    /// VisitMut (PLAN.md §3.5).
    fn scan_classic_jsx_pragma_import(&mut self, program: &Program) {
        let Program::Module(module) = program else {
            return;
        };
        for item in &module.body {
            let ModuleItem::ModuleDecl(ModuleDecl::Import(decl)) = item else {
                continue;
            };
            let userland_atom = decl.src.value.to_atom_lossy();
            let userland = userland_atom.as_str();
            if !is_compiled_module_source(userland, &self.import_sources) {
                continue;
            }
            for spec in &decl.specifiers {
                let ImportSpecifier::Named(named) = spec else { continue };
                if imported_name(named) != "jsx" {
                    continue;
                }
                // Upstream: state.pragma.classicJsxPragmaIsCompiled = true;
                //           state.pragma.classicJsxPragmaLocalName = specifier.local.name;
                self.state.pragma.classic_jsx_pragma_is_compiled = Some(true);
                self.state.pragma.classic_jsx_pragma_local_name =
                    Some(named.local.sym.as_ref().to_string());
                // §2.3(b): MutationRecorder.queue_specifier_remove(specifier_id)
                // — upstream's `path.remove()` hides the classic JSX
                // pragma from `@babel/plugin-transform-react-jsx` so it
                // doesn't re-emit `_jsx`-style calls. Deferred until
                // §2.4 ships the recorder. NOT load-bearing for any
                // §2.3 / §2.4 / §2.5 verification gate (the SWC
                // classic-runtime pipeline doesn't read this AST shape
                // until codegen, and no current fixture exercises that
                // path); load-bearing for §6.5 (css-prop) where the
                // pragma drives output divergence.
                return; // first match wins, matches upstream's early `return`
            }
        }
    }

    /// §2.3(a) — JSX-pragma comment scan. Walks the canonical
    /// module-level leading-comment position (the first body item's
    /// span.lo) and matches each comment's text against
    /// `JSX_SOURCE_ANNOTATION_REGEX` and `JSX_ANNOTATION_REGEX`.
    ///
    /// On `@jsxImportSource <origin>` where `<origin>` is a Compiled
    /// source: sets `pragma.jsx_import_source = Some(true)` and
    /// bootstraps `state.compiled_imports = Some(default)`.
    /// On `@jsx <name>` AND classic-pragma is recorded AND `<name>`
    /// matches the recorded local name: sets `pragma.jsx = Some(true)`
    /// and bootstraps `state.compiled_imports = Some(default)`.
    ///
    /// Mirrors upstream `babel-plugin.ts` lines 122–181. The comment
    /// store mutations upstream performs (filtering `file.ast.comments`
    /// and `body[0].leadingComments` to drop the matched pragma
    /// comment so `@babel/plugin-transform-react-jsx` ignores it) are
    /// DEFERRED to §2.3(b) — they pair naturally with the classic-
    /// pragma `path.remove()` and depend on the §2.4
    /// MutationRecorder's deferred-mutation discipline.
    ///
    /// Drift watch point: see `comments` field doc on this struct for
    /// why we anchor on `first_body_item.span.lo` rather than walking
    /// a flat comment list.
    fn scan_jsx_pragma_comments(&mut self, program: &Program) {
        let Program::Module(module) = program else {
            return;
        };
        let Some(first) = module.body.first() else {
            return;
        };
        let pos = first.span().lo;
        let Some(leading) = self.comments.get_leading(pos) else {
            return;
        };

        for comment in &leading {
            let text = comment.text.as_ref();

            // jsxSourceMatches: `@jsxImportSource <origin>`.
            if let Some(cap) = jsx_source_annotation_regex().captures(text) {
                if let Some(m) = cap.get(1) {
                    if self
                        .import_sources
                        .iter()
                        .any(|src| src.as_str() == m.as_str())
                    {
                        // Upstream: state.compiledImports = {};
                        //           state.pragma.jsxImportSource = true;
                        if self.state.compiled_imports.is_none() {
                            self.state.compiled_imports = Some(CompiledImports::default());
                        }
                        self.state.pragma.jsx_import_source = Some(true);
                        // §2.3(b): record `comment.span` for the
                        // MutationRecorder so the pragma comment is
                        // dropped from `body[0].leadingComments` (and
                        // upstream's `file.ast.comments` analog) at
                        // exit. SWC analog: `comments.take_leading(pos)`
                        // → filter → `add_leading_comments(pos, kept)`.
                    }
                }
            }

            // jsxMatches: `@jsx <name>`.
            if let Some(cap) = jsx_annotation_regex().captures(text) {
                let matches_classic = self
                    .state
                    .pragma
                    .classic_jsx_pragma_is_compiled
                    .unwrap_or(false)
                    && cap.get(1).map(|m| m.as_str())
                        == self.state.pragma.classic_jsx_pragma_local_name.as_deref();
                if matches_classic {
                    // Upstream: state.compiledImports = {};
                    //           state.pragma.jsx = true;
                    if self.state.compiled_imports.is_none() {
                        self.state.compiled_imports = Some(CompiledImports::default());
                    }
                    self.state.pragma.jsx = Some(true);
                    // §2.3(b): same comment-filter TODO as above.
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

impl<C: Comments> VisitMut for BabelPluginVisitor<C> {
    /// Dispatcher entry. Mirrors upstream `Program::enter` order:
    ///   1. `findClassicJsxPragmaImport` — recognition of classic
    ///      pragma's `import { jsx }` site (recognition only; the
    ///      `path.remove()` is §2.3(b) work).
    ///   2. JSX-pragma comment scan — `@jsx` / `@jsxImportSource`.
    ///   3. Children walk — `visit_mut_module_decl` handles
    ///      `ImportDeclaration` recognition (populates
    ///      `compiled_imports[apiName]`); other handlers stub out.
    /// Order matters: step 2 may set `state.compiled_imports = {}`
    /// (resetting the slot). Step 3's import recognition runs AFTER,
    /// so the bootstrapped empty struct gets populated rather than
    /// clobbering an existing one — exactly upstream's order.
    fn visit_mut_program(&mut self, program: &mut Program) {
        // §2.3(a) — recognition only, no AST mutations.
        self.scan_classic_jsx_pragma_import(program);
        self.scan_jsx_pragma_comments(program);

        // Children walk — ImportDeclaration / TaggedTemplateExpression /
        // CallExpression / JSXElement / JSXOpeningElement visitors fire here.
        program.visit_mut_children_with(self);

        // Upstream `Program::exit`: appendRuntimeImports, banner
        // comment, React/forwardRef injection, `pathsToCleanup`.
        // Deferred — needs §2.4 MutationRecorder to track what the
        // handlers want injected.
    }

    /// `ImportDeclaration` upstream visitor (lines 241–294).
    /// Recognition only — populates `state.compiled_imports[apiName]`
    /// and recurses into children. Specifier removal is §2.4 work.
    fn visit_mut_module_decl(&mut self, decl: &mut ModuleDecl) {
        if let ModuleDecl::Import(import) = decl {
            self.record_compiled_import(import);
        }
        decl.visit_mut_children_with(self);
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
    use swc_core::common::comments::SingleThreadedComments;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        Ident, ImportDecl, ImportNamedSpecifier, ImportPhase, Module, ModuleDecl, ModuleItem,
        Str,
    };

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

    fn named_specifier_str_imported(local: &str, imported_lit: &str) -> ImportSpecifier {
        ImportSpecifier::Named(ImportNamedSpecifier {
            span: DUMMY_SP,
            local: Ident::new(local.into(), DUMMY_SP, SyntaxContext::empty()),
            imported: Some(ModuleExportName::Str(Str {
                span: DUMMY_SP,
                value: imported_lit.into(),
                raw: None,
            })),
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

    fn fresh() -> BabelPluginVisitor<SingleThreadedComments> {
        BabelPluginVisitor::new(PluginOptions::default(), SingleThreadedComments::default())
    }

    #[test]
    fn resolve_import_sources_includes_defaults() {
        let opts = PluginOptions::default();
        let srcs = resolve_import_sources(&opts);
        assert!(srcs.iter().any(|s| s == "@compiled/react"));
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
        let srcs = vec!["@compiled/react".to_string(), "@atlaskit/css".to_string()];
        assert!(is_compiled_module_source("@compiled/react", &srcs));
        assert!(is_compiled_module_source("@atlaskit/css", &srcs));
        assert!(!is_compiled_module_source("react", &srcs));
        assert!(!is_compiled_module_source("@emotion/react", &srcs));
    }

    #[test]
    fn record_styled_import_populates_state() {
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
            vec![named_specifier("styled", None)],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports.expect("compiled_imports populated");
        let styled = imports.styled.expect("styled recorded");
        assert_eq!(styled, vec!["styled".to_string()]);
    }

    #[test]
    fn record_renamed_import_uses_local_name() {
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
            vec![named_specifier("MyCss", Some("css"))],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports.unwrap();
        assert_eq!(imports.css.unwrap(), vec!["MyCss".to_string()]);
    }

    #[test]
    fn record_multiple_apis_in_one_import() {
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
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
        let mut v = fresh();
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
                "@compiled/react",
                vec![named_specifier("styled", None), named_specifier("ClassNames", None)],
            ),
            import_decl("react", vec![named_specifier("useState", None)]),
        ]);
        let mut v = fresh();
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
    }

    // ───────── §2.3(a) — classic JSX pragma recognition ─────────

    #[test]
    fn classic_pragma_records_local_name_for_bare_jsx_import() {
        // `import { jsx } from '@compiled/react'` — bare specifier,
        // imported is None in SWC AST, local.sym = "jsx".
        let module = module_with_imports(vec![import_decl(
            "@compiled/react",
            vec![named_specifier("jsx", None)],
        )]);
        let mut v = fresh();
        v.scan_classic_jsx_pragma_import(&Program::Module(module));
        assert_eq!(v.state.pragma.classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma.classic_jsx_pragma_local_name.as_deref(),
            Some("jsx")
        );
    }

    #[test]
    fn classic_pragma_records_renamed_local() {
        // `import { jsx as myJsx } from '@compiled/react'` — imported
        // = Some(Ident("jsx")), local.sym = "myJsx".
        let module = module_with_imports(vec![import_decl(
            "@compiled/react",
            vec![named_specifier("myJsx", Some("jsx"))],
        )]);
        let mut v = fresh();
        v.scan_classic_jsx_pragma_import(&Program::Module(module));
        assert_eq!(v.state.pragma.classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma.classic_jsx_pragma_local_name.as_deref(),
            Some("myJsx")
        );
    }

    #[test]
    fn classic_pragma_handles_string_literal_imported() {
        // `import { 'jsx' as foo } from '@compiled/react'` — imported
        // = Some(Str("jsx")). Upstream parity: Babel matches both
        // Identifier and StringLiteral imported shapes.
        let module = module_with_imports(vec![import_decl(
            "@compiled/react",
            vec![named_specifier_str_imported("foo", "jsx")],
        )]);
        let mut v = fresh();
        v.scan_classic_jsx_pragma_import(&Program::Module(module));
        assert_eq!(v.state.pragma.classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma.classic_jsx_pragma_local_name.as_deref(),
            Some("foo")
        );
    }

    #[test]
    fn classic_pragma_skipped_for_non_compiled_source() {
        // `import { jsx } from '@emotion/react'` — emotion's jsx is
        // NOT a Compiled binding. Recognition must skip.
        let module = module_with_imports(vec![import_decl(
            "@emotion/react",
            vec![named_specifier("jsx", None)],
        )]);
        let mut v = fresh();
        v.scan_classic_jsx_pragma_import(&Program::Module(module));
        assert!(v.state.pragma.classic_jsx_pragma_is_compiled.is_none());
        assert!(v.state.pragma.classic_jsx_pragma_local_name.is_none());
    }

    #[test]
    fn classic_pragma_does_not_mutate_ast() {
        // §2.3(a) discipline: recognition must not call path.remove().
        // Verify the module body and specifier counts are unchanged
        // after the scan.
        let module = module_with_imports(vec![import_decl(
            "@compiled/react",
            vec![named_specifier("jsx", None), named_specifier("css", None)],
        )]);
        let mut v = fresh();
        let program = Program::Module(module);
        v.scan_classic_jsx_pragma_import(&program);
        if let Program::Module(m) = &program {
            assert_eq!(m.body.len(), 1);
            if let ModuleItem::ModuleDecl(ModuleDecl::Import(im)) = &m.body[0] {
                assert_eq!(im.specifiers.len(), 2);
            }
        }
    }

    // ───────── §2.3(a) — JSX pragma comment scan (helper-level) ─────────
    //
    // The full `scan_jsx_pragma_comments` path goes through the
    // `Comments` proxy and is exercised end-to-end by the parity
    // harness (matches strip-runtime's convention — visitor paths are
    // tested via fixtures, helper logic via unit tests). Here we test
    // the regex + state-mutation logic directly by constructing a
    // `SingleThreadedComments` store with synthetic comments.

    use swc_core::common::comments::{Comment, CommentKind};
    use swc_core::common::BytePos;

    fn comment(text: &str) -> Comment {
        Comment {
            kind: CommentKind::Block,
            span: DUMMY_SP,
            text: text.into(),
        }
    }

    fn module_with_first_body_at(pos: BytePos) -> Module {
        // A module whose first body item is anchored at `pos` so the
        // pragma scanner reads `comments.get_leading(pos)`.
        use swc_core::common::Span;
        let span = Span::new(pos, BytePos(pos.0 + 1));
        Module {
            span: DUMMY_SP,
            body: vec![ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                span,
                specifiers: Vec::new(),
                src: Box::new(Str {
                    span: DUMMY_SP,
                    value: "react".into(),
                    raw: None,
                }),
                type_only: false,
                with: None,
                phase: ImportPhase::Evaluation,
            }))],
            shebang: None,
        }
    }

    #[test]
    fn jsx_import_source_pragma_compiled_origin_sets_state() {
        // `/** @jsxImportSource @compiled/react */`
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsxImportSource @compiled/react "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert_eq!(v.state.pragma.jsx_import_source, Some(true));
        assert!(v.state.compiled_imports.is_some());
        assert!(v.state.pragma.jsx.is_none());
    }

    #[test]
    fn jsx_import_source_pragma_non_compiled_origin_ignored() {
        // `/** @jsxImportSource @emotion/react */` — must not enable.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsxImportSource @emotion/react "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert!(v.state.pragma.jsx_import_source.is_none());
        assert!(v.state.compiled_imports.is_none());
    }

    #[test]
    fn jsx_pragma_matching_classic_local_name_sets_state() {
        // `/** @jsx myJsx */` AND classic-pragma was registered with
        // local name "myJsx" → enables.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsx myJsx "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        v.state.pragma.classic_jsx_pragma_is_compiled = Some(true);
        v.state.pragma.classic_jsx_pragma_local_name = Some("myJsx".to_string());
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert_eq!(v.state.pragma.jsx, Some(true));
        assert!(v.state.compiled_imports.is_some());
    }

    #[test]
    fn jsx_pragma_mismatching_classic_local_name_ignored() {
        // `/** @jsx other */` but classic-pragma local is "myJsx" — no
        // match, so jsx pragma stays unset.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsx other "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        v.state.pragma.classic_jsx_pragma_is_compiled = Some(true);
        v.state.pragma.classic_jsx_pragma_local_name = Some("myJsx".to_string());
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert!(v.state.pragma.jsx.is_none());
    }

    #[test]
    fn jsx_pragma_without_classic_marker_ignored() {
        // `/** @jsx myJsx */` but no classic-pragma registered first —
        // upstream guards on `state.pragma.classicJsxPragmaIsCompiled`,
        // we mirror.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsx myJsx "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert!(v.state.pragma.jsx.is_none());
        assert!(v.state.compiled_imports.is_none());
    }

    #[test]
    fn pragma_scan_does_not_mutate_comment_store() {
        // §2.3(a) discipline: comment-store filtering is §2.3(b) work.
        // Verify the comment is still queryable after the scan.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsxImportSource @compiled/react "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        // The comment store still has the leading comment at `pos`.
        let still_there = v.comments.get_leading(pos);
        assert!(still_there.is_some());
        assert_eq!(still_there.unwrap().len(), 1);
    }

    #[test]
    fn end_to_end_pragma_then_import_records_both() {
        // Full visit_mut_program: classic-pragma `import { jsx }` +
        // `@jsx jsx` comment → state.pragma.{classic..., jsx}=Some(true)
        // AND state.compiled_imports.styled is populated by the
        // ImportDeclaration visitor walk.
        let comments = SingleThreadedComments::default();
        let pos = BytePos(100);
        comments.add_leading(pos, comment("* @jsx jsx "));
        let mut v: BabelPluginVisitor<SingleThreadedComments> =
            BabelPluginVisitor::new(PluginOptions::default(), comments);

        use swc_core::common::Span;
        let span = Span::new(pos, BytePos(pos.0 + 1));
        let module = Module {
            span: DUMMY_SP,
            body: vec![ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                span,
                specifiers: vec![
                    named_specifier("jsx", None),
                    named_specifier("styled", None),
                ],
                src: Box::new(Str {
                    span: DUMMY_SP,
                    value: "@compiled/react".into(),
                    raw: None,
                }),
                type_only: false,
                with: None,
                phase: ImportPhase::Evaluation,
            }))],
            shebang: None,
        };

        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        // Classic pragma recognised.
        assert_eq!(v.state.pragma.classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma.classic_jsx_pragma_local_name.as_deref(),
            Some("jsx")
        );
        // @jsx pragma comment matched the recorded local name.
        assert_eq!(v.state.pragma.jsx, Some(true));
        // ImportDeclaration walk populated styled (jsx is not a recognised API name).
        let imports = v.state.compiled_imports.expect("imports");
        assert_eq!(imports.styled.unwrap(), vec!["styled".to_string()]);
        assert!(imports.css.is_none());
    }
}
