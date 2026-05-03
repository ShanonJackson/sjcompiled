# Phased Execution Plan — `packages/css/src/transform.ts` → Rust

> Read `crates/PARITY_VERSIONS.md` first. This document tells you the **order**
> in which to do the work; that document tells you **what version of what** to
> port. They are read together.
>
> Every phase has an **exit gate**: a measurable, byte-level diff condition
> that must hold before the next phase begins. **Do not skip exit gates.** A
> divergence missed at phase N becomes an unfindable bug at phase N+5.

---

## Reading guide

- **Phase numbering** is dependency-ordered. Phases with the same number letter
  (e.g. `4a`, `4b`) can run in parallel — they have no dependency on each
  other and only depend on prior phases.
- **Exit gate** is the parity contract for that phase. The harness from Phase 0
  is the enforcement mechanism throughout.
- **Effort** is rough calendar weeks for one strong engineer. Multi-engineer
  parallelization can compress phases marked "parallel-friendly."
- **Risk** flags items where divergence is most likely. Budget extra time.

---

## Dependency graph (high level)

```
Phase 0  parity-runner + corpus  ──────────────────────────────────────────┐
                                                                            │
Phase 1  postcss-core   caniuse-db   colord   fraction-js                  │
            │              │                                                │
Phase 2  ───┼──────────────┼─── postcss-selector-parser                    │
            ├──────────────┼─── postcss-value-parser                       │
            ├──────────────┼─── postcss-values-parser                      │
            │              ├─── browserslist-shim                          │
            └──────────────┼─── cssnano-utils                              │
                           │                                                │
Phase 3                    └─── caniuse-api                                │
                                                                            │
Phase 4  compiled-css local plugins (5 sub-bands a–e, parallel-friendly)   │
                                                                            │
Phase 5  postcss-nested   postcss-normalize-whitespace   postcss-discard-duplicates(v6)
            │                                              │
            │                                              └─── sort() PARITY GATE
            │
Phase 6  cssnano plugins (8 sub-bands a–h, parallel-friendly)
            │
            └─── cssnano-preset-default orchestrator
                                                                            │
Phase 7  autoprefixer  (solo — biggest single port)                        │
                                                                            │
Phase 8  compiled-css-napi   transformCss assembly   sort assembly         │
                                                                            │
Phase 9  full-pipeline diff at scale (corpus replay + cargo-fuzz)          │
                                                                            │
Phase 10 rollout  (shadow → opt-in → default)                              │
                                                                            │
                                  ↑ all phases run their corpus diff via Phase 0
```

---

## Phase 0 — Differential harness + corpus

**Why first:** every later phase relies on this to detect divergence. If the
oracle is wrong, the port will silently drift.

**Deliverables:**
- `crates/parity-runner/` — Rust binary that:
  - Loads a corpus of `(css, opts)` tuples.
  - Runs the JS pipeline (subprocess: `node` invoking
    `packages/css/src/transform.ts` and `sort.ts` directly).
  - Runs the Rust pipeline (initially empty — feature-flagged).
  - Byte-compares both outputs.
  - On divergence, emits the smallest divergent byte range with surrounding
    context.
- `corpus/` — checked-in inputs. Sources:
  1. Every input from `packages/babel-plugin/**/__tests__/`.
  2. Every input from `packages/css/**/__tests__/`.
  3. A side-branch instrumentation of `transformCss` that dumps real inputs
     from a build of the largest internal consumer.
  4. Synthesized adversarial inputs (deeply nested selectors, comment
     placement edge cases, Unicode in values, BOMs, mixed line endings).
- `crates/parity-runner/README.md` — how to run, how to add inputs.
- CI job that runs the harness on every PR.

**Exit gate:**
- Harness runs the **JS pipeline against itself** across 1000+ corpus inputs
  with **zero non-determinism** across two consecutive runs and across two
  different machines (Linux + macOS or Linux + Windows). Any non-determinism
  here is a blocker — investigate browserslist resolution, env vars, file
  system order, etc., until JS-against-JS produces stable bytes.
