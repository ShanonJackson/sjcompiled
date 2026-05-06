//! `State` — per-file traversal state, encapsulated.
//!
//! Mirrors `State` in upstream `packages/babel-plugin/src/types.ts`
//! lines 117–211. Field-for-field shape parity; the difference is
//! enforcement: PLAN.md §3.9.8 mandates that `State`'s mutable
//! fields are `pub(crate)` only, with `pub fn` getters for read
//! access and ALL evaluation-visible mutations routed through
//! `MutationRecorder::apply` (defined in this module so it has
//! same-module access to private fields).
//!
//! Why this matters: the Phase 5 two-layer cache (`cache.bin`)
//! replays Layer 2 hits by applying the entry's `state_diffs` against
//! the consumer's state. ANY mutation that bypasses the recorder is a
//! silent HMR-divergence bug — bytes match (state isn't in the AST),
//! corpus diff stays clean, production goes wrong on edited files.
//! The encapsulation makes a missed-capture a Rust compile error
//! outside this module, and a pre-commit grep lint catches the
//! within-module regex pattern.
//!
//! Source enumeration: `crates/babel-plugin/STATE_MUTATIONS.md`
//! (Phase 0 artefact). Five `StateDiff` variants, five `apply` arms.
//! Adding a sixth requires amending STATE_MUTATIONS.md AND bumping
//! the cache schema version (Phase 5 §5.3 `CACHE_VERSION` +
//! `SCHEMA_HASH`).
//!
//! ### Fields under encapsulation contract (cache-replay-relevant)
//!
//! `compiled_imports`, `sheets`, `css_map`, `included_files`,
//! `ignore_member_expressions`. These are the fields whose mutations
//! map to `StateDiff` variants.
//!
//! ### Fields under same-visibility for compile-time enforcement (NOT diff-captured)
//!
//! `pragma`, `uses_xcss`, `imported_compiled_imports`,
//! `paths_to_cleanup`, `opts`, `import_sources`, `cache`. Per
//! STATE_MUTATIONS.md these are out-of-capture (set during
//! `Program::enter` / `ImportDeclaration` / once-per-file init), but
//! still `pub(crate)` so the encapsulation barrier is uniform.
//! Mutating methods on `State` (e.g. `set_pragma_jsx`,
//! `ensure_compiled_imports`) provide the controlled write paths.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use swc_core::common::comments::CommentKind;

use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
use crate::resolver::Resolver;
use crate::types::PluginOptions;

/// Per-file traversal state. Mirrors upstream `State` in
/// `packages/babel-plugin/src/types.ts` lines 117–211.
///
/// All mutable fields are `pub(crate)`. Tests in the same crate read
/// them directly; production code outside `state.rs` /
/// `mutation_recorder.rs` mutates them only through the methods on
/// this type (init-time / non-captured) or through
/// `MutationRecorder::apply` (cache-captured).
#[derive(Debug, Default)]
pub struct State {
    /// Set when the Compiled module import is found. Each known API
    /// gets its imported binding name(s) — the visitor uses these to
    /// match call sites.
    pub(crate) compiled_imports: Option<CompiledImports>,

    /// `true` if the module imports xcss from a Compiled origin.
    pub(crate) uses_xcss: Option<bool>,

    /// `importedCompiledImports.css` — set when `import { css } from
    /// '@compiled/react'` is found AND a host-imported alias
    /// shadows it (rare but supported upstream).
    pub(crate) imported_compiled_imports: Option<ImportedCompiledImports>,

    /// Module origins recognised as Compiled. Resolved from
    /// `PluginOptions.import_sources` ∪ `DEFAULT_IMPORT_SOURCES`.
    pub(crate) import_sources: Vec<String>,

    /// Pragma state — JSX classic vs automatic, source override, etc.
    pub(crate) pragma: PragmaState,

