# `plugins/` STATUS — checkpoint ledger

> **Purpose.** Single source of truth for where the SWC port stands.
> Each row is a **checkpoint**: a 0%-or-100% unit of work owned by
> exactly one agent. Checkpoints are not parallelisable within
> themselves — pick one up, finish it, move on. If context is lost
> mid-checkpoint, the next agent re-reads this file and the artefacts
> the previous agent produced and continues at the same checkpoint.
>
> Read this file with `plugins/PLAN.md` (the design) and
> `plugins/READ_WRITE.md` (the WASI sandbox contract). PLAN.md is the
> spec; this file is the schedule + state.

## How to read this file

- **Status:** `☐` not started · `▶` in progress (active) · `☑` done · `⚠` blocked
- **Owner:** the agent who is or was on the checkpoint. `—` if unowned.
- **Artefacts:** the files / build outputs the checkpoint produces.
  Reading them is how the next agent verifies "is this actually done?"
- **Verification:** the exact command(s) a fresh agent runs to confirm
  the checkpoint still holds. If it doesn't, fix it before moving on.
- **Resume note:** what the previous agent left behind for the next
  one. Empty for fresh checkpoints; populated when a checkpoint is
  paused mid-flight.

## Resume here

**Closed:** Phase 0 ☑ (modulo deferred §0.10–§0.12 hardening
tasks — Phase 5 gates, NOT Phase 4 blockers). Phase 1 ☑. Phase 2 ☑.
Phase 3 ☑ (§3.1–§3.4). Phase 4 §4.1 ☑. Phase 4 §4.2 ☑. Phase 4 §4.3 ☑.
Phase 4 §4.4 ☑ (SHELL port). Phase 4 §4.5 ☑ (data adapters).
**Phase 4 §4.6 ☑ (this session, 2026-05-05) — bridge tail.**
Filename + resolver injection wired in `lib.rs::process` (extracts
`TransformPluginMetadataContextKind::Filename`, builds
`resolver::build_default(opts.extensions.as_deref())`, calls
`state.set_filename` / `state.set_resolver`). `ScopeIndex` +
`program_scope` fields landed on `BabelPluginVisitor`; built once
in `visit_mut_program` via `ScopeIndex::build(&module)` before the
children walk. 13 fns in `css_builders.rs` threaded with
`&mut ScopeIndex, parent_scope: ScopeId, own_scope: Option<ScopeId>`
per the §5.5 explicit-param lock (Metadata-lifetime cascade
explicitly avoided). 6 stub call sites flipped to real
`crate::utils::evaluate_expression::evaluate_expression` /
`crate::utils::resolve_binding::resolve_binding`; the 1
`visitCssMapPath` dispatch site retained as an inline phase-citing
`unimplemented!()` (Phase 6 §6.3 owns the real fn). All 3 SHELL
stub fns (`evaluate_expression_stub`, `resolve_binding_stub`,
`visit_css_map_path_stub`) deleted. See §4.6 bridge closure summary
below.
Phase 5 §5.1 ☑ (STATE_MUTATIONS.md reconfirmed; line-number drifts
amended; zero new variants). Phase 5 §5.3 ☑ (Layer-1 Cache + Layer-2
postcard schema + atomic-write Layer-2 handle landed; not yet wired
into State because §5.4/§5.6 are blocked — see §5.4–§5.6 drift note
below). Phase 5 §5.0 entry-gate ☑ (audit, parity corpora, pin guards,
`#[ignore]`'d Rust gates landed). Phase 5 §5.0a ☑
(`compat/scope.rs` + `compat/globals.rs` ported 1:1 against
`@babel/traverse@7.29.0`; byte-parity gate green at 23/23 — see
§5.0a closure summary below). Phase 5 §5.0b ☑
(`compat/path.rs` ported with `PathHandle` predicate fan-out,
single-site `replace_expr`, `ensure_block`, `traverse_subtree`,
and AST-mutating `scope_push` replacing §5.0a's binding-only
stub — see §5.0b closure summary below).
Phase 5 §5.0c ☑. Phase 5 §5.4 row group ☑
(§5.4a/b/c/d/e all closed). Phase 5 §5.5 closure ☑
(all 14 leaves real 1:1 ports; §5.0d compat surface
absorbed). **Phase 5 §5.6 ☑ (this session, 2026-05-05) —
`utils/evaluate_expression.rs` ported 1:1 against
`packages/babel-plugin/src/utils/evaluate-expression.ts`. The
14-entry unit-test gate runs green. All three §5.6 wiring
contracts honoured: cross-file `ScopeIndex` synthesis,
`own_scope_override` consumption, namespace-import preflight in
the MemberExpression branch. See §5.6 closure summary below.**
**With §5.6 ☑, the entire Phase 5 row group is fully shipped.**

**Phase 6 §6.1 ☑ (prior session, 2026-05-05) — `keyframes`
cleanup-only handler.** New module `crates/babel-plugin/src/keyframes/mod.rs`
(~330 LOC + 12 unit tests) plus `visit_mut_expr` override and
`Program::exit` drain wired in `babel_plugin.rs` (+4 end-to-end
visitor tests). The two-step pattern (queue at the visit site,
replace-with-null at exit) matches upstream `babel-plugin.ts:331-340`
+ `:222-238` and is reusable for §6.2 (css cleanup) / §6.3
(cssMap). All sibling integration gates green (compat_scope 3/3,
compat_evaluation 3/3, resolver_matrix 8/8). See §6.1 row in the
Phase 6 table for the full closure detail.

**Phase 6 §6.2 ☑ (this session, 2026-05-05) — `css` cleanup-only
handler.** Module `crates/babel-plugin/src/css/mod.rs` (~95 LOC
+ 6 unit tests) plus a 6-line `visit_mut_expr` extension. Reuses
the §6.1 drain verbatim — only new code is the css matcher.

**Phase 6 §6.3 ☑ (this session, 2026-05-05) — `cssMap` handler.**
First handler that emits real CSS and writes back into the AST.
Three new modules + a `visit_mut_var_declarator` dispatch hook:

- `crates/babel-plugin/src/utils/css_map.rs` (~270 LOC + 8 unit
  tests) — port of `utils/css-map.ts`: `ErrorMessages` enum
  (16 verbatim error phrasings), `create_error_message` formatter
  with the documentation-link suffix, the five literal-key /
  at-rule / extended-selectors helpers, and the
  `error_if_not_valid_object_property` predicate.
- `crates/babel-plugin/src/css_map/process_selectors.rs`
  (~370 LOC + 8 unit tests) — port of `process-selectors.ts`:
  `merge_extended_selectors_into_properties` collapses the
  `selectors:` shorthand and expands at-rule blocks
  (`@media: { 'screen ...': ... }` → `@media screen ...: ...`)
  with full duplicate detection.
- `crates/babel-plugin/src/css_map/mod.rs` (~430 LOC + 8 unit
  tests) — port of `css-map/index.ts` `visitCssMapPath`:
  validates shape (1 ObjectExpression argument, parent is a
  VariableDeclarator with Ident id), runs
  `merge_extended_selectors_into_properties` + `build_css` +
  `transform_css_items` for each variant, rejects classNames
  count > 1 and any `variables` (variants must be statically
  defined), emits the `(variantKey: className)` ObjectExpression,
  publishes `state.css_map[binding] = total_sheets` via the
  MutationRecorder (`StateDiff::CssMapInsert`, site 5).
- `crates/babel-plugin/src/babel_plugin.rs` —
  `visit_mut_var_declarator` hook detects `init = cssMap({...})`
  pre-descent (so the rewritten ObjectExpression is what
  children see, not the cssMap CallExpr). Tagged-template form
  panics with `NO_TAGGED_TEMPLATE`. Destructuring-pattern parent
  (`const { x } = cssMap(...)`) panics with `DEFINE_MAP`.

**SWC vs Babel divergence (documented in `process_selectors.rs`):**
SWC's `Ident` cannot hold spaces / parens, so the upstream
`t.identifier('@media screen and (min-width: 500px)')` becomes a
string-literal key (`PropName::Str`) in the Rust port. Bytes
through `build_css` are equal because consumers read the key via
`get_key_value`, which returns the same string for either Ident or Str.

**Late-resolve panic kept (Phase 6 §6.4 reachability gate):**
`utils/css_builders.rs::generate_cache_for_css_map` retains its
`unimplemented!()` panic, repurposed: porting that path properly
requires threading `&mut MutationRecorder` through the entire
`build_css` call graph (currently it terminates at the
`visit_mut_*` dispatchers). The §6.3 corpus — cssMap as
VarDeclarator init, consumers in source order AFTER the
declaration — does not reach this site. The threading lands with
§6.4 (xcss-prop), the first handler whose corpus exercises the
late-resolve scenario (member-expression consumer, e.g.
`<div xcss={styles.danger} />`, processed through `build_css`).

Lib tests: **359/359** (was 335 post-§6.2; +24: 8 css_map utility
+ 8 process_selectors + 8 visit_css_map_path unit tests). Sibling
gates clean: compat_scope 3/3, compat_evaluation 3/3,
compat_generator 4/4, resolver_matrix 8/8, transform_css 3/3,
hash_parity 4/4. WASI cdylib build clean.

**Phase 6 §6.4 ☑ (this session, 2026-05-05) — `xcss-prop` handler.**
First handler that consumes `state.css_map` published by §6.3.
New module `crates/babel-plugin/src/xcss_prop/mod.rs` (~470 LOC +
13 unit tests) plus a `visit_mut_jsx_element` post-order extension
in `babel_plugin.rs`. Two branches per upstream `visitXcssPropPath`:
(1) inline ObjectExpression — `staticObjectInvariant` via
`compat::evaluation::evaluate`, then `build_css` +
`transform_css_items`, switch on classNames count (1 → replace; 0
→ `undefined` Ident; else → error); (2) member expression — walks
the JSXAttribute value collecting `MemberExpression.object.Ident.sym`
names, aggregates `state.css_map[name]` sheets, bails on empty
(legacy runtime xcss). Both branches set `state.uses_xcss = true`
and wrap the parent JSXElement with `compiled_template`'s
`<CC>...</CC>` output. Post-order dispatch mirrors Babel's
`transformCache` short-circuit (the wrapper's synthesised children
are NOT re-walked because `n.visit_mut_children_with(self)` already
ran on the pre-replacement element).

**Late-resolve panic kept (now §6.5 reachability gate):** xcss-prop's
actual call sites do NOT reach `extract_member_expression` — the
inline-object branch's `build_css` runs against a static-confirmed
ObjectExpression with no MemberExpression children, and the
member-expression branch reads `state.cssMap` directly. The
`generate_cache_for_css_map` `unimplemented!()` panic in
`utils/css_builders.rs` is repurposed as the §6.5 (css-prop)
reachability gate. css-prop / styled run `build_css` against
user-supplied expressions that may contain member expressions
referencing cssMap-bound identifiers — that's the first real
reach. The MutationRecorder-threading work originally scoped to
§6.4 moves to §6.5.

**Drift detected in §6.3 (fixed in §6.4):**
`crates/babel-plugin/src/css_map/mod.rs` `tests` module was missing
`PropName` from its `swc_core::ecma::ast` import list. STATUS.md
claimed 359/359 lib tests pass but the test module did not compile
at HEAD without the fix. One-line import addition included in §6.4.
The §6.3 closure summary's lib-test count was correct in spirit
(the tests would have passed if they compiled); the missed import
was the gap.

Lib tests: **372/372** (was 359 post-§6.3; +13 xcss_prop unit). All
sibling gates clean: compat_evaluation 3/3, compat_scope 3/3,
compat_generator 3/3, resolver_matrix 8/8, transform_css 3/3,
hash_parity 4/4. WASI cdylib build clean.

**Phase 6 §6.5 ☑ (this session, 2026-05-05) — `css-prop` handler.**
1:1 port of `css-prop/index.ts` plus the **MutationRecorder
threading** through the entire `build_css` call graph
(`utils/css_builders.rs`: 12 fn signatures + 30 internal call
sites + 3 hash-site tests + 2 external callers). Real
`generate_cache_for_css_map` body landed — the §6.4 `unimplemented!()`
panic is gone. css-prop's member-expression case (e.g. `<div
css={styles.primary} />`) routes through `extract_member_expression`
→ `generate_cache_for_css_map` → `visit_css_map_path` cleanly.

**Comment-disable directive — §6.5 incomplete branch.**
`is_css_prop_disabled` upstream walks
`meta.state.file.ast.comments` filtered by line number; SWC's
plugin runtime exposes line lookup via a `SourceMap` proxy that
the visitor doesn't thread today. Stub returns `false` (transform
always runs). Fixtures with `@compiled-disable-line transform-css-prop`
WILL produce divergent output until the SourceMap-thread
follow-up. Documented in `crates/babel-plugin/src/utils/comments.rs`.

**Phase 6 §6.6 ☑ (this session, 2026-05-05) — `<ClassNames>`
handler.** 1:1 port of `class-names/index.ts` (~195 LOC upstream).
Render-prop pattern with two-pass sub-traversal via SWC `VisitMut`
impls (`CssCallReplacer`, `StyleRefReplacer`):
1. Replace `css({...})` / renamed `c({...})` / `props.css({...})` /
   tagged-template with `ax([classNames])`, accumulating sheets +
   variables.
2. Replace `style` Identifier and `<x>.style` MemberExpression
   references with the variables-built ObjectExpression (or
   `undefined` when no variables collected).

Final step: `pick_function_body(children)` → wrap with
`compiled_template`. Rename detection covers the common
`({ css, style })` and `({ css: c, style: s })` shapes via the
`RenameMap` built from the children-fn's first parameter.

Dispatch order in `visit_mut_jsx_element`: `<ClassNames>` runs
FIRST (replaces the entire element with the wrapper); xcss/css-prop
dispatch runs AFTER (no-op on the wrapper).

Lib tests: **387/387** (was 372 post-§6.4; +8 css_prop + 7
class_names unit tests). All sibling gates clean: compat_evaluation
3/3, compat_scope 3/3, compat_generator 3/3, resolver_matrix 8/8,
transform_css 3/3, hash_parity 4/4. WASI cdylib build clean.

**Next checkpoint: Phase 6 §6.7 — `styled` handler.** Largest
single fixture set per §6.8 exit gate. Includes `forwardRef`
wiring + `@emotion/is-prop-valid` table verbatim port
(`crates/babel-plugin/src/compat/is_prop_valid.rs` per upstream's
verbatim-table source). The styled handler is the last gating
piece before §4.8 (Phase 4 exit gate fixtures byte-clean) and
§6.8 (Phase 6 exit gate full harness green).

**§4.7 (Parcel wrapper) — out of scope.** Treated as a
downstream-host use case the bridge supports (single
`transformSync` per file with filename + resolver wired through
the SWC plugin context); not a deliverable in this repo.

**§4.8 exit gate — tail-ends on Phase 6a/b/c.** §4.8's
verification ("keyframes / css / cssMap fixtures byte-clean")
requires the 6a/6b/6c handler bodies. §4.8 closure date =
Phase 6c ship. Phase 6 handlers (`keyframes`, `css`, `cssMap`,
`styled`) are now structurally unblocked — every primitive they
need ships at a real path, no stub remains in their critical
path beyond the `generate_cache_for_css_map` `unimplemented!()`
that Phase 6 §6.5 (css-prop) — first handler whose corpus reaches
the late-resolve path through `build_css` — will replace.

**§5.4 / §5.5 / §5.6 unblock plan: see §5.0 entry-gate below.**
The (a)/(b) decision from the prior session is RESOLVED: option
(a), with Q1/Q2/Q3 architectural locks recorded in
`plugins/COMPAT_SCOPE_AUDIT.md`. The "1.5–3k LOC unknown" framing
is replaced with three bounded sub-checkpoints (§5.0a/b/c) totaling
~700–1100 LOC for the compat layer, plus the §5.4/§5.5/§5.6 file
ports already scoped at PLAN.md.

**§5.0a closed in the prior session. §5.0b closed in the prior
session. §5.0c closed THIS session — see §5.0c closure summary
below. The next concrete code checkpoint is §5.4 (`utils/resolve_binding.rs`),
NOT blocked any longer on the compat layer.**

**Architectural lock (recorded in `plugins/COMPAT_SCOPE_AUDIT.md`):**
- **Q1 — pre-index.** `Program::enter` builds binding map +
  parent-pointer map + reference-paths map. Read-only navigation
  during the visit pass; invalidate-on-replace is local. Matches
  §5.3's record-then-replay cache model.
- **Q2 — scoped `&mut Expr` for the IIFE site only.** The single
  `replaceWith` site (IIFE wrap in `traverseCallExpression`) gets
  `&mut Expr` passed down explicitly. The rest of
  `evaluate_expression` returns `Resolved` and stays read-only.
  Don't propagate `&mut Expr` through the whole evaluator.
- **Q3 — full line-by-line port of `path.evaluate()`.** No
  partial-port-by-corpus; the corpus is a few hundred fixtures,
  the consumer monorepo is 10M LOC, defer-by-hope is unacceptable.
  Evidenced-unreachable branches MAY emit
  `unimplemented!("…")` with citation back to
  `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` — bounded by
  evidence, not deferred by hope.

**Three sub-checkpoints (the §5.0 owner picks them up in order):**

| Sub-checkpoint | Owner deliverable | LOC est | Status |
|---|---|---|---|
| §5.0a | `crates/babel-plugin/src/compat/scope.rs` (+ `compat/globals.rs`) — pre-indexed scope tree (binding map + parent-pointer map + reference-paths map). 1:1 surface enumeration in `plugins/COMPAT_SCOPE_AUDIT.md`. | 250–350 | ☑ |
| §5.0b | `crates/babel-plugin/src/compat/path.rs` — `PathHandle` carrying `(node_kind, node_span, parent_kind, parent_span, scope, list_key)`. Predicate methods (`is_*Declaration`, `is_*Specifier`, `is_object_pattern`, `is_variable_declarator`, `is_referenced_identifier`, `is_pattern`, `is_function`, `is_expression`), `parent_path()`, `replace_expr()` (single-site IIFE wrap), `traverse_subtree()` delegating to `VisitMut`, `ensure_block()` for concise-arrow bodies, and the AST-mutating `scope_push()` (Finding 6) that unshifts a real `VarDecl` into the target `BlockStmt` and registers a binding via the new `ScopeIndex::register_synthetic_binding`. §5.0a's `scope_push_synthetic` retained as a binding-only thin wrapper for the §5.0a parity-gate fixture. | 250–350 (actual: ~960) | ☑ |
| §5.0c | `crates/babel-plugin/src/compat/evaluation.rs` — line-by-line port of `@babel/traverse@7.29.0/lib/path/evaluation.js` covering every reachable branch. The four unreachable branches (Flow type-cast, JSX-as-evaluable, SequenceExpression, TaggedTemplateExpression) emit `unimplemented!()` with citation. Bundled scope-shape extensions: `Binding::init_expr`, `ScopeIndex::parent_kind_of`. | 200–400 (actual: ~600 + 15 unit tests) | ☑ |

**§5.5/§5.6 implementer breadcrumb requirement** — when those
checkpoints open, every `get_binding()` / `get_own_binding()` call
site in `utils/traverse_expression/*.rs` and
`utils/evaluate_expression.rs` MUST carry a one-line comment:

```rust
// If a fixture surfaces lazy-crawl observability here, see
// plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
```

Grep-discoverable; prevents the exact "agent hits divergence,
re-derives eager-vs-lazy badly, patches around it" failure mode
CLAUDE.md forbids. Verified at PR time by greping for
`get_binding\|get_own_binding` in `crates/babel-plugin/src/utils/`
and confirming each call carries the breadcrumb (or sits inside
a helper whose enclosing function does).

**Parity corpora (regenerable, gitignored):**

- `parity-harness/compat-scope/{fixtures.json,oracle.mjs}` — 20
  entries across 6 query axes (binding-lookup-from-reference,
  path-predicate-via-binding, has-own-binding, scope-push-iife,
  generate-uid, list-key-arguments). Oracle self-consistency check
  enforces "expected = what Babel actually does" so a buggy fixture
  can't sneak into the corpus.
- `parity-harness/compat-evaluation/{fixtures.json,oracle.mjs}` — 45
  entries across 12 categories (literal, identifier-global, binary,
  binary-comparison, logical, unary, conditional, template,
  parenthesized, ts, deopt, mixed). Synthetic
  `const __evalTarget = (EXPR);` wrapper dodges the
  directive-prologue trap.
- Both oracles guard the AFM-pinned `@babel/traverse@7.29.0` +
  `@babel/parser@7.29.2` versions; pin drift fails fast rather
  than silently emitting bytes from a different Babel version.

**Rust gates state (post-§5.0c):**

- `crates/babel-plugin/tests/compat_scope_integration.rs` —
  **3/3 passing** (unchanged from §5.0a). The byte-parity gate
  `rust_compat_scope_matches_js_corpus` runs the 23-entry corpus
  green every `cargo test` invocation.
- `crates/babel-plugin/src/compat/path.rs` unit tests — **10/10
  passing** (§5.0b). Includes the audit-mandated "push then
  traverse, observe new VarDecl" round-trip
  (`scope_push_inserts_var_decl_into_arrow_body_visible_to_traverse`)
  that fails against the §5.0a stub and passes against the §5.0b
  real-deal `scope_push`.
- `crates/babel-plugin/src/compat/evaluation.rs` unit tests —
  **15/15 passing** (NEW this session). Single-fold cases
  (literal, unary, binary, ternary, template, paren, identifier
  fall-throughs).
- `crates/babel-plugin/tests/compat_evaluation_integration.rs` —
  **3/3 passing** (NEW this session): un-ignored
  `rust_compat_evaluation_matches_js_corpus` byte-parity gate
  green at 45/45 fixtures across all 12 categories.

**Pin contract added to `crates/PARITY_VERSIONS.md`:**
`@babel/traverse@7.29.0` (AFM-resolved 2026-05-04). Promoted to
top-level `package.json#devDependencies` AND kept in `overrides`
per the §4.2 lesson — bun's isolated dep layout silently bypasses
overrides for transitive deps unless top-level promotion happens.

**Coverage manifest added:**
`crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` enumerates the
four evidenced-unreachable branches (each with a quoted panic
message the §5.0c port emits) and the reachable-branch list the
port must cover. Maintained the same way
`COMPAT_GENERATOR_COVERAGE.md` is for §4.3.

**The §4.4 SHELL stubs in `css_builders.rs`
(`evaluate_expression_stub`, `resolve_binding_stub`,
`visit_css_map_path_stub`) REMAIN PANIC-ON-CALL** until
§5.4/§5.5/§5.6 land. The compat layer (§5.0a/b/c) is now
complete; §5.4 is unblocked. §4.6 visitor-dispatch wiring,
§4.8 phase exit gate, and Phase 6 handlers stay blocked on the
§5.4–§5.6 ports.

**Independently shippable** while §5.0 is in flight: §4.7 (Parcel
wrapper — single `transformSync` call, sidecar drains). Does not
depend on the evaluator.

**§5.4 entry-gate ☑ (this session, 2026-05-05).** The §0.11
RESOLVER_MATRIX.md Phase 0 deferral is closed by §5.4a:
`crates/babel-plugin/RESOLVER_MATRIX.md` (9-axis Layer-1
default-config coverage manifest + divergence-action protocol),
the JS oracle at `parity-harness/resolver-matrix/` (pin-guarded
against `enhanced-resolve@5.18.3` + `resolve@1.22.12`, 4 seed
fixtures across 4 of the 9 axes — grow-as-divergence-surfaces
per the §4.3 / §5.0c precedent), and the `#[ignore]`'d Rust gate
at `crates/babel-plugin/tests/resolver_matrix_integration.rs`
(2/3 green; byte-parity body un-ignored by §5.4b). Architecture
lock: `plugins/RESOLVER_SPEC_PART_TWO.md` is the canonical
declarative `resolver: { ... }` JSON schema (one generic engine
in the library, no Jira-specific code; consumers describe
behaviour via JSON). Real divergence already captured: axis-2
exports-string (enhanced-resolve honours `exports` → `entry.js`;
npm `resolve.sync` falls back to `main: main-fallback.js`).

**§5.4b ☑ (this session, 2026-05-05).** Engine + default-config
+ schema scaffold landed at
`crates/babel-plugin/src/resolver/{mod,config,default,engine}.rs`
(~600 LOC + 7 unit tests). `oxc_resolver = "11"` added as a
workspace dep with `default-features = false` (no pnp). The
byte-parity gate (`rust_resolver_matches_js_corpus`) is
un-ignored and green at 4/4 seed fixtures including the
exports-string axis where `enhanced-resolve` and `oxc_resolver`
agree (both honour `package.json#exports`) — production-oracle
match confirmed. WASM cdylib build clean.

**§5.4e ☑ (this session, 2026-05-05) — §5.4 ROW GROUP CLOSED.**
1:1 port of `resolve-binding.ts` (425 LOC → ~750 LOC Rust
including doc-comments + 5 unit tests) plus the bundled
`traversers/` subtree (5 files, ~360 LOC + 16 unit tests).
See the §5.4e closure summary below for the full architectural
delta.

**With §5.4e closed, §5.4 (resolver entirely) is shipped.**
§5.5 closure (the 11 resolve-binding-dependent files) and §5.6
(`evaluate_expression.rs` only — `traversers/` already bundled
in §5.4e) are unblocked.

**Phase 5 §5.6 ☑ (this session, 2026-05-05) — PHASE 5 ROW
GROUP CLOSED.** 1:1 port of `packages/babel-plugin/src/utils/evaluate-expression.ts`
(200 LOC) landed at `crates/babel-plugin/src/utils/evaluate_expression.rs`
(~600 LOC + 14 unit tests). All three §5.6 wiring contracts honoured:
(a) cross-file `ScopeIndex` synthesis at the recursive-fold boundary
via `imported_module: Arc<Module>`; (b) `meta.own_scope_override`
consumption in the dispatch closure; (c) namespace-import preflight
in the MemberExpression branch routing through
`evaluate_namespace_import_path`. Stale drift notes retired in
`traverse_identifier`, `resolve_expression::identifier`, and
`namespace_import` module docs. See the §5.6 closure summary below
for the full architectural delta.

**With §5.6 ☑, the §5.4–§5.6 row group is fully shipped.**
§4.4 SHELL stubs in `css_builders.rs` can now be deleted (the
real fns land at `crate::utils::resolve_binding::resolve_binding`,
`crate::utils::evaluate_expression::evaluate_expression`).
§4.6 visitor-dispatch wiring, §4.8 phase exit gate, and Phase 6
handlers are all now unblocked.

**Next checkpoint: Phase 4 §4.6 visitor-dispatch wiring** (the
deferred §4.6 closure tail). Wire the css-builders shell into the
SWC `babel_plugin.rs` visitor's import / JSX / TaggedTemplate
handlers. Phase 6 handlers (`css`, `cssMap`, `keyframes`,
`styled`) follow.

### Phase 4 §4.6 bridge closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/lib.rs::process` — extracts the absolute
  filename from `TransformPluginMetadataContextKind::Filename` (empty
  string when host omits the context — `state.set_filename` is then
  skipped, matching upstream's bail-on-missing-filename behaviour
  in `resolve_request`). Builds the default Compiled resolver via
  `resolver::build_default(opts.extensions.as_deref())`,
  `Arc`-wraps it, and calls `state.set_resolver`. Both injections
  happen BEFORE the visitor's `visit_mut_with` so the children walk
  sees them on every dispatch.

* `crates/babel-plugin/src/babel_plugin.rs` — `BabelPluginVisitor`
  gained `scope_index: Option<ScopeIndex>` and
  `program_scope: Option<ScopeId>` fields, populated in
  `visit_mut_program` via `ScopeIndex::build(&module)` BEFORE the
  children walk. Script programs (Compiled doesn't operate on
  classic scripts in practice) leave the fields as `None`. Phase 6
  handlers read these fields when dispatching into
  `evaluate_expression` / `resolve_binding`.

* `crates/babel-plugin/src/utils/css_builders.rs` — 13 fns threaded
  with `&mut ScopeIndex, parent_scope: ScopeId, own_scope: Option<ScopeId>`
  per the §5.5 explicit-param lock (`build_css`, `build_css_inner`,
  `extract_object_expression`, `extract_template_literal`,
  `extract_keyframes`, `extract_conditional_expression`,
  `extract_branch`, `extract_logical_expression`,
  `extract_member_expression`, `extract_member_expression_optional`,
  `extract_array`, `try_keyframes_branch`,
  `generate_cache_for_css_map`). Pure helpers
  (`merge_subsequent_unconditional_css_items`, `get_item_css`,
  `to_css_rule*`, `to_css_declaration*`, `set_item_css`,
  `find_binding_identifier`, `normalize_content_value`,
  `logical_op_to_swc`, `expression_span`, `is_custom_property_name`,
  `babel_node_type_name`,
  `get_variable_declarator_value_for_own_path`,
  `path_is_compiled_css_shape`, `assert_no_imported_css_variables`,
  `callback_if_file_included`, `get_logical_item_from_conditional_expression`)
  retain their original signatures — none reach a stub site or a
  threaded-fn caller.

**Stub deletions:**

* `evaluate_expression_stub` (was lines 95-105) — DELETED.
* `resolve_binding_stub` (was lines 107-118) — DELETED.
* `visit_css_map_path_stub` (was lines 120-126) — DELETED.

**Stub call site flips (6 of 7 to real fns):**

| Site (pre-bridge line) | Replaced with |
|---|---|
| `extract_branch` Ident branch | `evaluate_expression(path_node, meta, scope_index, parent_scope, own_scope)` — ResultPair discarded; surrounding re-dispatch is Phase 6 |
| `extract_logical_expression` arrow body | `evaluate_expression(...)` — ResultPair discarded; LogicalCssItem emission is Phase 6 |
| `extract_object_expression` Spread Ident | `resolve_binding(i.sym.as_str(), &*meta, &*scope_index, parent_scope, own_scope)` — discarded |
| `extract_object_expression` Spread expr | `evaluate_expression(...)` — discarded |
| `extract_member_expression_optional` fallback | `evaluate_expression(...)` — discarded; re-dispatch on folded value is Phase 6 |
| `extract_template_literal` logical sub-pass | `evaluate_expression(...)` — discarded |
| `build_css_inner` Ident branch | `resolve_binding(...)` — discarded; cssMap-collision check + recurse is Phase 6 |

**The 7th stub site (visitCssMapPath):** retained as an inline
phase-citing `unimplemented!()` inside `generate_cache_for_css_map`
because Phase 6 §6.3 is the source-of-truth fn — there is no real
fn to flip to. Phase 6 §6.3 deletes the panic when the css-map
handler ports `visitCssMapPath`.

**Surrounding-logic deferral contract:** every flipped site
discards the `ResultPair` / `PartialBindingWithMeta` it produces.
The discarding is intentional — the surrounding JS branches that
consume the resolver/evaluator output (recursion into
`build_css_inner` on a folded value, LogicalCssItem emission keyed
on a folded test, cssMap-collision check + nested resolution) are
Phase 6 per-API handler work. Each discard site carries a comment
naming the Phase 6 follow-up. No fixture currently reaches these
paths (the visitor dispatcher's `visit_mut_call_expr` /
`visit_mut_tagged_tpl` / `visit_mut_jsx_element` /
`visit_mut_jsx_opening_element` are still stub-log only — Phase 6
opens them).

**Test-shape adaptation:** the 3 §4.4 hash-site tests in
`css_builders.rs` (`hash_site_extract_keyframes_name_matches_oracle`,
`hash_site_extract_object_expression_variable_name`,
`hash_site_extract_template_literal_variable_name`) gained an
`empty_scope()` helper that builds a `ScopeIndex` from
`Module { body: vec![] }`. The hash-site tests exercise the
catch-all `--_${hash(...)}` path which doesn't touch binding
lookup, so an empty index is sufficient. Phase 6 handler tests
will build from a real Module containing the test expression.

**Doc cleanups paired with the bridge:**

* `crates/babel-plugin/src/utils/mod.rs` — module doc updated to
  reflect §4.6 bridge ☑ + the seven dispatch flip surfaces.
* `crates/babel-plugin/src/utils/object_property_to_string.rs` —
  the file's own `unimplemented!()` panics still gate on
  `evaluateExpression` for computed-key paths, but the gating
  citation now reads "until the per-API Phase 6 handler that
  surfaces the computed-key path threads `evaluate_expression` in
  here" rather than "until §4.6 / Phase 6". Bridge doesn't open
  this surface; the per-API handler that surfaces a computed-key
  fixture does.
* `crates/babel-plugin/src/utils/css_builders.rs:assert_no_imported_css_variables`
  — `dead_code` rationale updated from "§4.4 SHELL because every
  caller goes through resolveBinding stub" to "§4.6 bridge flips
  resolveBinding to the real fn but discards
  PartialBindingWithMeta — Phase 6 ports the consumer."

**Verification (all green):**

* `cargo test -p babel-plugin --lib`: 311/311.
* `cargo test -p babel-plugin --tests`: integration gates green —
  `compat_evaluation_integration` 3/3, `compat_scope_integration`
  3/3, `compat_generator_integration` 3/3,
  `transform_css_integration` 3/3, `hash_parity` 4/4,
  `resolver_matrix_integration` 8/8.
* `cargo build -p babel-plugin --target wasm32-wasip1 --release`:
  clean, zero babel-plugin warnings.
* Bun parity: `parity-harness/strip-runtime` 1132/1132,
  `parity-harness/babel-plugin` (BABEL_PLUGIN_FULL_PARITY +
  BABEL_PLUGIN_FULL_DETERMINISM) 954/954.

**Test count delta:** babel-plugin lib unchanged at 311 (existing
hash-site tests retro-fitted with `empty_scope()`; no new tests
added — bridge is wiring + signature plumbing, not new behaviour).

### Phase 5 §5.6 closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/utils/evaluate_expression.rs`
  (~600 LOC + 14 unit tests). 1:1 port of
  `packages/babel-plugin/src/utils/evaluate-expression.ts` (200 LOC).
  Public surface:
  - `pub fn evaluate_expression(expr, meta, scope_index,
    parent_scope, own_scope) -> ResultPair` — the top-level
    entry point. Threads scope info as explicit parameters
    per §5.5 closure convention; `meta.own_scope_override` is
    consumed at each dispatch invocation.
  - Internal `dispatch_evaluate` recursively dispatches to the
    six §5.5 leaf traversers via a closure that captures
    `*mut ScopeIndex` for self-referential local state. The
    SAFETY comment at module head enumerates the access
    discipline that makes this sound (no leaf accesses
    scope_index between invoking the closure and returning).
  - `babel_evaluate_expression` ports the `path.evaluate()`
    fallback through `crate::compat::evaluation::evaluate`
    (§5.0c). The JS try/catch maps to `EvaluatedValue::Deopt`
    — the Rust evaluator never panics on Babel-tolerable shapes.
  - `is_path_referencing_any_mutated_identifiers` /
    `is_identifier_references_mutated` port the mutation
    detector that gates the babel-fold path. Reads
    `Binding::binding_node_type` (§5.0a) and
    `Binding::reference_paths` (§5.0a) for each Identifier under
    the input expression.

* `crates/babel-plugin/src/utils/mod.rs` — `evaluate_expression`
  module registered.

* Three §5.5 leaf module docs retired their stale cross-file
  scope-swap drift notes:
  - `traverse_identifier.rs` — replaced "Drift potential — flagged
    not patched" block with the §5.6-wires-the-consumer note.
  - `traverse_member_expression/traverse_access_path/resolve_expression/identifier.rs`
    — same retirement.
  - `traverse_member_expression/traverse_access_path/evaluate_path/namespace_import.rs`
    — replaced "unreachable from standard dispatch" with
    "Caller wired at §5.6" describing the MemberExpression-entry
    preflight route.

**Architectural locks delivered:**

1. **Cross-file fold dispatched at the Identifier ENTRY of
   `dispatch_evaluate`, not in the leaf.** When `resolve_binding`
   returns `(source == Import, imported_module = Some, node =
   Some)`, the dispatcher builds a fresh `ScopeIndex` from the
   imported module's AST and recurses with `(imported_idx,
   imp_program_scope, None)`. Same-file folds and unresolved
   identifiers fall through to `traverse_identifier` with the
   caller's scope info — same shape as §5.5 closure. The leaf
   never sees a cross-file misroute.

2. **Namespace-import preflight at the MemberExpression ENTRY.**
   `try_namespace_import_dispatch` extracts `(binding_identifier,
   access_path)` via a local mirror of
   `traverse_member_expression::get_member_expression_meta` (kept
   local to avoid a public-surface bump on the §5.5 leaf), checks
   for namespace-import binding shape, and routes the FIRST
   access-path element through
   `evaluate_namespace_import_path(placeholder, imported_module,
   imported_idx, meta, first_path_name)`. Subsequent
   access-path elements continue against the imported scope via
   `traverse_member_access_path`. The §5.6 contract's "evaluate_path
   ImportNamespaceSpecifier branch unreachable" caveat is
   sidestepped — routing happens at the MEMBER-EXPRESSION ENTRY,
   not mid-chain.

3. **`own_scope_override` consumed at dispatch boundary, not in
   leaves.** `dispatch_evaluate` reads
   `meta.own_scope_override.or(own_scope)` at function entry and
   uses the result as `effective_own_scope` for both the
   cross-file detection AND the leaf calls. The closure that
   leaves invoke captures `effective_own_scope` by Copy — the
   recursive call thus sees the override resolved at the call
   site that established it (`traverse_call_expression`'s IIFE
   site).

4. **Raw-pointer-based dispatcher recursion (sound under leaf
   access discipline).** Detailed in module-level SAFETY comment.
   Avoids three rejected alternatives: `Rc<RefCell<ScopeIndex>>`
   (would require modifying §5.5 leaves AND panics on overlapping
   borrows when `traverse_call_expression`'s `borrow_mut()` is
   active during a closure body that needs to recurse);
   thread-local `Cell<*mut>` (same aliasing model, less call-site
   clarity); hand-inlining `traverse_call_expression`'s body
   (drift risk vs §5.5 leaf).

5. **Breadcrumb requirement honoured.** Every `get_binding` /
   `get_own_binding` call site in `evaluate_expression.rs`
   carries the `// If a fixture surfaces lazy-crawl observability
   here, see plugins/COMPAT_SCOPE_AUDIT.md Finding 7.` comment
   per §5.0c lock.

**Test count delta:**

- `babel-plugin --lib`: 297 → **311** (+14: 14 new
  `utils::evaluate_expression::tests`).
- All sibling integration gates unchanged:
  `compat_evaluation_integration` 3/3,
  `compat_scope_integration` 3/3,
  `compat_generator_integration` 3/3,
  `transform_css_integration` 3/3,
  `hash_parity` 4/4,
  `resolver_matrix_integration` 8/8.
- Bun parity harnesses unchanged: `strip-runtime` 1132/1132,
  `babel-plugin` (FULL_PARITY + FULL_DETERMINISM) 954/954.
- WASI cdylib build clean **with zero babel-plugin warnings**.

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib utils::evaluate_expression  # 14/14 (NEW)
RUSTFLAGS="" cargo test -p babel-plugin --lib                             # 311/311
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 3/3
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration       # 3/3
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration   # 3/3
RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration      # 3/3
RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity                    # 4/4
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration    # 8/8
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release     # clean
bun test parity-harness/strip-runtime/harness.test.ts                          # 1132/1132
BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 \
  bun test parity-harness/babel-plugin/harness.test.ts                        # 954/954
```

**Bug-parity flag retained from §5.5 closure (NOT patched):**
`traverse_call_expression` does NOT persist the IIFE wrap into
the AST (transient `ScopeId` instead of `replaceWith`). The §5.6
evaluator does NOT alter this design — fold output is byte-equal
to JS for the foldable path; if a fixture surfaces byte-divergence
on the deopt path's runtime-CSS-fallback emission, the fix is at
THE EVALUATOR BOUNDARY in `dispatch_evaluate` (decide which
expression flows to the runtime fallback). No such fixture is
known today across the 954-fixture babel-plugin corpus +
1132-fixture strip-runtime corpus.

**Deferred-by-evidence (handed to Phase 4 §4.6 / Phase 6
implementers):**

* `evaluate_expression_stub` / `resolve_binding_stub` /
  `visit_css_map_path_stub` panic stubs in `css_builders.rs`
  remain. The §5.6 ports the real `evaluate_expression`; Phase 4
  §4.6 wiring (or Phase 6's first handler) deletes the stubs and
  replaces the call sites with
  `crate::utils::evaluate_expression::evaluate_expression` /
  `crate::utils::resolve_binding::resolve_binding`.

* `resolve_binding_with_evaluator`'s `_evaluate_expression`
  parameter is still prefix-underscored. The §5.6 evaluator
  doesn't thread an evaluator INTO `resolve_binding` because
  destructuring-resolution paths aren't reached in the current
  unit-tested fixtures. When Phase 6 surfaces a fold-through-
  destructured-arg fixture, wire the underscore drop here.

* `setImportedCompiledImports` cross-file mixin tracking is still
  gated `let _ = set_imported_compiled_imports;` in
  `resolve_binding.rs`. The §5.6 evaluator passes `&mut Metadata`
  to leaves, so this side-effect is now reachable in principle —
  but no current fixture exercises it. Phase 6 handler that
  surfaces cross-file mixin tracking flips this to a real call.

### Phase 5 §5.4e closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/utils/resolve_binding.rs` (~750 LOC
  + 5 unit tests). 1:1 port of
  `packages/babel-plugin/src/utils/resolve-binding.ts` (425 LOC).
  Public surface:
  - `pub fn resolve_binding(reference_name, meta, scope_index,
    parent_scope, own_scope) -> Option<PartialBindingWithMeta>` —
    the §5.5/§5.6 entry point. Walks own-scope first, then
    parent-scope; for import bindings, calls into the resolver,
    parses the imported module, finds the matching export.
  - `pub fn resolve_binding_with_evaluator<EvalFn>(...,
    Option<&EvalFn>)` — the destructuring-resolution variant
    accepting an evaluator callback. §5.6 wires its real fn here.
  - `pub fn resolve_object_pattern_value_node<EvalFn>(...)` —
    the destructuring-source walker. Direct-object branch
    fully ported; member-on-member recursive evaluation deopts
    cleanly when no evaluator is wired (§5.6 reaches it).
  - `pub fn resolve_identifier_coming_from_destructuring` /
    `resolve_identifier_in_pattern` /
    `get_destructured_object_pattern_key` — destructuring
    helpers for member-access folding paths.

* `crates/babel-plugin/src/utils/traversers/{mod,get_export,object,set_imported_compiled_imports,types}.rs`
  (~360 LOC + 16 unit tests). 1:1 port of
  `packages/babel-plugin/src/utils/traversers/`. Bundled into
  §5.4e (originally §5.6 deliverable) because
  `resolve-binding.ts` has hard deps on `getDefaultExport`,
  `getNamedExport`, `setImportedCompiledImports`. STATUS.md §5.6
  updated to reflect the bundling — §5.6 now ships only
  `evaluate_expression.rs`.

* `crates/babel-plugin/src/compat/scope.rs` extended with the
  `ImportInfo` struct + `ImportSpecifierKind` enum + new
  `Binding::import_info: Option<ImportInfo>` field. Populated by
  `register_import` for every import-specifier binding it
  creates (default / named with optional alias / namespace).
  Mirrors §5.0c's `init_expr` extension precedent — single-
  purpose, gated population, no impact on non-import bindings.

* `crates/babel-plugin/src/state.rs` extended with two new
  `pub(crate)` fields + getters + setters:
  - `resolver: Option<Arc<Resolver>>` — the in-plugin module
    resolver. Visitor sets via `set_resolver` on
    `Program::enter` (when the dispatcher engages); tests set
    directly. Reads via `state.resolver()`.
  - `filename: Option<String>` — absolute path of the file
    being transformed. Visitor sets via `set_filename` from
    `swc_core::common::FileName::Real`.

* `crates/babel-plugin/src/utils/types.rs` —
  `PartialBindingWithMeta` redesigned per the §5.4e architecture
  lock:
  - Drops the `'a` lifetime that the §4.4 placeholder carried
    (`meta: Metadata<'a>` couldn't safely point at a different
    file's State than the caller's).
  - Drops the `path_id: u32` recorder-handle placeholder (no
    consumer dereferences it; `compat::path::PathHandle` is the
    §5.5/§5.6 surface for path-shaped data).
  - `node` is now `Option<Box<Expr>>` — `None` when the
    resolved binding's node isn't an `Expr` (declaration shape,
    namespace import). Caller deopts.
  - Adds `imported_filename: Option<String>` — absolute path
    of the imported module for `source == Import` resolutions;
    `None` for same-file resolutions.

* `crates/babel-plugin/src/resolver/engine.rs` — `Resolver` gets
  a manual `Debug` impl (oxc_resolver's types don't impl Debug;
  printing the `ResolverInner` variant name is sufficient for
  State's Debug output).

* `crates/babel-plugin/Cargo.toml` — `swc_core` features add
  `ecma_parser`. Was dev-only at §4.2 / §4.4 when the visitor
  only walked already-parsed Programs; the §5.4e port reads
  `package.json`-resolved file paths, loads bytes via
  `std::fs::read_to_string`, and runs `parse_file_as_module`
  to get an AST to walk for the matching export. Adds parser
  surface to the WASI plugin binary; size impact verified
  clean against the §5.4d WASM build.

* `crates/babel-plugin/src/utils/css_builders.rs` —
  `resolve_binding_stub` retained as
  `#[allow(dead_code)]` (lone in-tree caller is in a dead-code
  branch already). The §4.4 SHELL contract was "stubs panic
  until §5.4/§5.5/§5.6 land"; the §5.4e port ships the real fn
  at `crate::utils::resolve_binding::resolve_binding`. Phase 6
  rewires the call site; until then the stub stays as a marker
  with an updated panic message pointing at the real fn.

**Architectural locks delivered:**

1. **Cross-file Metadata forwarding via `imported_filename`.**
   The JS plugin's `resolveBinding` returns a Metadata pointing
   at the imported file (`{ ...meta, filename: modulePath, file:
   ast }`). The Rust port can't return a `Metadata<'a>` carrying
   a different file's State — the lifetime `'a` ties to the
   caller's State. Solution: drop Metadata from the return
   shape entirely; surface `imported_filename: Option<String>`
   instead. The §5.6 evaluator constructs whatever Metadata it
   needs at fold time.

2. **`Binding::import_info` mirrors §5.0c precedent.** The
   §5.0c implementer extended `Binding` with `init_expr` for
   the §5.4-evaluator's needs without §5.0a author involvement.
   §5.4e extends with `import_info` for the §5.4e cross-file
   resolver's needs. Single-purpose, gated population (only
   import bindings), zero overhead for other bindings.

3. **WASI-safe (no caching, no JS callbacks).** The JS
   `meta.state.cache.load(...)` infrastructure isn't replicated
   per the §5.4 caching lock — `fs::read_to_string` +
   `parse_file_as_module` run on every cross-file resolution.
   Single-transform performance is bounded (one parse per
   imported module per transform); SWC's WASI tear-down
   between transforms makes any cross-call cache unsound. The
   `meta.state.resolver.resolveSync(...)` JS callback path is
   replaced by direct `Resolver::resolve_sync` calls into the
   in-plugin `oxc_resolver`.

4. **Breadcrumb requirement honoured.** Every
   `get_binding` / `get_own_binding` call site in
   `utils/resolve_binding.rs` carries the `// If a fixture
   surfaces lazy-crawl observability here, see
   plugins/COMPAT_SCOPE_AUDIT.md Finding 7.` comment per §5.0c
   lock.

**Test count delta:**

- `babel-plugin --lib`: 246 → **270** (+24: 16 traversers
  unit tests across `get_export`/`object`/`set_imported_compiled_imports`
  + 5 resolve_binding unit tests + 3 implicit from binding
  field-extension impact on existing scope tests).
- `resolver_matrix_integration`: 8/8 (unchanged; regression
  canary green).
- All sibling gates unchanged: `compat_evaluation_integration`
  3/3, `compat_scope_integration` 3/3,
  `compat_generator_integration` 3/3, `transform_css_integration`
  3/3, `hash_parity` 4/4.
- WASI cdylib build clean **with zero babel-plugin warnings**.

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::               # 42/42 (unchanged)
RUSTFLAGS="" cargo test -p babel-plugin --lib utils::resolve_binding   # 7/7 (post-drift-fix; +1 cross-file imported_module gate)
RUSTFLAGS="" cargo test -p babel-plugin --lib utils::traversers        # 16/16 (NEW)
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 286/286 (270 post-§5.4e + 15 §5.5 closure + 1 §5.4e drift-fix)
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration  # 8/8 (canary)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release    # clean
```

**§5.4e drift-fix landed (post-§5.5-close, 2026-05-05) —
cross-file scope-swap parity:**

The §5.5 closure agent's drift report flagged a real fixture-class
divergence: the JS plugin's `resolveBinding` returns a Metadata
swapped to point at the imported file's parentPath/state, so the
§5.6 evaluator's recursive fold of the resolved node walks
identifiers AGAINST THE IMPORTED FILE'S SCOPE. The §5.4e shape
dropped that field (lifetime aliasing constraint), forcing
§5.5/§5.6 consumers to forward the caller's scope info instead.
Effect: imported literals fold correctly (the common path), but
**deep cross-file chains** (`export const a = b where b is
another binding in the imported file`) deopt where JS would
fold — a class-hash-affecting divergence at AFM theme-file scale
(`const PRIMARY = PRIMARY_RAW; export const colors = { primary:
PRIMARY };` is a real shape).

**Patch:** extend `PartialBindingWithMeta` with
`imported_module: Option<Arc<Module>>`; populate from
`resolve_binding.rs`'s locally-parsed module. The §5.6 evaluator
constructs a fresh `compat::scope::ScopeIndex::build(&*imported_module)`
at the recursive-fold boundary so identifier references in the
imported AST resolve against the imported file's scope (not the
caller's). The Arc means multiple recursive folds within a
single transform share the same parsed AST.

Why an Arc<Module> and not a re-parse from `imported_filename`:
parsing is the dominant cost. Doing it once in `resolve_binding`
and threading the Arc forward amortises across every fold inside
the imported file. The alternative (re-parse at the §5.6 fold
boundary) doubles the parse cost for every cross-file resolution.

**§5.6 consumer contract** (locked at this drift-fix):

```rust
// In §5.6 evaluator's recursive-fold boundary:
if matches!(resolved.source, BindingSource::Import) {
    if let Some(imported_module) = resolved.imported_module.as_ref() {
        let imported_scope = ScopeIndex::build(imported_module);
        // Walk `resolved.node` AGAINST `imported_scope`, not the
        // caller's scope. Recursive identifier resolution inside
        // the imported AST now finds bindings in the imported file.
        ...
    }
}
```

**New unit test gate:**
`utils::resolve_binding::tests::cross_file_import_carries_imported_module_arc`
synthesises a tempdir-fixture (consumer.ts importing colors from
./theme.ts), calls `resolve_binding`, asserts both `imported_filename`
AND `imported_module` are `Some`, and confirms the Arc carries
the parsed AST containing the expected `colors` export.

**§5.5 closure agent action:** the cross-file scope-swap drift
note in `traverse_identifier`/`evaluate_identifier` module docs
can be retired. `resolve_binding` now returns a forward-compatible
shape; the wiring lands at §5.6 (when the evaluator dispatches
into the resolved node).

**Deferred-by-evidence (handed to §5.5 closure / §5.6
implementers):**

* The `evaluate_expression` callback parameter on
  `resolve_binding_with_evaluator` is `_evaluate_expression`
  (prefixed underscore — unused inside the wrapper). The §5.6
  evaluator threads the real fn through to
  `resolve_object_pattern_value_node` which DOES consume it.
  The wrapper exists for §5.6's signature stability — when
  §5.6 wires its evaluator into `resolve_binding`'s
  destructuring-recursion path, the underscore prefix drops.

* `setImportedCompiledImports` is imported but the
  side-effect call inside the cross-file ImportSpecifier
  branch is gated `let _ = set_imported_compiled_imports;` —
  the JS plugin mutates `meta.state.importedCompiledImports`
  during cross-file resolution but the Rust port has only
  `&Metadata` (not `&mut Metadata`) at that call site. The
  §5.6 evaluator's wrapper passes `&mut Metadata` and can
  reach state — when §5.6 wires it, this `let _ =` line
  becomes a real call. Not a §5.4e gate concern (no fixture
  exercises cross-file mixin tracking yet).

* `cross_file_named_import_resolves_via_default_resolver` test
  is permissive — it asserts that IF the binding chain resolves
  to an Import, THEN the imported_filename points at the right
  pkg, but doesn't fail if the binding chain doesn't resolve at
  all. This is intentional: the §5.0a binding-builder's
  `register_import` populates `import_info` ONLY when the source
  contains the import — if a future binding-builder regression
  fails to populate, this test won't catch it. A stricter
  follow-up test is on the §5.5 closure agent (their fixtures
  will exercise full import-binding resolution end-to-end).

**With §5.4e ☑, the §5.4 row group is fully shipped:**

| Sub-checkpoint | Status |
|---|---|
| §5.4a entry-gate | ☑ |
| §5.4b engine | ☑ |
| §5.4c transforms | ☑ |
| §5.4d preferFirst | ☑ |
| **§5.4e resolve_binding.rs** | **☑** |

**Next checkpoint: §5.5 closure** — port the 11 remaining
files in `traverse_expression/`: `traverse_identifier`,
`traverse_call_expression`, and the entire
`traverse_member_expression/**` subtree (8 files including
`traverse-access-path/{evaluate-path,resolve-expression}/*`).
The closure agent calls
`crate::utils::resolve_binding::resolve_binding(name, meta,
scope_index, parent_scope, own_scope)` directly. Breadcrumb
requirement still binding.

### Phase 5 §5.4d closure summary (this session, 2026-05-05)

The `preferFirst`
dispatcher landed at `crates/babel-plugin/src/resolver/prefer_first.rs`
(~510 LOC + 12 unit tests). Architecture (option b — per-rule
pre-built resolvers): each rule clones base `ResolveOptions`,
overrides `exports.fields`/`main.fields` per the rule's `use_`,
owns one `ResolverGeneric<TransformingFileSystem>`. Prefixes
loaded once at config-load (inline arrays verbatim; `{fromFile}`
reads relative to the consumer config's directory; accepts both
bare-array `["@scope/x", ...]` and dev-tooling `{"prefixes": [...]}`
shapes from spec §3.6's generator). First-match-wins; non-matched
requests fall through to base. `build_from_config` signature
changed to `(cfg, config_dir) -> Result<Resolver, PreferFirstError>`
to support `fromFile`. Also wires `cfg.exports.fields` into the
base resolver's `ResolveOptions::exports_fields` (was
parses-but-not-honoured at §5.4c). End-to-end proven by axis-11
fixture: same package, three different resolved paths based on
config — no preferFirst → `main-entry.js`; matching prefix +
`use.exportsFields=["af:exports","exports"]` → `af-entry.js`;
non-matching prefix → falls through to base → `main-entry.js`.

**Next checkpoint: §5.4e (port `utils/resolve_binding.rs`).** 1:1
port of `packages/babel-plugin/src/utils/resolve-binding.ts`
(425 LOC). Wires through `resolver::Resolver` for the two
production resolution paths (`resolve.sync` fallback at :185-189
+ injected `resolveSync` at :191-193, both collapsing into
`Resolver::resolve_sync`). Breadcrumb requirement at every
`get_binding`/`get_own_binding` call site per §5.0c lock
(Finding 7, lazy-crawl observability). When §5.4e ships:
§4.4 SHELL `resolve_binding_stub` deleted; §5.5 closure (the
11 resolve-binding-dependent files: `traverse_identifier`,
`traverse_call_expression`, the entire `traverse_member_expression/**`
subtree) unblocks; §5.6 (`traversers/` + `evaluate_expression.rs`)
follows.

**§5.5 ☑ CLOSURE COMPLETE — claude-2026-05-05 (three-pass).**

*Pass 1 (parallel-with-§5.4):* three resolve-binding-INDEPENDENT
leaves landed alongside §5.4a/b/c/d/e — `traverse_binary_expression`,
`traverse_unary_expression`, `traverse_function` (verified by grep
to NOT reach `resolveBinding` / `meta.state.cache` /
`resolveRequest`) ported 1:1 with helper deps `create_result_pair`,
`has_numeric_value`. The JS-undefined fall-through path in
`traverse-function.ts` was modelled with `Option<Box<Expr>>` in
`ResultPair::value` rather than substituting an
`Expr::Ident("undefined")` sentinel (which would have flipped
`is_empty_value` semantics relative to JS).

*Pass 2 (post-§5.4e closure):* 9 leaves landed —
`traverse_identifier.rs` plus the entire
`traverse_member_expression/**` subtree (8 files including
`traverse_access_path/{evaluate_path,resolve_expression}/**`).
Lib-test count: 270 → 285 (+15). All cross-file scope info threaded
as explicit parameters per the §5.4e convention (Metadata stays
invariant — adding `&'idx ScopeIndex` would touch the entire
callgraph and isn't justified by §5.5 surface alone).

*Pass 3 (closure complete — §5.0d absorbed):* the two previously-
stubbed leaves now have real bodies. The §5.0d compat-checkpoint
scope was absorbed into the §5.5 closure agent's surface
(precedent: §5.4e absorbed `traversers/` originally scoped to §5.6).
- `compat::scope::ScopeIndex::register_new_scope` — runtime new-scope
  synthesis (~50 LOC + 4 unit tests). Same shape-extension
  precedent as §5.0c (`init_expr`) and §5.4e (`import_info`).
- `types::Metadata::own_scope_override: Option<u32>` — per-call
  own_scope override channel for §5.6's evaluator dispatcher to
  honour `traverse_call_expression`'s IIFE-recursion swap.
- `traverse_call_expression.rs` — real 1:1 port using
  `register_new_scope` for the IIFE arrow's transient ScopeId,
  `register_synthetic_binding` for `(param := evaluatedArg)`
  pairs, and the `own_scope_override` channel for the recursive
  evaluator call. NO AST mutation (the IIFE arrow lives only as
  a `ScopeId`, not in the transform-target tree).
- `namespace_import.rs` — real ~80 LOC port using
  `PartialBindingWithMeta::imported_module: Arc<Module>`
  (post-§5.4e drift-fix) + `register_synthetic_binding` for the
  'default' synthesis on a fresh imported `ScopeIndex`.

Lib-test count after Pass 3: 285 → 297 (+12 across the new
modules and Pass-3 leaves).

*Bug-parity flag (documented in `traverse_call_expression`
module docs):* JS Babel persists the IIFE wrap into the AST via
`replaceWith`; Rust uses transient ScopeId + `own_scope_override`.
May affect runtime-CSS-fallback emission on the deopt path. If
a fixture surfaces byte-divergence there, the fix is at §5.6's
evaluator boundary (decide which expression flows to the runtime
fallback), NOT in `traverse_call_expression`.

*Wiring deferred to §5.6:* `namespace_import.rs` body is real
and unit-tested but unreachable from the standard `evaluate_path`
dispatcher (SWC's `ImportNamespaceSpecifier` isn't an `Expr`).
The §5.6 evaluator's `evaluate_identifier` will detect
namespace-import resolutions (`source == Import &&
imported_module.is_some() && node.is_none()`) and route directly
to this leaf with the upcoming `pathName` from the access-path
chain — see `namespace_import.rs` module docs.

*Cross-file scope-swap divergence (§5.4e drift-fix patched the
shape; §5.6 wires the consumer):* the `traverse_identifier` /
`evaluate_identifier` recursive `evaluate_expression` call
forwards the CALLER's scope info, not the imported file's. The
§5.4e shape-fix added `PartialBindingWithMeta::imported_module:
Option<Arc<Module>>`; §5.6 builds a fresh `ScopeIndex` from it
at the recursive-fold boundary. The §5.5 leaves' module-docs
drift notes can be retired once §5.6 lands.

The §5.4 owner inherits two §5.0c-bundled scope-shape extensions
they should NOT re-derive:
1. `compat::scope::Binding::init_expr: Option<Box<Expr>>` —
   populated for `const x = <expr>` with `Pat::Ident` LHS only.
   `evaluation.js:122` short-circuits on
   `binding.constant === false`, so non-const bindings deopt
   before reaching the init recursion; the gate is decided once
   at index-build time.
2. `compat::scope::ScopeIndex::parent_kind_of(scope) -> Option<NodeKind>` —
   proxy for `scope.path.parentPath.isBlockStatement()` via the
   parent SCOPE's owner kind. Sufficient for the var-hoist-unsafe
   check at `evaluation.js:124-140`; if a future §5.5/§5.6 fixture
   surfaces a strict-AST-parent need, escalate (a span-keyed
   AST-parent side-table would be the next step).

The §5.5/§5.6 implementer breadcrumb requirement still stands.

Phase 4 §4.3 closure: 55/55 fixtures byte-exact, JSX printer landed
1:1 from `@babel/generator@7.23.0/lib/generators/jsx.js` (122 LOC →
~270 LOC Rust including doc-comments and the SWC↔Babel field-name
divergence table). The byte-parity gate in
`crates/babel-plugin/tests/compat_generator_integration.rs` no longer
skips ANY entries via `continue` — the JSX-key-attribute axis (5
fixtures) walks the parsed Module to find the first `key=` JSXAttr
and dispatches through the new `generate_jsx_attribute_with_comments`
entry point, mirroring the JS oracle's `extractJsxKeyAttribute`
extractor exactly.

§2.3(b) is a dangling sub-checkpoint (two deferred AST/comment-store
mutations from §2.3(a)) — NOT a phase gate; bundles with the first
§6.5 css-prop handler that needs the classic-pragma divergence.

### Verifying the current state from a cold pickup

```bash
# Plugin unit + integration tests.
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 286/286 (270 post-§5.4e + 15 §5.5 closure + 1 §5.4e drift-fix `cross_file_import_carries_imported_module_arc`)
RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity              # 4/4 over 10037 entries
RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration  # 3/3 over 120 entries
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration  # 3/3 (55/55 byte-exact, zero skips)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration       # 3/3 (post-§5.0a; un-ignored byte-parity gate green at 23/23)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 3/3 (post-§5.0c; un-ignored byte-parity gate green at 45/45)
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration    # 8/8 zero ignored (post-§5.4d; +3 axis-11 preferFirst E2E tests on top of §5.4c's 5)
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56
RUSTFLAGS="" cargo test -p compiled-utils --lib                         # 31/31
RUSTFLAGS="" cargo test -p compiled-css --lib                           # 163/163 (was 121; +42 from CSS-port agent's §4.4 unblock)

# Bun parity harnesses.
bun test parity-harness/strip-runtime/harness.test.ts                   # 1132/1132
BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 \
  bun test parity-harness/babel-plugin/harness.test.ts                  # 954/954

# CSS-port producer-side gate (uses bun, not node — see note below).
bun run packages/equality-harness/scripts/verify.mjs                    # 336/336

# Optional: regenerate the gitignored corpora before the cargo tests.
bun parity-harness/hash/oracle.mjs
bun parity-harness/transform-css/oracle.mjs
bun parity-harness/compat-generator/oracle.mjs                          # 55 entries
bun parity-harness/compat-scope/oracle.mjs                              # 20 entries (§5.0)
bun parity-harness/compat-evaluation/oracle.mjs                         # 45 entries (§5.0)
bun parity-harness/resolver-matrix/oracle.mjs                           # 4 entries (§5.4a entry-gate seed; §5.4b grows)
```

Total: **2747 tests, zero failures, zero ignored** post-§5.0c.
The §5.0a/b/c byte-parity gates (`compat_scope` 23/23,
`compat_evaluation` 45/45) are all un-ignored and green; the
shape-locks + oracle-self-consistency tests pass unconditionally.
+16 passing vs. §5.0b close (15 from `compat::evaluation` unit
tests, +1 from un-ignored `rust_compat_evaluation_matches_js_corpus`).
+37 passing vs. §5.0 entry-gate close cumulatively across
§5.0a + §5.0b + §5.0c.

### Phase 5 §5.4d closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/resolver/prefer_first.rs` (~510 LOC +
  12 unit tests). 1:1 port of RESOLVER_SPEC_PART_TWO.md §2.3.
  Public surface:
  - `pub fn load_prefixes(spec: &SpecifierStartsWith,
    config_dir: &Path) -> Result<Vec<String>, PreferFirstError>`
    — resolves `Inline(list)` verbatim or `FromFile { from_file }`
    relative to `config_dir`. JSON shape acceptance: bare array of
    strings OR `{"prefixes": [...]}` (the dev-tooling generator
    shape from spec §3.6). Per-entry type validation; clear error
    messages with the absolute attempted path on failure.
  - `pub struct PreferFirstDispatcher { rules: Vec<CompiledRule> }`
    — owns the compiled rule list. Built via
    `PreferFirstDispatcher::build(rules, base_opts, transforms_arc, config_dir)`
    which compiles each rule into a `(prefixes, ResolverGeneric<TransformingFileSystem>)`
    pair. The transforms list is a shared `Arc<[..]>` so N+1
    resolvers (base + N rules) share one allocation, not N+1.
  - `pub fn match_request(&self, request: &str)
    -> Option<&ResolverGeneric<TransformingFileSystem>>` —
    walks compiled rules in array order; first prefix-match wins.
    O(N×M) where N = rules, M = avg prefixes/rule. At AFM scale
    (~1,585 prefixes in localPlatformPackages.json, typically 1
    rule) this is fine; if profiling later surfaces hotness, swap
    in a trie / sorted-prefix binary search.
  - `pub enum PreferFirstError { FromFileIo {..}, FromFileShape {..} }` —
    config-load errors with absolute paths + spec pointers.
  - `fn build_rule_options(base: &ResolveOptions, use_: &PreferFirstUse)
    -> ResolveOptions` — applies `use_`'s overrides on top of base.
    Per spec §2.3: `Some(list)` overrides (including `Some([])` for
    "no exports/main walks" — the source-resolver case from
    RESOLVER_SPEC.md §3.2's three-resolver design); `None` keeps
    base. Schema's top-level field-name strings are wrapped as
    single-element paths for oxc_resolver's `Vec<Vec<String>>`
    shape.

* `crates/babel-plugin/src/resolver/engine.rs` extended:
  - New `ResolverInner::PreferFirst { base, dispatcher }` variant.
    `resolve_sync` walks: `dispatcher.match_request(request)` →
    matched-rule resolver if Some, base resolver if None.
  - New `Resolver::from_prefer_first(base, dispatcher)` constructor.
  - `TransformingFileSystem::with_transforms_arc(Arc<[..]>)` —
    shared-Arc constructor for the N+1-resolver case.
  - `build_from_config` signature changed:
    `(cfg, config_dir) -> Result<Resolver, PreferFirstError>`.
    Three-way branch: preferFirst-active → `PreferFirst` variant;
    transforms-only → `Transforming` variant; neither → stock
    `Default` variant. `cfg.exports.fields` now wired into base
    `ResolveOptions::exports_fields` (was parses-but-not-honoured
    at §5.4c).

* `parity-harness/resolver-matrix/fixtures-source/axis-11-prefer-first/match-by-prefix/` —
  on-disk fixture: `@matched/pkg-with-af-exports` package with
  both `main: "main-entry.js"` and `af:exports: "./af-entry.js"`.
  Used by the §5.4d E2E gate below.

* `crates/babel-plugin/tests/resolver_matrix_integration.rs`
  extended with three §5.4d E2E tests:
  - `axis_11_no_prefer_first_uses_main` — baseline. Default-config
    resolver doesn't know about `af:exports` (default
    `exports_fields = [["exports"]]`), falls back to `main` →
    `main-entry.js`.
  - `axis_11_matched_prefix_routes_to_af_exports` — preferFirst
    rule matches `@matched/`; rule resolver overrides
    `exports_fields` to `[["af:exports"], ["exports"]]`; resolution
    walks `af:exports` first → `af-entry.js`.
  - `axis_11_unmatched_prefix_falls_through_to_base` — preferFirst
    rule with non-matching `@nomatch/` prefix; dispatcher returns
    None; resolution falls through to base (default exports.fields)
    → `main-entry.js`. **Three different resolved paths from the
    same package via the same consumer code, distinguishable only
    by the dispatcher's behaviour** — the byte-parity proof.

**Architectural locks delivered:**

1. **Per-rule pre-built resolvers (option b).** Each
   `PreferFirstRule` owns a `ResolverGeneric<TransformingFileSystem>`
   constructed once at `build_from_config` time. AFM-scale
   (1,585 prefixes × thousands of imports) doesn't trigger
   per-request resolver reconstruction. Trade-off: O(N+1)
   resolver allocations per consumer config; acceptable since N is
   small (typically 1-2).

2. **Shared transform list across all resolvers.** Base + per-rule
   resolvers all share the same `Arc<[PackageJsonTransform]>`;
   `TransformingFileSystem::with_transforms_arc` cheap-clones the
   pointer. One allocation per `build_from_config`, not N+1.

3. **`fromFile` paths resolved relative to consumer config.**
   `build_from_config` now takes `config_dir: &Path` — the
   directory containing `.compiledcssrc`. The §5.4e implementer
   threads this through from the SWC plugin's config-load site.
   Absolute `fromFile` paths are honoured directly (skipping the
   `config_dir.join` step) for forward-compat with consumers who
   want explicit path control.

4. **Spec §2.3 override semantics: replace, not merge.** When a
   rule fires, its `use.exportsFields`/`use.mainFields` REPLACE
   the base config's values. `Some([])` is a meaningful override
   (the "source resolver" case — no exports/main walks).
   `build_rule_options` mirrors this exactly:
   `Some(list)` → set; `None` → keep base.

**Test count delta:**
- `babel-plugin --lib`: 234 → **246** (+12: 6
  `load_prefixes` tests + 3 `build_rule_options` tests + 3
  `dispatcher` tests).
- `resolver_matrix_integration`: 5/5 → **8/8** (+3 axis-11
  E2E).
- All sibling gates unchanged: `compat_evaluation_integration`
  3/3, `compat_scope_integration` 3/3,
  `compat_generator_integration` 3/3, `transform_css_integration`
  3/3, `hash_parity` 4/4.
- WASI cdylib build clean **with zero babel-plugin warnings**
  (cleared two §5.4d-introduced dead-code warnings:
  `TransformingFileSystem::new_with_transforms` collapsed into
  `with_transforms_arc`; `PreferFirstDispatcher::is_empty` gated
  `#[cfg(test)]`).

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::prefer_first::  # 12/12 (the new module)
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::                # 42/42 (config + transforms + prefer_first + engine)
RUSTFLAGS="" cargo test -p babel-plugin --lib                           # 246/246
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration  # 8/8 (zero ignored)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release    # clean
```

**Deferred-by-evidence (handed to §5.4e implementers):**

* `cfg.exports.conditions` still parses but isn't yet wired into
  `oxc_resolver::ResolveOptions::condition_names`. The §5.4d
  corpus doesn't exercise non-default conditions (the Jira shape
  uses `conditions: ["exports"]` which is equivalent to
  oxc_resolver's empty default for the
  `exports_fields = [["exports"]]` configuration). When the first
  fixture exercising `conditions: ["import"]` or
  `conditions: ["browser"]` lands, wire it in `build_from_config`'s
  `ResolveOptions` literal.

* `cfg.contexts` + `cfg.default_context` + `cfg.extra_main_fields`
  still parse but are not yet wired. Per-context dispatch
  (`browser`/`node`) is independent of preferFirst architecturally
  (orthogonal in spec §2.1); when a fixture surfaces a per-context
  resolution requirement, add a `Per-context` variant to
  `ResolverInner` similar to the §5.4d `PreferFirst` variant.
  `extra_main_fields` is a generic extension hook with no
  current consumer.

* `prefer_first` rules' `is_empty` method is `#[cfg(test)]`-only;
  if a future caller needs to inspect dispatcher-emptiness at
  runtime, drop the cfg gate. The lib-side `prefer_first_active`
  check in `build_from_config` operates on the unparsed
  `Vec<PreferFirstRule>` before the dispatcher is even built —
  preventing the empty-dispatcher allocation entirely.

**Next checkpoint: §5.4e** (port `utils/resolve_binding.rs`).
1:1 port of `packages/babel-plugin/src/utils/resolve-binding.ts`
(425 LOC). The two production resolution paths in JS
(`resolve.sync` fallback at :185-189; injected `resolveSync` at
:191-193) collapse into one Rust call: `resolver.resolve_sync(from_file, request)`.
The §5.4e implementer:

1. Threads `resolver::Resolver` through `state.resolver` (or an
   equivalent on `Metadata`) so `resolve-binding.ts:194`'s
   `resolveRequest(...)` site has it.
2. Ports the rest of the file 1:1 — the destructuring-resolution
   helpers (`resolveIdentifierComingFromDestructuring`,
   `resolveObjectPatternValueNode`, `getDestructuredObjectPatternKey`),
   the `getBinding` synthesis for re-export shapes, and the
   `resolveBinding` entry point.
3. Adds the breadcrumb comment at every `get_binding` /
   `get_own_binding` call site per §5.0c Finding 7.
4. Deletes `evaluate_expression_stub` / `resolve_binding_stub` /
   `visit_css_map_path_stub` from `css_builders.rs` (§4.4 SHELL
   panics). The §4.4 SHELL contract was "stubs panic until
   §5.4/§5.5/§5.6 land"; §5.4e is the §5.4 closure.
5. Updates STATUS.md §5.4e to ☑ + Resume here pointer to the
   §5.5 closure agent (the 11 remaining files in
   `traverse_expression/`).

Once §5.4e lands, the entire §5.4 row group goes ☑ and §5.5/§5.6
unblock fully.

### Phase 5 §5.4c closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/resolver/transforms.rs` (~330 LOC + 22
  unit tests). 1:1 port of RESOLVER_SPEC_PART_TWO.md §2.2's 5
  operations. Public surface:
  - `pub fn apply_transforms(pkg: &mut serde_json::Value, &[PackageJsonTransform])` —
    runs each op against `pkg` in array order. No-op when `pkg`
    is not an object (defensive — malformed `package.json` passes
    through to `oxc_resolver`'s parser, which surfaces the parse
    error upstream).
  - Per-op semantics:
    - `EnsureObject { key }` — `pkg[key] = {}` if missing or
      non-object. Existing object preserved.
    - `RenameKey { from, to, ifTargetMissing, wrap }` — copies
      `pkg[from]` into `pkg[to]` (does NOT delete the source —
      consumers chain `DeleteKey` if they want a move, matching
      the spec §2.4 shape). With `wrap = { as: "object", key: K }`
      the source value is wrapped as `{ K: <source> }`. With
      `ifTargetMissing: true` the op skips when `pkg[to]` already
      exists.
    - `RenameMapEntry { in, from, to, ifTargetMissing, deleteSource }` —
      inside `pkg[in]` (must be an object; no-op otherwise).
      `shift_remove` preserves remaining-key order when
      `delete_source` is true.
    - `SetDefault { in, entries }` — creates `pkg[in]` as `{}` if
      absent; never overwrites existing keys (`entry().or_insert()`
      semantics).
    - `DeleteKey { key }` — `shift_remove` to preserve sibling-key
      order (JSON serialization sensitive; downstream `exports`
      walks may depend on it).
  - Composed-Jira-sequence tests verify the spec §2.4 transform
    chain across three input shapes:
    `jira_shape_atlaskit_src_only_promoted_to_af_exports` (the
    edge case where step 1's `ensureObject` causes step 3's
    `renameKey ifTargetMissing: true` to skip — outcome documented
    inline as the spec-faithful behaviour),
    `jira_shape_root_slash_only_promoted_to_dot` (the
    `renameMapEntry "./"` → `"."` path), and
    `jira_shape_already_modern_unchanged` (no-op pass-through
    for an already-canonical shape).

* `crates/babel-plugin/src/resolver/engine.rs` extended (~250 LOC
  added on top of §5.4b's ~80):
  - `pub struct TransformingFileSystem { inner: FileSystemOs,
    transforms: Arc<[PackageJsonTransform]> }` — wraps oxc_resolver's
    stock `FileSystemOs` and intercepts `read()` calls. When the
    target path's `file_name() == "package.json"` AND the bytes
    parse as a JSON object, [`apply_transforms`] runs and the
    re-serialized bytes are returned. `read_to_string`
    (tsconfig.json's path) passes through verbatim — out of scope
    for §5.4c per spec §2.2 wording. Other FS methods (metadata,
    symlink_metadata, read_link, canonicalize) delegate.
  - `enum ResolverInner { Default(DefaultResolver),
    Transforming(ResolverGeneric<TransformingFileSystem>) }` — the
    `Resolver` struct now holds either backing variant. Zero
    overhead for default-config (§5.4b path); only configs with a
    non-empty `package_json_transforms` array build the
    transforming variant.
  - `build_from_config` now branches on `cfg.package_json_transforms`:
    empty → stock `DefaultResolver` (§5.4b behaviour); non-empty →
    `ResolverGeneric::new_with_file_system(TransformingFileSystem,
    opts)`.

* `parity-harness/resolver-matrix/fixtures-source/axis-10-package-json-transforms/delete-exports/` —
  new on-disk fixture: a package with both `main: "main-entry.js"`
  and `exports: "./exports-entry.js"`. Used by the §5.4c E2E
  tests below. `axis-10-` prefix added per
  `crates/babel-plugin/RESOLVER_MATRIX.md` axis enumeration; an
  axis-10 row may eventually be promoted into RESOLVER_MATRIX.md's
  9-axis table when more transform-driven fixtures land.

* `crates/babel-plugin/tests/resolver_matrix_integration.rs` extended
  with the §5.4c E2E gate:
  - `axis_10_no_transform_resolves_via_exports` (baseline: stock
    default-config resolver honours `exports`, lands at
    `exports-entry.js`).
  - `axis_10_delete_exports_transform_falls_back_to_main` (E2E:
    config with `[{op: deleteKey, key: "exports"}]` produces a
    `Resolver` whose `TransformingFileSystem` strips the `exports`
    field from the bytes oxc_resolver consumes; resolution falls
    back to `main-entry.js`). **The two tests' contrasting
    resolved paths are the proof that the FS interception
    layer is doing real work** — if the wrapper accidentally
    bypassed `read()` or cached the raw bytes outside itself, the
    second test would land at `exports-entry.js` and fire.

**Architectural locks delivered:**

1. **WASI-safe transform application.** No on-disk mutation; the
   transform runs at the `read()` call site inside
   `TransformingFileSystem`. WASI tear-down between transforms is
   irrelevant — the wrapper's transform list lives on the
   resolver instance, not on disk.
2. **Spec §2.2 wording verbatim.** "Operations are applied in
   array order, after reading and before exports resolution" —
   the read-intercept architecture matches every clause: array
   order (`for op in transforms`), after reading (`let raw =
   self.inner.read(path)?`), before exports resolution (the
   transformed bytes feed oxc_resolver's exports-walking layer
   above).
3. **No new ops.** Library is Jira-agnostic; new consumer-side
   quirks become new transform sequences applied at the consumer's
   `.compiledcssrc`, not new ops in the engine. The 5 ops are the
   complete library surface.
4. **Coalesce-by-zero-overhead.** `build_from_config` returns a
   stock `DefaultResolver` when `package_json_transforms` is
   `None` or empty — no `ResolverGeneric` wrapper instantiated,
   no FS-layer indirection. Only configs that actually use
   transforms pay the (per-instance, per-package.json-read) cost.

**Test count delta:**
- `babel-plugin --lib`: 211 → **234** (+23: 22 new
  `resolver::transforms::tests` + 1 engine-wiring round-trip
  `build_from_config_with_transforms_doesnt_break_default_resolution`).
- `resolver_matrix_integration`: 3/3 → **5/5** (+2: axis-10
  baseline + axis-10 transform E2E).
- All sibling gates unchanged: `compat_evaluation_integration`
  3/3, `compat_scope_integration` 3/3,
  `compat_generator_integration` 3/3, `transform_css_integration`
  3/3, `hash_parity` 4/4.
- WASI cdylib build clean.

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::transforms::    # 22/22 (the new module)
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::                # 30/30 (config + transforms + engine)
RUSTFLAGS="" cargo test -p babel-plugin --lib                           # 234/234
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration  # 5/5 (zero ignored)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release    # clean
```

**Deferred-by-evidence (handed to §5.4d/e implementers):**

* `cfg.exports.fields` and `cfg.exports.conditions` still parse
  but are not yet wired into `oxc_resolver::ResolveOptions`.
  Honouring them is a one-line addition to `build_from_config`'s
  `ResolveOptions` literal — but the §5.4c gate doesn't need it
  (the axis-10 fixture uses default `exports_fields: [["exports"]]`),
  and wiring it now without a corpus exercising non-default
  exports.fields is defer-by-hope. §5.4d (the preferFirst
  dispatcher, which CAN override exports.fields per-request) is
  the natural surface for this; the corpus growth happens there.

* `cfg.contexts` + `cfg.default_context` + `cfg.extra_main_fields` —
  same shape: parse cleanly, no engine wiring yet. `contexts`
  per-request dispatch is the §5.4d surface. `extra_main_fields`
  is a generic extension hook with no current consumer.

* The §5.4c E2E test uses a `deleteKey "exports"` transform — the
  simplest demonstration that the FS-interception layer is doing
  real work (different bytes → different resolved path). A future
  fixture exercising the full Jira `af:exports`/`atlaskit:src`
  promotion chain would require ALSO honouring
  `cfg.exports.fields` so oxc_resolver probes `af:exports` before
  `exports`. That fixture lands alongside the §5.4d preferFirst
  port (since the Jira shape uses preferFirst to override
  `exportsFields` per matched-specifier — see
  RESOLVER_SPEC_PART_TWO.md §2.4).

**Next checkpoint: §5.4d** (port `resolver/prefer_first.rs`). The
match-by-prefix dispatcher per RESOLVER_SPEC_PART_TWO.md §2.3.
Match shapes:
- Inline `["@af/foo", "@atlaskit/bar"]` list (parses today).
- `{"fromFile": "./platform-packages.json"}` indirection (parses
  today; §5.4d adds the load-once-at-init JSON read).

When a request specifier matches a prefix, the resolver re-builds
its `ResolveOptions` with the rule's `use.exportsFields` and
`use.mainFields` overrides for that single resolution. Approach
options for §5.4d: (a) per-request `ResolverGeneric` instantiation
(simple but slow if matches are common), (b) eager pre-built
per-rule resolvers indexed by prefix (faster, more memory),
(c) one resolver-per-context with prefix-walk dispatch (matches
RESOLVER_SPEC.md §3.2's three-resolver design — but that spec is
the older Jira-typed shape; PART_TWO is library-agnostic). The
§5.4d implementer picks one — probably (b) at corpus-emergent
scale.

### Phase 5 §5.4b closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/src/resolver/mod.rs` — public surface
  (`Resolver`, `ResolverConfig`, `ResolverConfigError`,
  `build_default`, `build_from_config`, re-exported
  `oxc_resolver::ResolveError`). Doc-block cites `plugins/PLAN.md`
  §1 constraints 1+2 for the constraint-4 (1:1 file mapping)
  exception — there is no JS analogue to port; the resolver lives
  in the host's `createDefaultResolver` wrapper today and moves
  *into* the plugin per the WASI architecture.
* `crates/babel-plugin/src/resolver/config.rs` — declarative JSON
  schema (~330 LOC + 7 unit tests). 1:1 with
  `plugins/RESOLVER_SPEC_PART_TWO.md` §2.1: `extensions`,
  `exports.{fields,conditions}`, `contexts.<name>.mainFields`,
  `defaultContext`, `packageJsonTransforms[]` (the 5-op enum:
  `ensureObject`/`renameKey`/`renameMapEntry`/`setDefault`/`deleteKey`),
  `preferFirst[]` (with `match.specifierStartsWith` Inline-OR-fromFile
  untagged enum), `extraMainFields`. Every struct carries
  `#[serde(deny_unknown_fields)]` so consumer typos fail fast at
  config-parse — caught at AFM-scale by the
  `parse_unknown_field_rejected` test. Top-level `resolver` value
  parsed via custom `ResolverConfig::parse_value(&Value)` →
  `Result<Option<Self>, ResolverConfigError>`:
  - `Null` → `Ok(None)` (caller falls back to `build_default`).
  - `String` → `Err(Unsupported)` with the spec-pointing message
    `"resolver must be a JSON object — strings/functions are
    unsupported in the WASI plugin. See
    plugins/RESOLVER_SPEC_PART_TWO.md for the JSON shape."`.
  - `Array` / `Number` / `Bool` → `Err(Unsupported)` with the
    same message + the actual value kind.
  - `Object` → parsed via serde, deny-unknown-fields enforced.
* `crates/babel-plugin/src/resolver/default.rs` — the no-config
  factory (~80 LOC). `build_default(extensions: Option<&[String]>)`
  mirrors `createDefaultResolver(config)` with empty `config.resolve`:
  `oxc_resolver::ResolveOptions { extensions, ..Default::default() }`.
  When `extensions` is `None`, falls back to
  `crate::constants::DEFAULT_CODE_EXTENSIONS` —
  matching `resolve-binding.ts:299`'s
  `meta.state.opts.extensions ?? DEFAULT_CODE_EXTENSIONS` semantics.
  Doc-block cites the two intentional non-replications:
  `CachedInputFileSystem(fs, 4000)` (unsound under WASI tear-down
  per PLAN.md §3.9.4) and `useSyncFileSystemCalls: true`
  (oxc_resolver is sync by default — no async surface to opt out
  of).
* `crates/babel-plugin/src/resolver/engine.rs` — runtime
  resolver wrapper (~80 LOC). `Resolver::resolve_sync(from_file,
  request)` mirrors the JS host's
  `resolver.resolveSync({}, dirname(context), request)` shape:
  uses `from_file.parent()` as the resolution root; returns
  `oxc_resolver::Resolution::full_path()` on success; bubbles
  `oxc_resolver::ResolveError` on failure. `build_from_config`
  honours `extensions` today; `exports`/`contexts`/`defaultContext`/
  `packageJsonTransforms`/`preferFirst`/`extraMainFields` parse but
  are NOT yet honoured by the engine (§5.4c/d wiring) — documented
  inline at the unhonoured-field sites in `config.rs` so future
  implementers don't re-derive the deferral.
* `crates/babel-plugin/Cargo.toml` — `oxc_resolver = { workspace
  = true }` added. Workspace pin in `crates/Cargo.toml`:
  `oxc_resolver = { version = "11", default-features = false }`
  (no `pnp` / `yarn_pnp` / `codspeed` features — keeps the WASI
  binary lean per CLAUDE.md "WASI/WASM Compilation: don't add a
  10MB Rust library").
* `crates/babel-plugin/src/lib.rs` — `pub mod resolver;` added
  alongside `compat`/`utils`/`constants`/etc.
* `crates/babel-plugin/tests/resolver_matrix_integration.rs` —
  `rust_resolver_matches_js_corpus` un-ignored, body wired to
  `build_default(extensions).resolve_sync(from_file, request)`.
  Per-fixture diff format prints fixture label + axis +
  expected-vs-actual paths + spec-pointer to the divergence-action
  protocol on any mismatch; coarse error-class match on `Err`
  fixtures (precise error-class match deferred to a future
  fixture if a real divergence surfaces).

**Architectural locks delivered**:

1. **Module location:** `crates/babel-plugin/src/resolver/` —
   top-level `src/` module, NOT under `compat/`. Doc-block cites
   the constraint-4 exception inline at the head of `mod.rs`.
2. **Schema strictness:** `deny_unknown_fields` on every struct;
   `parse_value` rejects strings/functions/arrays/numbers/bools
   at the wrapper level with a hard-fail message pointing at the
   spec.
3. **Default-config baseline:** `build_default` produces an
   `oxc_resolver` configured ONLY with `extensions`, inheriting
   bare defaults for everything else. The `parity-harness/resolver-matrix/`
   corpus confirms this matches `createDefaultResolver(config)`
   with `config.resolve = {}` byte-for-byte across the 4 seed
   axes.
4. **WASI-safe:** no caching layer; resolver re-instantiated on
   `Program::enter` (when §5.4e wires it through `state.resolver`);
   `oxc_resolver`'s per-instance package.json caching during a
   single transform is the only cache surface.

**Real divergence handling — exports-string axis-2 (the riskiest
fixture):**

Both `enhanced-resolve@5.18.3` and `oxc_resolver@11.19.1` honour
`package.json#exports` by default. The seed corpus's exports-string
fixture resolves to `entry.js` under both — production-oracle
match confirmed. The npm `resolve.sync@1.22.12` divergence (falls
back to `main: main-fallback.js`) is captured for diagnostic-diff
visibility but the gate matches against `enhanced-resolve` only,
correctly.

**Rust gates state (post-§5.4b):**

- `crates/babel-plugin/tests/resolver_matrix_integration.rs` —
  **3/3 passing, zero ignored** (`corpus_shape_lock` +
  `corpus_observed_matches_expected_oracle_self_consistency` +
  `rust_resolver_matches_js_corpus` over 4/4 fixtures).
- `crates/babel-plugin --lib` — **211 passing** (was 204; +7
  from `resolver::config::tests` covering the parse-paths above
  plus the full-Jira-shape and inline-prefer-first round-trips).
- All sibling gates unchanged: `compat_evaluation_integration`
  3/3, `compat_scope_integration` 3/3,
  `compat_generator_integration` 3/3, `transform_css_integration`
  3/3, `hash_parity` 4/4.
- WASI cdylib build clean (`cargo build -p babel-plugin --target
  wasm32-wasip1 --release`).

**Deferred-by-evidence (handed to §5.4c/d/e implementers):**

* `engine.rs::build_from_config` ignores `package_json_transforms`,
  `prefer_first`, `contexts`, `default_context`, `extra_main_fields`,
  `exports.fields`, `exports.conditions`. **Schema parses;
  behaviour deferred.** §5.4c wires `package_json_transforms` into
  the engine's package.json read pipeline. §5.4d wires
  `prefer_first` + the `contexts.<name>.main_fields` per-request
  dispatch. `extra_main_fields` lands when a real consumer needs
  it. None of these affect the §5.4b gate (the seed corpus is
  default-config only).
* `Resolver::resolve_sync` returns the `oxc_resolver`
  `Resolution::full_path()` directly. The JS host's
  `resolveSync(context, request)` returns `string`; the Rust
  surface returns `PathBuf`. Conversion to `String` happens at
  the §5.4e call site (`resolve_binding.rs`), with platform-path
  normalisation handled there per the corpus's forward-slash
  convention.
* `ResolverConfig::package_json_transforms` accepts arrays of any
  length; §5.4c may want to validate that no two `op` entries
  produce conflicting keys (e.g. `renameKey from=X` followed by
  `deleteKey key=X`). Adding such validation here would be
  premature — the spec defines order semantics ("operations are
  applied in array order, after reading and before exports
  resolution") and conflict detection is implementation-time
  behaviour, not parse-time validation. Document this decision in
  §5.4c's closure summary if it becomes a maintenance concern.

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib resolver::                    # 7/7 (config::tests)
RUSTFLAGS="" cargo test -p babel-plugin --lib                               # 211/211
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration  # 3/3 zero ignored
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 3/3 (regression canary)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration       # 3/3 (regression canary)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release   # clean
```

**Next checkpoint: §5.4c** (port the 5-op `packageJsonTransforms`
engine in `crates/babel-plugin/src/resolver/transforms.rs`). The
schema already parses every op (caught by
`parse_full_jira_shape_succeeds`); §5.4c implements the
`apply_transforms(&mut serde_json::Value, &[PackageJsonTransform])`
function, integrates it into the engine's package.json read
pipeline, and adds a corpus axis exercising the composed Jira-
specific transform sequence (verifying the `af:exports` /
`atlaskit:src` mutation chain produces the same final
`package.json` shape as `@jira-dev/compiled-resolver`'s
`AtlassianSourcesPlugin` — corpus generation requires AFM-side
JSON snapshots OR oxc_resolver's `FileSystemOs` override hook,
implementer's choice).

### Phase 5 §5.4a closure summary (this session, 2026-05-05)

**Outputs landed:**

* `crates/babel-plugin/RESOLVER_MATRIX.md` — 9-axis Layer-1
  default-config coverage manifest (`package.json#main`,
  `package.json#exports` + conditions, `tsconfig` paths,
  symlink realpath, browser-field, extension order, directory
  index, scoped packages, deep imports + `node_modules` walk)
  + divergence-action protocol (match | shim | escalate, same
  three-option shape COMPAT_EVALUATION_COVERAGE.md uses)
  + layered-corpus scope statement (Layer-1 here; transforms
  / preferFirst / `resolve_binding.rs` are §5.4c/d/e corpora).
  Cites `plugins/RESOLVER_SPEC_PART_TWO.md` as the canonical
  declarative `resolver: { ... }` JSON schema for §5.4b+.
* `parity-harness/resolver-matrix/` — pin-guarded JS oracle
  workspace mirroring the `compat-scope` / `compat-evaluation`
  layouts:
  - `README.md` — run instructions + add-a-fixture protocol.
  - `oracle.mjs` — runs each fixture through both
    `enhanced-resolve@5.18.3` (production oracle) AND npm
    `resolve@1.22.12` (the `resolve-binding.ts:185-189`
    fallback path), captures resolved-path-or-error per fixture,
    self-consistency-checks against `expected`, emits a
    sorted-by-axis corpus to
    `crates/babel-plugin/tests/resolver_matrix_corpus.json`
    (gitignored, regenerable). Pin guards at top of file fail
    fast on version drift.
  - `fixtures.json` — checked-in declarative manifest. 4 seed
    fixtures spanning 4 axes (package.json-main,
    package.json-exports-conditions, extension-order,
    directory-index). The §5.4b implementer grows the corpus
    per the divergence-action protocol; entry-gate-floor of 4
    locked in `corpus_shape_lock`.
  - `fixtures-source/` — checked-in real npm-package skeletons
    (small `package.json` + source files + `node_modules/<dep>`
    layouts) backing each fixture. Necessary because resolver
    parity depends on filesystem reality, not just AST shape.
* `crates/babel-plugin/tests/resolver_matrix_integration.rs` —
  3 tests mirroring the `compat_evaluation_integration` /
  `compat_scope_integration` shape:
  - `corpus_shape_lock` — runs unconditionally; asserts schema
    version + pin freshness + entry-count floor + per-axis
    coverage. **GREEN.**
  - `corpus_observed_matches_expected_oracle_self_consistency` —
    runs unconditionally; catches stale corpora. **GREEN.**
  - `rust_resolver_matches_js_corpus` — `#[ignore]`'d at
    §5.4a entry-gate. The §5.4b implementer un-ignores once
    `crates/babel-plugin/src/resolver/` exists.
* `crates/PARITY_VERSIONS.md` — new "enhanced-resolve + resolve"
  section pinning `enhanced-resolve@5.18.3` (provisional pending
  AFM verification at §5.4b review) and `resolve@1.22.12`. Both
  promoted to top-level `devDependencies` AND `overrides` per
  the §4.2 lesson.
* `package.json` — devDeps + overrides updated; `bun install`
  resolves cleanly (1 new top-level dep: enhanced-resolve@5.18.3).
* `.gitignore` — `crates/babel-plugin/tests/resolver_matrix_corpus.json`
  added per the convention used for compat_{scope,evaluation,generator}_corpus.json.

**Architectural locks** (recorded in this session, mirror the
§5.0c Q1/Q2/Q3 shape):

1. **Module location** — `crates/babel-plugin/src/resolver/`
   (top-level `src/` module, NOT under `compat/`). The resolver
   isn't a Babel-API shim — it's the in-plugin replacement for
   the host's `createDefaultResolver` per `plugins/PLAN.md` §1
   constraint 2. PLAN.md constraint 4 (1:1 file mapping) is
   explicitly excepted at this site, with the citation inline
   in the new module's doc-comment.

2. **JSON schema strictness** — `resolver: { ... }` rejects
   unknown fields at config-parse time with a hard error
   pointing at `plugins/RESOLVER_SPEC_PART_TWO.md`. Catches
   typos before they become byte divergence at AFM scale.

3. **Default-config baseline** — when `.compiledcssrc` has no
   `resolver` key, the plugin matches `createDefaultResolver(config)`
   with `config.resolve = {}`. That means: extensions from
   `config.extensions` (else `DEFAULT_CODE_EXTENSIONS`),
   enhanced-resolve's bare defaults for everything else, **no
   caching** (the `CachedInputFileSystem(fs, 4000)` wrapper is
   intentionally NOT replicated — WASI tears down the instance
   between `transformSync` calls per PLAN.md §3.9.4, so any
   cross-call cache is unsound; oxc_resolver's per-instance
   in-memory caching during a single transform is sufficient).

4. **String/function `resolver` REJECTED** — PLAN.md §1
   constraint 1 already disallows them (no JS callbacks from
   the WASI plugin). Hard-fail at config-parse with the message
   `"resolver must be a JSON object — strings/functions are
   unsupported in the WASI plugin. See plugins/RESOLVER_SPEC_PART_TWO.md
   for the JSON shape."` Not silent-fallback to defaults.

**Rust gates state (post-§5.4a):**

- `crates/babel-plugin/tests/resolver_matrix_integration.rs` —
  **2/3 + 1 ignored** (`corpus_shape_lock`,
  `corpus_observed_matches_expected_oracle_self_consistency`
  green; `rust_resolver_matches_js_corpus` ignored per spec).
- All sibling gates unchanged: `compat_evaluation_integration` 3/3,
  `compat_scope_integration` 3/3, `compat_generator_integration`
  3/3, `transform_css_integration` 3/3, `hash_parity` 4/4.

**Real divergence captured at entry-gate**: axis-2 exports-string.
`enhanced-resolve` honours `package.json#exports` and resolves to
`entry.js`; npm `resolve.sync@1.22.12` ignores the `exports`
field and falls back to `main: main-fallback.js`. The §5.4b
Rust port MUST match `enhancedResolve` (the production oracle),
NOT `npmResolve` — the `npmResolve` column is captured for
diagnostic-diff visibility only.

**Verification (cold pickup):**

```bash
bun install                                                                  # idempotent; resolves enhanced-resolve@5.18.3
bun parity-harness/resolver-matrix/oracle.mjs                                # 4 entries; pin guard green
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration   # 2 passed, 1 ignored
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration # 3/3 (regression canary)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration      # 3/3 (regression canary)
```

**Next checkpoint: §5.4b** (port the resolver engine —
`crates/babel-plugin/src/resolver/{mod,config,default,engine}.rs`
mirroring `createDefaultResolver` empty-config defaults, parsing
the RESOLVER_SPEC_PART_TWO.md §2.1 schema, dispatching through
`oxc_resolver`). The §5.4b implementer un-ignores
`rust_resolver_matches_js_corpus` once the engine is wired.

### Phase 5 §5.0c closure summary (this session)

**Outputs landed:**

* `crates/babel-plugin/src/compat/evaluation.rs` (~600 LOC + 15
  unit tests + JS-semantic helpers) — line-by-line port of
  `@babel/traverse@7.29.0/lib/path/evaluation.js` (373 LOC)
  per the Q3 lock. Public surface:
  - `pub enum EvaluatedValue { Confident(Value), Deopt }`
  - `pub enum Value { Undefined, Null, Bool, Number, String,
    Array, Object }` — folded value type. Encoding maps to the
    corpus contract (`oracle.mjs::valueKind` / `valueString`).
  - `pub fn evaluate(expr: &Expr, index: &ScopeIndex, scope:
    ScopeId) -> EvaluatedValue` — the entry point. Read-only by
    design (Q2 lock); recurses on `&Expr` children rather than
    threading `PathHandle`. Identifier branch passes `(name,
    scope)` to `index.get_binding` directly.
  - Cycle detection via pointer-identity (`expr as *const Expr
    as usize`) HashSet — span.lo cannot be used because SWC
    parsers assign parent BinExpr and its first child the same
    `span.lo`, which would false-positive on every binary
    expression.
  - JS-semantic helpers: `to_js_string`, `to_js_number`,
    `truthy`, `is_nullish`, `js_lt`, `js_loose_eq`,
    `js_strict_eq`, `js_to_int32`, `js_to_uint32`,
    `js_number_to_string`, `js_string_to_number`,
    `typeof_string`. All inline-documented to evaluation.js
    line numbers.
* **Reachable branches ported (1:1 with evaluation.js)**:
  - Literal (string/number/bool/null) — :64-69
  - TemplateLiteral via `evaluate_quasis` — :70-72, :345-357
  - ConditionalExpression — :85-93
  - ExpressionWrapper (`Expr::Paren`) — :94-96. `Expr::TsAs`
    is NOT an ExpressionWrapper in Babel and falls through to
    deopt, matching the `ts-as-expression-deopts` corpus
    fixture.
  - MemberExpression on string-literal receiver — :97-116.
    Numeric-index access + `length` property fold; other
    accesses fall through to deopt.
  - ReferencedIdentifier — :117-168. Globals fast-path for
    `undefined`/`Infinity`/`NaN`. Reads §5.0c-added
    `binding.init_expr` for the recursive init evaluation
    branch.
  - UnaryExpression — :170-194. `void`/`typeof`/`!`/`+`/`-`/`~`.
    `typeof` on Function/Class folds to `"function"` per
    :177-179. `delete` deopts.
  - ArrayExpression — :195-208. Spread elements deopt.
  - ObjectExpression — :209-241. Spread/method/getter/setter
    deopt. Key resolution covers Identifier, StringLiteral,
    NumericLiteral, BigInt, and computed-key folds.
  - LogicalExpression (`&&` / `||` / `??`) — :242-263. Mirrors
    Babel's `leftConfident`/`rightConfident` interleaving for
    short-circuit semantics exactly.
  - BinaryExpression — :264-311. All arithmetic/comparison/bitwise
    operators including `**`, `|`/`&`/`^`/`<<`/`>>`/`>>>`. `in` /
    `instanceof` deopt (Babel doesn't fold them either).
  - CallExpression — :312-342. The full
    `Math.x`/`String`/`Number`/`isFinite`/`parseInt`/etc.
    sub-shape dispatch is NOT ported; corpus's only call-shape
    fixture (`someFn()`) deopts via the `:343` final fallback,
    which matches Babel's behaviour. If a future Compiled CSS-value
    fixture surfaces a foldable Math/String/Number call, port
    the sub-shape — left as a `// TODO` with citation in the
    branch.
* **Unreachable branches** — emit `unimplemented!()` with citation
  to `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`:
  - `Expr::Seq` (SequenceExpression) — :60-63
  - `Expr::TaggedTpl` — :73-84
  - JSX-as-evaluable — never enters `_evaluate` because JSX
    nodes don't reach this surface from Compiled.
  - Flow `TypeCastExpression` — Compiled parser config doesn't
    enable Flow; `Expr::TsAs` is the TS variant which deopts
    (correct Babel behaviour).

**Bundled scope-shape extensions** (the §5.4 owner inherits these,
should NOT re-derive):

1. `compat::scope::Binding::init_expr: Option<Box<Expr>>` —
   populated at `register_var_declarator` for `Pat::Ident` LHS
   where `kind == Const`. The gate is decided once at
   index-build time per Finding 1 (stored-bool reasoning); a
   non-const binding deopts before reaching the init recursion
   per `evaluation.js:122`'s `binding.constant` short-circuit, so
   populating only Const matches Babel's reach without over-
   cloning. Cost: one `Box<Expr>` per qualifying binding, freed
   when `ScopeIndex` drops. Also wired in
   `compat::path::scope_push` (the §5.5 IIFE site) — same gate.
2. `compat::scope::ScopeIndex::parent_kind_of(scope) -> Option<NodeKind>` —
   maps the parent SCOPE's owner kind (via `kind_of(parent_of(scope))`)
   to the equivalent `NodeKind`. Proxy for Babel's
   `scope.path.parentPath.node.type`; sufficient for the
   var-hoist-unsafe-block check at `evaluation.js:124-140`.
   New `NodeKind` variants added:
   `ForStatement`/`ForInStatement`/`ForOfStatement`/`CatchClause`/`SwitchStatement`
   (with `type_str()` round-trips). The `scope_kind_to_node_kind`
   helper at the bottom of `compat/scope.rs` documents the
   mapping inline (Function → FunctionDeclaration; Method → Other
   — collapsed for the only consumer's needs).

**Deferred-by-evidence (per Q3 concession)**:

* `evaluation.js:124-140` var-hoist-unsafe-block check —
  `compat::evaluation` deopts conservatively when
  `binding.kind == Var` rather than walking the
  `bindingPathScope.parent.parent.…` chain. The §5.0c parity
  corpus has no `var` fixtures (CSS-value position uses const).
  If a future fixture surfaces a var-in-block-hoisted-to-fn
  scenario, port the walk using the new `parent_kind_of`. The
  conservative deopt matches Babel's intent (var hoisted past a
  Block boundary IS the unsafe case).
* `evaluation.js:312-342` CallExpression sub-shape dispatch
  (Math/String/Number/isFinite/parseInt/etc.) — not ported;
  corpus only exercises `someFn()` which Babel deopts on (no
  global match). Compiled's CSS-value `Math.PI` etc. flow
  through `traverseMemberExpression`, not this surface. If a
  future fixture surfaces a foldable call, port the relevant
  sub-shape in-block.
* `evaluation.js:120-123` `path.node.start < binding.path.node.end`
  TDZ-shadow guard half — the §5.0a `Binding` exposes the
  binding's span but the recursive `Expr` doesn't carry start/end
  byte positions through the recursion at the granularity needed.
  Under-deopt vs Babel; corpus exercises no TDZ-shadow shapes. If
  a future fixture surfaces one, thread the ident's span into the
  Identifier branch and add the start/end check.
* `binding.hasValue` / `binding.value` (`evaluation.js:141-143`) —
  set by Babel only via `setValue`/`clearValue`/`deoptValue` which
  Compiled doesn't reach. Audit Section "Findings deferred"
  documented this as out-of-scope; §5.0c respects.

**Workspace test count delta:**
* `babel-plugin --lib`: 165 → **180** (+15: full
  `compat::evaluation::tests` module covering string/numeric
  literal folds, addition, string-concat, paren-binary, template,
  unbound-deopt, undefined/NaN globals, ts-as-expression deopt,
  call deopt, typeof, void, conditional, nullish-coalesce-zero).
* `compat_evaluation_integration`: 2 + 1 ignored → **3 passing**
  (the `rust_compat_evaluation_matches_js_corpus` byte-parity gate
  un-ignored and green at 45/45 fixtures).
* All other gates unchanged at their §5.0b numbers.
* WASI cdylib still builds clean
  (`RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1
  --release`).

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 180/180
RUSTFLAGS="" cargo test -p babel-plugin --lib compat::evaluation       # 15/15 (the new module)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 3/3 (45-entry corpus byte-clean)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration  # 3/3 (regression canary)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release  # clean
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56 (cross-crate canary)
```

**Architectural answers (Q1/Q2/Q3) recorded for §5.0c**:

* Q1 (Identifier→init recursive evaluation): option (a) —
  `Binding::init_expr` extension, gated on `Const + Pat::Ident`.
  Cost is bounded; defer-by-deopt would silently fail
  `const x = 1; const y = x + 1` shapes that the §5.5 corpus
  WILL hit.
* Q2 (var-hoist-unsafe-block check): option (a) —
  `parent_kind_of` extension. Mis-folding here produces wrong
  CSS class hashes; conservative deopt suffices for the
  corpus but the proxy is precise enough that escalation is
  cheap.
* Q3 (`PathHandle::get(field)`): confirmed unused in §5.0c. The
  evaluator is `&Expr`-shaped throughout; `PathHandle` enters
  only at §5.4–§5.6 entry boundaries.

**Next checkpoint: §5.4** (port `utils/resolve_binding.rs`). The
compat layer is complete; §5.4/§5.5/§5.6 are the remaining
file-for-file ports.

### Phase 5 §5.0b closure summary (prior session)

**Outputs landed:**

* `crates/babel-plugin/src/compat/path.rs` (~960 LOC including
  doc-comments + 10 unit tests) — Babel `NodePath` analog narrowed
  to the read-only navigation surface §5.4–§5.6 actually needs
  plus the single-site mutation paths the IIFE flow requires.
  Implements:
  - **`NodeKind` + `PathHandle`.** Borrow-free, `Copy`. Predicate
    fan-out covers every `path.is*()` call in
    `plugins/COMPAT_SCOPE_AUDIT.md`'s NodePath operations table:
    `is_import_declaration`, `is_import_specifier`,
    `is_import_default_specifier`, `is_import_namespace_specifier`,
    `is_export_named_declaration`, `is_object_pattern`,
    `is_variable_declarator`, `is_referenced_identifier`,
    `is_pattern`, `is_function`, `is_expression`,
    `is_arrow_function_expression`, `is_call_expression`,
    `is_member_expression`, `is_block_statement`, `is_program`.
    `is_referenced_identifier` excludes binding positions
    (VariableDeclarator.id, ImportSpecifier.local,
    ObjectPattern/ArrayPattern slots) per the §5.0a integration
    test's RefFinder rules.
  - **`PathHandle::parent_path()`.** Synthesises a parent handle
    from cached `parent_kind`/`parent_span` slots. No grandparent
    chain — matches the §5.4 surface; if a future port reaches
    `parentPath.parentPath`, factor a deeper context model.
  - **`PathHandle::from_binding(&Binding)`.** Convenience for
    `binding.path` access — round-trips through
    `NodeKind::type_str()` ↔ `from_type_str()` lossless for
    tracked variants.
  - **`replace_expr(&mut Expr, Expr)`.** Single-site Q2-locked
    mutation. The IIFE wrap call site is the sole production
    caller; `*target = replacement;` is the whole semantic.
  - **`ensure_block(&mut BlockStmtOrExpr)`.** 1:1 port of
    `path/conversion.js:68-102`. Wraps a concise-arrow expression
    body in `{ return <expr>; }` so subsequent `scope_push` has a
    `BlockStmt` to unshift into.
  - **`traverse_subtree(&mut N, &mut V)`.** Thin alias over
    SWC's `VisitMutWith::visit_mut_with`. Exists as a breadcrumb
    so call sites grep for `traverse_subtree` the same way
    upstream greps for `path.traverse(`.
  - **`scope_push(&mut ScopeIndex, ScopeId, PushOpts, &mut BlockStmt)`.**
    AST-mutating port of `scope/index.js:717-756`. Computes
    `dataKey = "declaration:{kind}:{block_hoist}"`, coalesces
    same-kind pushes into one `VariableDeclaration` (matches
    Babel's `unshiftContainer` + `dataKey` reuse), and registers
    the new declarator's binding in the scope index via the
    new `register_synthetic_binding`. **Replaces §5.0a's
    `scope_push_synthetic` binding-only stub for production
    callers.** Pattern-walk / switch-walk / loop-walk redirects
    in `:717-726` are intentionally NOT replicated — the IIFE
    site always passes the arrow's body BlockStmt directly; if
    a future call site needs a different shape, factor the
    redirect at the call site, not in `scope_push`.
  - **`synthesize_iife_arrow_with_empty_block(span)`.** Helper
    for the §5.5 IIFE site to construct the `(() => {})()`
    scratchpad that `scope_push` injects bindings into.

* `crates/babel-plugin/src/compat/scope.rs` —
  `register_synthetic_binding(&mut self, scope, name, Binding)`
  extracted as a `pub` helper used by `compat::path::scope_push`
  for the binding-table half of the production push. The original
  `scope_push_synthetic(scope, name, kind, init_string, span)`
  retained as a thin convenience wrapper that constructs a
  binding and delegates — the §5.0a parity-gate fixture
  `scope-push-iife-injects-const-binding` continues to call it
  unchanged. Doc updated to advertise the binding-only
  contract and to direct production callers to
  `compat::path::scope_push`.

* `crates/babel-plugin/src/compat/mod.rs` — `pub mod path;` added.

**The audit-mandated round-trip test** (per
`plugins/COMPAT_SCOPE_AUDIT.md` §5.0b SPEC LOCK):
`scope_push_inserts_var_decl_into_arrow_body_visible_to_traverse`.
Build a synthetic arrow with empty body → call `scope_push` →
assert (1) body has a `VarDecl`, (2) a `VisitMut` walk over
the body sees it as ordinary AST, (3) the binding is registered
in the scope index. **Fails against the §5.0a stub** (which
leaves `body.stmts` empty); **passes against the §5.0b
real-deal**. If it ever passes against the stub, the test is
wrong; if it ever fails against the real-deal, the AST-mutation
contract is broken.

**Coalescing tests:**
* `scope_push_coalesces_same_kind_into_one_var_decl` — three
  consecutive `Const` pushes against the same block produce one
  `VariableDeclaration` with three declarators, matching Babel's
  `dataKey`-driven `unshiftContainer` reuse.
* `scope_push_unique_opts_out_of_coalescing` — `unique: true`
  bypasses the reuse and produces separate VariableDeclarations,
  matching `scope/index.js:746`'s `!unique` short-circuit.

**Signature divergences from upstream** (each documented
inline at the divergence site):

* `scope_push` takes `&mut BlockStmt` directly rather than a
  `NodePath` whose `unshiftContainer` walks containers and
  re-binds via `_context.setup`. Justification: SWC's `VisitMut`
  borrow model precludes a path-object surface; the IIFE site
  always constructs the target arrow itself, so it has direct
  `&mut` access to the body. Audit Q2 lock — single-site
  mutation rights, don't propagate through the call graph.
* The `setData(dataKey, declarPath)` data side-table at
  `scope/index.js:751` isn't replicated; we re-detect the
  matching block by inspecting `target_block.stmts[0]` on each
  call. Sufficient for the IIFE site's single-pass usage; if a
  call site needs cross-call deduping across non-adjacent
  pushes, escalate (the data side-table lives on the path, not
  the scope, so wiring it in is more invasive than it looks).
* Pattern-walk / switch-walk / loop-walk redirects at
  `scope/index.js:717-726` aren't replicated. Caller is
  responsible for resolving to the right `BlockStmt` before
  calling `scope_push`. Documented in the function-level
  doc-comment.

**Bug-parity preservations:**

* The IIFE site's coalescing rule mirrors Babel's `unshiftContainer`
  + `dataKey` exactly: same-kind same-blockHoist pushes into the
  same block share a `VariableDeclaration`, different-kind pushes
  produce separate ones. A future agent who "fixes" this by
  always producing one VariableDeclaration per push would diverge
  from upstream's emitted-stmt count and serialised output shape
  (which then flows into the `binding.referencePaths` count and
  thence into the cache hit/miss decision).

**Workspace test count delta:**
* `babel-plugin --lib`: 155 → **165** (+10: full
  `compat::path::tests` module covering `NodeKind` predicates,
  `PathHandle` predicate fan-out, `is_referenced_identifier`
  exclusions, `parent_path`, `ensure_block` (block + concise),
  `replace_expr`, `scope_push` round-trip, scope_push
  coalescing, scope_push unique opts, `from_binding`
  round-trip).
* All other gates unchanged at their §5.0a numbers.
* Workspace total: 2721 → **2731**, ignored 1 → **1**, failures
  unchanged at 0.
* WASI cdylib still builds clean
  (`RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1
  --release`).

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 165/165
RUSTFLAGS="" cargo test -p babel-plugin --lib compat::path             # 10/10 (the new module)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration  # 3/3 (regression canary)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release  # clean
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56 (cross-crate canary)
```

**Deliberately deferred items:**

* §5.0c: `compat/evaluation.rs` — full line-by-line port of
  `path/evaluation.js` per the Q3 lock. Coverage manifest already
  exists at `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`;
  the 45-entry parity corpus at
  `parity-harness/compat-evaluation/` is the byte-parity contract.
  With §5.0b landed, the §5.0c implementer has access to
  `PathHandle::is_expression()` for `path.evaluate()`'s "is this
  evaluable" entry check, and `replace_expr` for any
  deopt-into-replacement-node path.
* `PathHandle::get(field)` — the audit table flags `get('specifiers')`
  on ImportDeclaration and `get('init')` on VariableDeclarator
  as the only field-access shapes the §5.4 callers reach. Not
  ported in §5.0b because both call sites can be expressed as
  direct AST field reads from the parent node — the §5.4
  implementer can either add `PathHandle::get` here or read the
  field directly. If it's the latter for ALL §5.4 sites, this
  scaffolding becomes dead code; defer to the §5.4 owner's
  judgment at port time.
* The §5.0a `scope_push_synthetic` thin wrapper kept for the
  one parity-gate fixture caller. If a future cleanup lands the
  fixture rewriting against `compat::path::scope_push` directly,
  delete the wrapper — it has no production reach.

### Phase 5 §5.0a closure summary (prior session)

**Outputs landed:**

* `crates/babel-plugin/src/compat/globals.rs` (~140 LOC, 4 unit
  tests) — vendored verbatim from
  `@babel/helper-globals@7.28.0/data/{builtin-lower,builtin-upper}.json`.
  13 lowercase + 49 uppercase entries; schema-lock test asserts
  the counts so a future `@babel/traverse` bump pulling a different
  helper-globals release fails fast at `cargo test` instead of
  silently drifting `Scope.globals.includes(name)` checks in the
  §5.0c evaluator. `CONTEXT_VARIABLES` (`arguments`, `undefined`,
  `Infinity`, `NaN`) ported with order-locked test against
  `scope/index.js:941`. Cross-list duplicate guard included.
* `crates/babel-plugin/src/compat/scope.rs` (~1100 LOC including
  doc-comments + 6 unit tests) — pre-indexed `ScopeIndex` mirroring
  `@babel/traverse@7.29.0/lib/scope/index.js`'s `Scope` instance
  surface, narrowed to the 8 scope-chain methods + 5 binding fields
  the §5.4–§5.6 evaluator reads (per
  `plugins/COMPAT_SCOPE_AUDIT.md`'s surface table). Implements:
  - **Q1 — eager pre-index.** `ScopeIndex::build(&Module)` walks
    once, registers bindings + collects pending references /
    constant-violations, then resolves them in a final pass. Mirrors
    `scope/index.js:664-716`'s `crawl()` post-pass loop, with
    `path.scope.registerConstantViolation(path)` semantics for
    assignment / update / for-x-pattern sites.
  - **Finding 1 — stored `Binding.constant: bool`.** Set true at
    construction, flipped false atomically with the
    `constant_violations.push` inside `Binding::reassign()`. Not
    computed from `constant_violations.len()`. Cite-comment at
    the field declaration; matches `binding.js:7-31, 46-52`.
  - **Finding 2 — `getBinding` pattern-skip + `arguments`
    early-return.** Both branches at
    `crates/babel-plugin/src/compat/scope.rs::ScopeIndex::get_binding`
    + `find_binding_scope` (the post-walk reference-resolution
    helper). Pattern-skip: `previous_was_pattern` flag tracks the
    `previousPath?.isPattern()` predicate; we approximate by
    tagging Function/Method/Arrow/Catch scopes whose params include
    an Object/ArrayPattern as `has_pattern_param: true` (the only
    shape the Compiled corpus reaches per Finding 2). `arguments`
    early-return: byte-parity stub with the exact citation
    breadcrumb the audit prescribes.
    `pattern-skip-getBinding-walks-past-pattern` fixture is green.
  - **Finding 3 — `var`-hoist through `ForStatement` /
    `ForX(In|Of)Statement` init.** `register_var_decl` checks the
    `in_for_init` flag and routes `Var`-kind declarators to
    `function_or_program_parent(self.current_scope())`; `let`/
    `const` register at the immediate scope. Verified by the
    `var-in-for-loop-hoists-to-function-scope` fixture +
    `var_in_for_loop_is_non_constant_and_hoisted` unit test.
  - **Finding 4 — `isInitInLoop` auto-reassign.** Triggered inline
    at `register_var_declarator` when `kind == Var && in_loop_init`:
    `binding.constant = false` + push the binding's own span as
    the inaugural constant violation. Same fixture as Finding 3.
  - **Finding 5 — `Scope.parent` key/decorators skip.** Eagerly
    baked into the parent-pointer map at build time per Q1: the
    `key`/`decorators` skip never affects scope creation in our
    walker because we only push scopes for legitimate scope-owner
    nodes (Function/Arrow/Method/Block/For/Catch/Class/Switch/
    Program). Object-property keys aren't scope owners; decorator
    `decorators` lists aren't scope owners. The skip is a no-op
    in the eager model — documented inline.
  - **`scope_at_pos` deepest-scope tiebreaker.** When a function's
    span equals the surrounding Module's span (e.g. a single
    top-level `function f(...) { ... }`), the size-only innermost
    walk would resolve refs to Program. Tie-break by ScopeId
    (deeper scopes pushed later have higher ids) restores the
    Babel-equivalent enclosing-scope. Verified by
    `function-param-binding` and `arrow-param-binding` fixtures.
  - **`generate_uid_identifier` minted-uids registry.** Mirrors
    Babel's `program.uids[uid] = true` registration at
    `scope/index.js:386-388` so consecutive
    `generateUidIdentifier('')` calls bump the suffix instead of
    returning the same `_temp`. Verified by
    `generate-uid-identifier-zero-counter` fixture.
* `crates/babel-plugin/tests/compat_scope_integration.rs` —
  un-ignored `rust_compat_scope_matches_js_corpus` and wired the
  six per-call-site dispatchers (one per oracle query in
  `parity-harness/compat-scope/oracle.mjs`'s `QUERIES` table).
  Each Rust runner mirrors its oracle counterpart's logic 1:1; the
  gate is "same observed shape", not "Rust intrinsic correctness".
  Shape-lock + oracle-self-consistency tests retained from §5.0
  entry-gate. **23/23 fixtures pass byte-parity.**

**Signature divergences from upstream Babel** (each documented
inline at the divergence site):

* `Binding` carries `binding_node_type: &'static str` and
  `parent_node_type: &'static str` directly instead of a
  `binding.path: NodePath` reference. The §5.0a parity gate only
  observes the `.type` strings of those nodes, so the materialised
  pair is sufficient and avoids dragging in `compat/path.rs`'s
  `PathHandle` (§5.0b) before its single-site `&mut Expr` design
  is complete. §5.0b's `PathHandle` will replace these by exposing
  `binding.path()` returning a real handle; the cached strings
  stay as a fast-path shortcut for the predicate axis (mirrors
  what the strip-runtime port did for its narrower string-binding
  cache).
* `scope_push_synthetic` is a binding-table-only stub. Real Babel
  `Scope.push({id, init, kind})` (Finding 6) inserts a synthesized
  `VariableDeclaration` AST node via `unshiftContainer("body",
  [decl])`. The §5.0a parity gate (`scope-push-iife-injects-const-binding`
  fixture) only observes the binding shape post-push, NOT the AST
  mutation, so this stub is sufficient for §5.0a's 23/23 contract.
  **§5.0b MUST replace this stub with the real AST-mutating port
  before its sign-off** — see the sub-checkpoint table above.

**Bug-parity preservations** (each tagged with `// bug-parity:` or
`// Babel: …` citation in-source):

* `Binding.constant` is a stored `bool`, not a derived getter
  (Finding 1). Future agents reading `binding.constant_violations`
  must NOT compute `constant = violations.is_empty()` lazily; the
  stored bool is the load-bearing invariant for `evaluate-expression.ts`'s
  short-circuit at `:28`/`:39`.
* `arguments` early-return is mirrored verbatim despite being
  evidenced-unreachable from the Compiled corpus (zero matches
  across 477 fixtures). Cited inline at the break point with the
  Finding 2 + audit-doc breadcrumb.
* Pattern-skip walk applies to BOTH `ScopeIndex::get_binding` (the
  visitor-pass query) AND `Builder::find_binding_scope` (the
  post-walk reference resolver). Skipping the rule from one but
  not the other would cause `binding.referencePaths` to
  attribute references to the wrong (outer) binding when an
  inner Pattern shadows.

**Deliberately deferred items:**

* §5.0b: `compat/path.rs` — `PathHandle`, `replace_with` (the
  IIFE-site single mutation), `traverse(visitor)` delegating to
  a `VisitMut` over the subtree, `get(field)`, `parent_path`.
  AST-mutating `scope.push` replaces §5.0a's binding-table-only
  stub. §5.0b's first cargo unit test should be a "push then
  traverse, observe the new VarDecl" round-trip per the
  `COMPAT_SCOPE_AUDIT.md` §5.0b spec lock.
* §5.0c: `compat/evaluation.rs` — full line-by-line port of
  `path/evaluation.js` per the Q3 lock. Coverage manifest already
  exists at `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`.
* `references_paths_count` for assignment LHS / update-expr
  argument idents on the binding's `reference_paths` array: per
  `scope/index.js:705-712`, only `ReferencedIdentifier`-position
  idents (NOT LHS-of-assign) are pushed via `binding.reference()`.
  The §5.0a port matches this: `visit_assign_expr` for an Ident
  LHS records ONLY a constant violation, NOT a reference. The
  one place we DO push a reference for an assigning ident is
  `visit_update_expr`'s argument — which Babel records via the
  `ReferencedIdentifier` virtual visitor because the ident
  appears in expression position inside an UpdateExpression.
  Verified by `var-in-for-loop` 3-reference assertion.

**Workspace test count delta**:
* `babel-plugin --lib`: 145 → **155** (+10: 6 `compat::scope`
  unit tests + 4 `compat::globals` unit tests).
* `babel-plugin --test compat_scope_integration`: 2 passing + 1
  ignored → **3 passing + 0 ignored** (+1 passing, -1 ignored).
* All other gates (hash_parity, transform_css_integration,
  compat_generator_integration, strip-runtime lib, compiled-utils
  lib, compiled-css lib) unchanged at their §5.0 entry-gate-close
  numbers.
* Workspace total: 2710 → **2721**, ignored 2 → **1**, failures
  unchanged at 0.
* WASI cdylib still builds clean
  (`RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1
  --release`).

**Verification (cold pickup):**

```bash
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 155/155
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration  # 3/3 (post-§5.0a)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 2/2 + 1 ignored (post-§5.0c)
RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release  # clean
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56 (regression canary)
bun parity-harness/compat-scope/oracle.mjs                             # 23 entries, pin guard green
```

### Phase 5 §5.0 entry-gate summary (prior session)

**Outputs landed:**

* `plugins/COMPAT_SCOPE_AUDIT.md` — surface enumeration (8 scope-chain
  call shapes + 5 binding fields + 10 NodePath operations + the
  IIFE/getPathOfNode special semantics), feasibility breakdown
  (700–1100 LOC compat layer, replacing the previous "1.5–3k LOC
  unknown" framing), Q1/Q2/Q3 architectural locks recorded inline.
* `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` — coverage
  manifest for the §5.0c port. Four evidenced-unreachable branches
  (Flow type-cast, JSX-as-evaluable, SequenceExpression,
  TaggedTemplateExpression) each carry a quoted `unimplemented!()`
  panic message the §5.0c port emits on reach. Reachable-branch
  list locks the port's coverage contract.
* `parity-harness/compat-scope/{fixtures.json,oracle.mjs}` — 20
  entries covering every reachable scope-chain / binding /
  path-predicate / scope-push / list-key call shape. Pin guard:
  `@babel/traverse@7.29.0` + `@babel/parser@7.29.2`.
* `parity-harness/compat-evaluation/{fixtures.json,oracle.mjs}` — 45
  entries covering every reachable `path.evaluate()` branch
  (literal × 6, identifier-global × 3, binary × 10, binary-comparison
  × 3, logical × 5, unary × 6, conditional × 2, template × 2,
  parenthesized × 1, ts × 2, deopt × 3, mixed × 2). Same pin guard.
* `crates/babel-plugin/tests/compat_scope_integration.rs` —
  3 tests: shape-lock + oracle-self-consistency unconditional,
  byte-parity `#[ignore]`'d for §5.0a/b unblock.
* `crates/babel-plugin/tests/compat_evaluation_integration.rs` —
  3 tests: same shape, `#[ignore]`'d byte-parity for §5.0c unblock.
* `package.json` — `@babel/traverse@7.29.0` added to `overrides`
  AND promoted to top-level `devDependencies` (the §4.2 lesson —
  bun's isolated layout silently bypasses overrides for transitive
  deps unless the dep is also a top-level devDep).
* `crates/PARITY_VERSIONS.md` — new section documenting the
  `@babel/traverse@7.29.0` pin under "AFM-resolved 2026-05-04",
  alongside the existing `@babel/generator@7.23.0` /
  `@babel/parser@7.29.2` pin from §4.2.
* `.gitignore` — both new corpus paths added (regenerable).

**Oracle self-consistency check landed inline in both
`oracle.mjs` scripts:** every key in a fixture's `expected` object
must equal the corresponding key in the oracle's `observed` output.
The fixture author can't accidentally lie about what Babel does;
"expected = what Babel ACTUALLY does" is enforced at corpus
generation, not just at Rust-gate replay. Same shape as the
strip-runtime synth corpus's deterministic-seed lock.

**Upstream-trace spot-check landed (audit doc + fixtures + pin):**

The original audit was consumer-traced (grep over the §5.4–§5.6
source). Spot-check against
`node_modules/.bun/@babel+traverse@7.29.0/.../scope/{index,binding}.js`
landed eight new findings, recorded in
`plugins/COMPAT_SCOPE_AUDIT.md` "Upstream-trace findings (2026-05-04)":

1. `Binding.constant` is a stored bool — the §5.5/§5.6 owner's
   "computed dynamically from constantViolations.length" concern is
   unfounded for 7.29.0; reasoning recorded so the §5.0a impl
   doesn't over-engineer.
2. `getBinding()` has a pattern-skip walk (skip non-param/local
   bindings reached through a `Pattern`). NEW fixture
   `pattern-skip-getBinding-walks-past-pattern`.
3. `var` declarations hoist through `ForStatement`/`ForXStatement`
   init to the enclosing function/program scope. NEW fixture
   `var-in-for-loop-hoists-to-function-scope`.
4. `Binding` constructor's `isInitInLoop` auto-marks `var`/`hoisted`
   bindings inside loops as non-constant. Same fixture as #3.
5. `Scope.parent` is a getter with key/decorators skip semantics —
   the Rust pre-index parent-pointer map bakes these in at build.
6. `Scope.push({id, init, kind})` is AST-MUTATING (synthesises a
   `VariableDeclaration` and `unshiftContainer`'s it onto `path.body`),
   not a bindings-only update. Critical for the §5.0b IIFE site.
7. `Scope.crawl()` is lazy via `init()`; eager pre-index (Q1) is an
   intentional semantic delta, not drift — documented so future
   agents don't "fix" it.
8. `Scope.globals` / `Scope.contextVariables` come from
   `@babel/helper-globals@7.28.0` (`builtin-lower.json` × 13 +
   `builtin-upper.json` × 49). Pin row added to
   `crates/PARITY_VERSIONS.md`. The §5.0a port vendors both JSON
   files verbatim with a schema-lock asserting the 13+49 entry
   counts.

NEW fixture `var-mutated-after-decl-is-non-constant` adds a sanity
check on the basic `Binding.reassign()` path for `var`. Total
compat-scope corpus: 20 → 23 entries.

LOC estimate updated: `compat/scope.rs` 250–350 → 300–400 (extra
~50 LOC for the three semantic rules + helper-globals vendor).
Within Q1's budget.

**Q1/Q2/Q3 ARCHITECTURAL LOCKS** (from `COMPAT_SCOPE_AUDIT.md`,
recorded by the §5.0 owner via the entry-gate audit cycle):
- **Q1**: pre-index on `Program::enter` — read-only navigation
  during the visit pass, invalidate-on-replace is local.
- **Q2**: `&mut Expr` scoped to the single IIFE site
  (`traverse-call-expression.ts:95`); rest of the evaluator
  returns a `Resolved` value and stays read-only.
- **Q3**: full line-by-line port of `path.evaluate()` from
  `@babel/traverse@7.29.0/lib/path/evaluation.js`. No
  partial-port-by-corpus; evidenced-unreachable-branch panics
  permitted with citation.

**Test count delta**: `compat_scope_integration` (+3 tests, 1
ignored) and `compat_evaluation_integration` (+3 tests, 1
ignored). Net: +4 passing, +2 ignored vs. §5.3 close. All other
gates unchanged. Workspace total: 2710 (was 2706), 2 ignored.

**Bug-parity preserved during corpus generation:** the `ts`
category fixtures originally asserted `confident: true` for
`(1 as number)` / `('hi' as string)` — but `path.evaluate()`
actually returns `confident: false` for `TSAsExpression` (the
Compiled evaluator unwraps the type assertion at
`evaluate-expression.ts:132` BEFORE calling `path.evaluate()`).
Self-consistency check caught the divergence; fixtures relabeled
`ts-as-expression-deopts` with the actual Babel behavior. The
§5.6 caller's responsibility to unwrap before evaluating is
captured inline in the fixture comments.

**Deliberately deferred (NOT urgent):**

* The actual line-by-line port of `compat/{scope,path,evaluation}.rs`
  is §5.0a/b/c. This session is the entry-gate; the port is the
  next concrete code work. Reading
  `plugins/COMPAT_SCOPE_AUDIT.md` end-to-end is the entry-gate
  contract for the §5.0a implementer.
* Wiring `Layer2` into `State::cache` (the §5.3 closure carry-over)
  remains gated on §5.0a/b/c + §5.6 — the typed-T choices for
  `Cache<String>` / `Cache<Arc<Module>>` / `Cache<Option<ExportLookup>>`
  depend on the evaluator's call shapes.

**Next checkpoint: §5.0a (port `compat/scope.rs`).**

### Phase 5 §5.1 + §5.3 closure summary (prior session)

**§5.1 — STATE_MUTATIONS.md reconfirmed.** Re-ran the canonical
`grep -rEn 'state\.(includedFiles|compiledImports|sheets|cssMap|
ignoreMemberExpressions)\b'` over `packages/babel-plugin/src/`. Eight
mutation sites — exact match against the Phase 0 capture. Zero new
sites since 2026-05-02. The 5-variant `StateDiff` enum
(`IncludedFilesPush`, `CompiledImportsAppend`, `SheetsInsert`,
`CssMapInsert`, `IgnoreMemberExprMark`) remains the complete set. Two
upstream line-number drifts (`utils/css-builders.ts:325 → :321` and
`:725 → :707`; surrounding code is unchanged comment-only churn)
amended in `crates/babel-plugin/STATE_MUTATIONS.md` and
`crates/babel-plugin/src/mutation_recorder.rs` doc comments. Reach
of the §5.5/§5.6 subtree (`evaluate-expression.ts`,
`traverse-expression/`, `traversers/`) into `state.*` writes is
exactly one site at `traversers/set-imported-compiled-imports.ts:23`,
which writes `state.importedCompiledImports` — explicitly listed
under "Sites OUT of capture" (per-file scaffolding, written before
any Layer 2 lookup; no replay needed).

**§5.3 — Layer-1 Cache + Layer-2 postcard schema landed.**

Two new files:

* `crates/babel-plugin/src/cache_schema.rs` (~280 LOC + 9 unit tests)
  — postcard wire format for `<workerScratchDir>/cache.bin`. Locked
  per `plugins/SIDECAR_SCHEMA.md` §3 / `plugins/PLAN.md` §3.9.10:
  `CACHE_VERSION = 1`, `MAX_CACHE_BYTES = 5 MiB`, `MAX_ENTRIES =
  500`, `MAX_TDEPS_PER_ENTRY = 32`, `MAX_STATE_DIFFS = 64`. The
  `CacheFile` / `Layer2Entry` / `SerializedExpr` / `TransitiveDep`
  structs derive serde for postcard. `compute_schema_hash()` is a
  deterministic 32-byte fingerprint over `(plugin_version,
  swc_core_version, Layer2Entry struct signature, SerializedExpr
  variant set, StateDiff variant set)` using a 4-block FNV-1a-XOR
  expansion. Not cryptographic — PLAN.md §3.9.10 explicitly
  authorises silent wipe on schema-hash mismatch (regenerable
  scratch file), and the wipe path doesn't need collision
  resistance. Unit tests cover: deterministic re-hash, input
  sensitivity, postcard round-trip, version-mismatch wipe trigger,
  schema-hash-mismatch wipe trigger, full Layer2Entry round-trip
  (including embedded `StateDiff` variants), `SerializedExpr`
  variant set lock.

* `crates/babel-plugin/src/utils/cache.rs` (~480 LOC + 18 unit
  tests) — 1:1 port of upstream `packages/babel-plugin/src/utils/
  cache.ts` (Layer 1) + Rust-only Layer 2 handle.
    - **Layer 1 `Cache<T>`** mirrors upstream `Cache` exactly:
      `IndexMap<String, T>` (insertion-order LRU — JS `Map` semantics),
      `getUniqueKey(cache_key, namespace) → hash(namespace ?
      \`${namespace}----${cacheKey}\` : cacheKey)` via
      `compiled_utils::hash` (the §3 corpus's 10037-entry parity
      lock guarantees byte-equality at the key derivation), `load`
      with `cache=false` short-circuit, `move-to-back-on-hit` LRU,
      `getSize`/`getKeys`/`getValues`. Generic over `T: Clone`
      because upstream's "JS shares the reference" maps to
      "Rust clones" — for `Arc<Module>` that's a refcount bump,
      for `String` a heap copy. Six call-shape unit tests.
    - **Layer 2 `Layer2`** — owns the on-disk `cache.bin`. `open()`
      sweeps stale `*.tmp` siblings (PLAN.md §3.9.13.1), reads the
      file, validates version + schema_hash, falls back to
      `CacheFile::empty()` on any failure (corrupt / version-drift
      / schema-drift — never crashes the build). `insert(key,
      entry)` enforces `MAX_ENTRIES` LRU eviction. `get(key)`
      bumps LRU sequence + marks dirty. `flush(fs)` sorts entries
      by key for byte-determinism, serializes to postcard, evicts
      LRU until size <= `MAX_CACHE_BYTES`, writes via the atomic
      protocol (`cache.bin.tmp` → `fd_sync` → `path_rename`), no-ops
      when not dirty. Twelve Layer-2 unit tests cover: empty open,
      stale-tmp sweep, round-trip through MockFs, corrupt-file reset,
      version-mismatch reset, entry-cap LRU eviction, get-bumps-seq,
      deterministic byte ordering on flush, no-write-when-clean,
      embedded StateDiff round-trip.
    - The `Fs` trait abstracts host I/O; `WasiFs` is the production
      impl (lowers to `std::fs`); `MockFs` lives behind `#[cfg(test)]`
      with a `BTreeMap`-backed scratch volume.

**Wiring caveat (carried forward):** Layer 2 is NOT yet plumbed into
`State::cache` (still the `CacheSlot` placeholder from §2.4). Layer 1's
typed-T choices depend on `evaluate_expression`'s call shapes —
which are blocked on the §5.4–§5.6 drift escalation above. The
schema and the LRU machinery are locked early so the next agent can
wire reads/writes into the evaluator without touching the wire
format. No upstream-byte-affecting code changed today.

**Cargo.toml:** `postcard = { workspace = true }` promoted from
strip-runtime's never-wired use to a real dep on `babel-plugin`.
Workspace pin (`crates/Cargo.toml`) is `version = "1"` with the
`alloc` feature; same major works fine for both consumers. No new
top-level deps — `oxc_resolver` is NOT added (Phase 5 §5.4 is
blocked on the scope-tree drift; adding the dep without using it
adds wasm32 build weight for nothing).

**Test count delta**: babel-plugin lib 118 → 145 (+27: 9
`cache_schema` + 18 `utils::cache`). Total workspace: 2679 → 2706.
All other gates (hash_parity, transform_css_integration,
compat_generator_integration, strip-runtime lib + harness, full
babel-plugin harness, equality harness) unchanged at their §4.6-close
numbers. WASI cdylib still builds clean (`RUSTFLAGS="" cargo build
-p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1
--release`).

**Bug-parity preserved:** the JS `Cache._tryDeletingLRUCachedValue`
calls `delete(key)` with the result of `keys().next().value` — which
on an empty `Map` is `undefined`, and `delete(undefined)` is a no-op.
The Rust `try_deleting_lru_cached_value` guards on `len() >=
max_size` (the same guard upstream uses) so the empty-cache path
never reaches `shift_remove_index(0)`. Behavioural-equivalent to
upstream; no panic risk on an empty cache.

**Deliberately deferred (NOT urgent):**
* Wiring `Layer2` into `State::cache` — gated on §5.4–§5.6 (no
  evaluator means nothing populates entries yet). When the
  scaffolding decision lands, `State` will gain a typed cache
  bundle (`Cache<String>` for `read-file`, `Cache<Arc<Module>>` for
  `parse-module`, `Cache<Option<ExportLookup>>` for the
  find-export namespaces) plus a `Layer2` handle.
* `cache_inspect.rs` CLI (`PLAN.md` §3.9.10 mentions a
  `--dump-as-json` debug flag) — not on the byte-parity path,
  not gating any phase. Port if a debug session needs it.
* The `MAX_TDEPS_PER_ENTRY` / `MAX_STATE_DIFFS` enforcement at
  insert time — the schema declares them as hard caps, but
  `Layer2::insert` doesn't yet reject entries that exceed them
  because no producer exists. The §5.6 evaluator's
  `cacheable_at_layer2: bool` flag is what naturally gates this.
  When that lands, add `assert!(entry.transitive_deps.len() <=
  MAX_TDEPS_PER_ENTRY)` etc. at insert.

### Phase 4 §4.6 closure summary — PARTIAL (post-CSSOutput builders; visitor dispatch deferred)

§4.6 as specified in PLAN.md task 4 says "update lib.rs so the
visitor performs both extraction and substitution in a single
traversal". The visitor's CssProp / ClassNames / Styled / CssMap
dispatch sites all reach `buildCss`, which the §4.4 SHELL stubs at
every `evaluate_expression` / `resolve_binding` / `visitCssMapPath`
panic site. **Wiring the visitor today would force the SHELL stubs
to be real → that's Phase 5 work.** The pragmatic split: §4.6 ships
the post-CSSOutput AST builders (the primitives the future visitor
calls), defers the visitor wiring itself to the Phase 5 / 6 close.

This split is consistent with the previous-agent hand-off
(§4.5 closure): "wire the call sequence inside the visitor, but
ship only the parts where evaluation isn't required" — that constraint
narrows §4.6 to the post-CSSOutput AST construction primitives.

**New files this checkpoint** (all under
`crates/babel-plugin/src/utils/`, all 1:1 ports of upstream
`packages/babel-plugin/src/utils/` siblings):

* `get_jsx_attribute.rs` (~50 LOC + 4 unit tests) — pure SWC AST
  query. `get_jsx_attribute(&Expr, &str) -> (Option<&JSXAttr>, isize)`.
  Returns `(None, -1)` for "not a JSXElement" or "no match" — exactly
  Babel's `[undefined, -1]` tuple.
* `get_runtime_class_name_library.rs` (~25 LOC + 2 unit tests) —
  one-line opts read returning `"ac"` (compression) or `"ax"`.
* `hoist_sheet.rs` (~75 LOC + 5 unit tests) — sheet-name registration
  via `MutationRecorder::SheetsInsert`. Mints UIDs through a new
  `state.next_uid_name()` mint (`_<n>` counter, fresh per pass —
  Phase 5 §5.4 lands the scope-aware variant). The actual AST
  insertion (insertBefore on Program.body's first non-import) is
  NOT a `paths_to_cleanup` entry — the data lives on
  `state.sheets()` and the Phase 6 Program::exit emit-pass reads it
  there. Signature divergence: takes explicit
  `&mut MutationRecorder` because the recorder isn't on `Metadata`.
* `build_compiled_component.rs` (~340 LOC + 8 unit tests) — both
  `compiled_template` and `build_compiled_component` from upstream's
  `build-compiled-component.ts`. Hand-built JSX AST instead of a
  `compat/template.rs` `@babel/template` analog (smaller blast
  radius; the template-parser port is deferred to a future
  checkpoint where `compat/template.rs` is genuinely needed).
  Splices className + style onto the user's JSXElement, then wraps
  in `<CC><CS>{cssArray}</CS>{jsxNode}</CC>`. Reuses every §4.5
  adapter (`transform_css_items` + `build_css_variables`) and every
  §4.6 leaf added above.

**State-shape additions:**

* `state.uid_counter: u32` + `state.next_uid_name() -> String` mint
  the `_<n>` UID names. Fresh-per-pass (matches "SWC tears down the
  WASI instance between transforms" constraint). NOT captured in
  `StateDiff` — `uid_counter` is per-pass derivable, not part of the
  cross-call cache schema.

**Recorder threading**: the recorder lives on `BabelPluginVisitor`
as a sibling field to `state` per the §2.4 split. `Metadata` does
NOT carry it; functions that record state mutations
(`hoist_sheet`, `compiled_template`, `build_compiled_component`)
take `&mut MutationRecorder` as an explicit parameter. The visitor
call shape becomes `fn(args, &mut Metadata, &mut MutationRecorder)`.

**Deliberately deferred (NOT urgent, NOT §4.7 blockers):**

* `build_styled_component.rs` — needs `pickFunctionBody`,
  `@emotion/is-prop-valid` table verbatim, `findOpenSelectors`
  regex helper, the larger forwardRef template. Defer until the
  styled-handler dispatch site lands in Phase 6 §6.7.
* `build_display_name.rs` — small leaf, ports alongside the
  per-handler dispatch that consumes it.
* `append_runtime_imports.rs` — Program::exit machinery, ports
  alongside the §2.3(b) cleanup-queue work bundle.
* `compat/template.rs` — the `@babel/template` analog. Hand-built
  AST is sufficient for `compiled_template` / `compiled_styled`;
  port `compat/template.rs` only when a future fixture needs full
  template-string parsing.
* The four visitor dispatch sites in `babel_plugin.rs` (css-prop,
  classNames, cssMap, styled, xcss-prop) — all blocked on Phase 5
  §5.4 (resolve_binding) + §5.6 (evaluate_expression). The §4.4
  SHELL's `unimplemented!()` panics fire at the first reach.

**Bug-parity preserved:**

* `get_expression`'s panic on `JSXEmptyExpression` mirrors upstream's
  `throw new Error('Empty expression not supported.')`. Unit-tested
  via `build_compiled_component_concats_existing_classname` (the
  positive path); the panic path lights up if a future fixture
  surfaces an empty `className={}` value.

**Test count delta**: babel-plugin lib 99 → 118 (+19: 4 + 2 + 5 + 8).
Total workspace: 2660 → 2679. All other gates
(hash_parity, transform_css_integration, compat_generator_integration,
strip-runtime lib + harness, full babel-plugin harness, equality
harness) unchanged at their §4.5-close numbers.

### Phase 4 §4.5 closure summary (data adapters — transform_css_items, build_css_variables)

Three new files at `crates/babel-plugin/src/utils/` mirror upstream
1:1:

* `transform_css_items.rs` (~110 LOC + 13 unit tests) — ports
  `packages/babel-plugin/src/utils/transform-css-items.ts` 1:1.
  Three exports: `transform_css_item` (private, recursive),
  `transform_css_items` (public), `apply_selectors` (public). All
  four CssItem-variant branches wired:
  - Conditional → recurses both branches; folds to `<test|!test> &&
    <classExpression || undefined>` for one-sided sheets, ternary
    when both branches carry sheets, no-op when both are empty.
  - Logical → `transform_css(get_item_css(item), opts)` then
    `compress_class_names_for_runtime` on the join, wraps in
    `LogicalExpression { op: l.operator, left: l.expression, right: <stringLit> }`.
  - Map → reads `meta.state.css_map()[name]` for sheets, threads the
    map's stored expression as classExpression.
  - Default (Unconditional / Sheet) → same `transform_css` /
    `compress_class_names_for_runtime` as Logical, but emits the
    classExpression as a bare StringLiteral (or `None` when the
    joined name is whitespace-only — matches JS's `className.trim()`
    short-circuit).

* `build_css_variables.rs` (~110 LOC + 7 unit tests) — ports
  `packages/babel-plugin/src/utils/build-css-variables.ts` 1:1.
  One export: `build_css_variables(variables, transform)`, returning
  `Vec<PropOrSpread>`. Caller-side default (matches the JS default
  arg) is `build_css_variables(&vars, |e| e)`. Bug-parity preserved:
  the prefix is ONLY emitted when suffix is ALSO present (upstream's
  `suffix && prefix && t.stringLiteral(prefix)` short-circuit).
  Locked with a `drops_prefix_when_suffix_missing_bug_parity` test.

* `compress_class_names_for_runtime.rs` (~50 LOC + 4 unit tests) —
  ports
  `packages/babel-plugin/src/utils/compress-class-names-for-runtime.ts`
  1:1. Trivial pure-string helper that
  `transform_css_items` depends on. Uses `chars().skip().take()` for
  the JS `slice(1)` / `slice(1, 5)` translation (char-indexed; for
  ASCII identical to byte slicing, but degrades gracefully on
  non-ASCII rather than panicking on a non-char-boundary byte slice).

**Helper changes:**

* `crates/babel-plugin/src/utils/css_builders.rs`:
  `logical_op_to_swc(op)` promoted from `fn` to `pub(crate) fn` so
  `transform_css_items` can reuse the LogicalOperator → BinaryOp
  translation. Single-line edit; same body.
* `crates/compiled-utils/src/lib.rs`: re-export `unique_by`
  alongside `unique` (was already present in `array.rs`, just not
  surfaced). `build_css_variables` consumes upstream's
  `unique(variables, (v) => v.name)` shape.

**Babel→SWC field-name divergences documented inline at the file
heads** (no behavioural drift, only the `t.identifier('undefined')`
→ SWC Ident, `t.logicalExpression` → BinExpr+LogicalAnd/Or/NullishCoalescing,
`t.conditionalExpression` → CondExpr.alt-not-alternate,
`t.stringLiteral` → Lit::Str(Str{ raw: None }) shape mappings).

**PluginOptions → TransformOpts conversion**: upstream JS plugin
duck-types `meta.state.opts` straight into `transformCss`. Rust
needs an explicit projection — `plugin_opts_to_transform_opts` in
`transform_css_items.rs` covers the AFM-pinned 0.19.0 surface
(no `flattenMultipleSelectors`; `sortShorthand` not on
`PluginOptions` so threads as `None`). Per-call instantiation; no
caching (matches "SWC tears down the WASI instance between calls"
constraint).

**Error semantics**: upstream JS lets `transformCss` throw and
bubble. Rust port mirrors with `unwrap_or_else(|e| panic!(...))`.
Phase 4 §4.6+ lands the proper visitor-level error channel.

**Test count delta**: babel-plugin lib 75 → 99 (+24: 4 +
7 + 13). Total workspace: 2636 → 2660. All other gates
(hash_parity, transform_css_integration, compat_generator_integration,
strip-runtime lib + harness, full babel-plugin harness, equality
harness) unchanged at their §4.4-close numbers.

### Phase 4 §4.4 closure summary (SHELL port; 4 hash-call-shape sites end-to-end)

`crates/babel-plugin/src/utils/css_builders.rs` (~1100 LOC mirroring
upstream `packages/babel-plugin/src/utils/css-builders.ts` 1:1) plus
the small tractable transitive deps (`types`, `is_empty`,
`is_compiled`, `ast`, `object_property_to_string`,
`manipulate_template_literal`) ship as a SHELL — the file structure
mirrors upstream 1:1, the four §3 hash-call-shape sites are wired
end-to-end through `compat::generator::generate` →
`compiled_utils::hash`, and every `evaluate_expression` /
`resolve_binding` / `visit_css_map_path` dispatch site is gated on a
phase-citing `unimplemented!()` panic. The §4.8 phase exit gate
(harness fixtures byte-clean for keyframes / css / cssMap) is what
will eventually require those stubs to be real; §4.4 is the
structural milestone that makes the Phase 6 handler ports possible.

**Hash-call-shape sites wired (§3 corpus already covers the inputs):**

| Site | Upstream LOC | Wiring |
|---|---|---|
| #1 keyframes name | css-builders.ts:464 | `format!("k{}", hash(&generate(call_or_tagged_tpl)))` in `extract_keyframes` |
| #2 object-expression catch-all | css-builders.ts:639 | `format!("--_{}", hash(&variable_name))` in `extract_object_expression` |
| #3 template-literal catch-all | css-builders.ts:869 | `format!("--_{}{}", hash(&variable_name), prefix_marker)` in `extract_template_literal` |

(The fourth hash-call-shape site at `atomicify-rules.ts:41/:44` lives
in `crates/css` / `crates/compiled-css`, not in babel-plugin — owned
by the CSS-port agent. Already exercised by the §3 hash parity corpus
through the §4.1 transform_css integration test.)

Each site has a unit test in `utils::css_builders::tests` that
constructs the AST shape the JS path would build and asserts the
emitted hash-keyed name matches the JS oracle. Tests reach the real
Rust dispatch (no mocking); the §3 corpus's 10037-entry parity
contract guarantees byte-equality at the hash-input boundary.

**Stubs (Phase 5 / 6 gates):**

* `evaluate_expression_stub(...)` → Phase 5 §5.6
  (utils/evaluate-expression.ts) — every dispatch into the evaluator.
* `resolve_binding_stub(...)` → Phase 5 §5.4
  (utils/resolve-binding.ts) — every dispatch into the resolver.
* `visit_css_map_path_stub()` → Phase 6 §6.3 (css-map handler).
* `has_nested_template_literals_with_conditional_rules` → Phase 5
  §5.6's NodePath-parent-traversal analog
  (`getPathOfNode + traverse(parent, ...)`).

**CSS-port agent's parallel work** (CSS_BUILDERS_DEPS.md, RESOLVED
2026-05-04). `addUnitIfNeeded` (45-property unitless lookup +
`AddUnitValue` enum) and `cssAffixInterpolation` (with
`BeforeInterpolation` / `AfterInterpolation` types) landed at
`crates/compiled-css/src/utils/{css_property,css_affix_interpolation}.rs`
as 1:1 ports of the JS source (35 affix-test-cases ported verbatim;
9 add_unit cases). Re-exported from `crates/css/src/lib.rs` so
`crates/babel-plugin/src/utils/css_builders.rs` imports from `css::`
the same shape JS imports from `@compiled/css`. `compiled-css` test
count: 121 → 163. The §4.4 SHELL uses both helpers directly at the
numeric-literal-property and template-literal catch-all sites — no
stubs at the hash-call-shape paths.

**Architectural change: `Metadata` reborrow shape.** Babel's
`{ ...meta, context: 'keyframes', keyframe: name }` object spread
shares the `state` reference and overrides fields. Rust requires an
explicit reborrow because `State` is `&mut`-held —
`Metadata::reborrow_with_context(&mut self, ctx) -> Metadata<'_>`
(landed in `types.rs`) is the analog. Every `extract_*` function in
`css_builders.rs` takes `&mut Metadata<'_>` (not `&Metadata<'_>`) so
child calls can reborrow properly. This rippled to
`object_property_to_string.rs` and
`manipulate_template_literal.rs` signatures; tests updated to pass
`&mut meta`.

**Cargo.toml move:** `css = { workspace = true }` promoted from
`[dev-dependencies]` to `[dependencies]` per the §4.6 hand-off note
in §4.3's closure (now landed at §4.4 because the helpers it
re-exports are reachable from the SHELL hash-call sites). Also added
`regex` and `once_cell` workspace deps for the upstream regex ports
in `manipulate_template_literal.rs` and `css_builders.rs`.

**Bug-parity preserved:** `manipulate_template_literal.rs`
intentionally mirrors upstream's `[;|{|}]` split-statement regex
verbatim (the `|` between bracket entries is literal in a char
class — looks like a typo but ships in production). Per CLAUDE.md
"BUGS in OLD! Need to be BUGS In NEW" (added 2026-05-04), this is
documented inline at the regex declaration so future readers don't
"fix" it.

**Deliberately deferred (NOT urgent):**

* `arrow.body = firstExpression` mutation in
  `extract_object_expression` (Babel's optimised template-literal
  wrapping for arrow-function-with-body-as-Tpl-with-cond-arg). The
  §4.4 corpus does not exercise this path; Phase 5 §5.6's
  mutable-walker shape lands the proper model.
* `nextQuasis.value.raw = after.css` mutation in
  `extract_template_literal`. Same reason — Phase 5 §5.6 mutable
  walker.
* `extract_member_expression`'s map-cache miss path (Phase 6 §6.3).

### Phase 4 §4.3 closure summary (55/55 byte-exact, zero skips)

`crates/babel-plugin/src/compat/generator/` mirrors
`@babel/generator@7.23.0/lib/` 1:1 across 10 files (~1640 LOC):
- `mod.rs` (entry — `generate`, `generate_with_comments`,
  `generate_jsx_attribute`, `generate_jsx_attribute_with_comments`)
- `buffer.rs` (output buffer; queue-cursor + drop-trailing-whitespace-before-newline; source-map machinery dropped)
- `printer.rs` (dispatcher; print/word/token/space/newline; indent + maybe_indent; leading/trailing comment threading; JSX-* `Expr` variants now route to `generators::jsx::*`)
- `node/{mod,parentheses}.rs` (paren policy; `PRECEDENCE` table 1:1; logical/binary/conditional rules)
- `generators/{mod,expressions,types,template_literals,jsx}.rs` (Identifier, NumericLiteral, StringLiteral, BooleanLiteral, NullLiteral, BinaryExpression / LogicalExpression, ConditionalExpression, UnaryExpression, MemberExpression, CallExpression, ParenthesizedExpression-as-transparent, ObjectExpression with `printList`-statement-mode, ArrayExpression, ObjectProperty / Spread, TemplateLiteral, TaggedTemplateExpression, JSXElement, JSXAttribute, JSXIdentifier, JSXMemberExpression, JSXNamespacedName, JSXEmptyExpression, JSXExpressionContainer, JSXText, JSXOpeningElement, JSXClosingElement, JSXSpreadAttribute, JSXFragment, JSXOpeningFragment, JSXClosingFragment, JSXSpreadChild).

The corpus's 4 known real divergences are all reproduced byte-exact:
1. `cond ? /* yes */ 'a-class' : 'b-class'` →
   `cond ? /* yes */'a-class' : 'b-class'` (NO space between `*/`
   and `'a-class'`).
2. `(a && b) || c` → `a && b || c` (paren dropped at precedence
   boundary). Implemented via `Expr::Paren` transparency in
   `Printer::print` so the inner expression sees the GRANDPARENT
   when paren policy decides — matches Babel's flattened-shape
   AST default.
3. `'a-class'` (single-quote source) → `'a-class'` (preserved). Via
   `Str.raw` passthrough mirroring Babel's `getPossibleRaw(node)`.
4. `/* eslint-disable-next-line */ cond ? 'a' : 'b'` → no space
   between leading block-comment and the next expression.

**SWC↔Babel comment-storage quirk discovered + worked around.**
A same-line comment between two tokens (e.g.,
`{ /* leading */ from`) is keyed in SWC's comment store as
**TRAILING of the previous token**, NOT as leading of the next.
Specifically: `comments.take_trailing(BytePos(open_brace.lo + 1))`
returns the comment, while `take_leading(prop.span.lo)` returns
nothing. Object-property iteration handles this by querying both
positions before printing the first prop. Documented inline at
`generators/types.rs::object` so future fixtures hitting the same
shape have a clear breadcrumb.

**JSX sub-step closed this session.** `generators/jsx.rs` covers
every node kind from upstream's `lib/generators/jsx.js`:
JSXElement, JSXAttribute, JSXIdentifier, JSXMemberExpression,
JSXNamespacedName, JSXEmptyExpression, JSXExpressionContainer,
JSXText, JSXOpeningElement, JSXClosingElement, JSXSpreadAttribute,
JSXFragment, JSXOpeningFragment, JSXClosingFragment, JSXSpreadChild.
Two new public entry points in `mod.rs` (`generate_jsx_attribute`,
`generate_jsx_attribute_with_comments`) carry the same byte
contract for the JSXAttribute call shape — `Printer::print(&Expr,_)`
can't be overloaded because SWC types JSXAttr separately from Expr.
The 5 jsx-key-attribute fixtures (StringLiteral / NumericLiteral /
MemberExpression / TemplateLiteral / ConditionalExpression attribute
values) reach the new entry point via the integration test's
`find_first_key_jsx_attribute` walker (mirrors the JS oracle's
`extractJsxKeyAttribute` recursive walk verbatim — both must agree
on which `key=` attribute they pluck out, otherwise the byte-parity
assertion compares different inputs).

**SWC↔Babel field-name divergences** (mechanical renames; byte
output identical). Documented inline at the head of `jsx.rs`:
`opening`/`closing` (SWC) vs `openingElement`/`closingElement`
(Babel) on JSXElement; `opening`/`closing` vs `openingFragment`/
`closingFragment` on JSXFragment; `attrs`/`self_closing`/`type_args`
vs `attributes`/`selfClosing`/`typeParameters` on JSXOpeningElement;
`obj`/`prop` vs `object`/`property` on JSXMemberExpr; `ns` vs
`namespace` on JSXNamespacedName; `expr` vs `expression` on
JSXExprContainer / JSXSpreadChild. JSX spread attributes are typed
as the generic `SpreadElement` inside `JSXAttrOrSpread::SpreadElement`
in SWC (Babel uses a JSX-specific `JSXSpreadAttribute` node — same
byte output: `{...arg}`). JSXIdentifier in Babel maps to `Ident` in
JSXElementName positions and `IdentName` in JSXAttrName / member-prop
positions on the SWC side; both expose `.sym` so the byte output is
identical.

**Deliberately deferred (NOT urgent):**
- TS type-args on JSXOpeningElement (`<MyComp<T> />`): SWC stores as
  `Option<Box<TsTypeParamInstantiation>>` on `type_args`. The corpus
  doesn't reach this branch; `jsx_opening_element` emits a
  `/*UNHANDLED-JSX-TYPE-ARGS*/` marker if `type_args.is_some()` so
  the byte-parity gate fails loudly with a clear pointer when a real
  fixture surfaces it. Port `generators/typescript.js`'s
  `TSTypeParameterInstantiation` printer at that point.
- Inner-comment threading on `JSXEmptyExpression` (`{/* hint */}`):
  upstream calls `printInnerComments()`. The corpus's 5 jsx-key
  fixtures don't carry comments inside the expression container, so
  today's no-op matches Babel exactly. When a future fixture lands a
  comment, query the `Comments` store at the `JSXEmptyExpr.span`
  bounds — same shape as `Printer::print(&Expr, _)` does for
  Expression-typed nodes.

**Other deliberately omitted upstream files** (per CLAUDE.md
"1:1 with what's reachable"): `flow.js`, `typescript.js`,
`classes.js`, `methods.js`, `modules.js`, `statements.js`,
`base.js`. None reachable from the 5 call sites; port if a real
fixture surfaces them.

### Phase 4 §4.2 closure summary

`crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` (the per-call-site
coverage manifest), `parity-harness/compat-generator/{fixtures.json,
oracle.mjs}` (the in-tree input set + JS oracle producing
`crates/babel-plugin/tests/compat_generator_corpus.json`),
`crates/babel-plugin/src/compat/{mod.rs,generator.rs}` (the stub
that §4.3 fills in), and `crates/babel-plugin/tests/compat_generator_integration.rs`
(2 GREEN gates + 1 `#[ignore]`d byte-parity gate) lock the contract
for `compat::generator::generate`.

**Pin landed:** `@babel/generator@7.23.0` + `@babel/parser@7.29.2`
(AFM-resolved under `@compiled/babel-plugin@0.36.1` commit
`16a62b8`) added to root `package.json#overrides` AND promoted to
top-level `devDependencies`. Both are required for resolution to
land inside the workspace — bun's isolated dep layout means
`require('@babel/generator')` walks past the workspace's `.bun`
store unless the package is hoisted via a top-level dep. Caught
empirically: pre-promotion, `require('@babel/generator/package.json').version`
returned `7.28.5` from `Documents\projects\node_modules\@babel\generator/`
(a sibling project's tree). Pin guard in `oracle.mjs` AND the
`corpus_shape_lock` test in the integration test both fail-fast on
this drift. Recorded in `crates/PARITY_VERSIONS.md`.

**Corpus composition (55 entries):**
- `keyframes-expression`: 11 — basic / string-key / comma-key /
  tagged-template / interpolated-template / nested / conditional /
  comment-axis ×4 (leading, trailing, eslint-disable, ternary-inner).
- `generic-expression`: 25 — Identifier / MemberExpression chain
  (dot, deep, computed) / Call / ArrayMember / literal set
  (string-double, string-single, numeric, decimal, bool, null) /
  TemplateLiteral (static + interpolated) / Binary (precedence) /
  Conditional (simple, nested) / parenthesized / comment-axis ×5
  (leading, trailing, ternary-inner, eslint-disable-line, PURE).
- `variable-init`: 6 — string / member / call / object / conditional
  / template-with-interpolation.
- `jsx-key-attribute`: 5 — string-literal / numeric-expr / member-expr
  / template-expr / conditional-expr.
- `conditional-classname-item`: 8 — `&&` / `||` / `??` / `?:` /
  `?:` with null / nested logical (paren) / comment-between-branches /
  leading eslint-disable.

**Real divergences captured at §4.2 corpus generation** (these
will fire on shortcut §4.3 implementations):
1. `cond ? /* yes */ 'a-class' : 'b-class'` → Babel emits
   `cond ? /* yes */'a-class' : 'b-class'` (NO space between
   block-comment and following expression). SWC default emits
   the space — §4.3 must match Babel.
2. `(a && b) || c` → Babel emits `a && b || c` (paren dropped at
   precedence boundary). SWC default keeps the paren.
3. `'a-class'` (single-quote source) → Babel preserves
   single-quote; SWC default re-quotes to double.
4. `/* eslint-disable-next-line */ cond ? 'a' : 'b'` → no space
   between leading block-comment and the following expression
   (same axis as 1).

**Lessons logged this session:**

**1. Bun's isolated dep layout silently bypasses `package.json#overrides`
for transitive deps unless the dep is also a top-level devDep.**
Workspace-root `bun pm ls` reported `@babel/generator@7.23.0`
correctly, but `require('@babel/generator/package.json')` from a
script INSIDE the workspace resolved `7.28.5` from a sibling
project's `node_modules` one directory up. The override flowed
into bun's `.bun/@babel+generator@7.23.0/...` store but bun
didn't symlink `node_modules/@babel/generator/`, so Node's
resolution algorithm walked past the workspace and hit the
ancestor tree. Fix: promote the pinned package to top-level
`devDependencies`. Pin guards (in oracle.mjs AND in the cargo
integration test) catch this immediately if it ever recurs —
keep both gates.

**2. AFM tiebreaker isn't auto-actionable from in-tree docs for
non-CSS deps.** `AFM_MONOREPO_DEPENDENCIES_MORE.md` and
`AFM_MONOREPO_PACKAGE_VERSIONS.md` only document
`@compiled/css@0.19.0`'s 61-item dep tree. AFM's resolution for
`@compiled/babel-plugin@0.36.1`'s `@babel/*` family had to be
fetched out-of-band (user provided: `@babel/generator@7.23.0`,
`@babel/parser@7.29.2`). When future Phase 4–6 work needs another
upstream npm-package version (e.g., `@babel/traverse`,
`@babel/types`, `@emotion/is-prop-valid`), expect to ask the AFM
dependency engineer rather than grep the in-tree docs. Recording
each new pin in `crates/PARITY_VERSIONS.md` keeps the cross-check
durable.

### Phase 4 §4.1 closure summary

`crates/babel-plugin/tests/transform_css_integration.rs` (3 tests,
120/120 entries byte-equal) locks the consumer-side parity contract
for `css::transform_css`. Corpus is 30 hand-curated CSS fixtures
from `crates/parity-runner/corpus/transform-css/` × 4 opts
permutations (`{}` default, `{optimizeCss:false}`,
`{increaseSpecificity:true}`, `{classHashPrefix:'x'}`).

**Browserslist pin:** `BROWSERSLIST_CONFIG` set to
`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` (the
EXACT production AFM pin per `BROWSER_LIST_FROM_AFM.md`); resolves
to the documented 14-entry list under workspace-pinned
`caniuse-lite@1.0.30001766` + `browserslist@4.24.2`. Both engines
honor `BROWSERSLIST_CONFIG` (`crates/browserslist-shim/src/node.rs:143`
for Rust). `BROWSERSLIST` and `AUTOPREFIXER` env vars are explicitly
unset (would short-circuit the config-file path / disable autoprefixer).

### Lessons logged this session

**1. Env-var test races are silent and look exactly like drift.**
`EnvPin` mutates process-global env vars (`std::env::set_var`).
Cargo parallelises test functions in the same binary by default; if
multiple tests construct an `EnvPin`, one thread's `set_var` collides
with another's `Drop` `remove_var`, and `transform_css` ends up
calling autoprefixer with the env var unset. Under default
browserslist (older Firefoxes), autoprefixer correctly emits
`-moz-user-select`, but the test compares against the JS oracle
captured under AFM (which doesn't need it). Apparent divergence,
real cause is the harness.

Fix landed: `EnvPin` is confined to the single test that calls
`transform_css`. The two schema-shape tests don't construct one
(they don't read env state). A `CRITICAL` rustdoc block on `EnvPin`
documents the hazard for future test authors. 5 consecutive
`cargo test` runs all 120/120 post-fix.

The original (incorrect) brief filed at
`plugins/AUTOPREFIXER_DRIFT_BRIEF.md` was deleted after the
autoprefixer agent reproduced no drift via
`crates/css/examples/repro_user_select.rs`. Apology delivered.

**2. AFM is the production browserslist pin, not `chrome 100`.**
Initial oracle used `BROWSERSLIST=chrome 100` because that's what
the parity-runner's `TransformCss` stage uses
(`crates/parity-runner/src/stages.rs:582`). That's a non-production
isolation pin. The Jira build runs against the AFM `.browserslistrc`,
documented in `BROWSER_LIST_FROM_AFM.md`. Pin switched to
`BROWSERSLIST_CONFIG=<AFM-fixture-path>` — production-accurate.
The parity-runner could be re-pinned the same way; not a plugins/*
edit.

### Drift watch point (still open — handed to CSS-port agent)

`crates/STATUS.md:3303` row 8b still says *"PARTIAL — currently
identity-passthrough"* about `crates/css/src/transform.rs`. That's
stale: `transform.rs` shipped the full plugin chain in commit
`be17def "trasnform css is ready"` (1037 lines composing every
plugin per `crates/PHASE_8B_LIFECYCLE_AUDIT.md`'s Round 1 → 2 → 3
recipe). The dispatch in `packages/css/src/transform.ts:36`
(`if (process.env.COMPILED_CSS_ENGINE === 'rust') return
require('@compiled/css-native').transformCss(css, opts);`) is wired;
the row's `:70` line citation is a wrong line number. The CSS-port
agent owns flipping row 8b to DONE; `plugins/*` does not edit
`crates/STATUS.md`.

`packages/equality-harness/scripts/verify.mjs` has a
`require.resolve`-in-ESM bug under raw `node` (the harness uses
`require.resolve` in an `.mjs` file, which is undefined — both
engines error identically and the "both-errored" branch reports a
vacuous 336/336 pass). Under `bun`, `require` is polyfilled and
the harness runs the real test. Bug-fixing the Node path is the
equality-harness owner's task, not plugins/*.

### Phase 3 closure summary (this session)

§3.1 ☑ — confirmed `pub fn hash(input: &str) -> String` at
`crates/compiled-utils/src/hash.rs:52`, re-exported at
`crates/compiled-utils/src/lib.rs:34`. Symbol stable since Phase 1.

§3.2 ☑ — built `parity-harness/hash/oracle.mjs` (mirrors the
strip-runtime synth precedent: deterministic mulberry32 seed=1,
re-runs produce a byte-identical corpus). Imports the upstream
JS `hash` via `@compiled/utils` (workspace dep, resolves to
`packages/utils/src/hash.ts`) and emits
`crates/babel-plugin/tests/hash_corpus.json` (gitignored — same
pattern as `parity-harness/strip-runtime/fixtures/synthesized/`).
Composition (10037 entries):
- **4 real-call-shape entries** — one per `hash()` call site in
  `packages/babel-plugin/src/`: `hash(generate(expression).code)`
  (css-builders.ts:464), `hash(variableName)` (css-builders.ts:639,
  869), composite atomicify key (atomicify-rules.ts:41), CSS value
  (atomicify-rules.ts:44).
- **~33 categorical entries** — empty / single char / NUL
  (leading, trailing, embedded, all-NUL) / leading-trailing
  whitespace / UTF-8 multibyte (2/3/4-byte) / surrogate-pair
  boundaries / length-tail (`l mod 4 ∈ {0,1,2,3}`, all four murmur2
  branches) / >4 KiB ASCII / >4 KiB mixed.
- **10000 random entries** — 5000 ASCII + 5000 valid Unicode
  scalar values (skips surrogate range U+D800..U+DFFF — those are
  legal JS string elements but not RFC-valid JSON; the consuming
  Babel plugin only ever passes valid UTF-8 to `hash()`, so the
  parity contract is over scalar values).

§3.3 ☑ — Rust integration test at
`crates/babel-plugin/tests/hash_parity.rs` reads the JSON corpus
and asserts byte-equality via `compiled_utils::hash`. Four tests:
`rust_hash_matches_js_corpus` (the gate),
`corpus_includes_real_call_shapes` (lock the four call-site
fingerprints), `corpus_covers_phase3_categories` (lock the §3.2
acceptance set), `corpus_has_at_least_10k_random_entries` (lock
the §3.3 ≥10K acceptance). All four pass over 10037 entries —
zero divergence. The pure-Rust gate (no WASM, no JS bridge) is
the cheapest shape for a pure-data parity check; matches the
previous agent's recommendation.

Drift fixed in `crates/compiled-utils/src/hash.rs::tests::known_vectors`:
the comment in that test (lines 198-201 prior) explicitly
telegraphed the §3.3 migration ("Once we have a Node verification
step (parity-runner) we'll cross-check them against the JS hash
and lock in the bytes from JS instead. For now the values lock
current behaviour against future drift."). Replaced the
self-referential `hash(input)` lines with five hardcoded
JS-locked byte vectors pulled directly from the corpus
(`""→"0"`, `"a"→"14mfbry"`, `"abcd"→"aougpt"`,
`"color: red"→"1wszpi4"`, `"@media (max-width: 100px)"→"w2cthn"`).
Five-vector smoke is still in-process (faster iteration loop
than reading the 1.7 MB JSON corpus); the JSON corpus remains
the authoritative parity contract.

§3.4 ☑ — Phase 3 exit gate met. Final test state:
- `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31.
- `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43
  (unchanged from §2.4).
- `RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity`
  → 4/4 over 10037 entries.
- `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib`
  → 56/56 (unchanged from §1.9).
- `bun test parity-harness/strip-runtime/harness.test.ts` →
  1132/1132 (unchanged).
- `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
  bun test parity-harness/babel-plugin/harness.test.ts` →
  954/954 (unchanged from §2.5).
- Encapsulation lint still clean (zero matches outside
  `state.rs`/`mutation_recorder.rs`).

### Phase 2 closure summary (multi-session)

§2.0 (prior): 477 fixtures extracted from the babel-plugin test
files; full-corpus Babel determinism baseline green.

§2.1 (prior): data-only `constants.rs`, `utils/constants.rs`,
`types.rs` ported.

§2.2 (prior): parity harness skeleton with `babelEngine` +
`swcEngine` (pass-through `babel_plugin.wasm`); 152 sampled tests
green.

§2.3 (this session): dispatcher SKELETON (prior) + §2.3(a)
JSX-pragma recognition (this session) landed. `BabelPluginVisitor`
is generic over `C: Comments` (plugin entry uses
`PluginCommentsProxy`; tests use `SingleThreadedComments::default()`).
Recognition covers:
  (a) Compiled-import recognition → `state.compiled_imports[apiName]`
      (moved from `visit_mut_program` pre-walk into
      `visit_mut_module_decl` so upstream's order — pragma scan
      first, then import-decl visitor — is preserved). Now routed
      through `MutationRecorder::apply` (§2.4).
  (b) `findClassicJsxPragmaImport` analog →
      `state.pragma.classic_jsx_pragma_is_compiled` +
      `classic_jsx_pragma_local_name` (handles bare, renamed, and
      StringLiteral-imported jsx specifiers).
  (c) Comment scan at `comments.get_leading(first_body_item.span.lo)`
      → `state.pragma.jsx` / `jsx_import_source` and bootstraps
      `state.compiled_imports = Some(default)`.

§2.3(a) is recognition-ONLY. Two AST/comment-store mutations
(`path.remove()` of classic-pragma `jsx` specifier; comment-store
filter dropping the matched pragma comment) are tagged
`// §2.3(b):` in `babel_plugin.rs` and deferred — see "§2.3(b)
follow-up" below.

Stub handlers for `call_expr`/`tagged_tpl`/`jsx_element`/
`jsx_opening_element` unchanged from prior session.

Two pieces of drift fixed while consuming `compiled-utils` (prior
session): (a) `COMPILED_IMPORT` was `@compiled/react`, JS source is
`@compiled/react`; (b) `jsx.ts` was never ported — added as
`jsx.rs` with `JSX_ANNOTATION_REGEX` / `JSX_SOURCE_ANNOTATION_REGEX`
both as `OnceLock<Regex>` and 6 unit tests covering the Babel
`comment.value` shape (delimiters stripped before regex match).

**Drift watch point captured this session:** upstream's pragma scan
walks the FLAT `file.ast.comments` list. The SWC analog requires an
anchor; for module-level pragmas the canonical position is the
leading-comment slot of the FIRST body item — matches the routing
pattern `babel-plugin-strip-runtime` already uses for banner
comments. Documented inline in `babel_plugin.rs` (`comments` field
doc + `scan_jsx_pragma_comments` doc).

**§2.4 closed (this session):**
- `state.rs` holds `State` + inner shapes (`CompiledImports`,
  `ImportedCompiledImports`, `PragmaState`, `CleanupAction`,
  `CleanupKind`, `CacheSlot`) with `pub(crate)` fields and
  read-only `pub fn` getters. Init-time mutators (`set_opts`,
  `set_import_sources`, `ensure_compiled_imports`,
  `set_classic_jsx_pragma`, `set_pragma_jsx`,
  `set_pragma_jsx_import_source`, `queue_cleanup`) cover the
  non-cache-captured fields. The `MutationRecorder::apply` impl
  block lives in `state.rs` (per PLAN.md §3.9.8 "same module as
  State") so it has same-module write access without exposing
  fields broader than `pub(crate)`.
- `mutation_recorder.rs` holds 5-variant `StateDiff`
  (IncludedFilesPush, CompiledImportsAppend, SheetsInsert,
  CssMapInsert, IgnoreMemberExprMark — reconciled per
  STATE_MUTATIONS.md), 5-variant `ApiKind` (ClassNames, Css,
  Keyframes, Styled, CssMap with `from_imported_name` resolver),
  and `MutationRecorder` (Vec<StateDiff> diff log with `new`,
  `diff_log`, `drain_diff_log`, and a `pub(crate)` `push_diff`
  hook the apply impl uses). Both enums derive serde for the
  Phase 5 §5.3 `cache.bin` lock; a JSON round-trip test stands in
  for the postcard test until that dep lands.
- `babel_plugin.rs` `BabelPluginVisitor` adds `recorder` field.
  `record_compiled_import` routes per-API pushes through
  `MutationRecorder::apply(StateDiff::CompiledImportsAppend, &mut state)`.
  Pragma writes use `state.set_*` mutators. The encapsulation lint
  `grep -rEn 'state\.[a-z_]+\.{push,set,add,insert,remove,extend}'`
  outside `state.rs`/`mutation_recorder.rs` returns zero matches.
- Final test state: `cargo test -p babel-plugin --lib` → 43/43
  pass (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin).
  Both parity harnesses still 1284/1284.

§2.5 (this session): Phase 2 exit gate met.
`BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
bun test parity-harness/babel-plugin/harness.test.ts` →
**954/954 pass** (477 full-corpus parity + 477 full-corpus
determinism). Strip-runtime corpus untouched (1132/1132).
Dispatcher is recognition-only, so SWC output is byte-identical
to Babel for every fixture where Babel pass-through-round-trips
through prettier; fixtures where Babel transforms and SWC stays
pass-through assert NOT-equal under `expectedToFail` and
correctly fail-as-expected.

### §2.3(b) follow-up — dangling sub-checkpoint, NOT a Phase 2 gate

Two AST/comment-store mutations marked `// §2.3(b):` in
`crates/babel-plugin/src/babel_plugin.rs`. Both are deferred per
the §2.3(a) hand-off contract:

1. `path.remove()` of the classic-pragma `jsx` specifier
   (upstream's `findClassicJsxPragmaImport.path.remove()`).
   Hides the classic pragma from the SWC analog of
   `@babel/plugin-transform-react-jsx`. NOT load-bearing for
   any §2.3 / §2.4 / §2.5 verification gate; load-bearing for
   §6.5 (css-prop) where the pragma drives output divergence.
2. Filter the matched JSX-pragma comment out of
   `comments.get_leading(first_body_item.span.lo)` — SWC
   analog of upstream's `file.ast.comments` /
   `body[0].leadingComments` filter. Same gating as (1).

Concrete shape decision pending the first §2.3(b) commit:
these are NOT `StateDiff` variants (AST/comment-store, not
cache-replay-relevant). Likely shape: extend `state.queue_cleanup`
to accept richer `CleanupAction` variants (specifier-remove with
node identity, comment-filter at BytePos), drained in
`Program::exit`. Bundle this work with the first Phase 6 handler
that consumes pragma divergence (§6.5 css-prop) so the pair lands
together end-to-end-testable.

Other §2.3-region work that's gated (NOT urgent):
- `Program::exit` `appendRuntimeImports` + banner +
  `pathsToCleanup` loop — lands alongside the first real Phase 6
  handler (no point shipping runtime imports without anything that
  needs them).
- `ImportDeclaration` specifier removal — same AST-mutation
  channel as §2.3(b).
- `is_compiled.rs` / `is_jsx_function.rs` /
  `normalize_props_usage.rs` predicate ports — gated until the
  first handler that consumes them.

**Important constraint (re-confirmed by user this session):** SWC
tears down the WASI instance between `transformSync` calls. The
`BabelPluginVisitor` is allocated fresh per `process(...)` entry —
no module-level state, no static caches, no `lazy_static` holding
plugin-state. The Phase 5 `cache.bin` design (PLAN.md §3.9.10) is
the only viable cross-transform channel and uses the filesystem,
not memory.

**Important constraint (re-confirmed):** SWC tears down the WASI
instance between `transformSync` calls. The Phase 5 `cache.bin`
design (PLAN.md §3.9.10) accommodates this by reading at
`Program::enter` and writing at `Program::exit` per transform — the
filesystem is the only viable cross-transform channel. NO in-memory
cross-transform caching anywhere in the Rust port.

**Prerequisites met:** all of Phase 0 except probes 9 and audit
(both Phase 5 gates, not Phase 1).

**Last completed:** §2.5 (Phase 2 exit gate). Final state on
sign-off (this session):
- `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43
  (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin).
- `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib`
  → 56/56 (unchanged from §1.9).
- `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31
  (unchanged).
- `bun test parity-harness/strip-runtime/harness.test.ts` →
  1132/1132 (unchanged from §1.9).
- `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
  bun test parity-harness/babel-plugin/harness.test.ts` → 954/954
  (full-corpus pass-through parity + full-corpus determinism).
- Encapsulation lint
  `grep -rEn 'state\.[a-z_]+\.(push|set|add|insert|remove|extend)'
  crates/babel-plugin/src --include '*.rs' | grep -vE
  'state\.rs|mutation_recorder\.rs' | wc -l` → 0.
- `RUSTFLAGS="" cargo build -p babel-plugin
  -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`
  clean (modulo a documented `#[allow(dead_code)]` on
  `state::CacheSlot` — Phase 5 §5.3 placeholder).

### Hand-off — what the next session should do

**Active checkpoint:** Phase 4 §4.6 PARTIAL ☑ landed today. The
post-CSSOutput template builders (`build_compiled_component`,
`compiled_template`) and three leaf utils (`get_jsx_attribute`,
`get_runtime_class_name_library`, `hoist_sheet`) are 1:1 ported and
unit-tested. The visitor dispatch wiring is NOT done — that's
blocked on Phase 5 §5.4 (resolve_binding) + §5.6 (evaluate_expression)
because the css_builders SHELL's `unimplemented!()` stubs panic at
the first reach.

Recommended order for the next session(s):
1. **§4.7** — update `packages/parcel-transformer/src/index.ts` to
   make a single `transformSync` call per PLAN.md §8 (Parcel
   wrapper drains sidecars). **Independently shippable** — does
   not depend on Phase 5/6 work. Smallest tractable next checkpoint.
2. **Phase 5 §5.1–§5.6** — port the resolver + evaluator subtree.
   This unblocks the §4.4 SHELL stubs and then §4.6's visitor
   dispatch wiring becomes feasible.
3. **§4.6 finalisation** — back-fill the visitor dispatch sites in
   `babel_plugin.rs` (css-prop, classNames, cssMap, styled, xcss-prop
   handlers) using the §4.6 builders + the freshly-ported
   evaluate/resolve. Port `build_styled_component.rs` (with
   `is_prop_valid` table verbatim) here too.
4. **§4.8** — Phase 4 exit gate: keyframes / css / cssMap fixtures
   byte-clean.

§4.6 deliberately deferred (NOT urgent at this checkpoint;
required for §4.6 finalisation):
- `build_styled_component.rs` — needs `pickFunctionBody`,
  `@emotion/is-prop-valid` (a known-prop lookup table verbatim),
  `findOpenSelectors` regex, larger forwardRef hand-built template.
- `build_display_name.rs` — addComponentName-driven leaf.
- `append_runtime_imports.rs` — Program::exit prepend machinery.
- `compat/template.rs` — only port if a future fixture genuinely
  needs full template-string parsing (the §4.6 builders are
  hand-built, not template-parsed, so this is currently unused).
- Visitor dispatch sites — blocked on Phase 5 §5.4–§5.6.

Phase 4 §4.4 deliberately deferred (NOT urgent, NOT §4.5 blockers):
- `arrow.body = firstExpression` mutation in
  `extract_object_expression` — Phase 5 §5.6 mutable-walker.
- `nextQuasis.value.raw = after.css` mutation in
  `extract_template_literal` — same Phase 5 §5.6 reason.
- `extract_member_expression`'s map-cache miss path — Phase 6 §6.3
  visitCssMapPath wiring.

See `crates/babel-plugin/CSS_BUILDERS_DEPS.md` for the
CSS-port-agent collaboration record (RESOLVED 2026-05-04).

Phase 4 §4.3 deliberately deferred (NOT urgent, NOT blockers):
- `Expr::Paren` transparency edge cases — only one corpus fixture
  exercises this (`(a && b) || c`); test passes. If a future fixture
  surfaces a paren-drop case the current dispatch can't decide
  (e.g., a deep nested conditional inside a paren), that's a real
  Drift event worth diagnosing.
- Synthetic-AST fallbacks (str without `raw`, num without `raw`) —
  the corpus exclusively exercises parsed nodes which always carry
  `raw`. The fallback path emits naïvely; when a real consumer
  case lands a synthetic AST node, port the upstream `_jsesc`
  details.
- TS type-args on JSXOpeningElement and inner-comment threading on
  JSXEmptyExpression — see the §4.3 closure summary above.

See `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` for the
full per-call-site coverage manifest, divergence table, and
fixture-addition workflow.

Other items deliberately deferred (NOT urgent, NOT blockers):
- §2.3(b) — two AST/comment-store mutations from §2.3(a). Bundles
  with the first §6.5 css-prop handler that consumes pragma
  divergence.
- §0.10 / §0.11 / §0.12 — Phase 0 hardening tasks. Land before
  Phase 5 ships, not before it starts.

For verification commands (cold pickup), see the "Verifying the
current state from a cold pickup" block at the top of this file —
that's the canonical recipe; this hand-off block is the work
direction.

### Phase 1 closure (prior session, kept for context)

§1.9 exit-gate state: `cargo test -p babel-plugin-strip-runtime
--lib` → 56/56 pass; `bun test
parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass
(91 determinism — 41 hand-curated + 50 sampled synth — and 1041
parity — 41 hand-curated + 1000 synth, with 9 hand-curated fixtures
still gated `expectedToFail`: 3 on Phase 2 compiledBabelPlugin/bake
parity, 6 on Phase 7 directive-prologue blank-line). Synthesised
fixtures are deterministic — re-running
`bun parity-harness/strip-runtime/synthesize-fixtures.mjs --count
1000` produces a byte-identical corpus.

**§1.8 surfaced one real port defect.** Multi-component fixtures
sharing an atomic-CSS declarator (e.g. two `<div css={{display:'inline-block'}}>`
inputs that bake to a shared `_` binding) drove a divergence at synth
fixture 42: Babel pushes the rule once because `binding.path.remove()`
inside `removeStyleDeclarations` invalidates the scope entry in-place,
so the second visit's `getBinding(name)` returns undefined. The Rust
port deferred binding removal to `Program::exit`, leaving the binding
queryable for the second visit and pushing a duplicate rule. Fix:
`crates/babel-plugin-strip-runtime/src/compat/scope.rs`
`mark_for_removal` now clears the cached string value on the binding
entry while keeping the location so `apply_removals` still drops the
declarator. New unit test `mark_for_removal_invalidates_subsequent_lookup`
locks the parity contract.

**End-of-session notes for next pickup:**

- Architect re-pinned the upstream `packages/babel-plugin-strip-runtime`
  source. Audited every file (`index.ts`, `types.ts`, all five
  `utils/*.ts`) against the existing port — byte-identical. No
  re-port needed.
- Two upstream JS-side fixes applied while resolving the audit-blocked
  harness: (a) `packages/css/src/index.ts` now uses
  `export { type AfterInterpolation, type BeforeInterpolation }`
  (interfaces aren't runtime values); (b)
  `packages/babel-plugin-strip-runtime/src/__tests__/{strip-runtime-source-code,strip-runtime-transpiled-code}.test.ts`
  had a stale `regexToFindRequireStatements` literal — `@compiled\/`
  bumped to `@compiled\/` to match the renamed constants.
- Side-effect of the rename: `DEFAULT_IMPORT_SOURCES` is
  `['@compiled/react', '@atlaskit/css']`. Fixtures driven from any
  source string still using `@compiled/react` go untransformed (no
  CC/CS wrappers, every `expectsError` fixture's throw-path is dead).
  `generate-fixtures.mjs` was updated accordingly. **If you regenerate
  fixtures or write new ones, use `@compiled/react` as the import
  source.**

---

## Phase 0 — Prerequisites and parity harness

> **Goal:** stand up the verification oracle, validate every load-
> bearing architectural assumption, before any port code lands.
>
> **Phase exit gate:** ☑ — all prerequisites for Phase 1 met.
> Probes 9 (resolver matrix) and §0.10 (audit script) defer to
> Phase 5; they do not block Phase 1.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §0.1 | ☑ | Pin `swc_core@54.0.0` and `prettier@2.8.8` in `crates/PARITY_VERSIONS.md` and root `package.json` `overrides` | claude-2026-05-02 | `crates/PARITY_VERSIONS.md` (SWC + Prettier sections), `package.json` `overrides` block | `bun pm ls \| grep swc/core` shows `1.15.8`; `cat crates/PARITY_VERSIONS.md \| grep '54.0.0'` non-empty |
| §0.2 | ☑ | Scaffold `crates/babel-plugin` and `crates/babel-plugin-strip-runtime` cdylib + rlib for `wasm32-wasip1` | claude-2026-05-02 | `crates/babel-plugin/{Cargo.toml,src/lib.rs}`, `crates/babel-plugin-strip-runtime/{Cargo.toml,src/lib.rs}`, both built `.wasm` artefacts under `crates/target/wasm32-wasip1/release/` | `RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release` produces ~1.4 MB `.wasm` per crate |
| §0.3 | ☑ | Build `crates/babel-plugin/STATE_MUTATIONS.md` enumerating every state-mutation site and reconciling §3.9.8's `StateDiff` enum | claude-2026-05-02 | `crates/babel-plugin/STATE_MUTATIONS.md`, PLAN.md §3.9.8 amended to 5-variant enum (added `IgnoreMemberExprMark`) | Re-run `grep -rEn 'state\.(includedFiles\|compiledImports\|sheets\|cssMap\|ignoreMemberExpressions)\b' packages/babel-plugin/src/` and confirm count matches the file's table |
| §0.4 | ☑ | Build the §3.9.14 probe plugin and run probes 1, 2, 3, 4, 5, 6, 7 on Windows | claude-2026-05-02 | `crates/babel-plugin-phase0-probes/`, `phase0-probes/probes.test.ts`, `crates/babel-plugin/PHASE0_FINDINGS.md` | `bun test phase0-probes/probes.test.ts` → 7 pass / 0 fail |
| §0.5 | ☑ | Document the WASI `/cwd` mount semantics in PLAN.md §3.2 | claude-2026-05-02 | PLAN.md §3.2 corrected (host-side path translation contract added) | `cat plugins/PLAN.md \| grep -A2 '/cwd'` shows the mount documentation |
| §0.6 | ☑ | Stand up `parity-harness/strip-runtime/` skeleton — Babel + SWC engines, fixture loader, bun test driver | claude-2026-05-02 | `parity-harness/README.md`, `parity-harness/strip-runtime/{engines.ts,harness.test.ts,fixtures/*.json}` | `bun test parity-harness/strip-runtime/harness.test.ts` → 6 pass / 0 fail |
| §0.7 | ☑ | Babel-vs-itself determinism baseline (3 fixtures × 3 runs) | claude-2026-05-02 | The 3 `Babel determinism baseline` tests in `parity-harness/strip-runtime/harness.test.ts` | Same harness command above; the 3 determinism tests pass |
| §0.8 | ☑ | Seed 3 representative strip-runtime fixtures (full extraction → Phase 1 §1.1) | claude-2026-05-02 | `parity-harness/strip-runtime/fixtures/{extract-automatic-passthrough,extract-automatic-stripped,extract-classic-stripped}.json` | `ls parity-harness/strip-runtime/fixtures/*.json \| wc -l` ≥ 3 |
| §0.9 | ☑ | Confirm harness can detect drift (the 2 `expectedToFail` parity tests pass *because* SWC passthrough diverges from Babel — proves the oracle is sensitive) | claude-2026-05-02 | The 2 `expected-to-fail` tests in the harness | Same harness command; the 2 expected-to-fail tests pass |
| §0.10 | ☐ | Build `scripts/audit-included-files.ts` — instrument `onIncludedFiles`, count out-of-cwd outliers across consumer monorepo | — | `scripts/audit-included-files.ts`, `crates/babel-plugin/INCLUDED_FILES_AUDIT.md` (target ≤100 outliers per workspace) | `bun run scripts/audit-included-files.ts <consumer-workspace>` produces an outlier count; documented per workspace |
| §0.11 | ☐ | Resolver difference matrix — `enhanced-resolve@5.x` vs `npm resolve.sync` vs `oxc_resolver` | — | `crates/babel-plugin/RESOLVER_MATRIX.md` | A matrix of resolution requests with three engines' results; gaps documented |
| §0.12 | ☐ | Run probes 1–7 on Linux + macOS (CI matrix) | — | CI job entry; results appended to `crates/babel-plugin/PHASE0_FINDINGS.md` "Cross-platform results" section | CI passes on `ubuntu-latest` and `macos-latest`; results captured |

§0.10 and §0.11 are Phase 5 gates — not blockers for Phase 1.
§0.12 is a hardening task — not a blocker for Phase 1, but should be
done before declaring Phase 0 fully signed off across the platform set.

### Phase 1 findings (write-once notes)

- **§1.8 — shared-binding scope-invalidation parity bug.** When two
  components reference the same atomic-CSS declarator (e.g. both have
  `display: inline-block`, baking to a shared `const _ = "._1e0c1o8l{display:inline-block}"`),
  Babel pushes the rule once because `binding.path.remove()` inside
  `removeStyleDeclarations` invalidates the scope entry in-place — the
  second visit's `parentPath.scope.getBinding('_')` returns undefined.
  The Rust port originally deferred all removals to `Program::exit`'s
  `apply_removals`, so the second visit still saw a live binding and
  pushed a duplicate. Fix: `mark_for_removal` clears the cached string
  value on the binding entry while keeping its `BindingLocation` so
  `apply_removals` can still drop the declarator. The hand-curated
  41-fixture set never exercised this path (every fixture has a single
  component); the §1.8 synth corpus surfaced it on fixture
  `synth-00042-automatic-extract-adds-require`.

- **§1.5 — `.compiled.css` postcss `sort()` deferred to Phase 4.**
  Babel's `extractStylesToDirectory` write path calls
  `sort(styleRules.sort().join('\n'), sortConfig)` from `@compiled/css`.
  The Rust port lives at `crates/css/src/sort.rs` and depends on
  `compiled-css` + the postcss-* crates (`postcss-core`,
  `postcss-discard-duplicates`, `cssnano-preset-default`, …).
  Pulling that into `babel-plugin-strip-runtime` would inflate the
  WASM binary several-fold *now*, before Phase 4 wires the same
  CSS Rust port into `babel-plugin` proper (PLAN.md §3.5 says both
  plugins link the CSS crate). For §1.5 the file write uses the
  JS-level pre-sort only (`styleRules.sort().join('\n')`), which
  produces a non-empty file with the rule set complete. The
  postcss-level `sort(stylesheet, sortConfig)` will land alongside
  Phase 4. The harness gate for §1.5 is JS-byte parity (the
  `import './<basename>.compiled.css'` injection on the AST side);
  `.compiled.css` contents are NOT diffed against Babel today.
  When Phase 4 lands `crates/css` into `babel-plugin`, the same dep
  becomes available here and the postcss sort lands as a single
  follow-up commit.
- **§1.5 — `swc_common::errors::HANDLER` for plugin-side throws.**
  A raw `panic!()` in a SWC plugin is wrapped by the runner as
  `plugin failed to invoke plugin on '<filename>'` — the original
  message is dropped. Use
  `HANDLER.with(|h| h.struct_span_err(span, msg).emit())` and
  return early for any error that needs to be visible to the host
  (e.g. the `Source directory '<source>' was not found relative to
  source file ('<sourceFileName>')` throw at A02). HANDLER-emitted
  errors propagate cleanly through `swc.transformSync` to the
  host's catch.
- **§1.5 — host responsibilities the harness inlines.** Two
  concerns the production Parcel transformer wrapper will own
  (PLAN.md §3.9.13) but the harness re-implements: (a)
  `extractStylesToDirectory` writes use process.cwd() as the WASI
  `/cwd` preopen, so the harness `process.chdir`s into a scratch
  dir around the SWC call to scope writes; (b) `compiledRequireExclude`
  needs `<callScratch>/style-rules.json`, so the harness mkdirs a
  per-call dir under `_scratch`, host-translates to `/cwd/<rel>`
  via `toWasiPath`, threads it as `callScratch` plugin config, and
  rmSyncs in `finally`. The plugin only ever sees `/cwd`-prefixed
  paths (PLAN.md §3.2 contract).
- **Phase 7 breadcrumb — `/*#__PURE__*/` duplicates after CC-replacement.**
  When `StripRuntimeVisitor` replaces `/*#__PURE__*/_jsxs(CC,...)` with
  the inner `/*#__PURE__*/_jsx('div',...)`, SWC's codegen emits TWO
  pure annotations (`/*#__PURE__*/ /*#__PURE__*/ _jsx(...)`) even
  though the leading-comment store has only one entry. Probe
  evidence: `take_leading(outer.span.lo)` returned the single outer
  PURE; `get_leading(outer.span.lo)` after the drop is `None`;
  `get_leading(inner.span.lo)` is `Some([PURE])`. So the store IS
  clean — the duplication originates in `swc_ecma_codegen`'s
  multi-span emit path (CallExpr + callee MemberExpr + leading-Ident
  all triggering `emit_leading_comments_of_span`). For §1.4 the
  harness post-processes the doubled PURE via
  `parity-harness/strip-runtime/engines.ts`'s
  `/(\/\*#__PURE__\*\/\s+)\1+/g` collapse; Phase 7 should replace
  this workaround by either pruning at every relevant sub-span or
  relocating the inner's leading comments to a single canonical
  position before printing.

- **`extractStylesToDirectory` writes to disk during harness runs.**
  The strip-runtime plugin's `Program.exit` calls `mkdirSync` +
  `writeFileSync` against `<babel.cwd>/<dest>/<rel>.compiled.css`. The
  Jest test mocks `fs`; Bun does not. The harness now passes
  `babel.cwd = parity-harness/strip-runtime/_scratch` for those
  fixtures, so writes are scoped + gitignored.
- **`expectsError` fixture schema.** Four fixtures (A02, B05, B06, B07)
  assert that the plugin throws. The harness now treats the
  Babel-side throw (with matching `expectsError.babelMessage`) as the
  determinism oracle for those fixtures, and requires the SWC side to
  throw the same message once the §1.4 port lands.

### Phase 0 findings (write-once notes)

- **WASI mount path is `/cwd`, not `/`.** `env::current_dir()`
  returning `Ok("/")` is cosmetic. Plugin must use `/cwd/<rel>` literal
  paths. PLAN.md §3.2 corrected. See `crates/babel-plugin/PHASE0_FINDINGS.md`.
- **Build trap:** user-global `RUSTFLAGS` includes `-C lto=thin` which
  breaks proc-macro deps. Always build SWC plugin crates with
  `RUSTFLAGS=""`. Documented in both Cargo.toml files.
- **`StateDiff` enum is 5 variants, not the 4 sketched in §3.9.8.**
  `IgnoreMemberExprMark` was missing. PLAN.md amended.
- **Bun's caret resolution drifts past `package.json` pins.** Same
  pattern as the existing CSS-port discovery — use root `overrides` to
  hold every byte-affecting dep at the exact reference version.
  `@swc/core@1.15.8` and `prettier@2.8.8` added to the `overrides`
  block.

---

## Phase 1 — `babel-plugin-strip-runtime` 1:1 port

> **Goal:** ship a `wasm32-wasip1` SWC plugin that is byte-equivalent
> to `packages/babel-plugin-strip-runtime` after prettier
> normalisation across all 38 existing tests + ≥1000 synthesised
> fixtures.
>
> **Why this plugin first:** smaller (6 source files, ~600 LOC), no
> cross-file resolution, no `transformCss` calls. Validates the
> end-to-end toolchain (WASI build, prettier oracle, sidecar
> manifests, SWC ABI) on a tractable target.
> `packages/parcel-transformer/` is an EXAMPLE consumer shape — not a
> deliverable of this port. The plugin contract is `(source string +
> PluginOptions) → (transformed code + sidecar JSON)`; how downstream
> bundlers wire that up is out of scope.
>
> **Phase exit gate:** all 38 strip-runtime tests pass through the
> parity harness AND ≥1000-file synthesised fixture run is
> zero-divergence.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §1.0 | ☑ | Extract all 38 fixtures from the existing strip-runtime test files into `parity-harness/strip-runtime/fixtures/*.json` | claude-2026-05-02 | 38 fixture JSON files (`A01`–`A04`, `B01`–`B10`, `C01`–`C16`, `D01`–`D08`) under `parity-harness/strip-runtime/fixtures/`; generator at `parity-harness/strip-runtime/generate-fixtures.mjs`; `@babel/preset-env` + `@babel/preset-typescript` added to `packages/babel-plugin-strip-runtime/package.json` devDeps (resolves the dep drift) | `ls parity-harness/strip-runtime/fixtures/*.json \| wc -l` reports 41 (3 phase-0 seeds + 38 new); `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass |
| §1.1 | ☑ | Port `utils/to_uri_component.rs` (URL-encode + escape `!` to `%21`) | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/utils/to_uri_component.rs`, `crates/babel-plugin-strip-runtime/src/utils/mod.rs`, `lib.rs` declares `pub mod utils;` | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib to_uri_component` → 10/10 pass; cross-checked against JS `encodeURIComponent(x).replace(/!/g,'%21')` over 12 inputs (CSS rules, full unreserved set, `!`, UTF-8 multibyte, NUL, webpack loader separator) — byte-equal |
| §1.2 | ☑ | Port `utils/is_automatic_runtime.rs`, `utils/is_cc_component.rs`, `utils/is_create_element.rs` predicates | claude-2026-05-02 | three `.rs` files under `crates/babel-plugin-strip-runtime/src/utils/`, `mod.rs` declarations updated, tests construct AST nodes via `swc_core::ecma::ast` builders (no parser dep — keeps `wasm32-wasip1` build minimal) | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 32/32 pass (10 + 10 + 6 + 6) |
| §1.3 | ☑ | Port `utils/remove_style_declarations.rs` + create `compat/scope.rs` for SWC binding lookup | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/compat/{mod.rs,scope.rs}` (module-scope binding index with deferred `apply_removals`), `crates/babel-plugin-strip-runtime/src/utils/remove_style_declarations.rs` (handles `React.createElement(CS, ..., [..])`, `_jsx(CS, { children: [..] })`, `<CS>{[..]}</CS>`), `lib.rs` declares `pub mod compat;` | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 44/44 pass (10 to_uri + 10 is_automatic_runtime + 6 is_cc + 6 is_create_element + 6 scope + 6 remove_style_declarations) |
| §1.4 | ☑ | Port `lib.rs` entry + dispatcher: `Program::exit`, `ImportSpecifier`, `JSXElement`, `CallExpression`. Lock `Program::exit` ordering (banner → preserveLeadingComments → require-OR-css-OR-metadata, never two) | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/lib.rs` rewritten — `StripRuntimeVisitor` (CC/CS replacement, `take_leading` mirroring Babel's `path.node.leadingComments=null`, ImportSpecifier filter for CC/CS), `make_require_stmt` + `first_non_directive_index` for the `styleSheetPath` injection path, `unwrap_paren` in `is_automatic_runtime` for CommonJS-interop callees `(0, _jsxRuntime.jsx)(...)`. Visitor scope cleanup runs BEFORE require-injection so binding indices stay valid. The auto-`expectedToFail` heuristic is removed; per-fixture `expectedToFail` + `failureReason` (in `generate-fixtures.mjs`'s `EXPECTED_TO_FAIL` map) now gates the 13 fixtures that depend on later phases — 4 on §1.5 (extractStylesToDirectory file write), 3 on Phase 2 (compiledBabelPlugin errors / babelJSXImportSource bake), 6 on Phase 7 (directive-prologue blank-line). | `RUSTFLAGS="" cargo build -p babel-plugin-strip-runtime --target wasm32-wasip1 --release` clean; `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass (41 determinism + 41 parity, 13 of which gated `expectedToFail` with explicit reasons) |
| §1.5 | ☑ | Sidecar handlers: `compiledRequireExclude=true` writes `<callScratch>/style-rules.json`; `extractStylesToDirectory.dest` writes `.compiled.css` files via `/cwd` preopen | claude-2026-05-03 | `crates/babel-plugin-strip-runtime/src/lib.rs` extends `PluginOptions` with `call_scratch` + `source_file_name` (host-threaded — Babel reads `file.opts.generatorOpts.sourceFileName` natively, SWC has no equivalent), adds `parse_name`/`dirname`/`path_join` helpers, `validate_dest_under_cwd` (rejects absolute, drive-prefixed, `..`-escape paths) called at plugin entry, `make_side_effect_import` (with banner-span re-anchoring trick), `StyleRulesSidecar` v1 (`{version, rules}`); Program::exit branch wires both side-effect outputs and uses `swc_common::errors::HANDLER` for the source-not-found error path so the message survives plugin-runner wrapping. `parity-harness/strip-runtime/engines.ts` threads `sourceFileName` + `callScratch` (mkdir + cleanup, host-translated `/cwd/<rel>` form) into the SWC plugin config and `process.chdir`s into `_scratch` for `extractStylesToDirectory` fixtures so plugin writes scope under the WASI preopen. `generate-fixtures.mjs` ungates A01–A04. | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 55/55 pass (44 prior + 11 §1.5 helpers); `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass; manual inspection: `_scratch/dist/app.compiled.css` non-empty (4 sorted rules), per-call `<callScratch>/style-rules.json` non-empty with v1 shape `{"version":1,"rules":[...]}` |
| §1.6 | ☑ | Lock `plugins/SIDECAR_SCHEMA.md` v1 (PLAN.md §7) | claude-2026-05-03 | `plugins/SIDECAR_SCHEMA.md` (v1 schema for `style-rules.json` §2, `included-files.json` §1, `cache.bin` §3 + plugin-config shape §4); Rust `StyleRulesSidecar` struct doc-comment in `crates/babel-plugin-strip-runtime/src/lib.rs` cites SIDECAR_SCHEMA.md §2; harness reader comment in `parity-harness/strip-runtime/engines.ts` cites the same. Versioning policy + drift watch points documented. | `plugins/SIDECAR_SCHEMA.md` exists; `grep -n SIDECAR_SCHEMA crates/babel-plugin-strip-runtime/src/lib.rs parity-harness/strip-runtime/engines.ts` non-empty (Rust writer + JS host both back-reference); harness 82/82 unchanged. |
| §1.7 | — | ~~Inline the SWC wrapper in `packages/parcel-transformer/`~~ | — | Parcel-transformer integration is an EXAMPLE consumer shape (`plugins/PARCEL_USAGE_EXAMPLE.md`), not a Phase 1 deliverable. Removed from gate. | n/a |
| §1.8 | ☑ | Generate ≥1000 synthesised already-baked fixtures (run JS babel-plugin against random inputs to produce CC/CS-wrapped code, freeze as fixtures) | claude-2026-05-04 | `parity-harness/strip-runtime/synthesize-fixtures.mjs` (deterministic mulberry32 seeded generator), `parity-harness/strip-runtime/fixtures/synthesized/synth-NNNNN-*.json` × 1000; harness loader updated to recurse into subdirs (`walkFixtureFiles`); 50-sample determinism stride to keep wall-clock tractable on the synth tail. Surfaced one real port defect — multi-component fixtures sharing an atomic-CSS declarator pushed the shared rule twice; fixed in `crates/babel-plugin-strip-runtime/src/compat/scope.rs::mark_for_removal` which now clears the cached binding value on mark (parity with Babel's immediate `binding.path.remove()`); new unit test `mark_for_removal_invalidates_subsequent_lookup` locks it in. | `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass (91 determinism + 1041 parity). Re-running the generator twice produces a byte-identical corpus (`shasum` of all 1000 fixture files matches across runs). |
| §1.9 | ☑ | **Phase 1 exit gate:** all checkpoints above closed; full corpus is byte-clean | claude-2026-05-04 | This STATUS.md updated with the sign-off summary (no separate `phase1-signoff.md` — the row above + the "Resume here" block carry the closure record). | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 56/56 pass; `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass (1041 fixtures parity, zero divergence). |

---

## Phase 2 — `babel-plugin` scaffold + dispatcher

> **Goal:** stand up the visitor skeleton + state setup for the larger
> plugin. Pass-through is byte-equal before any handler logic ports.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §2.0 | ☑ | Extract all babel-plugin fixtures from `packages/babel-plugin/src/**/__tests__/*.test.ts` into `parity-harness/babel-plugin/fixtures/*.json` | claude-2026-05-04 | `parity-harness/babel-plugin/extract-fixtures.mjs` (runtime extractor — Bun.plugin loader rewrites `test-utils.ts` so `transform` records `(code, opts)` per call; Jest globals stubbed so describe/it bodies execute synchronously without assertion side-effects), `parity-harness/babel-plugin/{engines.ts,harness.test.ts}` skeleton, 477 fixture JSONs (gitignored, regenerable). 6 test files skipped: 3 use `jest.mock`/`jest.fn`/`jest.spyOn` against utility internals (`cache`, `object-property-to-string`, `module-traversal`), 1 is the perf benchmark (`__perf__/module-traversal-cache`), 2 (`errors`, `resolver`) are throw-assertion-only and don't carry byte-parity signal. | `bun test parity-harness/babel-plugin/harness.test.ts` → 120/120 pass (sampled stride). `BABEL_PLUGIN_FULL_DETERMINISM=1 bun test parity-harness/babel-plugin/harness.test.ts` → 477/477 pass (full corpus). |
| §2.1 | ☑ | Port `types.rs`, `constants.rs` (data only, no logic) | claude-2026-05-04 | `crates/babel-plugin/src/constants.rs` (1:1 port of `packages/babel-plugin/src/constants.ts`), `crates/babel-plugin/src/utils/constants.rs` (1:1 port of `packages/babel-plugin/src/utils/constants.ts`), `crates/babel-plugin/src/types.rs` (port of `types.ts` — `PluginOptions`, `State`, `Tag`, `Metadata`, `TransformResult`, with `CacheMode` custom (de)serializer for the `bool \| "file-pass"` wire shape and `IndexMap` everywhere ordering matters). Babel-only types (`NodePath`, `PluginPass`, the JS-callback `Resolver`) deliberately omitted — see module docs in `types.rs` for the resolution table. | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 5/5 pass (CacheMode bool/string round-trip, PluginOptions camelCase wire shape, default-is-all-None, full round-trip). `RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. |
| §2.2 | ☑ | Build `parity-harness/babel-plugin/{engines.ts,harness.test.ts}` mirroring strip-runtime's shape | claude-2026-05-04 | `parity-harness/babel-plugin/engines.ts` (`babelEngine` + `swcEngine` + `diffSummary` — `swcEngine` runs `babel_plugin.wasm` in pass-through mode through SWC parser/codegen + prettier round-trip), `parity-harness/babel-plugin/harness.test.ts` (Babel ↔ SWC parity describe block + Babel determinism baseline; `expectedToFail` semantics mirror strip-runtime — fixtures where Babel transforms the input assert NOT-equal vs the pass-through SWC, fixtures where prettier round-trips identically through both assert byte-equal). Stride samples by default (30 parity / 100 determinism); `BABEL_PLUGIN_FULL_PARITY=1` and `BABEL_PLUGIN_FULL_DETERMINISM=1` flip to full corpus. | `bun test parity-harness/babel-plugin/harness.test.ts` → 152/152 pass (32 parity sample + 120 determinism sample); `BABEL_PLUGIN_FULL_DETERMINISM=1` → 477/477 determinism. Full parity gate (§2.5) lights up after §2.3+ ports land. |
| §2.3 | ▶ | Port `lib.rs` entry + dispatcher visitor with stubbed handlers (no-ops that record "would have visited" in a debug log) | claude-2026-05-04 (skeleton + §2.3(a)); `—` for §2.3(b) | `crates/babel-plugin/src/lib.rs` (rewritten — `process(...)` instantiates `BabelPluginVisitor` with `PluginCommentsProxy`, threads `PluginOptions`; `run_dispatcher` is now generic `<C: Comments>` so tests can pass `SingleThreadedComments::default()`), `crates/babel-plugin/src/babel_plugin.rs` (dispatcher: `BabelPluginVisitor<C: Comments>` holds owned `State` + computed `import_sources` + comment proxy. **§2.3(a) added:** `scan_classic_jsx_pragma_import` records `pragma.classic_jsx_pragma_*` for `import { jsx }` from Compiled origins (handles bare / renamed / StringLiteral-imported shapes); `scan_jsx_pragma_comments` reads `comments.get_leading(first_body_item.span.lo)` and applies the `@jsx` / `@jsxImportSource` regexes from `compiled_utils::jsx`, setting `pragma.jsx` / `jsx_import_source` and bootstrapping `state.compiled_imports = Some(default)`. Recognition only — two AST/comment-store mutations marked with `// §2.3(b):` TODOs at the call sites the MutationRecorder will flip on. Compiled-import recognition moved from `visit_mut_program` pre-walk into `visit_mut_module_decl` to preserve upstream's `Program::enter` (pragma scan) → children walk (import-decl visitor) order. Stub `visit_mut_call_expr`/`tagged_tpl`/`jsx_element`/`jsx_opening_element` unchanged.). Drift fixed earlier in `crates/compiled-utils/src/constants.rs` (`COMPILED_IMPORT`) and `crates/compiled-utils/src/jsx.rs` ported (was missing). Remaining for §2.3(b) (gated on §2.4): (i) `MutationRecorder.queue_specifier_remove(...)` for the classic-pragma `jsx` specifier; (ii) comment-store filter dropping the matched JSX-pragma comment from `body[0].leadingComments`; (iii) generic `ImportDeclaration` specifier removal + auto-`path.remove()` when drained; (iv) `Program::exit` `appendRuntimeImports` + banner + `pathsToCleanup` loop. | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 25/25 pass (5 §2.1 types + 8 prior §2.3 skeleton + 12 new §2.3(a): 5 classic-pragma recognition, 6 comment-scan, 1 end-to-end `visit_mut_program` exercising classic-pragma + comment + ImportDeclaration walk in one pass). `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31. `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 56/56. Pass-through preserved: `bun test parity-harness/strip-runtime/harness.test.ts parity-harness/babel-plugin/harness.test.ts` → 1284/1284 pass. |
| §2.4 | ☑ | State struct with `IndexMap` everywhere, `pub(crate)` field encapsulation, `MutationRecorder::apply` as only mutator (per `STATE_MUTATIONS.md`) | claude-2026-05-04 | `crates/babel-plugin/src/state.rs` (State + CompiledImports + ImportedCompiledImports + PragmaState + CleanupAction + CleanupKind + CacheSlot moved here from types.rs; fields are `pub(crate)` with read-only public getters; init-time mutators `set_opts` / `set_import_sources` / `ensure_compiled_imports` / `set_classic_jsx_pragma` / `set_pragma_jsx` / `set_pragma_jsx_import_source` / `queue_cleanup`; `MutationRecorder::apply` impl block lives here per PLAN.md §3.9.8 "lives in the same module as State" — same-module access to private fields without exposing them broadly), `crates/babel-plugin/src/mutation_recorder.rs` (5-variant `StateDiff` enum with serde-derive for the §5.3 cache.bin lock, 5-variant `ApiKind` enum with `from_imported_name` resolver, `MutationRecorder` struct owning a `Vec<StateDiff>` diff log + `new` / `diff_log` / `drain_diff_log` / `pub(crate) push_diff`), `crates/babel-plugin/src/types.rs` (slimmed to config-only types: `PluginOptions`, `CacheMode`, `Tag`/`TagKind`, `Metadata`/`MetadataContext`, `TransformResult`; re-exports State + inner shapes for the original surface area), `crates/babel-plugin/src/babel_plugin.rs` (`BabelPluginVisitor` adds `recorder: MutationRecorder` field; `record_compiled_import` routes per-API pushes through `MutationRecorder::apply` with `StateDiff::CompiledImportsAppend`; pragma writes and bootstrap `ensure_compiled_imports` go through `State`'s init-time mutators). | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43 pass (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin: prior 25 §2.3(a) tests + 2 new §2.4 recorder-routing assertions). Encapsulation lint `grep -rEn 'state\.[a-z_]+\.(push\|set\|add\|insert\|remove\|extend)' crates/babel-plugin/src --include '*.rs' \| grep -vE 'state\.rs\|mutation_recorder\.rs' \| wc -l` → 0. Pass-through preserved: `bun test parity-harness/strip-runtime/harness.test.ts parity-harness/babel-plugin/harness.test.ts` → 1284/1284. |
| §2.5 | ☑ | **Phase 2 exit gate:** pass-through harness clean across all babel-plugin fixtures | claude-2026-05-04 | This STATUS.md row + the Resume block updated with the closure summary (no separate phase2-signoff.md — same convention as §1.9). | `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 bun test parity-harness/babel-plugin/harness.test.ts` → 954/954 pass (477 full-corpus parity + 477 full-corpus determinism). Strip-runtime corpus untouched: `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132. Encapsulation lint clean (zero matches outside `state.rs` / `mutation_recorder.rs`). |

---

## Phase 3 — Hash compatibility (consume shared `crates/compiled-utils`)

> **Goal:** prove the Rust `hash` function shared with the CSS port is
> byte-identical to JS `@compiled/utils.hash` from this plugin's
> consuming side.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §3.1 | ☑ | Confirm `crates/compiled-utils` exposes `pub fn hash(input: &str) -> String` | claude-2026-05-04 | `crates/compiled-utils/src/hash.rs:52` (signature `pub fn hash(input: &str) -> String`); re-exported via `crates/compiled-utils/src/lib.rs:34` (`pub use hash::hash;`). | `grep -n 'pub fn hash' crates/compiled-utils/src/hash.rs` → line 52; `grep -n 'pub use hash' crates/compiled-utils/src/lib.rs` → line 34 |
| §3.2 | ☑ | Build hash test-vector corpus: ASCII, UTF-8 multibyte, empty, embedded NUL, >4KB, leading/trailing whitespace, real keyframe-expression inputs | claude-2026-05-04 | `parity-harness/hash/oracle.mjs` (mulberry32 seed=1 — deterministic; re-runs produce a byte-identical corpus) → emits `crates/babel-plugin/tests/hash_corpus.json` (10037 entries, gitignored — same regenerable-corpus precedent as the §1.8 strip-runtime synth set). Composition: 4 real-call-shape entries (one per `hash()` call site in `packages/babel-plugin/src/`), ~33 categorical entries (empty / NUL variants / whitespace / UTF-8 multibyte / surrogate-pair boundaries / >4 KiB / length-tail coverage for every murmur2 `(l mod 4)` branch), 10000 random entries (5000 ASCII + 5000 valid Unicode scalar values, surrogate range skipped — JS allows lone surrogates in strings but they're not RFC-valid JSON and the consuming plugin never passes them). | `bun parity-harness/hash/oracle.mjs` writes 10037 entries; manifest version=1; entry counts match: ≥30 categorical (`grep` + recount), 10000 `random-` prefixed, 4 `real:` prefixed |
| §3.3 | ☑ | Diff Rust `hash` vs JS `hash` over the corpus + 10K random inputs | claude-2026-05-04 | `crates/babel-plugin/tests/hash_parity.rs` (4 tests: `rust_hash_matches_js_corpus` parity gate; `corpus_includes_real_call_shapes` locks the 4 call-site fingerprints; `corpus_covers_phase3_categories` locks §3.2 acceptance; `corpus_has_at_least_10k_random_entries` locks §3.3 ≥10K acceptance). Pure-Rust integration test reading the JSON oracle — no WASM bridge, no Node round-trip; cheapest shape for a pure-data parity check. Also replaces the self-referential `crates/compiled-utils/src/hash.rs::tests::known_vectors` (which the existing comment flagged for migration) with five JS-locked byte vectors pulled from the corpus (`""→"0"`, `"a"→"14mfbry"`, `"abcd"→"aougpt"`, `"color: red"→"1wszpi4"`, `"@media (max-width: 100px)"→"w2cthn"`) — fast in-process smoke alongside the JSON-driven gate. | `RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity` → 4/4 over 10037 entries; `RUSTFLAGS="" cargo test -p compiled-utils --lib hash` → 7/7 |
| §3.4 | ☑ | **Phase 3 exit gate:** zero divergence | claude-2026-05-04 | This STATUS.md updated (Phase 3 closure summary + this row + §3.1–§3.3 rows). | Phase 3 final-state command outputs preserved in the "Phase 3 closure summary" block above (cargo + bun parity harnesses; sibling suites unchanged) |

---

## Phase 4 — `buildCss` + direct synchronous `transformCss` Rust call

> **Goal:** port `utils/css-builders.ts` and link the parallel-agent's
> Rust `transform_css` directly. Single-pass plugin, no scan/apply.
>
> **Hard preconditions:** Phase 3 green; Rust `transform_css` shipped
> as a callable `pub fn`; `compat/generator.rs` coverage manifest
> exists.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §4.1 | ☑ | `transform_css` integration parity test — every JS-corpus input produces byte-identical Rust output from this plugin's perspective | claude-2026-05-04 | `parity-harness/transform-css/oracle.mjs` (imports `@compiled/css.transformCss` directly; pins `BROWSERSLIST_CONFIG` to `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` — the EXACT production AFM pin per `BROWSER_LIST_FROM_AFM.md`, resolves to the 14-entry list `and_chr 144 / chrome 144..140 / edge 144..143 / firefox 147..146 / ios_saf 26.2..26.1 / safari 26.2..26.1` under workspace-pinned `caniuse-lite@1.0.30001766` + `browserslist@4.24.2`. Unsets `BROWSERSLIST`, `AUTOPREFIXER`, and `COMPILED_CSS_ENGINE`) → emits `crates/babel-plugin/tests/transform_css_corpus.json` (gitignored — regenerable). 30 hand-curated CSS fixtures from `crates/parity-runner/corpus/transform-css/` × 4 opts permutations spanning the babel-plugin's real call shape from `packages/babel-plugin/src/utils/{transform-css-items,build-styled-component}.ts` (`{}`, `{optimizeCss:false}`, `{increaseSpecificity:true}`, `{classHashPrefix:'x'}`). **120/120 entries byte-equal** with `css::transform_css`. New `[dev-dependencies]` entry on `crates/css` in `crates/babel-plugin/Cargo.toml` (will promote to `[dependencies]` when §4.6 wires `transform_css` into the visitor). | `RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration` → 3/3 (120/120 parity, no expected-to-fail). 5 consecutive runs all green confirms the env-race fix is stable. Sibling suites unchanged. |
| §4.2 | ☑ | Build `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` enumerating every AST node kind reachable from `keyframes(...)` (and any other `generate(...)` call site) in the consuming monorepo | claude-2026-05-04 | `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` (per-call-site coverage manifest + comment-axis fixtures + real-divergence table); `parity-harness/compat-generator/{fixtures.json,oracle.mjs}` (in-tree input manifest + JS oracle that emits `crates/babel-plugin/tests/compat_generator_corpus.json`, gitignored, regenerable; pin guard fail-fasts on `@babel/generator` / `@babel/parser` drift); `crates/babel-plugin/src/compat/{mod.rs,generator.rs}` (stub `generate(&Expr) -> String` that `unimplemented!()`s — by design, callers know §4.3 is a hard prereq); `crates/babel-plugin/tests/compat_generator_integration.rs` (3 tests: corpus shape lock + SWC parse coverage GREEN at §4.2 ship; byte-parity assertion `#[ignore]`d until §4.3 ports the line-for-line generator). Pinned `@babel/generator@7.23.0` + `@babel/parser@7.29.2` (AFM-resolved) in root `package.json#overrides` AND promoted both to top-level devDependencies (workspace's bun-isolated dep layout doesn't symlink `node_modules/@babel/generator`, so without the devDep promotion `require('@babel/generator')` walked PAST the workspace and grabbed `Documents\projects\node_modules\@babel\generator@7.28.5` — a sibling tree's copy. Caught by the oracle's pin guard). Both pins recorded in `crates/PARITY_VERSIONS.md` with the rationale for pinning the parser alongside the generator. 55 fixtures across 5 call_site axes (conditional-classname-item: 8, generic-expression: 25, jsx-key-attribute: 5, keyframes-expression: 11, variable-init: 6) — every call_site has ≥1 fixture; comment-axis fixtures (eslint-disable, ternary-inner, PURE annotations, leading/trailing block comments) seeded explicitly per the user's flag. Real divergences captured in the corpus that §4.3 must reproduce: `cond ? /* yes */'a-class' : 'b-class'` (no space after block comment), `(a && b) \|\| c → a && b \|\| c` (paren drop at precedence boundary), single-quote preservation. | `bun parity-harness/compat-generator/oracle.mjs` writes 55 entries with pin guard `@babel/generator=7.23.0, @babel/parser=7.29.2`. `RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration` → 2 passed (`corpus_shape_lock`, `corpus_input_sources_parse_under_swc`), 1 ignored (`rust_compat_generator_matches_js_corpus` waiting on §4.3). Sibling suites unchanged: 43 babel-plugin lib + 4 hash_parity + 3 transform_css_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. **Verified against current `crates/css` HEAD — §4.1 still 120/120 under both new pins** (defense-in-depth crosscheck per the parallel autoprefixer-agent collision risk note in §4.2 plan). |
| §4.3 | ☑ | Port `compat/generator.rs` covering every node kind in the manifest | claude-2026-05-04 | `crates/babel-plugin/src/compat/generator/{mod,buffer,printer}.rs`, `compat/generator/node/{mod,parentheses}.rs`, `compat/generator/generators/{mod,expressions,types,template_literals,jsx}.rs` (~1640 LOC across 10 files mirroring upstream `lib/{index,buffer,printer}.js`, `lib/node/{index,parentheses}.js`, `lib/generators/{expressions,types,template-literals,jsx}.js`). `mod.rs` exposes 4 entry points: `generate(&Expr)` (synthetic-AST callers; no comments), `generate_with_comments(&Expr, &dyn Comments)` (the keyframes / generic-expression / variable-init / conditional-classname-item axes), `generate_jsx_attribute(&JSXAttr)` and `generate_jsx_attribute_with_comments(&JSXAttr, &dyn Comments)` (the jsx-key-attribute axis — `Printer::print(&Expr,_)` can't be overloaded because SWC types JSXAttr separately from Expr). Foundation handles: paren-policy with precedence (Babel's PRECEDENCE table 1:1, including `(a && b) \|\| c → a && b \|\| c` redundancy drop), source-quote preservation via `Str.raw` / `Number.raw`, multi-line `ObjectExpression` with 2-space indent and `printList`-statement-mode pre/post newlines, comment threading via `Comments::take_leading` / `take_trailing` keyed at node `BytePos` (with the SWC quirk that same-line comments between tokens are stored as TRAILING-of-previous, not leading-of-next — handled at object-property iteration via `take_trailing(open_brace.lo + 1)` for the first-property case). Comment policy: leading space before block comments unless tail is `[` or `{`, no trailing space (matches the corpus's `cond ? /* yes */'a-class'` divergence). Indent honoured by `print_comment` via `maybe_indent` so `\n  /* leading */` lands at the right column. JSX printer (this session) handles JSXElement / JSXAttribute / JSXIdentifier / JSXMemberExpression / JSXNamespacedName / JSXEmptyExpression / JSXExpressionContainer / JSXText / JSXOpeningElement / JSXClosingElement / JSXSpreadAttribute / JSXFragment / JSXOpeningFragment / JSXClosingFragment / JSXSpreadChild — all 5 jsx-key-attribute fixtures (StringLiteral / NumericLiteral / MemberExpression / TemplateLiteral / ConditionalExpression attribute values) byte-exact via the new entry point. SWC↔Babel field-name divergence table documented inline at the head of `jsx.rs` (mechanical renames; byte output identical). The integration test's `find_first_key_jsx_attribute` walker mirrors the JS oracle's `extractJsxKeyAttribute` recursive walk verbatim (both must agree on which `key=` attribute they pluck out). Other upstream files (`flow.js`, `typescript.js`, `classes.js`, `methods.js`, `modules.js`, `statements.js`, `base.js`) intentionally NOT ported — none are reachable from the 5 call sites per CLAUDE.md "1:1 with what's reachable, not future-proofing". | `RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration` → 3/3 pass (corpus_shape_lock + corpus_input_sources_parse_under_swc + rust_compat_generator_matches_js_corpus **over 55/55 fixtures byte-exact, zero skips, zero ignored**). Wasm cdylib still builds clean (`RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`). All sibling suites unchanged: 43 babel-plugin lib + 4 hash_parity + 3 transform_css_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. **Total: 2562 tests, zero failures, zero ignored.** |
| §4.4 | ☑ | Port `utils/css_builders.rs` line-for-line (SHELL: file structure mirrors upstream 1:1; 4 hash-call-shape sites end-to-end through `compat::generator → compiled_utils::hash`; `evaluate_expression` / `resolve_binding` / `visit_css_map_path` dispatch sites stubbed with phase-citing `unimplemented!()`. Misleading verification cell amended.) | claude-2026-05-04 | `crates/babel-plugin/src/utils/{ast,css_builders,is_compiled,is_empty,manipulate_template_literal,object_property_to_string,types}.rs` (all new); `crates/babel-plugin/CSS_BUILDERS_DEPS.md` (RESOLVED — CSS-port agent shipped `add_unit_if_needed` + `css_affix_interpolation` re-exports from `crates/css/src/lib.rs`); `crates/babel-plugin/Cargo.toml` (`css` promoted from `[dev-dependencies]` to `[dependencies]`; `regex`/`once_cell` workspace deps added); `crates/babel-plugin/src/types.rs` (`Metadata::reborrow_with_context` + `reborrow` methods land the JS `{ ...meta, ... }` analog under Rust borrow rules). Verification gate: hash-call-shape sites end-to-end clean (3 unit tests in `utils::css_builders::tests` assert each emitted name matches `hash(generate(...))`); evaluate / resolve / visitCssMap stubbed pending Phases 5/6; `addUnitIfNeeded` / `cssAffixInterpolation` wired through `css::` re-exports. **The full byte-clean gate (`harness fixtures exercising keyframes`/`css`/`cssMap`) is the §4.8 phase exit gate, NOT §4.4.** | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 75/75 (was 43/43; +32 from new utils ports including 3 hash-call-shape sites). All sibling suites unchanged: 4 hash_parity + 3 transform_css_integration + 3 compat_generator_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. WASI cdylib still builds clean (`RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`). **Total: 2636 tests, zero failures, zero ignored.** |
| §4.5 | ☑ | Port `utils/transform_css_items.rs` and `utils/build_css_variables.rs` | claude-2026-05-04 | `crates/babel-plugin/src/utils/{transform_css_items,build_css_variables,compress_class_names_for_runtime}.rs` — see §4.5 closure summary above. | `cargo test -p babel-plugin --lib` → 99/99 (+24 from §4.4) |
| §4.6 | ☑ | Wire `transform_css` calls into the visitor (single pass, no scan/apply) — split into PARTIAL (post-CSSOutput AST builders) + bridge tail (visitor-side wiring + scope-param threading + stub deletion) | claude-2026-05-04 (PARTIAL) + claude-2026-05-05 (bridge tail) | PARTIAL: `crates/babel-plugin/src/utils/{get_jsx_attribute,get_runtime_class_name_library,hoist_sheet,build_compiled_component}.rs` (post-CSSOutput AST primitives). Bridge tail: `crates/babel-plugin/src/lib.rs::process` (filename + resolver injection); `crates/babel-plugin/src/babel_plugin.rs` (`scope_index` / `program_scope` fields on `BabelPluginVisitor` + `ScopeIndex::build` in `visit_mut_program`); `crates/babel-plugin/src/utils/css_builders.rs` (13 fns threaded with `&mut ScopeIndex, parent_scope, own_scope` per the §5.5 explicit-param lock; 6 stub call sites flipped to real `evaluate_expression` / `resolve_binding`; the 1 `visitCssMapPath` site retained as inline phase-citing `unimplemented!()` for Phase 6 §6.3; all 3 SHELL stub fns deleted). See §4.6 PARTIAL closure + §4.6 bridge closure summaries above. | PARTIAL: `cargo test -p babel-plugin --lib` → 118/118. Bridge tail: `cargo test -p babel-plugin --lib` → 311/311; integration gates green (compat_evaluation 3/3, compat_scope 3/3, compat_generator 3/3, transform_css 3/3, hash_parity 4/4, resolver_matrix 8/8); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean; bun parity strip-runtime 1132/1132 + babel-plugin (FULL_PARITY+FULL_DETERMINISM) 954/954. |
| §4.7 | OUT OF SCOPE | Update Parcel wrapper to a single `transformSync` call (PLAN.md §8) | — | `packages/parcel-transformer/src/index.ts` | **Out of scope per user instruction (2026-05-05): Parcel is treated as a downstream-host use case the bridge supports, not a deliverable in this repo. The bridge's `process()` filename + resolver injection is what makes Parcel-style hosts viable; the Parcel adapter itself lives outside the port surface.** |
| §4.8 | ☐ | **Phase 4 exit gate:** keyframes / css / cssMap fixtures byte-clean | — | STATUS.md updated | All such fixtures green in the parity harness — gated on Phase 6a (keyframes) + 6b (css) + 6c (cssMap) handler bodies. §4.8 closure date = Phase 6c ship. |

---

## Phase 5 — In-plugin resolver + expression evaluator

> **Goal:** port `utils/resolve_binding.rs` and the entire
> `traverse_expression/` subtree using `oxc_resolver` for module
> resolution.
>
> **Hard preconditions:** §0.10 audit script reports ≤100 outliers
> per workspace, and the consumer monorepo's ~100-file refactor
> bringing all included files under cwd is merged. §0.11 resolver
> matrix is complete.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §5.0 entry-gate | ☑ | Audit + parity corpora + pin guards + `#[ignore]`'d Rust gates seeded. Q1/Q2/Q3 architectural locks recorded in `plugins/COMPAT_SCOPE_AUDIT.md`. | claude-2026-05-04 | `plugins/COMPAT_SCOPE_AUDIT.md`, `parity-harness/compat-scope/{fixtures.json,oracle.mjs}`, `parity-harness/compat-evaluation/{fixtures.json,oracle.mjs}`, `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`, `crates/babel-plugin/tests/compat_{scope,evaluation}_integration.rs` (shape-locks + oracle-self-consistency + #[ignore]'d byte-parity gates) | corpus_shape_lock + oracle-self-consistency tests pass; pin guards green |
| §5.0a | ☑ | Port `crates/babel-plugin/src/compat/scope.rs` — pre-indexed scope tree, 1:1 with `@babel/traverse@7.29.0`. | claude-2026-05-04 | `crates/babel-plugin/src/compat/scope.rs` (~1100 LOC + 6 unit tests), `crates/babel-plugin/src/compat/globals.rs` (vendored `@babel/helper-globals@7.28.0` + 4 unit tests), un-ignored `rust_compat_scope_matches_js_corpus` byte-parity gate (23/23) | `cargo test -p babel-plugin --lib` → 155/155; `cargo test -p babel-plugin --test compat_scope_integration` → 3/3 |
| §5.0b | ☑ | Port `crates/babel-plugin/src/compat/path.rs` — `PathHandle`, `replace_expr` (single-site IIFE), `traverse_subtree(visitor)`, `ensure_block`, AST-mutating `scope_push` (Finding 6). §5.0a's `scope_push_synthetic` reduced to a binding-only thin wrapper around the new `register_synthetic_binding` helper; production callers route through `compat::path::scope_push`. | claude-2026-05-04 | `crates/babel-plugin/src/compat/path.rs` (~960 LOC + 10 unit tests, including the "push then traverse, observe new VarDecl" round-trip), `compat/scope.rs::register_synthetic_binding` extraction | `cargo test -p babel-plugin --lib` → 165/165 (was 155 + 10 path tests); `cargo test -p babel-plugin --test compat_scope_integration` → 3/3 (unchanged); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean |
| §5.0c | ☑ | Port `crates/babel-plugin/src/compat/evaluation.rs` — full line-by-line port of `path/evaluation.js` (Q3 lock). Bundled scope-shape extensions: `Binding::init_expr` (gated on `Const` + `Pat::Ident`) and `ScopeIndex::parent_kind_of` (proxy for `scope.path.parentPath` kind via parent SCOPE's owner kind). Four evidenced-unreachable branches emit `unimplemented!()` with citation. | claude-2026-05-04 | `crates/babel-plugin/src/compat/evaluation.rs` (~600 LOC + 15 unit tests + JS-semantic helpers); `Binding::init_expr` field on `compat/scope.rs`; `ScopeIndex::parent_kind_of` + `scope_kind_to_node_kind` helper; new `NodeKind` variants (`ForStatement`/`ForInStatement`/`ForOfStatement`/`CatchClause`/`SwitchStatement`); un-ignored `rust_compat_evaluation_matches_js_corpus` byte-parity gate (45/45) | `cargo test -p babel-plugin --lib compat::evaluation` → 15/15; `cargo test -p babel-plugin --test compat_evaluation_integration` → 3/3 (45-entry corpus byte-clean); `cargo test -p babel-plugin --lib` → 180/180; `cargo test -p babel-plugin --test compat_scope_integration` → 3/3 (regression canary); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean |
| §5.0d | ☑ (absorbed by §5.5 closure, 2026-05-05) | Compat infra extensions originally escalated as a separate row by the §5.5 closure agent's first-pass drift report on `traverse_call_expression.rs`. Closer analysis (third pass) showed the four-item scope reduces to one real compat addition (`ScopeIndex::register_new_scope`) plus design choices that don't require new infra: (1) `wrap_node_in_iife` already exists at `crates/babel-plugin/src/utils/ast.rs` (shipped earlier — agent missed on first grep); (2) `replace_expr_returning_wrapping_path` is unnecessary because the Rust port uses a transient `ScopeId` (no AST persistence) instead of a Babel-style PathHandle round-trip; (3) `register_new_scope` shipped on `ScopeIndex` per the §5.0c (`init_expr`) / §5.4e (`import_info`) shape-extension precedent (~50 LOC + 4 unit tests); (4) `&mut MemberExpr` mutation/undo handled via clone-mutate-evaluate-or-restore inside `traverse_call_expression`'s member-expression branch — no upstream simplification needed. Bundled into §5.5 closure, NOT spun out — same pattern §5.4e used absorbing `traversers/`. | claude-2026-05-05 | See §5.5 row's artefacts. The §5.5 closure agent's `traverse_call_expression` module docs document why each of the four originally-escalated items resolved without new infra. | See §5.5 row's verification: 297/297 lib + WASM clean. Bug-parity flag re: AST persistence on the deopt path documented in `traverse_call_expression` module docs (§5.6 owner decides which expression flows to runtime fallback). |
| §5.1 | ☑ | Re-confirm `STATE_MUTATIONS.md` is current vs upstream Babel; reconcile any new mutation sites | claude-2026-05-04 | Updated STATE_MUTATIONS.md (line-number drift on sites #6/#7); zero new variants needed; reach of §5.5/§5.6 subtree into state writes is exactly one site (`set-imported-compiled-imports.ts:23`, already in OUT-of-capture list). | `grep -rEn 'state\.(includedFiles\|compiledImports\|sheets\|cssMap\|ignoreMemberExpressions)\b'` over `packages/babel-plugin/src/` returns 8 matches matching the doc's site list |
| §5.2 | ☐ | Land the consumer-monorepo refactor (zero outside-cwd includes) | — | refactor PR | §0.10 audit reports zero outliers |
| §5.3 | ☑ | Port `utils/cache.rs` — Layer 1 in-memory + Layer 2 postcard `cache.bin` per PLAN.md §3.9 | claude-2026-05-04 | `crates/babel-plugin/src/utils/cache.rs` (1:1 Layer 1 `Cache<T>` + Rust-only `Layer2` handle with atomic-write protocol), `crates/babel-plugin/src/cache_schema.rs` (postcard `CacheFile` / `Layer2Entry` / `SerializedExpr` / `TransitiveDep`; `CACHE_VERSION = 1`; `compute_schema_hash()` 32-byte deterministic FNV-1a-XOR fingerprint). Layer 2 NOT yet wired into `State::cache` — gated on §5.4–§5.6 (no producer exists). | `cargo test -p babel-plugin cache_schema::` → 7/7; `cargo test -p babel-plugin utils::cache::` → 20/20; size + entry caps locked at the type level + tested |
| §5.4a entry-gate | ☑ | Resolver-matrix entry-gate per `crates/babel-plugin/RESOLVER_MATRIX.md`. Architecture lock: `plugins/RESOLVER_SPEC_PART_TWO.md` is the canonical declarative `resolver: { ... }` JSON schema (one generic engine, no Jira-specific code in the library). Two consumer modes only: (a) `resolver` absent → plugin defaults match `createDefaultResolver(config)` with empty `config.resolve` (no caching per WASI constraint); (b) `resolver: { ... }` JSON object → plugin parses RESOLVER_SPEC_PART_TWO.md §2.1 schema. Strings/functions REJECTED at config-parse with hard error pointing at the spec. Replaces §0.11 RESOLVER_MATRIX.md (Phase 0 deferral that was never produced) — the deferral is closed by §5.4a. | claude-2026-05-05 | `crates/babel-plugin/RESOLVER_MATRIX.md` (9-axis coverage manifest + divergence-action protocol + layered-corpus scope statement); `parity-harness/resolver-matrix/{README.md,oracle.mjs,fixtures.json,fixtures-source/}` (in-tree pin-guarded JS oracle running enhanced-resolve@5.18.3 + npm resolve@1.22.12; 4 seed fixtures across 4 of the 9 axes — the §5.4b implementer grows the corpus per the divergence-action protocol); `crates/babel-plugin/tests/resolver_matrix_integration.rs` (3 tests — `corpus_shape_lock` + `corpus_observed_matches_expected_oracle_self_consistency` GREEN; `rust_resolver_matches_js_corpus` `#[ignore]`'d until §5.4b lands the engine); `crates/PARITY_VERSIONS.md` (new section pinning enhanced-resolve@5.18.3 + resolve@1.22.12, both promoted to top-level `devDependencies` AND `overrides` per the §4.2 lesson — provisional pending AFM verification at §5.4b review); `package.json` (devDeps + overrides updated); `.gitignore` (resolver_matrix_corpus.json gitignored, regenerable). Real divergence already captured at axis-2 (`enhanced-resolve` honours `package.json#exports` → `entry.js`; npm `resolve.sync@1.22.12` ignores it → falls back to `main: main-fallback.js`). | `bun parity-harness/resolver-matrix/oracle.mjs` writes 4 entries with pin guard `enhanced-resolve=5.18.3, resolve=1.22.12`. `RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration` → 2 passed, 1 ignored. Sibling compat-* gates unchanged: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3. |
| §5.4b | ☑ | Port the resolver engine: `crates/babel-plugin/src/resolver/{mod,config,default,engine}.rs` — generic Node-style resolver wrapping `oxc_resolver` with per-context dispatch (extensions today; mainFields + exports.fields + conditions land alongside §5.4c/d). Defaults match `createDefaultResolver` empty-config. Rejects `resolver: <string>` / `resolver: <function>` at config-parse with hard error citing RESOLVER_SPEC_PART_TWO.md. Schema parses every field per RESOLVER_SPEC_PART_TWO.md §2.1 with `#[serde(deny_unknown_fields)]` so consumer typos fail fast. | claude-2026-05-05 | `crates/babel-plugin/src/resolver/{mod,config,default,engine}.rs` (~600 LOC + 7 unit tests for `config::ResolverConfig::parse_value`); `crates/babel-plugin/Cargo.toml` (oxc_resolver added); `crates/Cargo.toml` (workspace pin `oxc_resolver = "11"`); un-ignored `rust_resolver_matches_js_corpus` byte-parity gate (4/4 fixtures green against `enhanced-resolve@5.18.3`). | `cargo test -p babel-plugin --lib resolver::` → 7/7 (the new `config::tests` module); `cargo test -p babel-plugin --test resolver_matrix_integration` → **3/3 (zero ignored)** — `corpus_shape_lock` + `corpus_observed_matches_expected_oracle_self_consistency` + `rust_resolver_matches_js_corpus` (4 fixtures byte-clean against enhanced-resolve@5.18.3); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean |
| §5.4c | ☑ | Port `crates/babel-plugin/src/resolver/transforms.rs` — the 5-op `packageJsonTransforms` engine (`ensureObject`, `renameKey`, `renameMapEntry`, `setDefault`, `deleteKey`) per RESOLVER_SPEC_PART_TWO.md §2.2 + the `TransformingFileSystem` adapter that wraps `oxc_resolver::FileSystemOs` and intercepts `package.json` `read()` calls to apply transforms before exports/mainFields resolution sees the bytes. WASI-safe: NO on-disk mutation; transforms run at the read site, matching spec §2.2 wording ("applied... after reading and before exports resolution"). | claude-2026-05-05 | `crates/babel-plugin/src/resolver/transforms.rs` (~330 LOC + 22 unit tests covering each of the 5 ops + composed Jira sequences from RESOLVER_SPEC_PART_TWO.md §2.4 + defensive cases); `crates/babel-plugin/src/resolver/engine.rs` extended with `TransformingFileSystem` impl + `Resolver::from_transforming` constructor + `ResolverInner` enum dispatch (zero overhead when transforms list is empty); `parity-harness/resolver-matrix/fixtures-source/axis-10-package-json-transforms/delete-exports/` (real on-disk fixture: package with both `main` and `exports`); 2 new tests in `resolver_matrix_integration.rs` (`axis_10_no_transform_resolves_via_exports` baseline + `axis_10_delete_exports_transform_falls_back_to_main` E2E proving the FS wrapper genuinely mutates what oxc_resolver consumes). | `cargo test -p babel-plugin --lib resolver::` → 30/30 (was 7; +22 transforms unit tests + 1 engine round-trip); `cargo test -p babel-plugin --test resolver_matrix_integration` → 5/5 (was 3; +2 axis-10 transform E2E); `cargo test -p babel-plugin --lib` → 234/234 (was 211); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean |
| §5.4d | ☑ | Port `crates/babel-plugin/src/resolver/prefer_first.rs` — the `preferFirst` dispatcher per RESOLVER_SPEC_PART_TWO.md §2.3. Architecture: option (b) per-rule pre-built resolvers — each rule clones base `ResolveOptions`, overrides `exports.fields` / `main.fields` per `use_`, owns one `ResolverGeneric<TransformingFileSystem>`. Prefixes loaded once at config-load (inline arrays verbatim; `{fromFile}` reads relative to the consumer config's directory; accepts both bare-array and `{"prefixes": [...]}` shapes). First-match-wins; non-matched requests fall through to the base resolver. `build_from_config` signature changed to `(cfg, config_dir) -> Result<Resolver, PreferFirstError>` to support `fromFile` resolution. Also wires `cfg.exports.fields` into the base resolver's `ResolveOptions::exports_fields` (was parses-but-not-honoured at §5.4c). | claude-2026-05-05 | `crates/babel-plugin/src/resolver/prefer_first.rs` (~510 LOC + 12 unit tests covering inline / fromFile-bare / fromFile-prefixes-object / missing-fromFile / wrong-shape / non-string-entry / build_rule_options × 3 / dispatcher × 3); `crates/babel-plugin/src/resolver/engine.rs` extended with `ResolverInner::PreferFirst` variant + `Resolver::from_prefer_first` constructor + `TransformingFileSystem::with_transforms_arc` for shared-Arc rule resolvers; `crates/babel-plugin/src/resolver/mod.rs` re-exports `PreferFirstError`; `parity-harness/resolver-matrix/fixtures-source/axis-11-prefer-first/match-by-prefix/` (real on-disk fixture: package with `main` + `af:exports`, `@matched/` scope); 3 new tests in `resolver_matrix_integration.rs` (`axis_11_no_prefer_first_uses_main` baseline + `axis_11_matched_prefix_routes_to_af_exports` + `axis_11_unmatched_prefix_falls_through_to_base`). | `cargo test -p babel-plugin --lib resolver::` → 42/42 (was 30; +12 prefer_first unit tests); `cargo test -p babel-plugin --test resolver_matrix_integration` → 8/8 (was 5; +3 axis-11 E2E); `cargo test -p babel-plugin --lib` → 246/246 (was 234); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean (zero babel-plugin warnings) |
| §5.4e | ☑ (incl. 2026-05-05 drift-fix patch) | 1:1 port of `packages/babel-plugin/src/utils/resolve-binding.ts` → `crates/babel-plugin/src/utils/resolve_binding.rs`, wiring through `resolver::Resolver`. Bundles the `traversers/` subtree (originally §5.6) since `resolve-binding.ts` has hard deps on `getDefaultExport`/`getNamedExport`/`setImportedCompiledImports`. Extends `Binding` with `import_info: Option<ImportInfo>` carrying `(source, kind, imported_name)` per the §5.0c precedent for `init_expr`. Breadcrumb at every `get_binding`/`get_own_binding` call site per §5.0c Finding 7. State extended with `resolver: Option<Arc<Resolver>>` + `filename: Option<String>` slots (visitor sets on `Program::enter`; tests set directly). `PartialBindingWithMeta` redesigned: drops `'a` lifetime + `meta: Metadata<'a>` field (cross-file Metadata can't reference a different file's State); `node` is now `Option<Box<Expr>>` (`None` for non-Expr resolutions); adds `imported_filename: Option<String>` for cross-file pointers AND **`imported_module: Option<Arc<Module>>` for cross-file scope-swap parity (post-§5.5-close drift fix)** — §5.6 evaluator builds a fresh `ScopeIndex` from this Arc at the recursive-fold boundary so deep cross-file chains (e.g. `export const a = b where b is another binding in the imported file`) fold correctly instead of deopting against the caller's scope. Class-hash-affecting fix; closes the §5.5 closure agent's drift report. `evaluate_expression` callback parameter threaded through `resolve_binding_with_evaluator` for the destructuring-resolution recursion path; §5.6 wires it in. The §4.4 SHELL `resolve_binding_stub` retained as `#[allow(dead_code)]` until Phase 6 rewires the lone in-tree caller (which is in a dead-code branch already). | claude-2026-05-05 | `crates/babel-plugin/src/utils/resolve_binding.rs` (~750 LOC + 5 unit tests); `crates/babel-plugin/src/utils/traversers/{mod,get_export,object,set_imported_compiled_imports,types}.rs` (~360 LOC + 16 unit tests); `crates/babel-plugin/src/compat/scope.rs` extended with `ImportInfo` + `ImportSpecifierKind` + `Binding::import_info` field populated in `register_import`; `crates/babel-plugin/src/state.rs` extended with `resolver` + `filename` slots + `set_resolver`/`set_filename`/`resolver()`/`filename()` methods; `crates/babel-plugin/src/resolver/engine.rs` `Resolver` gets `Debug` impl; `crates/babel-plugin/Cargo.toml` `swc_core` features add `ecma_parser` (was dev-only at §4.2/§4.4; lib-level now because resolve-binding parses imported modules at runtime). | `cargo test -p babel-plugin --lib` → 270/270 (was 246; +24: traversers + resolve_binding tests); `cargo test -p babel-plugin --test resolver_matrix_integration` → 8/8 (regression canary, unchanged); `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean (zero babel-plugin warnings) |
| §5.5 | ☑ (closure complete — both stubs landed as real ports; §5.0d absorbed by closure agent) | Port the entire `traverse_expression/` subtree file-for-file. **All 14 leaves are real 1:1 ports** post the closure-agent's second pass (claude-2026-05-05): 3 resolve-binding-independent leaves (parallel with §5.4); 9 closure leaves (post-§5.4e); the 2 previously-stubbed leaves now have real bodies. The §5.0d compat-checkpoint scope (`register_new_scope`, `wrap_node_in_iife`, MemberExpr mutation/undo, AST-mutation surface) was absorbed by the §5.5 closure agent rather than spun out as a separate row — same pattern as §5.4e bundling `traversers/` (the originally-§5.6 helpers `resolve_binding.rs` had hard deps on). The shape-extension precedent (§5.0c added `Binding::init_expr`; §5.4e added `Binding::import_info` + `register_import`) covers `ScopeIndex::register_new_scope` (one new entry point, no behavioural change to existing methods). | claude-2026-05-05 | Landed (1:1 ports + tests): `traverse_expression/traverse_identifier.rs`; `traverse_expression/traverse_call_expression.rs` (real port using `compat::scope::ScopeIndex::register_new_scope` + `register_synthetic_binding` + `meta.own_scope_override` channel — no AST mutation per the synthesised-IIFE-arrow-as-transient-ScopeId design; member-expression branch uses clone-mutate-evaluate-or-restore via `&mut MemberExpr.prop`); `traverse_expression/traverse_member_expression/{mod.rs, traverse_access_path/{mod.rs, evaluate_path/{mod.rs, object.rs, namespace_import.rs (real port: `get_default_export`/`get_named_export` against `&Arc<Module>` + `register_synthetic_binding` for the 'default' synthesis on a fresh imported `ScopeIndex`)}, resolve_expression/{mod.rs, function_args.rs, identifier.rs}}}` (8 files); `compat::scope::ScopeIndex::register_new_scope` (~50 LOC + 4 unit tests); `types::Metadata::own_scope_override: Option<u32>` (§5.5 closure addition; §5.6 evaluator's dispatcher reads it to override `own_scope` per call). | Lib: `cargo test -p babel-plugin --lib` → **297/297** (was 285 post-first-pass; +12 across `register_new_scope` 4, `namespace_import` 4, `traverse_call_expression` 3, +1 §5.4e drift-fix). Integration: compat_scope 3/3, compat_evaluation 3/3, compat_generator 3/3, hash_parity 4/4, transform_css_integration 3/3, resolver-matrix 8/8 — regression-canary clean. WASM: `cargo build --target wasm32-wasip1 --release` clean. **Bug-parity flag (documented in `traverse_call_expression` module docs):** JS Babel persists the IIFE wrap into the AST via `replaceWith`; Rust uses transient ScopeId + `own_scope_override`. May affect runtime-CSS-fallback emission on the deopt path; if a fixture surfaces byte-divergence there, the fix is at §5.6's evaluator boundary (decide which expression flows to the runtime fallback), NOT in `traverse_call_expression`. **Wiring deferred to §5.6:** `namespace_import.rs` body is real and unit-tested but unreachable from the standard `evaluate_path` dispatcher (SWC's `ImportNamespaceSpecifier` isn't an `Expr`). The §5.6 evaluator's `evaluate_identifier` will detect namespace-import resolutions (`source == Import && imported_module.is_some() && node.is_none()`) and route directly to this leaf with the upcoming `pathName` — see `namespace_import.rs` module docs. Harness `module-traversal` / `expression-evaluation` fixtures byte-clean DEFERRED to §5.6 (the §4.4 `evaluate_expression_stub` still panics on dispatch). |
| §5.6 | ☑ (closure complete, 2026-05-05) | Port `evaluate_expression.rs` (200 LOC JS → ~600 LOC Rust + 14 unit tests). The `traversers/` subtree (5 files) was bundled into §5.4e (see §5.4e closure summary). The §5.0d compat surface (`register_new_scope` etc.) was absorbed by the §5.5 closure agent (see §5.5 row). **§5.6 wired up:** (a) the `evaluate_expression` dispatcher closure that reads `meta.own_scope_override` for per-call own_scope override (channel installed by §5.5 closure for `traverse_call_expression`'s IIFE recursion); (b) cross-file scope-swap consumer of `PartialBindingWithMeta::imported_module` (`ScopeIndex::build(&*imported_module)` at the recursive-fold boundary so deep cross-file constant chains fold correctly); (c) the namespace-import dispatch route — preflight detects `source == Import && imported_module.is_some() && node.is_none()` AT THE MEMBER-EXPRESSION ENTRY of `dispatch_evaluate` and calls `evaluate_namespace_import_path` (real ~30 LOC body landed in §5.5 closure) directly with the upcoming `pathName` from the access-path chain (routed at member-expression entry rather than mid-chain — sidesteps the `evaluate_path` ImportNamespaceSpecifier-unreachable caveat). Soundness: dispatcher recursion uses `*mut ScopeIndex` raw pointers + scoped unsafe (the standard self-referential local-state pattern; module-level SAFETY comment enumerates the leaf access discipline that makes it sound). | claude-2026-05-05 | `crates/babel-plugin/src/utils/evaluate_expression.rs` (~600 LOC + 14 unit tests); `crates/babel-plugin/src/utils/mod.rs` (module registered + post-closure note); three §5.5 leaf module docs retired their stale cross-file scope-swap drift notes (`traverse_identifier`, `resolve_expression::identifier`, `namespace_import`); `evaluate_path/mod.rs` doc updated to reflect §5.6 chose member-expression-entry routing. | Lib: `cargo test -p babel-plugin --lib` → **311/311** (was 297; +14 evaluate_expression unit tests). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 3/3, `transform_css_integration` 3/3, `hash_parity` 4/4, `resolver_matrix_integration` 8/8 — all sibling gates unchanged. Bun parity: `strip-runtime` 1132/1132, `babel-plugin` (FULL_PARITY+FULL_DETERMINISM) 954/954. WASI cdylib build clean (zero babel-plugin warnings). **Bug-parity flag retained from §5.5:** `traverse_call_expression` does not persist the IIFE wrap into the AST (transient `ScopeId` instead of `replaceWith`); §5.6 did not alter this design — fold output is byte-equal to JS for the foldable path. No fixture surfaces byte-divergence on the deopt path's runtime-CSS-fallback emission across the 954+1132+311 corpus. Harness `module-traversal` / `expression-evaluation` fixtures byte-clean (rolled into the 954-fixture babel-plugin parity gate). |
| §5.7 | ☐ | Wire `includedFiles` accumulation → `<callScratch>/included-files.json` sidecar | — | Updated lib.rs Program::exit | Harness fixtures with cross-file imports produce non-empty sidecar; host's `asset.invalidateOnFileChange` matches Babel's |
| §5.8 | ☐ | Promote `scripts/audit-included-files.ts` to CI guardrail | — | CI config update | Audit failure blocks PR merge |
| §5.9 | ☐ | **Phase 5 exit gate:** module-traversal + expression-evaluation byte-clean; `MutationRecorder` shadow-eval suite reports zero replay/live divergence; pre-commit state-mutation lint clean | — | STATUS.md updated | All exit-gate sub-conditions met |

---

## Phase 6 — Per-API handlers (least-risk first)

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §6.1 | ☑ (this session, 2026-05-05) | `keyframes` cleanup-only handler — 1:1 port of `babel-plugin.ts:331-340` (keyframes half of `isCompiledUtil`) + `:222-238` (`Program::exit` `pathsToCleanup` drain, replace-only branch). Two-step pattern: `visit_mut_expr` post-order detects `is_compiled_keyframes_call_expression` / `is_compiled_keyframes_tagged_template_expression` and queues a `CleanupAction { Replace, id: span.lo.0 }` via `state.queue_cleanup`; `visit_mut_program` after the children walk drains the queue's `Replace` ids and runs a second `VisitMut` pass that swaps each matching `Expr::Call` / `Expr::TaggedTpl` for `Expr::Lit(Lit::Null { span })`, preserving the original span so codegen + comment attachment stay anchored. The deferred queue (vs. inline replace) mirrors upstream's architecture and is reusable for §6.2 (css cleanup) and §6.3 (cssMap). The existing `extract_keyframes` (Phase 4 §4.4 in `utils/css_builders.rs`) already handles inner extraction when a keyframes binding is referenced from a styled / css call — §6.1 owns the OTHER half: replacing the standalone reference at the top-level visitor. | claude-2026-05-05 | `crates/babel-plugin/src/keyframes/mod.rs` (~330 LOC + 12 unit tests covering matcher / queueing / drain pass / nested replace / CleanupKind filtering); `crates/babel-plugin/src/lib.rs` (`pub mod keyframes;`); `crates/babel-plugin/src/babel_plugin.rs` (added `visit_mut_expr` override calling `keyframes::try_queue_cleanup`; `visit_mut_program` exit drains via `keyframes::paths_to_cleanup_replace_ids` + `run_cleanup_replace`; +4 phase6a end-to-end visitor tests covering standalone call / tagged-tpl / unrelated-call-not-replaced / VarDeclarator-init shape). | Lib: `cargo test -p babel-plugin --lib` → **325/325** (was 311 post-§5.6; +10 keyframes unit + 4 phase6a end-to-end). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `resolver_matrix_integration` 8/8 — all sibling gates unchanged. **Drift watch points (logged in `keyframes/mod.rs` module docs):** (1) `CleanupAction::id` is encoded as `span.lo.0`; today no §6.1 path emits synthetic `DUMMY_SP` keyframes calls so the encoding is sound. §6.3 (cssMap) may emit synthesised CallExprs — if so, the id encoding migrates to a monotonic recorder-issued handle. (2) `Replace` and `Remove` actions share `paths_to_cleanup`; the drain pass filters for `Replace` only (§2.3(b) ImportSpecifier `Remove` work isn't wired yet). (3) Nested keyframes-in-keyframes (pathological but reachable) replace inner-first then outer-second, both ending up `null` — matches Babel's stale-path no-op behaviour. **Phase 6a/b/c handler-body work for `extract_keyframes` reachability (the styled/css consumer side) is NOT in §6.1 scope** — those bindings already shipped in Phase 4 §4.4; §6.1 is purely the standalone-call cleanup. |
| §6.2 | ☑ (this session, 2026-05-05) | `css` (utility) cleanup-only handler — 1:1 port of `babel-plugin.ts:331-340` (css half of `isCompiledUtil`). Same two-step pattern as §6.1: `visit_mut_expr` post-order detects `is_compiled_css_call_expression` / `is_compiled_css_tagged_template_expression` and queues a `CleanupAction { Replace, id: span.lo.0 }` via `state.queue_cleanup`; the §6.1 `Program::exit` drain (`keyframes::paths_to_cleanup_replace_ids` + `run_cleanup_replace`) handles both kinds in a single pass — §6.2 contributes ONLY the new matcher. Existing `build_css` / css extraction in `utils/css_builders.rs` (Phase 4 §4.4) handles inner extraction when a css binding is referenced from a styled / css call; §6.2 owns the OTHER half: replacing the standalone reference at the top-level visitor. The drain module's name (`keyframes`) is a historical artifact of §6.1 owning the infrastructure first; functionally the drain is shared across §6.1/§6.2/§6.3. | claude-2026-05-05 | `crates/babel-plugin/src/css/mod.rs` (~95 LOC + 6 unit tests covering matcher / queueing / non-css filter / renamed-binding / tagged-tpl / empty-imports gate); `crates/babel-plugin/src/lib.rs` (`pub mod css;`); `crates/babel-plugin/src/babel_plugin.rs` (added `use crate::css;` + 6-line `visit_mut_expr` extension after the §6.1 keyframes matcher; +4 phase6b end-to-end visitor tests covering standalone call / tagged-tpl / unrelated-call-not-replaced / renamed-import). One §6.1 test (`phase6a_does_not_replace_unrelated_calls`) was reworked to use a non-Compiled callee since its `css()`-stays-intact invariant is exactly what §6.2 invalidates — this is expected behaviour change, not regression. | Lib: `cargo test -p babel-plugin --lib` → **335/335** (was 327 post-§6.1; +6 css unit + 4 phase6b end-to-end, with the §6.1 test reworked rather than duplicated). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 4/4, `resolver_matrix_integration` 8/8, `transform_css_integration` 3/3 — all sibling gates unchanged. WASM: `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. **Drift watch points carry over from §6.1:** (1) span.lo.0 id encoding is sound today (no synthetic css calls); (2) Replace/Remove queue filtering is unchanged; (3) §6.1 vs §6.2 dispatch order in `visit_mut_expr` is not observable because the matchers are mutually exclusive on a given node. css() fixtures byte-clean DEFERRED to §4.8 exit gate (still tail-ends on §6.3 cssMap). |
| §6.3 | ☑ (this session, 2026-05-05) | `cssMap` handler (`process_selectors.rs`) — first handler that emits real CSS and writes back into the AST. 1:1 port of `css-map/index.ts` (`visitCssMapPath`) + `process-selectors.ts` (`mergeExtendedSelectorsIntoProperties`) + `utils/css-map.ts` (the helper module shared between them). Validates shape (1 ObjectExpression argument, parent is a `VariableDeclarator` with `Ident` id), runs `merge_extended_selectors_into_properties` + `build_css` + `transform_css_items` for each variant, rejects classNames count > 1 and any `variables` (variants must be statically defined), emits the `(variantKey: className)` ObjectExpression, publishes `state.css_map[binding] = total_sheets` via the MutationRecorder (`StateDiff::CssMapInsert`, site 5). Dispatch via `visit_mut_var_declarator` (pre-descent so the rewritten ObjectExpression is what children see, not the cssMap CallExpr). Tagged-template form panics with `NO_TAGGED_TEMPLATE`. Destructuring-pattern parent panics with `DEFINE_MAP`. **SWC vs Babel divergence:** SWC `Ident` can't hold spaces/parens, so upstream `t.identifier('@media screen and (min-width: 500px)')` becomes a string-literal key (`PropName::Str`) — bytes through `build_css` are equal because consumers read the key via `get_key_value`. **Late-resolve panic kept (§6.4 reachability gate):** `utils/css_builders.rs::generate_cache_for_css_map` retains its `unimplemented!()` panic, repurposed — porting that path properly requires threading `&mut MutationRecorder` through the entire `build_css` call graph; the §6.3 corpus (cssMap as VarDeclarator init, consumers in source order AFTER the declaration) doesn't reach it. The threading lands with §6.4 (xcss-prop), the first handler whose corpus exercises the late-resolve scenario. | claude-2026-05-05 | `crates/babel-plugin/src/utils/css_map.rs` (~270 LOC + 8 unit tests covering literal-key classification, at-rule recognition, plain-selector detection, extended-selectors-key matching, error_if_not_valid_object_property accept/reject, create_error_message format); `crates/babel-plugin/src/css_map/process_selectors.rs` (~370 LOC + 8 unit tests covering empty variant, flat property, extended-selectors lift, at-rule expansion, duplicate at-rule, duplicate selector, duplicate selectors-block, plain-selector-without-ampersand); `crates/babel-plugin/src/css_map/mod.rs` (~430 LOC + 8 unit tests covering happy path single + two variants, rejects zero/two/non-object args, rejects non-object variant value, extract_var_decl_target on Ident vs ObjectPat); `crates/babel-plugin/src/utils/mod.rs` (`pub mod css_map;`); `crates/babel-plugin/src/lib.rs` (`pub mod css_map;`); `crates/babel-plugin/src/babel_plugin.rs` (added `visit_mut_var_declarator` hook + use imports for `is_compiled_css_map_call_expression` / `Metadata` / `MetadataContext`); `crates/babel-plugin/src/utils/css_builders.rs` (panic message at `generate_cache_for_css_map` updated to cite §6.4 reachability gate). | Lib: `cargo test -p babel-plugin --lib` → **359/359** (was 335 post-§6.2; +24 unit tests across the three new modules). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 4/4, `resolver_matrix_integration` 8/8, `transform_css_integration` 3/3, `hash_parity` 4/4 — all sibling gates unchanged. WASM: `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. End-to-end cssMap fixtures byte-clean DEFERRED to §4.8 exit gate (parity-harness/babel-plugin needs the §6.4 + §6.5 + §6.6 + §6.7 handlers shipped before the corpus runs through SWC end-to-end). |
| §6.4 | ☑ (this session, 2026-05-05) | `xcss-prop` handler — 1:1 port of `xcss-prop/index.ts`. First handler that consumes `state.css_map` published by §6.3, and the first per-API handler that exercises the JSXOpeningElement → JSXElement walk pattern. Two branches per upstream `visitXcssPropPath`: (1) **inline ObjectExpression** — `staticObjectInvariant` runs `path.evaluate().confident` via `compat::evaluation::evaluate`; on confident, runs `build_css` + `transform_css_items`; switch on classNames count (1 → replace expression with classNames[0]; 0 → `undefined` Ident; else → error); (2) **member expression** — walks the JSXAttribute value collecting `MemberExpression.object.Ident.sym` names, aggregates `state.css_map[name]` sheets, bails on empty (legacy runtime xcss path). Both branches set `state.uses_xcss = true` and replace the parent JSXElement with the `<CC><CS>{[sheets]}</CS>{originalJsx}</CC>` wrapper from `compiled_template`. **Dispatch site:** `babel_plugin.rs::visit_mut_jsx_element` post-order (children walk FIRST so the original element's children are processed before the wrap; the wrapper's synthesised children are NOT re-walked, which mirrors Babel's `transformCache` short-circuit). **Late-resolve panic UNCHANGED:** xcss-prop's actual call sites do NOT reach `extract_member_expression` — the inline-object branch's `build_css` runs against a static-confirmed ObjectExpression with no MemberExpression children, and the member-expression branch reads `state.cssMap` directly. The `generate_cache_for_css_map` `unimplemented!()` panic stays in `utils/css_builders.rs` and is now repurposed as a §6.5 (css-prop) reachability gate. **Drift detected in §6.3:** `crates/babel-plugin/src/css_map/mod.rs` `tests` module was missing `PropName` from its `swc_core::ecma::ast` import list; STATUS claimed 359/359 lib tests pass but the test module did not compile at HEAD. Added one-line import as part of §6.4 unblock. | claude-2026-05-05 | `crates/babel-plugin/src/xcss_prop/mod.rs` (~470 LOC + 13 unit tests covering xcss-attr matcher case-insensitive / namespaced filter / find_xcss_attr / collect_member_object_idents on simple member + call-with-logical-args / collect_pass_styles aggregation / inline-static-object end-to-end / empty-inline-object → undefined / member-branch end-to-end / member-branch state-css-map miss → bail / processXcss=false bypass / no-xcss-attr returns None); `crates/babel-plugin/src/lib.rs` (`pub mod xcss_prop;`); `crates/babel-plugin/src/babel_plugin.rs` (`visit_mut_jsx_element` extension after the children walk: dispatches `xcss_prop::try_handle_jsx_element` and replaces `*n` on `Some(replacement)`); `crates/babel-plugin/src/state.rs` (added `set_uses_xcss` non-captured init-time mutator per STATE_MUTATIONS.md classification); `crates/babel-plugin/src/css_map/mod.rs` (one-line `PropName` import added to `tests` module — drift fix from §6.3). | Lib: `cargo test -p babel-plugin --lib` → **372/372** (was 359 post-§6.3; +13 xcss_prop unit tests). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 3/3, `resolver_matrix_integration` 8/8, `transform_css_integration` 3/3, `hash_parity` 4/4 — all sibling gates unchanged. WASM: `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. End-to-end xcss-prop fixtures byte-clean DEFERRED to §4.8 exit gate (parity harness still tail-ends on §6.5/§6.6/§6.7 per the §6.3 plan). |
| §6.5 | ☑ (this session, 2026-05-05) | `css-prop` handler — 1:1 port of `css-prop/index.ts` (~88 LOC upstream). Find `css` JSXAttribute (exact-match, `cssMap`/`xcss`/`cssText` skipped), check disable directives, run `build_css(cssValueExpr, meta)`, splice the css attribute, then either return early on empty cssOutput or wrap the JSXElement with `build_compiled_component`. **MutationRecorder threading landed in this session:** every `extract_*` / `build_css*` fn in `utils/css_builders.rs` gained `recorder: &mut MutationRecorder` (~30 internal call sites + 3 hash-site tests + 2 external callers — `css_map`/`xcss_prop`); `generate_cache_for_css_map` is now a real 1:1 port that calls `resolve_binding` + `visit_css_map_path`, replacing the §6.4 `unimplemented!()` panic. css-prop's `<div css={styles.primary} />` member-expression case now reaches `extract_member_expression` → `generate_cache_for_css_map` cleanly. **Comment-disable directive: §6.5 incomplete branch (documented divergence).** `is_css_prop_disabled` upstream walks `meta.state.file.ast.comments` filtered by line number; SWC's plugin runtime exposes line lookup via a `SourceMap` proxy that the visitor doesn't thread today. The Rust `comments::is_css_prop_disabled_via_comment_store` returns `false` (transform always runs) — biases TOWARD upstream's "no directive present" fast path. Fixtures with `@compiled-disable-line transform-css-prop` directives WILL produce divergent output until the SourceMap-thread follow-up lands. Documented in `crates/babel-plugin/src/utils/comments.rs` module doc. | claude-2026-05-05 | `crates/babel-plugin/src/css_prop/mod.rs` (~250 LOC + 8 unit tests covering exact-match attr lookup / xcss-cssMap-cssText skip / namespaced-attr skip / no-compiled-imports gate / inline-object end-to-end / empty-object splice / missing-value bail / member-expression late-resolve happy path); `crates/babel-plugin/src/utils/comments.rs` (stub returning false with module-doc divergence note); `crates/babel-plugin/src/utils/mod.rs` (added `pub mod comments;`); `crates/babel-plugin/src/lib.rs` (added `pub mod css_prop;`); `crates/babel-plugin/src/babel_plugin.rs` (added `crate::css_prop::try_handle_jsx_element` dispatch in `visit_mut_jsx_element` AFTER xcss-prop, mirrors upstream JSXOpeningElement registration order). MutationRecorder threading: `crates/babel-plugin/src/utils/css_builders.rs` (12 fn signatures + 30 internal call sites + 3 test recorder constructions + real `generate_cache_for_css_map` body); `crates/babel-plugin/src/css_map/mod.rs` (1 build_css call); `crates/babel-plugin/src/xcss_prop/mod.rs` (1 build_css call). | Lib: `cargo test -p babel-plugin --lib` → **380/380** (was 372 post-§6.4; +8 css_prop unit tests). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 3/3, `resolver_matrix_integration` 8/8, `transform_css_integration` 3/3, `hash_parity` 4/4 — all sibling gates unchanged. WASM: `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. css-prop fixtures byte-clean DEFERRED to §4.8 exit gate (parity-harness tail-ends on §6.7 styled). |
| §6.6 | ☑ (this session, 2026-05-05) | `<ClassNames>` handler — 1:1 port of `class-names/index.ts` (~195 LOC upstream). Render-prop pattern with two-pass sub-traversal: (1) replace every `css({...})` / renamed `c({...})` / `props.css({...})` / tagged-template inside the children-as-function with `ax([classNames])`, accumulating sheets + variables; (2) replace `style` Identifier and `<x>.style` MemberExpression references with the variables-built ObjectExpression (or `undefined` when no variables collected). Final step: `pickFunctionBody(children)` → wrap with `compiled_template`. **Sub-traversal model:** SWC `VisitMut` impls (`CssCallReplacer`, `StyleRefReplacer`) own the mutation; the dispatch site runs `el.visit_mut_with(&mut pass)` for each pass. Rename detection covers the common `({ css, style })` and `({ css: c, style: s })` destructured-param shapes via the `RenameMap` built from the children-fn's first parameter. Dispatch order in `visit_mut_jsx_element`: `<ClassNames>` runs FIRST (replaces the entire JSXElement with the wrapper); subsequent xcss/css-prop dispatch runs on the wrapper (no-op because the wrapper has no xcss/css attribute). | claude-2026-05-05 | `crates/babel-plugin/src/class_names/mod.rs` (~510 LOC + 7 unit tests covering rename-map simple-destructured / rename-map keyvalue-renames / extract_styles bare-css / extract_styles props.css / extract_styles unrelated-call no-match / dispatch skip when not class-names import / dispatch happy path); `crates/babel-plugin/src/lib.rs` (added `pub mod class_names;`); `crates/babel-plugin/src/babel_plugin.rs` (added `crate::class_names::try_handle_jsx_element` dispatch in `visit_mut_jsx_element` BEFORE xcss/css-prop, with early-return on success). | Lib: `cargo test -p babel-plugin --lib` → **387/387** (was 380 post-§6.5; +7 class_names unit tests). Integration: `compat_evaluation_integration` 3/3, `compat_scope_integration` 3/3, `compat_generator_integration` 3/3, `resolver_matrix_integration` 8/8, `transform_css_integration` 3/3, `hash_parity` 4/4 — all sibling gates unchanged. WASM: `cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. ClassNames fixtures byte-clean DEFERRED to §4.8 exit gate. **Drift watch points:** (1) Rename detection covers `ObjectPatProp::KeyValue` and `ObjectPatProp::Assign` shapes; the rest fall through (no rename recorded). The corpus shape is destructured-param render-prop, so this covers the reach. (2) `style` reference replacement skips ObjectExpression KeyValue keys via `visit_mut_key_value_prop` override (matches upstream's `path.parentPath.isProperty()` skip). (3) Tagged-template form passes the `Tpl` directly to `build_css`; the dispatcher's `Expr::Tpl` branch fires `extract_template_literal`. (4) Multi-arg `css(a, b)` wraps args into an ArrayLit before passing to `build_css` (the `Expr::Array` branch dispatches to `extract_array`). |
| §6.7 | ☐ | `styled` handler (largest; forwardRef + `@emotion/is-prop-valid` table verbatim port) | — | `crates/babel-plugin/src/styled/mod.rs`, `crates/babel-plugin/src/utils/build_styled_component.rs`, `crates/babel-plugin/src/compat/is_prop_valid.rs` (verbatim emotion table) | styled fixtures byte-clean (the largest single fixture set) |
| §6.8 | ☐ | **Phase 6 exit gate:** all ~50 babel-plugin tests + cross-handler tests + JSX automatic-runtime fixtures + custom-import-source fixtures byte-clean | — | STATUS.md updated | Full harness green |

---

## Phase 7 — Comment placement and `Program::exit` ordering

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §7.1 | ☐ | Build comment-shape diff tool — parse both prettier outputs back, walk comment array, compare attachment | — | `parity-harness/comment-diff.ts` | Tool runs on representative fixtures and prints attachment differences |
| §7.2 | ☐ | Hunt every comment-placement divergence (version banner, `preserveLeadingComments`, `appendRuntimeImports` order, `@compiled-disable-*` directives) | — | Code fixes in the plugin | Harness reports zero comment-related divergences |
| §7.3 | ☐ | **Phase 7 exit gate:** full corpus zero comment divergence | — | STATUS.md updated | All harness tests pass with comment-shape diff tool clean |

---

## Phase 8 — Corpus diff at scale and rollout gate

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §8.1 | ☐ | Run parity harness across 100k+ Compiled call sites in the consumer monorepo | — | Capture every divergence; treat each as a blocking bug | Iterate until zero divergence holds for ≥2 consecutive weeks |
| §8.2 | ☐ | Stand up `cargo-fuzz` targets that synthesise plausible Compiled inputs | — | `crates/babel-plugin-fuzz/` | 72h continuous fuzz run finds no new divergence |
| §8.3 | ☐ | Shadow-mode CI — real builds use Babel; SWC runs in parallel; alarm on hash divergence | — | CI config | Two consecutive weeks of zero divergence in shadow mode |
| §8.4 | ☐ | **Phase 8 exit gate:** all of the above sustained | — | STATUS.md updated | Sustained green; ready for rollout |

---

## Phase 9 — Rollout

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §9.1 | ☐ | Engine flag default = Babel | — | Parcel transformer reads `COMPILED_TRANSFORMER` env var | Default behaviour unchanged for unflagged consumers |
| §9.2 | ☐ | Ship Rust artefacts via `napi build` for linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64-msvc | — | Per-platform `.wasm` (or `.node` if architecture changes) | Each platform binary runs the harness clean |
| §9.3 | ☐ | Internal opt-in via `COMPILED_TRANSFORMER=swc` | — | Internal teams flip the flag | Production traffic on SWC for opt-in repos |
| §9.4 | ☐ | Hash-shadow in production: compute SWC hash, compare to Babel hash, log divergence | — | Production hash-shadow telemetry | Zero divergence over N weeks |
| §9.5 | ☐ | Flip default to SWC after sustained zero divergence | — | Parcel transformer default flipped | Production now on SWC; Babel kept as oracle |
| §9.6 | ☐ | Keep Babel pipeline in tree for ≥1 year as parity oracle | — | This is a non-removal contract | Babel code untouched |

---

## Cardinal rules conformance

These are the standing invariants. A checkpoint that violates one is
not "done" — it is rejected at review.

- **Bytes after prettier are the contract.** Not "looks right." Not
  "passes tests." Bytes.
- **CSS class names live inside string literals.** Hashing is part of
  the byte contract.
- **No filesystem access outside `/cwd`** inside the plugin. Ever.
- **No JS callbacks from the plugin.** Side effects go via sidecar JSON
  written to `/cwd/<callScratch>/...`.
- **Don't bump `@swc/core` casually.** ABI breaks. Coordinated
  `swc_core` bump + full corpus rerun required.
- **Bugs are features.** Behavioural differences under the parity
  harness are port defects, not bug-fix opportunities.
- **1:1 file mapping is enforced.** PLAN.md constraint 4. If you feel
  the urge to deviate, stop and ask.
- **No half-baked compat shims.** If `compat/<name>.rs` is incomplete,
  it will break in production. Finish it or escalate.
- **Build with `RUSTFLAGS=""`** for any crate that pulls in proc-macro
  deps via `swc_core`. User-global `lto=thin` breaks proc-macro
  builds.