- Harness has a `--rust` flag that runs Rust output (currently empty / panic)
  and reports diff. Wire is live, just empty.

**Effort:** 2 weeks. **Risk:** medium — JS-against-JS non-determinism is
common to discover here.

---

## Phase 1 — Foundation crates (parallel-friendly, 4 streams)

These have no inter-dependencies. Each can be ported by a separate engineer.

### 1a. `crates/postcss-core` — `postcss@8.4.31`

- Port `node_modules/postcss/lib/tokenize.js` → `tokenize.rs`.
- Port `node_modules/postcss/lib/parser.js` → `parser.rs`.
- Port `node_modules/postcss/lib/{root,atrule,rule,declaration,comment,container,node}.js` → AST modules.
- Port `node_modules/postcss/lib/stringifier.js` → `stringify.rs`.
- The `raws` object is the load-bearing detail. Every `before`/`after`/`between`/`semicolon`/`afterName`/`left`/`right`/`value.raw` field must be preserved 1:1.
- Use `IndexMap` (insertion-ordered) wherever upstream uses Object.
- Number formatting: any place upstream stringifies a JS number, the Rust port must produce the byte-identical string (audit each call site; ryū may disagree on edge cases).
- Inline-port `nanoid@3.3.6` for source-id only if any output path reaches it.
- Inline-port `picocolors@1.1.1` for error message strings (errors are user-visible).

**Exit gate:** for every CSS input in the corpus, `parse(css).toString() === css` produces zero diff against the JS `postcss.parse(css).toString()`. Round-trip identity is the prerequisite for any plugin work.

**Effort:** 4 weeks. **Risk:** high — UTF-16 vs UTF-8 column counting, regex divergence, raws edge cases.

### 1b. `crates/caniuse-db` — pure data

- Vendor `node_modules/caniuse-lite/data/` JSON snapshots into `crates/caniuse-db/data/`.
- Vendor `node_modules/electron-to-chromium/` and `node_modules/node-releases/data/` similarly.
- `build.rs` codegens Rust static tables from the JSON.
- Expose Rust APIs that mirror `caniuse-lite`'s exported shape (feature lookups, agents, regions).

**Exit gate:** unit tests that load a sampled set of features (`flexbox`, `grid`, `css-variables`, `mask`, etc.) and verify the agent support matrix matches the JS-side `caniuse-lite` output byte-for-byte (JSON.stringify with stable key order).

**Effort:** 1.5 weeks. **Risk:** low (data only).

### 1c. `crates/colord` — `colord@2.9.1`

- Port `node_modules/colord/index.mjs` and the small set of plugins our consumers reach.
- Color math (HSL ↔ RGB ↔ Hex) must produce byte-identical strings — `#fff` vs `#ffffff`, leading zeros, alpha format, etc.

**Exit gate:** every public function exercised in the cssnano plugins that depend on colord is byte-tested against JS output across a color corpus.

**Effort:** 1.5 weeks. **Risk:** medium (float rounding in HSL conversions).

### 1d. `crates/fraction-js` — `fraction.js@4.2.0`

- Small library; port `node_modules/fraction.js/fraction.js`.
- Used by `autoprefixer`. (NOT by `postcss-convert-values@5.1.3` — verified during Phase 6f; upstream uses plain `Number`/`Math.round`, no fraction.js import.) The output depends on stringified fraction outputs, so byte parity matters.

**Exit gate:** all public-API operations (`add`, `sub`, `mul`, `div`, `toString`, `toFraction`) byte-tested against fraction.js across a number corpus.

**Effort:** 1 week. **Risk:** low.

---

## Phase 2 — Layer-2 utilities (parallel-friendly, 5 streams)

All depend on Phase 1.

### 2a. `crates/postcss-selector-parser` — `6.0.13`

- Depends on: `postcss-core`.
- Port `node_modules/postcss-selector-parser/dist/`.
- Round-trip identity test: for selector corpus `parse(s).toString() === s`.