    /// `pathsToCleanup` is a Babel-NodePath construct. The Rust port
    /// records these as deferred mutations on the `MutationRecorder`
    /// (§2.4); this field stays as a marker for parity but the
    /// concrete representation moves to that recorder. See PLAN.md
    /// §3.9.8 `StateDiff::CleanupPath`.
    pub(crate) paths_to_cleanup: Vec<CleanupAction>,

    /// User-supplied options. Owned here so handlers don't thread an
    /// extra param.
    pub(crate) opts: PluginOptions,

    /// Hoisted style sheets — `name → identifier`. Order preserved
    /// (insertion order matches Babel; the AST emit order depends on
    /// it). Babel stores `t.Identifier`; we store the symbol name and
    /// reconstruct the SWC `Ident` on emit (Phase 6).
    pub(crate) sheets: IndexMap<String, String>,

    /// Cache for evaluated paths. The concrete cache type lands in
    /// §5.3 (`utils::cache::Cache`); this is a placeholder slot.
    #[allow(dead_code)] // Phase 5 §5.3 wires this; pre-locked for shape stability.
    pub(crate) cache: CacheSlot,

    /// Files included in this transformation pass. Drained at
    /// `Program::exit` and serialised to `included-files.json`
    /// sidecar (§5.7 / SIDECAR_SCHEMA.md §1).
    pub(crate) included_files: Vec<String>,

    /// Evaluated `cssMap()` outputs — `localName → vec of css rules`.
    /// Order preserved (visitor walks in source order).
    pub(crate) css_map: IndexMap<String, Vec<String>>,

    /// MemberExpression names to skip — populated when a binding is
    /// known not to be a Compiled API (avoids re-resolving across
    /// many references). Mirrors `state.ignoreMemberExpressions`.
    pub(crate) ignore_member_expressions: IndexMap<String, bool>,

    /// Per-pass UID counter. The Rust analog of Babel's
    /// `meta.parentPath.scope.generateUidIdentifier('')` — without
    /// full scope tracking (Phase 5 §5.4 lands that). For §4.6 hoists
    /// this is enough: every call to `next_uid_name` returns a fresh
    /// `_<n>` string distinct from prior calls in the same pass.
    /// Per-pass scope; SWC tears down the WASI instance between
    /// transforms so a fresh visitor starts at 0 every time.
    pub(crate) uid_counter: u32,

    /// In-plugin module resolver. Built once at `Program::enter` from
    /// `state.opts.resolver` (Phase 5 §5.4b/c/d shipped the engine;
    /// the visitor-dispatch wiring lands when the dispatcher
    /// engages). `None` until `set_resolver` is called — see
    /// `resolve_binding.rs` for the consumer surface and §5.4
    /// closure summaries in `plugins/STATUS.md` for the architecture.
    /// Stored as `Arc` so the resolver can be cheaply cloned into
    /// per-rule preferFirst dispatchers without duplicating
    /// `oxc_resolver`'s package.json caches.
    pub(crate) resolver: Option<Arc<Resolver>>,

    /// Absolute path of the file being transformed. Mirrors Babel's
    /// `state.filename`. Used by `resolve_binding.rs` to resolve
    /// relative import specifiers and to skip module-traversal when
    /// the visitor was invoked without a filename (e.g. anonymous
    /// in-memory transforms in some test harnesses). `None` until
    /// `set_filename` is called by the visitor on `Program::enter`.
    pub(crate) filename: Option<String>,

    /// §6.5 comment-store: every comment in the file with its line
    /// numbers resolved up-front. Mirrors upstream's
    /// `meta.state.file.ast.comments` flat list — Babel walks it by
    /// line during `getNodeComments`. SWC's plugin-comments proxy is
    /// BytePos-keyed and doesn't expose iteration, so the visitor
    /// builds this list at `Program::enter` by walking the AST and
    /// querying `comments.get_leading/get_trailing` for every span,
    /// then resolves each `BytePos` to a 1-indexed line via the
    /// plugin's source-map proxy. Out-of-capture per
    /// STATE_MUTATIONS.md (per-file scaffolding, not part of the
    /// cross-file caching contract — same classification as
    /// `pragma`). Empty in tests / contexts that lack a source-map
    /// (`is_css_prop_disabled` returns false there, matching
    /// upstream's "no directive present" fast path).
    pub(crate) comment_lines: Vec<LineComment>,

