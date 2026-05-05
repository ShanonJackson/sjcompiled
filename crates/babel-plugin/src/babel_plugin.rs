//! 1:1 port of `packages/babel-plugin/src/babel-plugin.ts` — DISPATCHER.
//!
//! Phase 2 §2.3 / §2.4 status:
//!   * §2.3 skeleton (prior session): Compiled-import recognition into
//!     `state.compiled_imports`. Pass-through preserved.
//!   * §2.3(a): JSX-pragma recognition. Walks the classic-pragma
//!     `import { jsx }` site and the canonical module-level
//!     leading-comment position. Recognition only — two §2.3(b) AST
//!     mutations marked with TODOs.
//!   * §2.4 (this checkpoint): all evaluation-visible state
//!     mutations route through `MutationRecorder::apply` per PLAN.md
//!     §3.9.8. The visitor holds `state: State` and `recorder:
//!     MutationRecorder` as separate fields; init-time non-captured
//!     writes go through `State`'s `set_*` / `ensure_*` /
//!     `set_classic_jsx_pragma` methods. The lint
//!     `grep state\.[a-z_]+\.{push,set,add,insert,remove,extend}`
//!     stays clean outside `state.rs` / `mutation_recorder.rs`.
//!
//! Stubs that NEXT-SESSION (§2.3(b) / Phase 6) work fills in:
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
//!     `path.remove()` when the source is fully drained). The
//!     queue lands via `state.queue_cleanup(action)` once §2.3(b)
//!     wires the deferred AST-mutation path.
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
    Expr, ImportDecl, ImportSpecifier, ModuleDecl, ModuleExportName, ModuleItem, Program,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::css;
use crate::keyframes;
use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
use crate::state::State;
use crate::types::PluginOptions;

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
/// Holds owned `State` (Babel's PluginPass analog) and a
/// `MutationRecorder` (PLAN.md §3.9.8) as SEPARATE fields so the
/// borrow checker can split-borrow `&mut self.recorder` and
/// `&mut self.state` simultaneously. The `process(...)` entry in
/// `lib.rs` allocates this once per transform; SWC tears the WASI
/// instance down between transforms, so per-call state is the
/// only safe shape (PLAN.md cross-transform-caching constraint
/// re-confirmed in `plugins/STATUS.md`).
///
/// Generic over `C: Comments` so the SWC plugin entry can pass
/// `PluginCommentsProxy` (the host-channel proxy) and unit tests can
/// pass `SingleThreadedComments::default()` (an in-process empty
/// store). The generic is monomorphised, so there's no runtime cost.
pub struct BabelPluginVisitor<C: Comments> {
    pub state: State,
    /// Per-evaluation diff capture. `apply(diff, &mut state)` is the
    /// SOLE channel for cache-replay-relevant state writes (PLAN.md
    /// §3.9.8). Phase 5 §5.3 will drain `recorder.diff_log()` into
    /// `Layer2Entry::state_diffs` at evaluation completion.
    pub recorder: MutationRecorder,
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
    /// §4.6 bridge: scope index built lazily at `visit_mut_program`
    /// entry from the Module AST. `None` until `Program::enter`
    /// fires (or when the program is a Script — Compiled doesn't
    /// operate on classic scripts in practice). Phase 6 handler
    /// dispatch sites pass `&mut self.scope_index` into
    /// `evaluate_expression` / `resolve_binding`.
    pub scope_index: Option<ScopeIndex>,
    /// §4.6 bridge: cached `program_scope()` snapshot taken at the
    /// same point `scope_index` is built. Avoids an extra method
    /// dispatch on the hot path.
    pub program_scope: Option<ScopeId>,
}

impl<C: Comments> BabelPluginVisitor<C> {
    pub fn new(opts: PluginOptions, comments: C) -> Self {
        let import_sources = resolve_import_sources(&opts);
        let mut state = State::default();
        // Init-time mutators — non-captured per STATE_MUTATIONS.md
        // (these set per-file scaffolding written exactly once, not
        // cross-file caching contract).
        state.set_opts(opts);
        state.set_import_sources(import_sources.clone());
        // Mirror upstream `pre()` initialisation: `sheets`, `cssMap`,
        // `ignoreMemberExpressions`, `includedFiles`, `pathsToCleanup`,
        // `pragma`, `usesXcss=false`. The Default impl of `State`
        // already zeroes these; documenting here so a future audit
        // sees the parity intent.
        Self {
            state,
            recorder: MutationRecorder::new(),
            import_sources,
            comments,
            #[cfg(debug_assertions)]
            stub_log: Vec::new(),
            scope_index: None,
            program_scope: None,
        }
    }