**Exit gate:** zero round-trip diff over a selector corpus extracted from the CSS corpus.
**Effort:** 3 weeks. **Risk:** medium (selector tokenization edge cases).

### 2b. `crates/postcss-value-parser` — `4.2.0`

- Depends on: `postcss-core` (loosely — only AST type compatibility).
- Port `node_modules/postcss-value-parser/lib/`.
- Round-trip identity required.

**Exit gate:** zero round-trip diff over a value corpus.
**Effort:** 1.5 weeks. **Risk:** low–medium.

### 2c. `crates/postcss-values-parser` — `6.0.2` (plural — distinct package)

- Depends on: `postcss-core`.
- Port `node_modules/postcss-values-parser/`.
- **Different code from `postcss-value-parser`. Different AST node types
  (`Numeric`, `Word`, `Func`). Do not collapse the two crates.**

**Exit gate:** zero round-trip diff over a value corpus, plus AST-shape parity test (every node type, every property) across the same corpus.
**Effort:** 2 weeks. **Risk:** medium.

### 2d. `crates/browserslist-shim` — `browserslist@4.24.4` config + defaults

- Depends on: `caniuse-db`.
- Wraps `oxc_browserslist` for query parsing.
- Hand-port `node_modules/browserslist/node.js` for config resolution: `package.json` `browserslist` field, `.browserslistrc`, `BROWSERSLIST`, `BROWSERSLIST_CONFIG`, `BROWSERSLIST_ENV`, `BROWSERSLIST_DISABLE_CACHE`, `BROWSERSLIST_STATS`. Match precedence verbatim.
- Override `oxc_browserslist`'s defaults to match `browserslist@4.24.4` exactly (default query, dead-browser rules).
- Disable any caching that's keyed on file mtimes — we want deterministic output.

**Exit gate:** for 1000+ real-world `package.json` + `.browserslistrc` combinations, the resolved browser list matches Node's `require('browserslist')(query)` byte-for-byte.
**Effort:** 2 weeks. **Risk:** high — config resolution has many env-var edge cases.

### 2e. `crates/cssnano-utils` — `cssnano-utils@3.1.0`

- Depends on: `postcss-core`.
- Port `node_modules/cssnano-utils/src/`.
- Used by ~every cssnano plugin we run.

**Exit gate:** all public functions byte-tested against the JS implementation across an input corpus.
**Effort:** 1 week. **Risk:** low.

---

## Phase 3 — Layer-3 utilities

### 3a. `crates/caniuse-api` — `caniuse-api@3.0.0`

- Depends on: `caniuse-db`, `browserslist-shim`.
- Port `node_modules/caniuse-api/src/`.
- The query-against-targets API used by `postcss-colormin`, `postcss-minify-params`, `postcss-reduce-initial`.

**Exit gate:** for a Cartesian sample of `(feature, browserslist-target)` pairs, Rust `caniuse_api::isSupported(feature, targets)` matches JS exactly.
**Effort:** 1 week. **Risk:** medium (hinges on caniuse-db parity and browserslist-shim parity already being solid).

---

## Phase 4 — Local plugins from `packages/css/src/plugins/`

All in `crates/compiled-css`. Sub-bands run parallel-friendly **within Phase 4**.

### 4a. Trivial plugins (parallel)
- `discard-empty-rules`
- `discard-duplicates` (LOCAL — distinct from npm `postcss-discard-duplicates@6`)
- `extract-stylesheets`

**Exit gate (per plugin):** corpus diff with this plugin spliced into the JS pipeline (rest JS, this one Rust). Zero bytes diff.
**Effort:** 1 week total. **Risk:** low.

### 4b. Selector-touching plugins (parallel) — depends on Phase 2a
- `parent-orphaned-pseudos`
- `flatten-multiple-selectors`
- `increase-specificity`

**Exit gate:** as 4a.
**Effort:** 2 weeks total. **Risk:** medium.