    /// §6.5 span→line index. Populated alongside `comment_lines` at
    /// `Program::enter` so dispatch sites can resolve a node span's
    /// 1-indexed line without holding a source-map themselves. The
    /// pre-pass visits every node touched by the comment-collection
    /// walk (Stmt / Expr / JSX nodes / VarDeclarator / Pat /
    /// BlockStmt / Function / ArrowExpr / Class / Module / Script)
    /// and records `(span.lo → line)` and `(span.hi → line)`. Lookup
    /// of an unknown `BytePos` returns `None`, treated upstream as
    /// "no loc, skip the directive check".
    pub(crate) span_lines: HashMap<u32, usize>,
    // `transformCache` (Babel WeakMap on NodePath) is a Babel-only
    // construct — the Rust visitor's single-pass design (PLAN.md §3.5)
    // eliminates the re-visit problem the WeakMap was guarding against.
}

// ───────── Read-only getters (public API) ─────────
//
// Every captured field has a `&`-returning getter. Outside this
// module + `mutation_recorder.rs`, reads go through these. Writes
// have no public counterpart — `MutationRecorder::apply` is the
// only writer for captured fields; the methods below in the
// "Init-time mutators" block cover non-captured fields.

impl State {
    pub fn compiled_imports(&self) -> Option<&CompiledImports> {
        self.compiled_imports.as_ref()
    }

    pub fn uses_xcss(&self) -> Option<bool> {
        self.uses_xcss
    }

    pub fn imported_compiled_imports(&self) -> Option<&ImportedCompiledImports> {
        self.imported_compiled_imports.as_ref()
    }

    pub fn import_sources(&self) -> &[String] {
        &self.import_sources
    }

    pub fn pragma(&self) -> &PragmaState {
        &self.pragma
    }

    pub fn paths_to_cleanup(&self) -> &[CleanupAction] {
        &self.paths_to_cleanup
    }

    pub fn opts(&self) -> &PluginOptions {
        &self.opts
    }

    pub fn sheets(&self) -> &IndexMap<String, String> {
        &self.sheets
    }

    pub fn included_files(&self) -> &[String] {
        &self.included_files
    }

    pub fn css_map(&self) -> &IndexMap<String, Vec<String>> {
        &self.css_map
    }

    pub fn ignore_member_expressions(&self) -> &IndexMap<String, bool> {
        &self.ignore_member_expressions
    }

    /// `&Resolver` if the visitor has wired one in via [`Self::set_resolver`].
    /// `resolve_binding` consumers must check `is_some` — non-set
    /// resolvers produce an `unimplemented!` upstream rather than a
    /// silent no-op (per the §5.4e closure contract; the visitor
    /// dispatcher MUST set the resolver on `Program::enter`).
    pub fn resolver(&self) -> Option<&Resolver> {
        self.resolver.as_deref()
    }

    /// `&str` of the file being transformed. `None` if
    /// `set_filename` hasn't been called.
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// File-wide comment store with pre-resolved line numbers (§6.5).
    /// Empty when the visitor was invoked without a source-map (tests).
    pub fn comment_lines(&self) -> &[LineComment] {
        &self.comment_lines
    }

    /// 1-indexed line of `byte_pos` if the §6.5 pre-pass observed
    /// the position, else `None`. `None` matches upstream's
    /// `path.node?.loc?.start.line` undefined-guard ("no loc → skip
    /// the directive check").
    pub fn line_of(&self, byte_pos: u32) -> Option<usize> {
        self.span_lines.get(&byte_pos).copied()
    }
}