    /// Recognise an `ImportDeclaration` and update `state.compiledImports`
    /// with each Compiled API's local name(s). Does NOT remove the
    /// specifier or the import — that's §2.3(b) work via
    /// `state.queue_cleanup(...)`. Output stays pass-through.
    ///
    /// API-name pushes route through `MutationRecorder::apply` with
    /// `StateDiff::CompiledImportsAppend` (STATE_MUTATIONS.md site 4 —
    /// the FIRST evaluation-visible mutation, captured by the cache).
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
        // STATE_MUTATIONS.md site 3 (init, not captured).
        self.state.ensure_compiled_imports();

        for spec in &decl.specifiers {
            let ImportSpecifier::Named(named) = spec else { continue };
            // `imported` defaults to the local name when absent
            // (`import { foo }` vs `import { foo as bar }`).
            let name = imported_name(named);
            if let Some(api) = ApiKind::from_imported_name(&name) {
                let local = named.local.sym.as_ref().to_string();
                self.recorder.apply(
                    StateDiff::CompiledImportsAppend {
                        api,
                        local_name: local,
                    },
                    &mut self.state,
                );
            }
            // §2.3(b): state.queue_cleanup(CleanupAction { action:
            // Remove, id: <recorder-issued-handle> }) — upstream calls
            // `specifier.remove()` here and, when the specifier list
            // empties, `path.remove()`. Deferred until §2.3(b) wires
            // the AST-handle table.
        }
    }

    /// §2.3(a) — `findClassicJsxPragmaImport` analog. Walk the module
    /// body for `ImportDeclaration`s whose source is a Compiled origin
    /// and look for an `ImportSpecifier::Named` where the imported
    /// name (or local name when imported is None — the `{ jsx }`
    /// no-rename shape) is `"jsx"`. Records the local name and sets
    /// `pragma.classic_jsx_pragma_is_compiled = Some(true)` via
    /// `state.set_classic_jsx_pragma(local)`.
    ///
    /// Recognition only — does NOT call `path.remove()` on the
    /// specifier. The `// §2.3(b):` TODO inside marks where the
    /// queue_cleanup call lands.
    ///
    /// Mirrors upstream `babel-plugin.ts` lines 43–66.
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
                // STATE_MUTATIONS.md classifies `pragma.*` writes as
                // out-of-capture (per-file scaffolding). Use the
                // init-time mutator on `State`.
                self.state
                    .set_classic_jsx_pragma(named.local.sym.as_ref().to_string());
                // §2.3(b): state.queue_cleanup(CleanupAction { action:
                // Remove, id: <recorder-issued-handle> }) — upstream's
                // `path.remove()` hides the classic JSX pragma from
                // `@babel/plugin-transform-react-jsx` so it doesn't
                // re-emit `_jsx`-style calls. NOT load-bearing for any
                // §2.3 / §2.4 / §2.5 verification gate; load-bearing
                // for §6.5 (css-prop) where the pragma drives output
                // divergence.
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
    /// All writes go through `state.set_*` / `state.ensure_*` methods
    /// (§2.4 encapsulation contract).
    ///
    /// Mirrors upstream `babel-plugin.ts` lines 122–181. The comment
    /// store mutations upstream performs (filtering `file.ast.comments`
    /// and `body[0].leadingComments` to drop the matched pragma
    /// comment so `@babel/plugin-transform-react-jsx` ignores it) are
    /// DEFERRED to §2.3(b).
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
                        // STATE_MUTATIONS.md sites 1 + pragma write —
                        // both classified as init / non-captured.
                        self.state.ensure_compiled_imports();
                        self.state.set_pragma_jsx_import_source();
                        // §2.3(b): record `comment.span` for the AST-
                        // mutation queue so the pragma comment is
                        // dropped from the comment store at exit.
                    }
                }
            }

            // jsxMatches: `@jsx <name>`.
            if let Some(cap) = jsx_annotation_regex().captures(text) {
                let matches_classic = self
                    .state
                    .pragma()
                    .classic_jsx_pragma_is_compiled
                    .unwrap_or(false)
                    && cap.get(1).map(|m| m.as_str())
                        == self.state.pragma().classic_jsx_pragma_local_name.as_deref();
                if matches_classic {
                    self.state.ensure_compiled_imports();
                    self.state.set_pragma_jsx();
                    // §2.3(b): same comment-filter TODO as above.
                }
            }
        }
    }
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
    /// Order matters: step 2 may bootstrap `state.compiled_imports`
    /// (via `ensure_compiled_imports()`). Step 3's import recognition
    /// runs AFTER, so the bootstrapped struct gets populated rather
    /// than clobbering — exactly upstream's order.
    fn visit_mut_program(&mut self, program: &mut Program) {
        // §2.3(a) — recognition only, no AST mutations.
        self.scan_classic_jsx_pragma_import(program);
        self.scan_jsx_pragma_comments(program);

        // §4.6 bridge: build the scope index over the Module AST
        // before the children walk. Phase 6 handler dispatch sites
        // read `self.scope_index` / `self.program_scope` to feed
        // `evaluate_expression` / `resolve_binding`. Script programs
        // are not produced by Compiled call sites in practice
        // (consumer monorepo is ESM/JSX); the field stays `None`.
        if let Program::Module(module) = &*program {
            let idx = ScopeIndex::build(module);
            self.program_scope = Some(idx.program_scope());
            self.scope_index = Some(idx);
        }

        // Children walk — ImportDeclaration / TaggedTemplateExpression /
        // CallExpression / JSXElement / JSXOpeningElement visitors fire here.
        program.visit_mut_children_with(self);

        // Phase 6 §6.1 — `pathsToCleanup` drain (keyframes half).
        // Mirrors upstream `babel-plugin.ts:222-238`. Today only the
        // keyframes branch (§6.1) populates `Replace` entries; §6.2
        // (`css` cleanup) and §6.3 (`cssMap`) reuse the same drain.
        // The remaining upstream `Program::exit` work
        // (`appendRuntimeImports`, banner comment, React/forwardRef
        // injection) is Phase 7 territory — this drain is the
        // smallest deferred-AST-mutation slice.
        let replace_ids = keyframes::paths_to_cleanup_replace_ids(&self.state);
        if !replace_ids.is_empty() {
            if let Program::Module(m) = program {
                keyframes::run_cleanup_replace(m, &replace_ids);
            }
        }
    }

    /// `ImportDeclaration` upstream visitor (lines 241–294).
    /// Recognition only — populates `state.compiled_imports[apiName]`
    /// via `MutationRecorder::apply` and recurses into children.
    /// Specifier removal is §2.3(b) work.
    fn visit_mut_module_decl(&mut self, decl: &mut ModuleDecl) {
        if let ModuleDecl::Import(import) = decl {
            self.record_compiled_import(import);
        }
        decl.visit_mut_children_with(self);
    }

    /// Post-order Expr hook. `visit_mut_call_expr` /
    /// `visit_mut_tagged_tpl` see `&mut CallExpr` / `&mut TaggedTpl`
    /// — they cannot replace themselves with a different `Expr`
    /// variant. The dispatch sites whose action is "replace this
    /// node with a different Expr kind" (Phase 6 §6.1's keyframes
    /// cleanup → `null`, §6.2's css cleanup → `null`) need the
    /// enclosing `Expr` reference. This override is the
    /// post-order detection hook for those.
    ///
    /// Children walk runs FIRST (so nested matches are recognised)
    /// and existing `visit_mut_call_expr` / `visit_mut_tagged_tpl`
    /// stubs still fire as part of the descent. After the descent
    /// returns we run the per-API matchers in upstream's dispatch
    /// order:
    ///   * §6.1 keyframes — queue `Replace` entry (this checkpoint).
    ///   * §6.2 css cleanup — queue `Replace` entry (next).
    ///   * §6.3 cssMap, §6.7 styled — early-return paths that
    ///     mutate the Expr directly (their work happens in their
    ///     own modules; this hook is the call site).
    ///
    /// The actual `null` substitution does NOT happen here — it's
    /// deferred to `Program::exit`'s drain pass via
    /// `keyframes::run_cleanup_replace`. Mirrors upstream
    /// `pathsToCleanup` semantics.
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);

        // §6.1 — keyframes cleanup-only. Returns `true` if the node
        // matched and was queued; the caller short-circuits further
        // dispatch on the same node (matches upstream's `return`
        // after `pathsToCleanup.push`).
        if keyframes::try_queue_cleanup(n, &mut self.state) {
            return;
        }

        // §6.2 — css cleanup-only. Mirrors the css half of upstream's
        // `isCompiledUtil` short-circuit (lines 331–340). Queues into
        // the SAME `paths_to_cleanup` channel keyframes uses; the
        // shared drain at `Program::exit` swaps both for `null`. The
        // matchers are mutually exclusive on a given node, so order
        // between §6.1 and §6.2 here is not observable.
        if css::try_queue_cleanup(n, &mut self.state) {
            return;
        }
    }

    /// `'TaggedTemplateExpression|CallExpression'` upstream. The
    /// upstream visitor:
    ///   1. throws if `isTransformedJsxFunction(...)` — Phase 2
    ///      §2.3 stub: skip.
    ///   2. dispatches to css-map / styled / css/keyframes utility
    ///      branches. Stubbed.
    fn visit_mut_call_expr(&mut self, n: &mut swc_core::ecma::ast::CallExpr) {
        #[cfg(debug_assertions)]
        if self.state.compiled_imports().is_some() {
            self.stub_log.push("call_expr_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_tagged_tpl(&mut self, n: &mut swc_core::ecma::ast::TaggedTpl) {
        #[cfg(debug_assertions)]
        if self.state.compiled_imports().is_some() {
            self.stub_log.push("tagged_tpl_visited".to_string());
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_jsx_element(&mut self, n: &mut swc_core::ecma::ast::JSXElement) {
        #[cfg(debug_assertions)]
        if self
            .state
            .compiled_imports()
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
        let _process_xcss = self.state.opts().process_xcss.unwrap_or(true);
        #[cfg(debug_assertions)]
        if self.state.compiled_imports().is_some() {
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
        let imports = v.state.compiled_imports().expect("compiled_imports populated");
        let styled = imports.styled.as_deref().expect("styled recorded");
        assert_eq!(styled, &["styled".to_string()][..]);
    }

    #[test]
    fn record_renamed_import_uses_local_name() {
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
            vec![named_specifier("MyCss", Some("css"))],
        );
        v.record_compiled_import(&decl);
        let imports = v.state.compiled_imports().expect("compiled");
        assert_eq!(imports.css.as_deref(), Some(&["MyCss".to_string()][..]));
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
        let imports = v.state.compiled_imports().expect("compiled");
        assert_eq!(imports.styled.as_deref(), Some(&["styled".to_string()][..]));
        assert_eq!(imports.css.as_deref(), Some(&["css".to_string()][..]));
        assert_eq!(
            imports.keyframes.as_deref(),
            Some(&["keyframes".to_string()][..])
        );
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
        assert!(v.state.compiled_imports().is_none());
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
        let imports = v.state.compiled_imports().expect("compiled");
        assert_eq!(imports.styled.as_deref(), Some(&["styled".to_string()][..]));
        assert_eq!(
            imports.class_names.as_deref(),
            Some(&["ClassNames".to_string()][..])
        );
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

    // ───────── §2.4 — recorder-routing assertions ─────────

    #[test]
    fn record_compiled_import_pushes_to_diff_log() {
        // §2.4 contract: every captured mutation lands in
        // recorder.diff_log() in iteration order.
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
            vec![
                named_specifier("styled", None),
                named_specifier("css", None),
            ],
        );
        v.record_compiled_import(&decl);
        let log = v.recorder.diff_log();
        assert_eq!(log.len(), 2);
        assert!(matches!(
            &log[0],
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name,
            } if local_name == "styled"
        ));
        assert!(matches!(
            &log[1],
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Css,
                local_name,
            } if local_name == "css"
        ));
    }

    #[test]
    fn jsx_specifier_does_not_emit_diff_for_api_pushes() {
        // `import { jsx } from '@compiled/react'` — `jsx` is NOT one
        // of the 5 Compiled APIs, so the recorder must not capture an
        // append for it. The classic-pragma flags ARE set (init-time,
        // not captured).
        let mut v = fresh();
        let decl = import_decl(
            "@compiled/react",
            vec![named_specifier("jsx", None)],
        );
        v.record_compiled_import(&decl);
        // No StateDiff entries — `jsx` isn't an ApiKind.
        assert!(v.recorder.diff_log().is_empty());
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
        assert_eq!(v.state.pragma().classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma().classic_jsx_pragma_local_name.as_deref(),
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
        assert_eq!(v.state.pragma().classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma().classic_jsx_pragma_local_name.as_deref(),
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
        assert_eq!(v.state.pragma().classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma().classic_jsx_pragma_local_name.as_deref(),
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
        assert!(v.state.pragma().classic_jsx_pragma_is_compiled.is_none());
        assert!(v.state.pragma().classic_jsx_pragma_local_name.is_none());
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
        assert_eq!(v.state.pragma().jsx_import_source, Some(true));
        assert!(v.state.compiled_imports().is_some());
        assert!(v.state.pragma().jsx.is_none());
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
        assert!(v.state.pragma().jsx_import_source.is_none());
        assert!(v.state.compiled_imports().is_none());
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
        v.state.set_classic_jsx_pragma("myJsx".into());
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert_eq!(v.state.pragma().jsx, Some(true));
        assert!(v.state.compiled_imports().is_some());
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
        v.state.set_classic_jsx_pragma("myJsx".into());
        let module = module_with_first_body_at(pos);
        v.scan_jsx_pragma_comments(&Program::Module(module));
        assert!(v.state.pragma().jsx.is_none());
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
        assert!(v.state.pragma().jsx.is_none());
        assert!(v.state.compiled_imports().is_none());
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
        assert_eq!(v.state.pragma().classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            v.state.pragma().classic_jsx_pragma_local_name.as_deref(),
            Some("jsx")
        );
        // @jsx pragma comment matched the recorded local name.
        assert_eq!(v.state.pragma().jsx, Some(true));
        // ImportDeclaration walk populated styled (jsx is not a recognised API name).
        let imports = v.state.compiled_imports().expect("imports");
        assert_eq!(imports.styled.as_deref(), Some(&["styled".to_string()][..]));
        assert!(imports.css.is_none());
        // Recorder captured exactly one CompiledImportsAppend (for styled).
        assert_eq!(v.recorder.diff_log().len(), 1);
    }

    // ───────── Phase 6 §6.1 — keyframes cleanup-only end-to-end ─────────

    #[test]
    fn phase6a_standalone_keyframes_call_replaced_with_null() {
        // Module:
        //   import { keyframes } from '@compiled/react';
        //   keyframes();
        //
        // Expectation after visit_mut_program:
        //   import { keyframes } from '@compiled/react';
        //   null;
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            CallExpr, Callee, Expr as AstExpr, ExprStmt, Ident, Lit, Stmt,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("keyframes", None)],
        )));
        let call_span = Span::new(BytePos(500), BytePos(510));
        let kf_call = AstExpr::Call(CallExpr {
            span: call_span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Ident(Ident::new(
                "keyframes".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![],
            type_args: None,
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(kf_call),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let body = &m.body;
        // Import preserved (specifier removal is §2.3(b), not 6a).
        assert!(matches!(
            &body[0],
            ModuleItem::ModuleDecl(ModuleDecl::Import(_))
        ));
        // The standalone keyframes call was replaced with `null`,
        // span anchored at the original call span.
        let ModuleItem::Stmt(Stmt::Expr(es)) = &body[1] else {
            panic!("expected ExprStmt at body[1]");
        };
        match &*es.expr {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 500);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
        // Cleanup queue captured the replace action.
        assert_eq!(v.state.paths_to_cleanup().len(), 1);
    }

    #[test]
    fn phase6a_standalone_keyframes_tagged_tpl_replaced_with_null() {
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            Expr as AstExpr, ExprStmt, Ident, Lit, Stmt, TaggedTpl, Tpl,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("keyframes", None)],
        )));
        let tpl_span = Span::new(BytePos(700), BytePos(720));
        let kf_tpl = AstExpr::TaggedTpl(TaggedTpl {
            span: tpl_span,
            ctxt: SyntaxContext::empty(),
            tag: Box::new(AstExpr::Ident(Ident::new(
                "keyframes".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
            type_params: None,
            tpl: Box::new(Tpl {
                span: DUMMY_SP,
                exprs: vec![],
                quasis: vec![],
            }),
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(kf_tpl),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Expr(es)) = &m.body[1] else {
            panic!("expected ExprStmt at body[1]");
        };
        match &*es.expr {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 700);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
    }

    #[test]
    fn phase6a_does_not_replace_unrelated_calls() {
        // `import { keyframes } from '@compiled/react'; unrelated();`
        // — `unrelated()` is not a Compiled API, so neither §6.1 nor
        // §6.2's matchers should fire. (Originally this asserted that
        // a `css()` call stays intact under §6.1-only wiring; with
        // §6.2 active the css call is now legitimately replaced, so
        // we use a truly unrelated callee here to preserve the
        // "non-Compiled call left alone" invariant.)
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            CallExpr, Callee, Expr as AstExpr, ExprStmt, Ident, Stmt,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![
                named_specifier("keyframes", None),
                named_specifier("css", None),
            ],
        )));
        let unrelated_call = AstExpr::Call(CallExpr {
            span: Span::new(BytePos(900), BytePos(910)),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Ident(Ident::new(
                "unrelated".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![],
            type_args: None,
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(unrelated_call),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Expr(es)) = &m.body[1] else {
            panic!("expected ExprStmt");
        };
        // Still a call expression — §6.1 left it alone.
        assert!(matches!(&*es.expr, AstExpr::Call(_)));
        assert!(v.state.paths_to_cleanup().is_empty());
    }

    // ───────── Phase 6 §6.2 — css cleanup-only end-to-end ─────────

    #[test]
    fn phase6b_standalone_css_call_replaced_with_null() {
        // Module:
        //   import { css } from '@compiled/react';
        //   css();
        //
        // Expectation after visit_mut_program:
        //   import { css } from '@compiled/react';
        //   null;
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            CallExpr, Callee, Expr as AstExpr, ExprStmt, Ident, Lit, Stmt,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("css", None)],
        )));
        let call_span = Span::new(BytePos(1500), BytePos(1510));
        let css_call = AstExpr::Call(CallExpr {
            span: call_span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Ident(Ident::new(
                "css".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![],
            type_args: None,
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(css_call),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let body = &m.body;
        assert!(matches!(
            &body[0],
            ModuleItem::ModuleDecl(ModuleDecl::Import(_))
        ));
        let ModuleItem::Stmt(Stmt::Expr(es)) = &body[1] else {
            panic!("expected ExprStmt at body[1]");
        };
        match &*es.expr {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 1500);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
        assert_eq!(v.state.paths_to_cleanup().len(), 1);
    }

    #[test]
    fn phase6b_standalone_css_tagged_tpl_replaced_with_null() {
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            Expr as AstExpr, ExprStmt, Ident, Lit, Stmt, TaggedTpl, Tpl,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("css", None)],
        )));
        let tpl_span = Span::new(BytePos(1700), BytePos(1720));
        let css_tpl = AstExpr::TaggedTpl(TaggedTpl {
            span: tpl_span,
            ctxt: SyntaxContext::empty(),
            tag: Box::new(AstExpr::Ident(Ident::new(
                "css".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
            type_params: None,
            tpl: Box::new(Tpl {
                span: DUMMY_SP,
                exprs: vec![],
                quasis: vec![],
            }),
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(css_tpl),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Expr(es)) = &m.body[1] else {
            panic!("expected ExprStmt at body[1]");
        };
        match &*es.expr {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 1700);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
    }

    #[test]
    fn phase6b_does_not_replace_unrelated_calls() {
        // `import { css, styled } from '@compiled/react'; styled.div();`
        // — `styled.div()` is §6.7's target, not §6.2's. After §6.2
        // alone, the styled call must stay intact.
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            CallExpr, Callee, Expr as AstExpr, ExprStmt, Ident, MemberExpr, MemberProp, Stmt,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![
                named_specifier("css", None),
                named_specifier("styled", None),
            ],
        )));
        let styled_call = AstExpr::Call(CallExpr {
            span: Span::new(BytePos(1900), BytePos(1910)),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(AstExpr::Ident(Ident::new(
                    "styled".into(),
                    DUMMY_SP,
                    SyntaxContext::empty(),
                ))),
                prop: MemberProp::Ident(
                    Ident::new("div".into(), DUMMY_SP, SyntaxContext::empty()).into(),
                ),
            }))),
            args: vec![],
            type_args: None,
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(styled_call),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Expr(es)) = &m.body[1] else {
            panic!("expected ExprStmt");
        };
        assert!(matches!(&*es.expr, AstExpr::Call(_)));
        assert!(v.state.paths_to_cleanup().is_empty());
    }

    #[test]
    fn phase6b_renamed_css_call_replaced() {
        // `import { css as c } from '@compiled/react'; c();`
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            CallExpr, Callee, Expr as AstExpr, ExprStmt, Ident, Lit, Stmt,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("c", Some("css"))],
        )));
        let call_span = Span::new(BytePos(2100), BytePos(2110));
        let css_call = AstExpr::Call(CallExpr {
            span: call_span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Ident(Ident::new(
                "c".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![],
            type_args: None,
        });
        let stmt = ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(css_call),
        }));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, stmt],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Expr(es)) = &m.body[1] else {
            panic!("expected ExprStmt at body[1]");
        };
        match &*es.expr {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 2100);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
    }

    #[test]
    fn phase6a_keyframes_call_inside_var_declarator_init_replaced() {
        // Mirrors the realistic shape:
        //   import { keyframes } from '@compiled/react';
        //   const fade = keyframes({ from: { opacity: 1 }, to: { opacity: 0 } });
        // After §6.1: `const fade = null;` (the binding stays but the
        // RHS is null'd — exactly what upstream's
        // `pathsToCleanup` replaces).
        use swc_core::common::{Span, SyntaxContext};
        use swc_core::ecma::ast::{
            BindingIdent, CallExpr, Callee, Decl, Expr as AstExpr, Ident, Lit, ObjectLit, Pat,
            Stmt, VarDecl, VarDeclKind, VarDeclarator,
        };

        let import = ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl(
            "@compiled/react",
            vec![named_specifier("keyframes", None)],
        )));

        let kf_span = Span::new(BytePos(1100), BytePos(1200));
        let kf_call = AstExpr::Call(CallExpr {
            span: kf_span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(AstExpr::Ident(Ident::new(
                "keyframes".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![swc_core::ecma::ast::ExprOrSpread {
                spread: None,
                expr: Box::new(AstExpr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props: vec![],
                })),
            }],
            type_args: None,
        });
        let decl = ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident::new("fade".into(), DUMMY_SP, SyntaxContext::empty()),
                    type_ann: None,
                }),
                init: Some(Box::new(kf_call)),
                definite: false,
            }],
        }))));
        let module = Module {
            span: DUMMY_SP,
            body: vec![import, decl],
            shebang: None,
        };

        let mut v = fresh();
        let mut program = Program::Module(module);
        v.visit_mut_program(&mut program);

        let Program::Module(m) = &program else {
            panic!("expected Module")
        };
        let ModuleItem::Stmt(Stmt::Decl(Decl::Var(vd))) = &m.body[1] else {
            panic!("expected VarDecl");
        };
        let init = vd.decls[0].init.as_deref().expect("init");
        match init {
            AstExpr::Lit(Lit::Null(n)) => {
                assert_eq!(n.span.lo.0, 1100);
            }
            other => panic!("expected null literal init, got {:?}", other),
        }
    }
}