### 4c. At-rule and sort plugins (parallel)
- `merge-duplicate-at-rules`
- `sort-atomic-style-sheet`
- `normalize-current-color`
- `sort-pseudo-selectors` (utility used by sort-atomic-style-sheet)
- `sort-shorthand-declarations` (utility used by sort-atomic-style-sheet)

**Exit gate:** as 4a. Pay extra attention to **stable sort** tie-breaks — Rust `sort_by` is stable (matches JS since ES2019), but the comparator must produce identical orderings, including for "equal" elements.
**Effort:** 2 weeks total. **Risk:** medium (sort tie-breaks are subtle).

### 4d. `atomicify-rules` — **CRITICAL** (single stream)
- Reads the hash function from `@compiled/utils` (path: `packages/utils/src/`).
- Whatever hash that is — likely a small custom hash like `hash` or murmur — its Rust port must produce **bit-identical hashes** for identical byte inputs. This is the function whose output becomes class names.
- The class-name compression map (`opts.classNameCompressionMap`) iteration order matters; preserve insertion order (`IndexMap`).

**Exit gate:** zero diff on the corpus, with **specific dedicated tests** that verify hash output for known input strings byte-for-byte against the JS hash.
**Effort:** 1.5 weeks. **Risk:** **HIGH**. This is the single most important plugin. Budget review time.

### 4e. `expand-shorthands/*` — depends on Phase 2c
- Port every file in `packages/css/src/plugins/expand-shorthands/`:
  - `index.ts`, `background.ts`, `flex.ts`, `flex-flow.ts`, `margin.ts`, `outline.ts`, `overflow.ts`, `padding.ts`, `place-content.ts`, `place-items.ts`, `place-self.ts`, `text-decoration.ts`, `utils.ts`, `types.ts`.
- Uses `postcss-values-parser@6.0.2`.
- Watch the CSS variable bailout — `valueIsNotSafeToExpand` returns `true` for `var(--*)` references; that branch must trigger identically.

**Exit gate:** as 4a.
**Effort:** 2.5 weeks. **Risk:** medium-high (lots of CSS shorthand semantics).

---

## Phase 5 — Direct pipeline plugins (parallel-friendly, 3 streams)

### 5a. `crates/postcss-nested` — `5.0.6`
- Depends on: `postcss-core`, `postcss-selector-parser`.
- Port `node_modules/postcss-nested/index.js` line-for-line.
- The `bubble: ['starting-style', ...]` and `unwrap: [...]` config in `transform.ts:48-61` is part of the call site, not the plugin — but the plugin's interpretation of those options must match v5 exactly.
- v5 → v6 changed selector merging semantics; do not consult v6 source.

**Exit gate:** corpus diff with postcss-nested-rust spliced in. Zero bytes.
**Effort:** 2.5 weeks. **Risk:** medium-high (recursion + selector merging).

### 5b. `crates/postcss-normalize-whitespace` — `5.1.1`
- Depends on: `postcss-core`.
- Small plugin. Port `node_modules/postcss-normalize-whitespace/src/`.

**Exit gate:** as 5a.
**Effort:** 1 week. **Risk:** low.

### 5c. `crates/postcss-discard-duplicates` — `6.0.0` (the v6 used by `sort.ts`)
- Depends on: `postcss-core`.
- Port `node_modules/postcss-discard-duplicates/src/`.
- **Distinct from the local `discard-duplicates` ported in Phase 4a.**

**Exit gate:** as 5a.
**Effort:** 0.5 week. **Risk:** low.

### 5x. **`sort()` parity gate**

After 5c + 4c (`merge-duplicate-at-rules` + `sort-atomic-style-sheet`), the
`sort()` entry point can be assembled in Rust as a self-contained pipeline.
Run the corpus through Rust `sort()` and JS `sort()` — **zero bytes diff.**
This is a **partial parity milestone** — the smaller of the two hashing entry
points is now provably byte-exact. Do not cut over yet; just record the
green-light.

---