// ───────── Init-time / non-captured mutators ─────────
//
// These methods write to fields that STATE_MUTATIONS.md classifies as
// "not part of the cross-file caching contract" — `opts`,
// `import_sources`, `pragma`, `compiled_imports` bootstrap (init from
// None to Some(default)), `paths_to_cleanup` (deferred AST mutation
// queue, read at Program::exit). They live as methods (not direct
// field writes) so the lint gate
// (`grep state\.X\.{push,set,add,insert,remove,extend}`) stays
// applicable; calls of the form `state.set_pragma_jsx()` don't trip
// the regex (only one `.` after `state`).

impl State {
    pub(crate) fn set_opts(&mut self, opts: PluginOptions) {
        self.opts = opts;
    }

    /// Wire an `Arc<Resolver>` into the state. The visitor calls
    /// this once on `Program::enter` after building the resolver
    /// from `self.opts.resolver` per RESOLVER_SPEC_PART_TWO.md.
    /// Tests / integration callers in `resolve_binding.rs::tests`
    /// also use this directly with a hand-built resolver.
    pub fn set_resolver(&mut self, resolver: Arc<Resolver>) {
        self.resolver = Some(resolver);
    }

    /// Wire the absolute filename of the current source. The
    /// visitor calls this once on `Program::enter` from
    /// `swc_core::common::FileName::Real(...)`. Tests construct it
    /// directly. `resolve_binding.rs` reads via `Self::filename`.
    pub fn set_filename(&mut self, filename: String) {
        self.filename = Some(filename);
    }

    /// Wire the pre-resolved comment-line store. §6.5 — called once
    /// from `lib.rs::process` after the AST-walk pre-pass that
    /// resolves every `BytePos` to a 1-indexed line via the plugin
    /// source-map proxy. Tests can populate this directly to exercise
    /// the disable-directive code path without a real source-map.
    pub fn set_comment_lines(&mut self, lines: Vec<LineComment>) {
        self.comment_lines = lines;
    }

    /// §6.5 — wire the pre-resolved `BytePos → 1-indexed line` map.
    /// Tests that exercise `is_css_prop_disabled` without going through
    /// the plugin entry pre-pass populate this manually with the JSX
    /// element / JSX attribute span positions they care about.
    pub fn set_span_lines(&mut self, lines: HashMap<u32, usize>) {
        self.span_lines = lines;
    }

    pub(crate) fn set_import_sources(&mut self, sources: Vec<String>) {
        self.import_sources = sources;
    }

    /// Bootstrap `state.compiled_imports` to `Some(default)` if it's
    /// `None`. Idempotent. Mirrors upstream's
    /// `state.compiledImports = state.compiledImports || {}` pattern
    /// at sites 1, 2, 3 of STATE_MUTATIONS.md (all classified as
    /// pre-evaluation init, NOT captured by `StateDiff`).
    pub(crate) fn ensure_compiled_imports(&mut self) {
        if self.compiled_imports.is_none() {
            self.compiled_imports = Some(CompiledImports::default());
        }
    }

    /// `state.pragma.classic_jsx_pragma_is_compiled = true;
    ///  state.pragma.classic_jsx_pragma_local_name = local;`
    /// Mirrors `findClassicJsxPragmaImport` lines 57–58 in upstream.
    pub(crate) fn set_classic_jsx_pragma(&mut self, local_name: String) {
        self.pragma.classic_jsx_pragma_is_compiled = Some(true);
        self.pragma.classic_jsx_pragma_local_name = Some(local_name);
    }

    /// `state.pragma.jsx = true;` — the `@jsx` pragma fired and
    /// matched the recorded classic-pragma local name.
    pub(crate) fn set_pragma_jsx(&mut self) {
        self.pragma.jsx = Some(true);
    }

    /// `state.pragma.jsxImportSource = true;` — the
    /// `@jsxImportSource` pragma fired and named a Compiled origin.
    pub(crate) fn set_pragma_jsx_import_source(&mut self) {
        self.pragma.jsx_import_source = Some(true);
    }

