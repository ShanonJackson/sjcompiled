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

**Next checkpoint:** §1.5 — sidecar handlers. Two distinct outputs:
(1) `compiledRequireExclude=true` writes the accumulated `style_rules`
to `<callScratch>/style-rules.json`; (2) `extractStylesToDirectory.dest`
writes `.compiled.css` files via the `/cwd` preopen. Validate `dest`
against the preopen at plugin entry; fail loudly if outside.

**Prerequisites met:** all of Phase 0 except probes 9 and audit
(both Phase 5 gates, not Phase 1).

**Last completed:** §1.4. Phase 1 §1.0–§1.4 are all ☑. Final state on
sign-off: `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib`
→ 44/44 pass; `bun test parity-harness/strip-runtime/harness.test.ts`
→ 82/82 pass (41 determinism + 41 parity, with 13 fixtures gated
`expectedToFail` via `generate-fixtures.mjs`'s `EXPECTED_TO_FAIL` map
and explicit `failureReason` strings naming the phase that graduates
each: 4 on §1.5, 3 on Phase 2, 6 on Phase 7).

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
  bumped to `@sjcompiled\/` to match the renamed constants.
- Side-effect of the rename: `DEFAULT_IMPORT_SOURCES` is
  `['@sjcompiled/react', '@atlaskit/css']`. Fixtures driven from any
  source string still using `@compiled/react` go untransformed (no
  CC/CS wrappers, every `expectsError` fixture's throw-path is dead).
  `generate-fixtures.mjs` was updated accordingly. **If you regenerate
  fixtures or write new ones, use `@sjcompiled/react` as the import
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
| §1.5 | ☐ | Sidecar handlers: `compiledRequireExclude=true` writes `<callScratch>/style-rules.json`; `extractStylesToDirectory.dest` writes `.compiled.css` files via `/cwd` preopen | — | sidecar write code in `lib.rs` Program::exit; validation of `dest` against preopen at plugin entry; clear error if outside | Harness fixtures with `compiledRequireExclude: true` produce non-empty `style-rules.json`; fixtures with `extractStylesToDirectory` produce non-empty `.compiled.css` |
| §1.6 | ☐ | Lock `plugins/SIDECAR_SCHEMA.md` v1 (PLAN.md §7) | — | `plugins/SIDECAR_SCHEMA.md` with versioned schemas for `style-rules.json`, `included-files.json`, `cache.bin` | Schema file present; both Rust serde structs and JS host parser reference it |
| §1.7 | — | ~~Inline the SWC wrapper in `packages/parcel-transformer/`~~ | — | Parcel-transformer integration is an EXAMPLE consumer shape (`plugins/PARCEL_USAGE_EXAMPLE.md`), not a Phase 1 deliverable. Removed from gate. | n/a |
| §1.8 | ☐ | Generate ≥1000 synthesised already-baked fixtures (run JS babel-plugin against random inputs to produce CC/CS-wrapped code, freeze as fixtures) | — | `parity-harness/strip-runtime/fixtures/synthesized/*.json` | Harness `bun test parity-harness/strip-runtime/harness.test.ts` runs across all synthesised fixtures with zero divergence |
| §1.9 | ☐ | **Phase 1 exit gate:** all checkpoints above closed; full corpus is byte-clean | — | A `phase1-signoff.md` (or update to this STATUS.md) summarising the corpus run | All harness tests pass; ≥1038 fixtures run zero-divergence |

---

## Phase 2 — `babel-plugin` scaffold + dispatcher

> **Goal:** stand up the visitor skeleton + state setup for the larger
> plugin. Pass-through is byte-equal before any handler logic ports.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §2.0 | ☐ | Extract all ~50+ babel-plugin fixtures from `packages/babel-plugin/src/**/__tests__/*.test.ts` into `parity-harness/babel-plugin/fixtures/*.json` | — | One fixture per `it(...)` | Babel-determinism baseline passes for every fixture |
| §2.1 | ☐ | Port `types.rs`, `constants.rs` (data only, no logic) | — | `crates/babel-plugin/src/{types.rs,constants.rs}` | `cargo check -p babel-plugin` passes |
| §2.2 | ☐ | Build `parity-harness/babel-plugin/{engines.ts,harness.test.ts}` mirroring strip-runtime's shape | — | New harness directory parallel to `strip-runtime/` | `bun test parity-harness/babel-plugin/harness.test.ts` runs Babel-determinism baseline cleanly |
| §2.3 | ☐ | Port `lib.rs` entry + dispatcher visitor with stubbed handlers (no-ops that record "would have visited" in a debug log) | — | `crates/babel-plugin/src/lib.rs`, `crates/babel-plugin/src/babel_plugin.rs` | Pass-through SWC plugin produces byte-equal output through the prettier oracle for every fixture (no handler logic yet) |
| §2.4 | ☐ | State struct with `IndexMap` everywhere, `pub(self)` field encapsulation, `MutationRecorder::apply` as only mutator (per `STATE_MUTATIONS.md`) | — | `crates/babel-plugin/src/state.rs`, `crates/babel-plugin/src/mutation_recorder.rs` | Pre-commit lint `grep -rEn 'state\.[a-z_]+\.(push\|set\|add\|insert\|remove\|extend)' crates/babel-plugin/src --include '*.rs' \| grep -v 'state\.rs\|mutation_recorder\.rs'` returns zero matches |
| §2.5 | ☐ | **Phase 2 exit gate:** pass-through harness clean across all babel-plugin fixtures | — | Updated STATUS.md | `bun test parity-harness/babel-plugin/harness.test.ts` passes for every fixture |

---

## Phase 3 — Hash compatibility (consume shared `crates/sjcompiled-utils`)

> **Goal:** prove the Rust `hash` function shared with the CSS port is
> byte-identical to JS `@sjcompiled/utils.hash` from this plugin's
> consuming side.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §3.1 | ☐ | Confirm `crates/sjcompiled-utils` exposes `pub fn hash(input: &str) -> String` | — | `crates/sjcompiled-utils/src/lib.rs` already does this per `crates/STATUS.md` line 243 | `cargo doc -p sjcompiled-utils` shows the public symbol |
| §3.2 | ☐ | Build hash test-vector corpus: ASCII, UTF-8 multibyte, empty, embedded NUL, >4KB, leading/trailing whitespace, real keyframe-expression inputs | — | `crates/babel-plugin/tests/hash_corpus.json` | Corpus has ≥30 entries covering every above case |
| §3.3 | ☐ | Diff Rust `hash` vs JS `hash` over the corpus + 10K random inputs | — | `crates/babel-plugin/tests/hash_parity.rs` (or bun test) | `cargo test -p babel-plugin hash_parity` passes; 10K random inputs all match |
| §3.4 | ☐ | **Phase 3 exit gate:** zero divergence | — | This STATUS.md updated | Parity test green |

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
| §4.1 | ☐ | `transform_css` integration parity test — every JS-corpus input produces byte-identical Rust output from this plugin's perspective | — | `crates/babel-plugin/tests/transform_css_integration.rs` | Diff is zero across the parallel agent's full fixture corpus |
| §4.2 | ☐ | Build `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` enumerating every AST node kind reachable from `keyframes(...)` (and any other `generate(...)` call site) in the consuming monorepo | — | The coverage manifest + parity fixtures under `crates/babel-plugin/tests/compat-generator/` | Manifest reviewed; one parity fixture per node kind |
| §4.3 | ☐ | Port `compat/generator.rs` covering every node kind in the manifest | — | `crates/babel-plugin/src/compat/generator.rs` | All compat-generator parity fixtures byte-clean |
| §4.4 | ☐ | Port `utils/css_builders.rs` line-for-line | — | `crates/babel-plugin/src/utils/css_builders.rs` | Harness fixtures exercising `keyframes`, `css`, `cssMap` are byte-clean |
| §4.5 | ☐ | Port `utils/transform_css_items.rs` and `utils/build_css_variables.rs` | — | Both `.rs` files | Same harness gate |
| §4.6 | ☐ | Wire `transform_css` calls into the visitor (single pass, no scan/apply) | — | Updated `lib.rs` | Harness clean for `keyframes`, `css`, `cssMap` fixtures |
| §4.7 | ☐ | Update Parcel wrapper to a single `transformSync` call (PLAN.md §8) | — | `packages/parcel-transformer/src/index.ts` | Wrapper produces a single SWC call, drains sidecars, returns to Parcel |
| §4.8 | ☐ | **Phase 4 exit gate:** keyframes / css / cssMap fixtures byte-clean | — | STATUS.md updated | All such fixtures green in the parity harness |

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
| §5.1 | ☐ | Re-confirm `STATE_MUTATIONS.md` is current vs upstream Babel; reconcile any new mutation sites | — | Updated STATE_MUTATIONS.md if needed | `grep` enumeration matches doc |
| §5.2 | ☐ | Land the consumer-monorepo refactor (zero outside-cwd includes) | — | refactor PR | §0.10 audit reports zero outliers |
| §5.3 | ☐ | Port `utils/cache.rs` — Layer 1 in-memory + Layer 2 postcard `cache.bin` per PLAN.md §3.9 | — | `crates/babel-plugin/src/utils/cache.rs`, `crates/babel-plugin/src/cache_schema.rs` | `cargo test -p babel-plugin cache::` passes; size + entry caps enforced |
| §5.4 | ☐ | Port `utils/resolve_binding.rs` using `oxc_resolver` configured per §0.11 matrix | — | `crates/babel-plugin/src/utils/resolve_binding.rs` | Resolver-matrix corpus byte-clean against this plugin's resolver wrapper |
| §5.5 | ☐ | Port the entire `traverse_expression/` subtree file-for-file (leaves first) | — | `crates/babel-plugin/src/utils/traverse_expression/**` | Harness `module-traversal` and `expression-evaluation` fixtures byte-clean |
| §5.6 | ☐ | Port `traversers/` and `evaluate_expression.rs` | — | `crates/babel-plugin/src/utils/{traversers,evaluate_expression.rs}/**` | Same as above |
| §5.7 | ☐ | Wire `includedFiles` accumulation → `<callScratch>/included-files.json` sidecar | — | Updated lib.rs Program::exit | Harness fixtures with cross-file imports produce non-empty sidecar; host's `asset.invalidateOnFileChange` matches Babel's |
| §5.8 | ☐ | Promote `scripts/audit-included-files.ts` to CI guardrail | — | CI config update | Audit failure blocks PR merge |
| §5.9 | ☐ | **Phase 5 exit gate:** module-traversal + expression-evaluation byte-clean; `MutationRecorder` shadow-eval suite reports zero replay/live divergence; pre-commit state-mutation lint clean | — | STATUS.md updated | All exit-gate sub-conditions met |

---

## Phase 6 — Per-API handlers (least-risk first)

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §6.1 | ☐ | `keyframes` cleanup-only handler | — | `crates/babel-plugin/src/keyframes/mod.rs` | Keyframes fixtures byte-clean |
| §6.2 | ☐ | `css` (utility) cleanup-only handler | — | `crates/babel-plugin/src/css/mod.rs` | css() fixtures byte-clean |
| §6.3 | ☐ | `cssMap` handler (`process_selectors.rs`) | — | `crates/babel-plugin/src/css_map/{mod.rs,process_selectors.rs}` | cssMap fixtures byte-clean |
| §6.4 | ☐ | `xcss-prop` handler | — | `crates/babel-plugin/src/xcss_prop/mod.rs` | xcss fixtures byte-clean |
| §6.5 | ☐ | `css-prop` handler (comment-placement-sensitive) | — | `crates/babel-plugin/src/css_prop/mod.rs` | css-prop fixtures byte-clean |
| §6.6 | ☐ | `ClassNames` handler | — | `crates/babel-plugin/src/class_names/mod.rs` | ClassNames fixtures byte-clean |
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