## Phase 6 — cssnano plugins (parallel-friendly, 8 sub-bands)

All 14 cssnano plugins from the `cssnano-preset-default@5.2.14` manifest in
`PARITY_VERSIONS.md`. Grouped by dependency surface.

### 6a. Simple — postcss-core only (parallel)
- `crates/cssnano-postcss-discard-comments` (5.1.2) — 0.5 week.

### 6b. value-parser dependents (parallel) — depends on Phase 2b
- `crates/cssnano-postcss-normalize-string` (5.1.0) — 0.5 week.
- `crates/cssnano-postcss-normalize-positions` (5.1.1) — 0.5 week.
- `crates/cssnano-postcss-normalize-timing-functions` (5.1.0) — 0.5 week.
- `crates/cssnano-postcss-normalize-url` (5.1.0) — 1 week (URL parsing edge cases).

### 6c. selector-parser dependents — depends on Phase 2a
- `crates/cssnano-postcss-minify-selectors` (5.2.1) — 1.5 weeks. Selector serialization quirks under minification.

### 6d. value-parser + arithmetic
- `crates/cssnano-postcss-ordered-values` (5.1.3) — depends on Phase 2b, 2e. — 1.5 weeks.
- `crates/postcss-calc` (8.2.4) — depends on Phase 2b. — 2 weeks (calc expression evaluation; high diff risk on float math).

### 6e. browserslist-aware, simple — depends on Phase 2d, 3a
- `crates/cssnano-postcss-normalize-unicode` (5.1.1) — 1 week.
- `crates/cssnano-postcss-reduce-initial` (5.1.2) — 1 week.

### 6f. browserslist + value-parser — depends on Phase 2b, 2d, 3a
- `crates/cssnano-postcss-convert-values` (5.1.3) — **DONE** Phase 6f. Pure `Number`/`Math.round` arithmetic (no fraction-js, despite earlier scaffold claim).
- `crates/cssnano-postcss-minify-params` (5.1.4) — 2 weeks (params syntax + caniuse-api gating).

### 6g. Hardest — color + caniuse — depends on Phase 1c, 1d, 2b, 2e, 3a
- `crates/cssnano-postcss-minify-gradients` (5.1.1) — 2 weeks.
- `crates/cssnano-postcss-colormin` (5.3.1) — 3 weeks. **Highest-risk cssnano plugin.** Color downgrade decisions depend on caniuse, colord rounding, and original-vs-minified byte length comparison.

### 6h. Orchestrator
- `crates/cssnano-preset-default` (5.2.14).
- Depends on: every plugin from 6a–6g being byte-clean.
- Replicates `node_modules/cssnano-preset-default/src/index.js`'s plugin tuple list **and order**.
- Exposes the same shape that `normalize-css.ts:66` consumes (`preset.plugins` is `[[creator, options], ...]`).
- **Effort:** 0.5 week.

**Exit gate (per plugin):** corpus diff with that single cssnano plugin spliced into the JS pipeline. Zero bytes diff.