    /// `state.usesXcss = true;` — set when the xcss-prop handler emits
    /// the wrapping `<CC>...</CC>` element. Read at `Program::exit` by
    /// the runtime-import emitter (Phase 7) to gate the
    /// `@compiled/react/runtime` import even when no css/styled call
    /// was found in the file. STATE_MUTATIONS.md classifies this as
    /// out-of-capture (per-file scaffolding, not part of the
    /// cross-file caching contract).
    pub(crate) fn set_uses_xcss(&mut self) {
        self.uses_xcss = Some(true);
    }

    /// Append to `paths_to_cleanup`. The §2.3(b) follow-up uses this
    /// to queue specifier removals (`{ action: Remove, id }`) and
    /// path replacements (`{ action: Replace, id }`). The `id` is a
    /// recorder-issued handle — Phase 5 §5.3 wires the concrete
    /// node-identity table; for §2.4 the id space is allocated
    /// per-visitor and not yet persistent.
    #[allow(dead_code)]
    pub(crate) fn queue_cleanup(&mut self, action: CleanupAction) {
        self.paths_to_cleanup.push(action);
    }

    /// Mint a fresh `_<n>` UID name for this pass. Mirrors Babel's
    /// `scope.generateUidIdentifier('')` shape — see
    /// `@babel/traverse/lib/scope/index.js::generateUid` (~line 376):
    ///
    /// ```js
    /// let i = 0;
    /// do {
    ///   uid = `_${name}`;
    ///   if (i >= 11) uid += i - 1;
    ///   else if (i >= 9) uid += i - 9;
    ///   else if (i >= 1) uid += i + 1;
    ///   i++;
    /// } while (hasBinding(uid) || ...);
    /// ```
    ///
    /// For empty input name, this produces the sequence
    /// `_, _2, _3, _4, _5, _6, _7, _8, _9, _0, _1, _10, _11, _12, ...`.
    /// The bare `_` is i=0; `_0` and `_1` slot in between `_9` and `_10`.
    ///
    /// **§6.8h:** rewritten to match the upstream three-bucket suffix
    /// formula. The §6.8a-iv impl produced `_, _2, ..., _9, _10, _11`
    /// which diverged for fixtures with ≥10 hoisted sheets (e.g.
    /// `0248-styled-tests-behaviour--should-handle-destructuring-in-interpolation-functions`
    /// — Babel emits `_0` for the 10th, we emitted `_10`).
    ///
    /// Future Phase 5 §5.4 work will make this fully scope-aware
    /// (collision-walk against existing bindings); today's bump
    /// preserves the format Babel actually emits for the common case
    /// (no `_<n>` collisions in user source).
    pub(crate) fn next_uid_name(&mut self) -> String {
        let i = self.uid_counter;
        self.uid_counter += 1;
        let suffix: String = if i >= 11 {
            (i - 1).to_string()
        } else if i >= 9 {
            (i - 9).to_string()
        } else if i >= 1 {
            (i + 1).to_string()
        } else {
            String::new()
        };
        format!("_{}", suffix)
    }
}

// ───────── MutationRecorder::apply — captured-field mutations ─────────
//
// PLAN.md §3.9.8: "the only public mutator is
// `MutationRecorder::apply(diff: StateDiff, state: &mut State)`,
// which lives in the same module as `State`". This impl block holds
// `apply`, with same-module access to `State`'s `pub(crate)` fields
// — no extra mutator surface needed.
//
// Every variant must:
//   1. Mutate state to match the upstream Babel write semantics.
//   2. Push the diff into the recorder's log so the §5.3 cache can
//      replay it later.
//
// Order matters within `apply`: write FIRST, log SECOND. If the write
// panics (it shouldn't, but defensively), the diff log doesn't capture
// a phantom mutation that didn't actually take effect.

impl MutationRecorder {
    /// Apply a captured state mutation. The SOLE public mutator into
    /// `State`'s cache-captured fields outside this module +
    /// `mutation_recorder.rs`.
    ///
    /// Variant arms map 1:1 to STATE_MUTATIONS.md sites 4–8. Site
    /// numbering and reasoning lives in that doc; this code stays
    /// minimal (one Rust line per Babel write).
    pub fn apply(&mut self, diff: StateDiff, state: &mut State) {
        match &diff {
            StateDiff::CompiledImportsAppend { api, local_name } => {
                // Site 4 — babel-plugin.ts:282-284. Bootstraps the
                // bucket if absent (matches upstream's `apiArray =
                // state.compiledImports[apiName] || []`), then pushes.
                state.ensure_compiled_imports();
                let imports = state.compiled_imports.as_mut().expect("ensured above");
                let slot = match api {
                    ApiKind::Styled => &mut imports.styled,
                    ApiKind::ClassNames => &mut imports.class_names,
                    ApiKind::Css => &mut imports.css,
                    ApiKind::Keyframes => &mut imports.keyframes,
                    ApiKind::CssMap => &mut imports.css_map,
                };
                slot.get_or_insert_with(Vec::new).push(local_name.clone());
            }
            StateDiff::IncludedFilesPush { path } => {
                // Site 6 — utils/css-builders.ts:325. Highest-frequency
                // mutation; per-file-open append.
                state.included_files.push(path.clone());
            }
            StateDiff::SheetsInsert {
                sheet_text,
                hoisted_name,
            } => {
                // Site 8 — utils/hoist-sheet.ts:32. IndexMap preserves
                // insertion order so the AST emit order at Phase 6's
                // hoist site matches Babel.
                state.sheets.insert(sheet_text.clone(), hoisted_name.clone());
            }
            StateDiff::CssMapInsert { binding, sheets } => {
                // Site 5 — css-map/index.ts:115. Whole-array publish
                // per binding; not per-element append.
                state.css_map.insert(binding.clone(), sheets.clone());
            }
            StateDiff::IgnoreMemberExprMark { name } => {
                // Site 7 — utils/css-builders.ts:725. Presence-check
                // set; value is always `true`.
                state.ignore_member_expressions.insert(name.clone(), true);
            }
        }
        self.push_diff(diff);
    }
}

// ───────── Inner shapes (kept here for the encapsulation boundary) ─────────

/// `state.compiledImports` — which Compiled API names are bound in
/// this module, and under which local identifiers. Empty vec means
/// "imported but no aliases" / "imported with default name".
#[derive(Debug, Default)]
pub struct CompiledImports {
    pub class_names: Option<Vec<String>>,
    pub css: Option<Vec<String>>,
    pub keyframes: Option<Vec<String>>,
    pub styled: Option<Vec<String>>,
    pub css_map: Option<Vec<String>>,
}

/// `state.importedCompiledImports` — kept narrow because upstream
/// only ever sets `css` here. Adding a new variant requires bumping
/// the cache schema (§5.3 `schema_hash`).
#[derive(Debug, Default)]
pub struct ImportedCompiledImports {
    pub css: Option<String>,
}

/// `state.pragma` — JSX-pragma awareness used by the css-prop and
/// classnames handlers (Phase 6). Defaults are all `false` / `None`.
#[derive(Debug, Default)]
pub struct PragmaState {
    /// `/** @jsx ... */` is set on this file.
    pub jsx: Option<bool>,
    /// `/** @jsxImportSource ... */` is set on this file.
    pub jsx_import_source: Option<bool>,
    /// Classic-pragma name resolves to a Compiled binding.
    pub classic_jsx_pragma_is_compiled: Option<bool>,
    /// Local name of the classic-pragma identifier (after rename).
    pub classic_jsx_pragma_local_name: Option<String>,
}