**Exit gate (Phase 6 overall):** corpus diff with the **entire** cssnano subset spliced into the JS pipeline (Rust replaces `normalize-css.ts`'s output) — zero bytes. **Special attention:** the filter-then-execute order must match cssnano-preset-default's source order, not normalize-css.ts's array order (Anomaly #7 in PARITY_VERSIONS.md).

**Effort total for Phase 6:** ~16 weeks. **Risk:** high — this is the longest band. cssnano-postcss-colormin alone is a multi-week port.

---

## Phase 7 — `crates/autoprefixer` — `autoprefixer@10.4.14`

Solo phase. Single largest port.

- Depends on: `postcss-core`, `postcss-value-parser`, `postcss-selector-parser`, `browserslist-shim`, `caniuse-db`, `fraction-js`.
- Port every file in `node_modules/autoprefixer/lib/`:
  - `prefixer.js`, `prefixes.js`, `processor.js`, `value.js`, `selector.js`, `at-rule.js`, `declaration.js`, `resolution.js`, etc.
  - The hack files (`flex.js`, `grid.js`, `gradient.js`, `flex-flow.js`, etc.) — each is an autoprefixer special-case. Port verbatim.
  - The data tables (`data/prefixes.js`) — codegen if helpful, otherwise port literal.
- Honor the `process.env.AUTOPREFIXER === 'off'` switch in `transform.ts:75` (skip the plugin entirely).

**Exit gate:**
- Corpus diff with autoprefixer-rust spliced in (rest JS) — zero bytes. Run across a **broad** browserslist target sweep (default, modern-only, IE11, mobile-only) since autoprefixer's output is a function of the browser list.
- Sub-feature gates: dedicated diff tests for flexbox legacy, grid (IE/Edge legacy syntax), gradients, transforms, filters, position-sticky.

**Effort:** 8 weeks. **Risk:** **highest** — this is where most regressions surface. Budget iteration time.

---

## Phase 8 — Bridge & assembly

### 8a. `crates/compiled-css-napi`

- Depends on: every other crate.
- Expose:
  - `transformCss(css: string, opts: TransformOpts) -> { sheets: string[]; classNames: string[] }`
  - `sort(stylesheet: string, opts: SortOpts) -> string`
- Use `napi-rs`. Build platform binaries for `linux-x64-gnu`, `linux-arm64-gnu`, `darwin-x64`, `darwin-arm64`, `win32-x64-msvc`.

### 8b. Wire `packages/css/src/transform.ts` and `sort.ts`

- Add a feature flag `COMPILED_CSS_ENGINE` (`js` | `rust`, default `js`).
- When `rust`, call into `@compiled/css-native`. When `js`, run the existing pipeline unchanged.
- **Do not delete the JS pipeline.** It stays as the parity oracle and emergency fallback.

**Exit gate:** Phase 0 harness runs both engines under the flag and gets zero bytes diff across the **full corpus**. Both `transformCss` and `sort` are byte-clean end-to-end for the first time.

**Effort:** 2 weeks. **Risk:** medium (NAPI value marshaling, especially around the `callback` patterns in `atomicifyRules` and `extractStyleSheets` — make sure those become Rust-internal vec pushes, not JS-callable functions, since callbacks introduce latency and ordering risk).

---

## Phase 9 — Diff at scale

### 9a. Corpus replay at PR-time
- The Phase 0 harness becomes a required CI check.
- Both engines run on every PR. Any byte diff blocks merge.

### 9b. Coverage-guided fuzzing
- `crates/parity-fuzz/` with `cargo-fuzz` targets:
  - `fuzz_target_transform_css` — arbitrary bytes → assert byte-equality.
  - `fuzz_target_sort` — arbitrary bytes → assert byte-equality.
  - `fuzz_target_postcss_roundtrip` — arbitrary bytes → assert `parse.toString === input`.
- Run on dedicated infra continuously (weeks of compute). Any divergence becomes a corpus entry.

### 9c. Shadow run on a real consumer codebase
- In the largest internal consumer's CI, run both engines, hash both outputs, alarm on divergence. **No production impact** — just observation.

**Exit gate:** 4 consecutive weeks of zero divergence across all three streams (PR replay, fuzz, shadow CI).

**Effort:** ongoing; minimum 4 calendar weeks of clean signal before Phase 10.

---

## Phase 10 — Rollout

### 10a. Hash-shadow in production
- In production builds, run both engines, compute hashes, log divergence — **use only the JS output**. This catches inputs that exist in production but not the test corpus.

### 10b. Internal opt-in
- Flip `COMPILED_CSS_ENGINE=rust` for internal teams via environment override.
- Monitor for build failures, hash mismatches, performance regressions.

### 10c. Default flip
- After ≥ 4 weeks of zero divergence in 10a + 10b, flip the default to `rust`.
- JS engine stays in tree as fallback for at least 12 months.

### 10d. (much later) JS engine removal
- Only after 12+ months of stable Rust default with zero rollback events.
- Even then, the JS source stays as the parity oracle for any future debugging.

**Exit gate:** N/A — this phase is the destination.

**Effort:** 6 calendar weeks (mostly waiting on signal).

---

## Effort summary

| Phase | Description | Calendar weeks (1 eng) | Parallelizable? |
|---|---|---|---|
| 0 | Parity harness + corpus | 2 | no |
| 1 | Foundation crates | 4 (longest sub-stream: postcss-core) | yes (4 streams) |
| 2 | Layer-2 utilities | 3 (longest: selector-parser) | yes (5 streams) |
| 3 | caniuse-api | 1 | no |
| 4 | Local plugins | 6.5 | yes (5 sub-bands) |
| 5 | Direct pipeline plugins | 2.5 | yes (3 streams) |
| 6 | cssnano plugins (14) | 16 | yes (8 sub-bands) |
| 7 | autoprefixer | 8 | no |
| 8 | NAPI + assembly | 2 | no |
| 9 | Diff at scale | 4+ | n/a (ongoing) |
| 10 | Rollout | 6+ | n/a (calendar) |

**Single-engineer total:** ~55–60 weeks (12–14 months).
**With 3 engineers parallel-working:** ~30–35 weeks (7–8 months) — bounded by sequential phases (0 → autoprefixer → NAPI → rollout) and the longest single port (autoprefixer at 8 weeks).

---

## Cross-cutting policies

These apply throughout every phase. Violations are merge-blockers.

1. **Every Rust crate carries a `lib.rs` header naming the JS package and version it ports.** Example: `//! Byte-for-byte port of postcss-colormin@5.3.1`.
2. **Every Rust file maps 1:1 to a JS source file where possible.** `parser.rs` ↔ `parser.js`. Comments cite line numbers in the upstream source for non-obvious sections.
3. **No "improvements" to upstream behavior.** Bugs are features. If you spot a bug, file an issue noting the upstream commit that introduced it; do not fix it in Rust.
4. **`HashMap` is banned in any code path that produces output bytes.** Use `IndexMap`. Add a clippy lint or CI grep.
5. **Every plugin port has a dedicated diff test using the Phase 0 harness, scoped to that plugin spliced into the JS pipeline.** No exceptions.
6. **No version bumps to anything in `PARITY_VERSIONS.md` without a hash-rotation plan.** That document is the contract.
7. **`REFERENCE_LOCK_FILE/yarn.lock` is read-only.** Never regenerate.
8. **Float-to-string formatting is audited per call site.** When in doubt, use a JS-double-to-string algorithm port; do not assume `ryū` matches.
9. **Every source-position-related field uses JS-equivalent (UTF-16 code unit) counting wherever positions reach output.** Otherwise UTF-8.
10. **The JS pipeline stays in tree as the oracle.** Do not delete `packages/css/src/transform.ts` or `sort.ts` until Phase 10d (12+ months post-rollout).

---

## Failure mode playbooks

### "Phase N's exit gate has a 1-byte diff."
- **Stop.** Do not proceed to Phase N+1.
- Use `parity-runner`'s diff output to localize the divergent byte range.
- Compare against the upstream JS file at the pinned version (read `node_modules/<pkg>/`).
- If the divergence is in stringification: check raws preservation, number formatting, IndexMap order.
- If in a plugin: check plugin run order, default options, browserslist resolution.
- Add the divergent input to the corpus permanently.

### "Phase 0 finds JS-against-JS non-determinism."
- Likely browserslist resolution differs across machines. Audit env vars, `BROWSERSLIST_DISABLE_CACHE`, file system order in config discovery.
- Likely caniuse-lite has been updated on one machine. Force-pin to the lockfile version.
- Do not proceed until the JS pipeline is deterministic.

### "Autoprefixer Phase 7 is taking too long."
- Expected. Allocate more time, not shortcuts. The single highest-risk port.
- If a sub-feature (e.g., grid) is consistently divergent, isolate it as its own crate sub-module with its own diff test.