/// One entry of `state.pathsToCleanup`. Babel's `NodePath` reference
/// is replaced with an opaque ID the Phase 5 mutation recorder
/// knows how to resolve to a concrete AST mutation.
///
/// `id` here corresponds to a `MutationRecorder`-issued handle
/// (§2.4); the recorder owns the actual AST identity (BytePos /
/// node-index).
#[derive(Debug, Clone, Copy)]
pub struct CleanupAction {
    pub action: CleanupKind,
    pub id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    Replace,
    Remove,
}

/// Placeholder for the Phase 5 `Cache`. Today this is empty so
/// dispatcher code can hold a `&mut State.cache` without a type
/// dependency on the unported cache module.
#[derive(Debug, Default)]
pub struct CacheSlot;

/// One entry in the §6.5 file-wide comment store. `start_line` /
/// `end_line` are 1-indexed lines as resolved through the SWC
/// plugin's source-map proxy at `Program::enter`. Mirrors the shape
/// `getNodeComments` upstream consumes from `comment.loc.start.line`
/// / `comment.loc.end.line` on a Babel `CommentLine` / `CommentBlock`.
#[derive(Debug, Clone)]
pub struct LineComment {
    pub start_line: usize,
    pub end_line: usize,
    pub kind: CommentKind,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_state() -> State {
        State::default()
    }

    // ───────── Init-time mutator tests ─────────

    #[test]
    fn ensure_compiled_imports_is_idempotent() {
        let mut s = fresh_state();
        assert!(s.compiled_imports().is_none());
        s.ensure_compiled_imports();
        assert!(s.compiled_imports().is_some());
        // Calling again leaves the existing struct in place — no
        // clobbering. (Upstream behaviour: `state.compiledImports =
        // state.compiledImports || {}` only assigns when null.)
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name: "X".into(),
            },
            &mut s,
        );
        s.ensure_compiled_imports(); // no-op
        let imports = s.compiled_imports().expect("populated");
        assert_eq!(imports.styled.as_deref(), Some(&["X".to_string()][..]));
    }

    #[test]
    fn classic_jsx_pragma_setter() {
        let mut s = fresh_state();
        s.set_classic_jsx_pragma("myJsx".into());
        assert_eq!(s.pragma().classic_jsx_pragma_is_compiled, Some(true));
        assert_eq!(
            s.pragma().classic_jsx_pragma_local_name.as_deref(),
            Some("myJsx")
        );
    }

    #[test]
    fn pragma_jsx_and_jsx_import_source_setters() {
        let mut s = fresh_state();
        s.set_pragma_jsx();
        s.set_pragma_jsx_import_source();
        assert_eq!(s.pragma().jsx, Some(true));
        assert_eq!(s.pragma().jsx_import_source, Some(true));
    }

    // ───────── MutationRecorder::apply tests (5 variants) ─────────

    #[test]
    fn apply_compiled_imports_append_bootstraps_when_none() {
        // Documents the contract STATE_MUTATIONS.md sites 1/2/3 +
        // site 4 jointly produce: even if init-time bootstrap was
        // skipped, applying the first append must NOT panic — it
        // bootstraps the option.
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name: "MyStyled".into(),
            },
            &mut s,
        );
        let imports = s.compiled_imports().expect("bootstrapped");
        assert_eq!(
            imports.styled.as_deref(),
            Some(&["MyStyled".to_string()][..])
        );
    }

    #[test]
    fn apply_compiled_imports_append_routes_per_api() {
        // Each ApiKind goes to its own slot. Append-order preserved.
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        for (api, name) in &[
            (ApiKind::Styled, "s1"),
            (ApiKind::Styled, "s2"), // accumulate within slot
            (ApiKind::ClassNames, "cn"),
            (ApiKind::Css, "c"),
            (ApiKind::Keyframes, "kf"),
            (ApiKind::CssMap, "cm"),
        ] {
            r.apply(
                StateDiff::CompiledImportsAppend {
                    api: *api,
                    local_name: (*name).to_string(),
                },
                &mut s,
            );
        }
        let imports = s.compiled_imports().expect("populated");
        assert_eq!(
            imports.styled.as_deref(),
            Some(&["s1".to_string(), "s2".to_string()][..])
        );
        assert_eq!(
            imports.class_names.as_deref(),
            Some(&["cn".to_string()][..])
        );
        assert_eq!(imports.css.as_deref(), Some(&["c".to_string()][..]));
        assert_eq!(imports.keyframes.as_deref(), Some(&["kf".to_string()][..]));
        assert_eq!(imports.css_map.as_deref(), Some(&["cm".to_string()][..]));
    }

    #[test]
    fn apply_included_files_push() {
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::IncludedFilesPush {
                path: "src/theme.ts".into(),
            },
            &mut s,
        );
        r.apply(
            StateDiff::IncludedFilesPush {
                path: "src/colors.ts".into(),
            },
            &mut s,
        );
        // Append-order preserved; matches upstream `Array.prototype.push`.
        assert_eq!(
            s.included_files(),
            &["src/theme.ts".to_string(), "src/colors.ts".to_string()][..]
        );
    }

    #[test]
    fn apply_sheets_insert_preserves_index_map_order() {
        // Babel iterates `sheets` in insertion order on hoist. The
        // IndexMap-backed `sheets` field guarantees this; the test
        // locks it in case anyone "optimises" by switching to HashMap.
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::SheetsInsert {
                sheet_text: "._zzz{color:red}".into(),
                hoisted_name: "_zzz".into(),
            },
            &mut s,
        );
        r.apply(
            StateDiff::SheetsInsert {
                sheet_text: "._aaa{color:blue}".into(),
                hoisted_name: "_aaa".into(),
            },
            &mut s,
        );
        let keys: Vec<&str> = s.sheets().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["._zzz{color:red}", "._aaa{color:blue}"]);
        assert_eq!(s.sheets().get("._zzz{color:red}"), Some(&"_zzz".into()));
    }

    #[test]
    fn apply_css_map_insert_whole_array_publish() {
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CssMapInsert {
                binding: "vars".into(),
                sheets: vec!["._a{color:red}".into(), "._b{color:blue}".into()],
            },
            &mut s,
        );
        let cm = s.css_map();
        assert_eq!(cm.len(), 1);
        assert_eq!(
            cm.get("vars").map(|v| v.as_slice()),
            Some(&["._a{color:red}".to_string(), "._b{color:blue}".to_string()][..])
        );
    }

    #[test]
    fn apply_css_map_insert_overwrites_per_binding() {
        // Upstream: per-binding whole-array publish, not per-element
        // append. Two writes to the same binding REPLACE, not merge.
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CssMapInsert {
                binding: "vars".into(),
                sheets: vec!["._a{color:red}".into()],
            },
            &mut s,
        );
        r.apply(
            StateDiff::CssMapInsert {
                binding: "vars".into(),
                sheets: vec!["._b{color:blue}".into()],
            },
            &mut s,
        );
        let v = s.css_map().get("vars").expect("present");
        assert_eq!(v, &["._b{color:blue}".to_string()][..]);
    }

    #[test]
    fn apply_ignore_member_expr_mark() {
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::IgnoreMemberExprMark {
                name: "theme".into(),
            },
            &mut s,
        );
        assert_eq!(
            s.ignore_member_expressions().get("theme"),
            Some(&true)
        );
    }

    #[test]
    fn diff_log_captures_every_apply_in_order() {
        // The Phase 5 cache writer drains `recorder.diff_log()` at
        // evaluation completion. Order MUST match application order
        // so replay reproduces upstream-equivalent state.
        let mut s = fresh_state();
        let mut r = MutationRecorder::new();
        let diffs = vec![
            StateDiff::IncludedFilesPush {
                path: "a".into(),
            },
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name: "X".into(),
            },
            StateDiff::IgnoreMemberExprMark {
                name: "theme".into(),
            },
        ];
        for d in &diffs {
            r.apply(d.clone(), &mut s);
        }
        assert_eq!(r.diff_log(), diffs.as_slice());
    }
}
