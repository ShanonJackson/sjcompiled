# Parity Versions — Read This First

> **This document is a contract, not a reference.** Every Rust crate under `crates/`
> is a byte-for-byte port of a specific version of a JavaScript package. If a
> version drifts, hashes change. If hashes change, the consuming codebase
> (~10,000–20,000 call sites) experiences silent breakage that is effectively
> impossible to debug case-by-case.
>
> **Bugs in the upstream JS at the pinned version are bugs we replicate.** We do
> not "fix" them in Rust. We do not "improve" output. We do not "modernize."
> Output bytes are an invariant.

---

## The Stakes

`packages/css/src/transform.ts` produces atomic CSS sheets. Class names are
derived from a hash of the generated CSS bytes. A single byte of difference —
one extra space, one re-ordered declaration, one different vendor prefix —
produces a different hash, which breaks every consumer that expects the
original class names.

The Rust port must produce **identical bytes** for **every input** that the JS
pipeline accepts. Whitespace, comment passthrough, declaration order, vendor
prefix selection, sort stability, number formatting, and error messages are all
in scope.

---

## Source of Truth

The authoritative version pin for every dependency is the **AFM/JIRA monorepo
resolution** as captured in `AFM_MONOREPO_DEPENDENCIES_MORE.md` (the
fully-resolved 61-item dependency manifest of `@compiled/css@0.19.0` as it is
actually installed inside Atlassian's Frontend Monorepo).

`REFERENCE_LOCK_FILE/yarn.lock` is the **upstream `compiled` repo's**
lockfile — it differs from AFM resolution for several byte-affecting
packages: `postcss` (8.4.31 vs 8.5.6), `postcss-selector-parser`
(6.0.13 vs 6.1.2), `browserslist` (4.24.4 vs 4.24.2), `caniuse-lite`
(1.0.30001690 vs 1.0.30001766), `colord` (2.9.1 vs 2.9.3),
`electron-to-chromium` (1.5.76 vs 1.5.41), `node-releases` (2.0.19 vs
2.0.18). **AFM wins**; the reference lockfile is retained only as a
historical artifact of the original fork.

The Rust port targets the **AFM-resolved** versions because byte-equality
is measured against the bytes that JIRA's installed `@compiled/css@0.19.0`
emits — not against the bytes that `compiled@HEAD` would emit.

### JS oracle source pin (the source code we port from)

`packages/css/src/` mirrors `@compiled/css@0.19.0` at upstream commit
`40a45489eaaacc023110c3f107d702a389232892` (`Version Packages (#1787)`,
2025-01-28). **Do not** overlay this directory with `compiled@HEAD`
source — HEAD is at `0.21.0`, which adds `flatten-multiple-selectors`
to the pipeline (changes hashes), changes `expand-shorthands/flex.ts`
(47-line delta), `sort-atomic-style-sheet.ts` (10-line delta), and
renames `parse-at-rule.ts` → `parse-media-query.ts`. None of those
exist in the 0.19.0 line that AFM consumes.

`packages/utils/src/` mirrors `@compiled/utils@0.13.2` at upstream
commit `130ed3b4ae8a48926892939679c2f1479375f2a8`. The source is
byte-identical between `130ed3b` and `compiled@HEAD` (no diff in
`packages/utils/src`), so the hash function is unaffected by the
version-coordination issue described above.

When in doubt about any package version not listed in the table
below, look it up in `AFM_MONOREPO_DEPENDENCIES_MORE.md`. That answer
is final.

---

## Pinned Versions (the parity manifest)

These are the versions reachable from `packages/css/package.json` after yarn
resolution. The Rust crates listed in the right-most column port these specific
versions and no others.

### SWC plugin runtime (used by `crates/babel-plugin/`, `crates/babel-plugin-strip-runtime/`)

The Rust SWC plugins compile against a specific `swc_core` crate version
that is ABI-compatible with a specific `@swc/core` runtime release.
Mismatch = plugin rejected at load time.

| npm package | Pinned version | Rust crate | Notes |
|---|---|---|---|
| `@swc/core` | **1.15.8** | n/a (npm package, used at runtime by Parcel transformer wrapper) | Plugin loader. ABI surface frozen here. |
| `swc_core` | **54.0.0** | crate dep in `crates/babel-plugin/Cargo.toml` and `crates/babel-plugin-strip-runtime/Cargo.toml` | Verified via `https://github.com/swc-project/swc/blob/v1.15.8/crates/swc_core/Cargo.toml`. |

**Cardinal rule for SWC pins:** bumping `@swc/core` requires a coordinated
`swc_core` bump and a full corpus re-run. The plan §1 constraint 7 is
load-bearing: the wasm32-wasip1 ABI between plugin and runtime is what
makes "drop-in replacement" feasible at all.

### Prettier (used by the parity oracle)

The verification oracle (`plugins/PLAN.md` §2) is post-prettier byte
equality: `prettier(babelOutput) === prettier(swcOutput)`. Both calls
must run on the same prettier version, otherwise the oracle drifts.

| npm package | Pinned version | Notes |
|---|---|---|
| `prettier` | **2.8.8** | Resolved from `REFERENCE_LOCK_FILE/yarn.lock`. Parser: `babel-ts`. Pinned in root `package.json` `overrides` so bun's caret resolution cannot drift past it. |

### `@babel/generator` + `@babel/parser` (used by `packages/babel-plugin/src/utils/` and the Phase 4 compat-generator parity oracle)

`packages/babel-plugin@0.36.1`'s `css-builders.ts:464` calls
`hash(generate(expression).code)` to compute keyframe class names.
Output bytes from `@babel/generator` feed `compiled-utils::hash`
with no prettier downstream, so any whitespace / paren / quote
divergence between versions silently renames classes in production.
`crates/babel-plugin/src/compat/generator.rs` (Phase 4 §4.3) ports
this version verbatim; `packages/babel-plugin/package.json:18`
declares only the floor (`^7.26.10`) and bun's caret resolution
floats past it (observed pre-pin: `@babel/generator@7.29.1`,
`@babel/parser@7.29.3`).

The Phase 4 §4.2 oracle (`parity-harness/compat-generator/oracle.mjs`)
parses `input_source` strings via `@babel/parser` before calling
`generate(ast).code`. Both packages must be pinned together — a
floating parser hands the pinned generator an AST shape from a
later version, which surfaces as silent generator-shape drift on
edge cases (TS syntax, JSX, optional-chaining shape).

| npm package | Pinned version | Notes |
|---|---|---|
| `@babel/generator` | **7.23.0** | AFM-resolved under `@compiled/babel-plugin@0.36.1` (commit `16a62b8`). Source of truth: AFM dependency engineer (2026-05-04). |
| `@babel/parser` | **7.29.2** | AFM-resolved under `@compiled/babel-plugin@0.36.1` (commit `16a62b8`). Source of truth: AFM dependency engineer (2026-05-04). |

Both pinned in root `package.json#overrides` (2026-05-04). Verified
no perturbation to the existing 2559-test verification block
(43 babel-plugin lib + 4 hash_parity over 10037 entries +
3 transform_css_integration over 120 entries + 56 babel-plugin-strip-runtime lib +
31 compiled-utils lib + 1132 strip-runtime harness +
954 babel-plugin harness + 336 equality-harness verify).

### Direct dependencies of `@compiled/css`

| npm package | Range in `packages/css/package.json` | Resolved version | Rust crate | Used in |
|---|---|---|---|---|
| `postcss` | `^8.4.31` | **8.5.6** | `crates/postcss-core` | `transform.ts`, `sort.ts`, every plugin. Bumped from 8.4.31 → 8.5.6 after empirical diff confirmed identical byte-output for `parse(css).toString()` and for full plugin pipelines (26/26 raw round-trips + 30/30 plugin × input pairs). See `crates/_vendor/test-postcss-versions/` for the diff harness. Changes between versions are diagnostic/sourcemap surface only — none reach the hashing path. |
| `postcss-nested` | `^5.0.6` | **5.0.6** | `crates/postcss-nested` | `transform.ts:48` |
| `postcss-normalize-whitespace` | `^5.1.1` | **5.1.1** | `crates/postcss-normalize-whitespace` | `transform.ts:76` |
| `postcss-selector-parser` | `^6.0.13` | **6.1.2** | `crates/postcss-selector-parser` | local selector-touching plugins. AFM-resolved version (compiled@HEAD lockfile pins 6.0.13; AFM resolves 6.1.2). Diff in upstream `dist/` between 6.0.13 and 6.1.2 is small and must be re-audited against `crates/postcss-selector-parser`. |
| `postcss-discard-duplicates` | `^6.0.0` | **6.0.0** | `crates/postcss-discard-duplicates` | **`sort.ts:2`** (second hashing entry point) |
| `postcss-values-parser` | `^6.0.2` | **6.0.2** | `crates/postcss-values-parser` | every file in `plugins/expand-shorthands/` |
| `autoprefixer` | `^10.4.14` | **10.4.14** | `crates/autoprefixer` | `transform.ts:75` |
| `cssnano-preset-default` | `^5.2.14` | **5.2.14** | `crates/cssnano-preset-default` (orchestrator) | `plugins/normalize-css.ts:1` (loads 14 sub-plugins — see manifest below) |

### Transitive dependencies that affect output bytes

| npm package | Resolved version | Why it matters | Rust crate |
|---|---|---|---|
| `postcss-value-parser` | **4.2.0** | Used by `autoprefixer` and many cssnano plugins | `crates/postcss-value-parser` |
| `browserslist` | **4.24.2** | Resolves browser targets for `autoprefixer` AND for several cssnano plugins (see below). AFM-resolved (compiled@HEAD lockfile pins 4.24.4 — AFM is one patch *behind*). | `crates/browserslist-shim` (wraps `oxc_browserslist`) |
| `caniuse-lite` | **1.0.30001766** | **The silent invariant.** Drives `autoprefixer` AND `caniuse-api` (which `postcss-colormin` etc. use). Vendor the JSON snapshot from `node_modules/caniuse-lite/data/` and codegen Rust tables via `build.rs`. AFM-resolved (compiled@HEAD lockfile pins 1.0.30001690; ~76 monthly snapshots later). | `crates/caniuse-db` |
| `caniuse-api` | **3.0.0** | Wrapper used by cssnano plugins (`postcss-colormin`, etc.) to query caniuse-lite via browserslist targets | `crates/caniuse-api` |
| `colord` | **2.9.3** | Color manipulation; used by `postcss-colormin` and `postcss-minify-gradients`. AFM-resolved (compiled@HEAD lockfile pins 2.9.1). | `crates/colord` |
| `cssnano-utils` | **3.1.0** | Shared helpers used by ~every cssnano plugin we run | `crates/cssnano-utils` |
| `electron-to-chromium` | **1.5.41** | Feeds browserslist resolution. AFM-resolved (compiled@HEAD lockfile pins 1.5.76). | `crates/caniuse-db` (vendored alongside) |
| `node-releases` | **2.0.18** | Feeds browserslist resolution. AFM-resolved (compiled@HEAD lockfile pins 2.0.19). | `crates/caniuse-db` (vendored alongside) |
| `fraction.js` | **4.2.0** | Used in autoprefixer's grid math. **NOT** used by `postcss-convert-values@5.1.3` (the upstream source has no fraction.js import — pure `Number`/`Math.round` arithmetic; verified during the Phase 6f port). | `crates/fraction-js` |
| `nanoid` | **3.3.6** | Source-id generation in postcss | port inline into `crates/postcss-core` (only if reachable from output bytes) |
| `picocolors` | **1.1.1** | Error formatting in postcss | port inline into `crates/postcss-core` (errors are user-visible — match strings) |
| `source-map-js` | **1.0.2** | Sourcemap generation in postcss | not on the hashing path; pin anyway |
| `update-browserslist-db` | **1.1.1** | Build-time only | not relevant at runtime |

### `cssnano-preset-default@5.2.14` sub-plugin manifest

`packages/css/src/plugins/normalize-css.ts` instantiates
`cssnano-preset-default@5.2.14`, then keeps only the plugins whose
`postcssPlugin` name matches the union of `BASE_PLUGINS` and (when
`optimizeCss` is true) `PROD_PLUGINS`. **The execution order is
cssnano-preset-default's source order, NOT the order they appear in
`normalize-css.ts`.** The Rust port must reproduce both the filter and the
underlying source-defined order.

These 14 plugins are part of the hashing path. **Each is a separate
byte-for-byte port.**

#### Always run (BASE_PLUGINS)

| npm package | Resolved version | Rust crate | Browserslist-aware? |
|---|---|---|---|
| `postcss-minify-selectors` | **5.2.1** | `crates/cssnano-postcss-minify-selectors` | no |
| `postcss-minify-params` | **5.1.4** | `crates/cssnano-postcss-minify-params` | **yes** (uses `caniuse-api`) |

#### Run when `optimizeCss !== false` (PROD_PLUGINS)

| npm package | Resolved version | Rust crate | Browserslist-aware? |
|---|---|---|---|
| `postcss-ordered-values` | **5.1.3** | `crates/cssnano-postcss-ordered-values` | no |
| `postcss-reduce-initial` | **5.1.2** | `crates/cssnano-postcss-reduce-initial` | **yes** (uses `caniuse-api`) |
| `postcss-convert-values` | **5.1.3** | `crates/cssnano-postcss-convert-values` | **yes** (uses browserslist) |
| `postcss-colormin` | **5.3.1** | `crates/cssnano-postcss-colormin` | **yes** (uses `caniuse-api` + `colord`) |
| `postcss-normalize-url` | **5.1.0** | `crates/cssnano-postcss-normalize-url` | no |
| `postcss-normalize-unicode` | **5.1.1** | `crates/cssnano-postcss-normalize-unicode` | **yes** (uses browserslist) |
| `postcss-normalize-string` | **5.1.0** | `crates/cssnano-postcss-normalize-string` | no |
| `postcss-normalize-positions` | **5.1.1** | `crates/cssnano-postcss-normalize-positions` | no |
| `postcss-normalize-timing-functions` | **5.1.0** | `crates/cssnano-postcss-normalize-timing-functions` | no |
| `postcss-minify-gradients` | **5.1.1** | `crates/cssnano-postcss-minify-gradients` | uses `colord` + `cssnano-utils` |
| `postcss-discard-comments` | **5.1.2** | `crates/cssnano-postcss-discard-comments` | no |
| `postcss-calc` | **8.2.4** | `crates/postcss-calc` | no |

Plus one local plugin appended after the cssnano list when `optimizeCss !== false`:

| Local plugin | Source | Rust home |
|---|---|---|
| `normalize-current-color` | `packages/css/src/plugins/normalize-current-color.ts` | `crates/compiled-css` |

### Transitive dependencies present but NOT on the hashing path

These are pinned for completeness but do not need Rust ports unless audit shows
they touch output bytes.

| npm package | Resolved version |
|---|---|
| `postcss-selector-parser` (older path `^3.0.0`) | 3.1.2 (legacy, not used by our pipeline) |
| `postcss-value-parser` (older path `^3.0.0`) | 3.3.1 (legacy, not used by our pipeline) |
| `postcss-discard-duplicates` (older path `^5.1.0`) | 5.1.0 (loaded by cssnano-preset-default but filtered out before execution; v6.0.0 is the one we actually run) |

---

## Hashing entry points

There are **two** public functions whose outputs must be byte-exact:

1. **`transformCss(css, opts)`** in `packages/css/src/transform.ts:33`.
   Runs the full pipeline:
   `discardDuplicates (local) → discardEmptyRules (local) → parentOrphanedPseudos (local) → postcss-nested@5.0.6 → normalizeCSS (cssnano subset + normalizeCurrentColor) → expandShorthands (local, uses postcss-values-parser@6.0.2) → atomicifyRules (local) → increaseSpecificity (local, conditional) → sortAtomicStyleSheet (local) → autoprefixer@10.4.14 → postcss-normalize-whitespace@5.1.1 → extractStyleSheets (local)`.

2. **`sort(stylesheet, opts)`** in `packages/css/src/sort.ts:13`.
   Runs:
   `postcss-discard-duplicates@6.0.0 → mergeDuplicateAtRules (local) → sortAtomicStyleSheet (local)`.

Both must be wired through the NAPI bridge and both must be diff-tested against
the JS implementation as part of the parity gate.

## Anomalies (read these — they will bite you)

1. **`postcss-nested@5.0.6`, NOT 6.x.** The v5 → v6 rewrite changed selector
   merging semantics. The `bubble: ['starting-style', ...]` workaround in
   `transform.ts:48-61` is specifically a v5-era workaround. Porting v6 source
   will silently change output for nested rules.

2. **`postcss-normalize-whitespace@5.1.1`, NOT 4.x or 6.x.** Each major line
   has different whitespace rules. v5 is what the JS pipeline runs. Anything
   else changes output bytes.

3. **`caniuse-lite@1.0.30001766`** is the silent invariant.
   `caniuse-lite` updates monthly, and autoprefixer's vendor-prefix decisions
   depend entirely on this DB. The Rust crate `crates/caniuse-db/` MUST vendor
   the JSON snapshot from this exact version. Do not let it auto-update.
   **Note:** caniuse-lite drives more than just autoprefixer. Several cssnano
   plugins (`postcss-colormin`, `postcss-minify-params`, `postcss-reduce-initial`,
   `postcss-convert-values`, `postcss-normalize-unicode`) also gate decisions
   on browser support. Same DB, multiple consumers.

4. **`browserslist@4.24.2` defaults** are version-specific (default query,
   "dead browser" list, evaluation semantics). `oxc_browserslist` may default
   to a newer version's behavior. The shim crate must override defaults to
   match 4.24.2 exactly. Browserslist is consumed by autoprefixer **and** by
   every "browserslist-aware" cssnano plugin in the manifest above.
   **Note:** AFM is one patch *behind* the upstream `compiled` lockfile
   (which pins 4.24.4). Verify the 4.24.4 → 4.24.2 patch direction with the
   AFM dependency engineer before assuming defaults are equivalent.

5. **Two versions of `postcss-discard-duplicates` are present in the tree.**
   - **v6.0.0** is a direct dep, used by `sort.ts` — port this.
   - **v5.1.0** is pulled in transitively by `cssnano-preset-default@5.2.14`,
     but is filtered out by `normalize-css.ts:62-72` before execution (the
     filter only keeps the 14 named plugins). **Do not port v5.1.0** — it
     never runs on our hashing path.
   The two are different code; do not conflate them.

6. **`postcss-values-parser@6.0.2` (plural)** is distinct from
   `postcss-value-parser@4.2.0` (singular). Both exist in the tree, both are
   on the hashing path, both must be ported as separate crates. Naming them
   carelessly in Rust will lead to silent substitution.

7. **cssnano plugin execution order is cssnano-preset-default's source order**,
   not the order in `normalize-css.ts`'s `BASE_PLUGINS`/`PROD_PLUGINS` arrays.
   `normalize-css.ts` filters by name from a pre-ordered list. The Rust port
   must mirror cssnano-preset-default@5.2.14's `src/index.js` plugin tuple
   order verbatim.

8. **Each cssnano plugin runs with `creator()` — i.e., default options.** The
   default options for each pinned plugin version must be replicated exactly.
   Some defaults (e.g., `postcss-discard-comments`'s removal predicate) have
   subtle effects on output.

9. **Local `discardDuplicates` (in `packages/css/src/plugins/`) and
   `postcss-discard-duplicates@6.0.0` are not the same plugin.** The local one
   runs in `transformCss`. The npm one runs in `sort()`. Different code paths,
   different ports — do not collapse them into one Rust crate.

---

## Modification Procedure

Changes to this file or to any pinned version require:

1. A documented justification on a per-version basis.
2. A full corpus diff run showing the byte impact across all known inputs.
3. Sign-off from owners of every consuming codebase, because every version
   change is potentially a breaking hash change.
4. A migration plan for class-name rotation in consumers (this is the part
   that takes months — do not assume it can be done quickly).

**The default answer to "should we bump this?" is no.** The Rust port targets
the versions in this table forever, even after they are EOL upstream.

---

## Crate Ownership Map

Every Rust crate under `crates/` declares which JS package + version it ports
in its `Cargo.toml` description and at the top of its `lib.rs`. Example:

```rust
//! crates/postcss-core
//! Byte-for-byte Rust port of `postcss@8.5.6`.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.
```

### Core PostCSS infrastructure

| Rust crate | Ports | At version | Upstream source location |
|---|---|---|---|
| `crates/postcss-core` | `postcss` | 8.5.6 | `node_modules/postcss/lib/*.js` |
| `crates/postcss-selector-parser` | `postcss-selector-parser` | 6.1.2 | `node_modules/postcss-selector-parser/` |
| `crates/postcss-value-parser` | `postcss-value-parser` | 4.2.0 | `node_modules/postcss-value-parser/lib/` |
| `crates/postcss-values-parser` | `postcss-values-parser` (plural — distinct package) | 6.0.2 | `node_modules/postcss-values-parser/` |

### Browserslist + caniuse data (shared by autoprefixer and several cssnano plugins)

| Rust crate | Ports | At version | Upstream source location |
|---|---|---|---|
| `crates/browserslist-shim` | `browserslist` config resolution + defaults (wraps `oxc_browserslist`) | 4.24.2 | `node_modules/browserslist/node.js`, `index.js` |
| `crates/caniuse-db` | `caniuse-lite` + `electron-to-chromium` + `node-releases` data tables (codegen via `build.rs`) | 1.0.30001766 / 1.5.41 / 2.0.18 | `node_modules/caniuse-lite/data/`, `node_modules/electron-to-chromium/`, `node_modules/node-releases/data/` |
| `crates/caniuse-api` | `caniuse-api` query helper used by cssnano plugins | 3.0.0 | `node_modules/caniuse-api/` |

### Pipeline plugins (transformCss)

| Rust crate | Ports | At version | Upstream source location |
|---|---|---|---|
| `crates/postcss-nested` | `postcss-nested` | 5.0.6 | `node_modules/postcss-nested/index.js` |
| `crates/postcss-normalize-whitespace` | `postcss-normalize-whitespace` | 5.1.1 | `node_modules/postcss-normalize-whitespace/src/` |
| `crates/autoprefixer` | `autoprefixer` | 10.4.14 | `node_modules/autoprefixer/lib/` |
| `crates/fraction-js` | `fraction.js` | 4.2.0 | `node_modules/fraction.js/` |

### Pipeline plugins (sort)

| Rust crate | Ports | At version | Upstream source location |
|---|---|---|---|
| `crates/postcss-discard-duplicates` | `postcss-discard-duplicates` (the v6 used by `sort.ts`) | 6.0.0 | `node_modules/postcss-discard-duplicates/` |

### cssnano-preset-default sub-plugins (instantiated by `normalize-css.ts`)

| Rust crate | Ports | At version | Upstream source location |
|---|---|---|---|
| `crates/cssnano-preset-default` | preset orchestrator (plugin tuple list + source order) | 5.2.14 | `node_modules/cssnano-preset-default/src/index.js` |
| `crates/cssnano-utils` | `cssnano-utils` shared helpers | 3.1.0 | `node_modules/cssnano-utils/` |
| `crates/colord` | `colord` color manipulation | 2.9.3 | `node_modules/colord/` |
| `crates/cssnano-postcss-minify-selectors` | `postcss-minify-selectors` | 5.2.1 | `node_modules/postcss-minify-selectors/` |
| `crates/cssnano-postcss-minify-params` | `postcss-minify-params` | 5.1.4 | `node_modules/postcss-minify-params/` |
| `crates/cssnano-postcss-ordered-values` | `postcss-ordered-values` | 5.1.3 | `node_modules/postcss-ordered-values/` |
| `crates/cssnano-postcss-reduce-initial` | `postcss-reduce-initial` | 5.1.2 | `node_modules/postcss-reduce-initial/` |
| `crates/cssnano-postcss-convert-values` | `postcss-convert-values` | 5.1.3 | `node_modules/postcss-convert-values/` |
| `crates/cssnano-postcss-colormin` | `postcss-colormin` | 5.3.1 | `node_modules/postcss-colormin/` |
| `crates/cssnano-postcss-normalize-url` | `postcss-normalize-url` | 5.1.0 | `node_modules/postcss-normalize-url/` |
| `crates/cssnano-postcss-normalize-unicode` | `postcss-normalize-unicode` | 5.1.1 | `node_modules/postcss-normalize-unicode/` |
| `crates/cssnano-postcss-normalize-string` | `postcss-normalize-string` | 5.1.0 | `node_modules/postcss-normalize-string/` |
| `crates/cssnano-postcss-normalize-positions` | `postcss-normalize-positions` | 5.1.1 | `node_modules/postcss-normalize-positions/` |
| `crates/cssnano-postcss-normalize-timing-functions` | `postcss-normalize-timing-functions` | 5.1.0 | `node_modules/postcss-normalize-timing-functions/` |
| `crates/cssnano-postcss-minify-gradients` | `postcss-minify-gradients` | 5.1.1 | `node_modules/postcss-minify-gradients/` |
| `crates/cssnano-postcss-discard-comments` | `postcss-discard-comments` | 5.1.2 | `node_modules/postcss-discard-comments/` |
| `crates/postcss-calc` | `postcss-calc` | 8.2.4 | `node_modules/postcss-calc/` |

### Local plugins (live in this repo, no upstream npm package)

| Rust crate | Ports | Source |
|---|---|---|
| `crates/compiled-css` | every plugin under `packages/css/src/plugins/` not covered above (atomicifyRules, discardDuplicates-local, discardEmptyRules, expandShorthands, extractStyleSheets, increaseSpecificity, mergeDuplicateAtRules, normalizeCurrentColor, parentOrphanedPseudos, sortAtomicStyleSheet). **`flattenMultipleSelectors` is NOT in this list** — it was added post-0.19.0 (in the 0.20+ series) and is not part of the AFM-pinned pipeline. | `packages/css/src/plugins/` |

### Bridge + tooling

| Rust crate | Purpose |
|---|---|
| `crates/compiled-css-napi` | NAPI bindings exposing `transformCss` and `sort` to Node |
| `crates/parity-runner` | Differential harness running JS and Rust pipelines on a corpus and asserting byte-equality |
| `crates/parity-fuzz` | `cargo-fuzz` targets for coverage-guided divergence discovery |

---

## The Cardinal Rules

1. **Bytes are the contract.** Not behavior. Not semantics. **Bytes.**
2. **Bugs are features.** If `autoprefixer@10.4.14` emits a "wrong" prefix, the
   Rust port emits the same "wrong" prefix.
3. **`AFM_MONOREPO_DEPENDENCIES_MORE.md` is the source of truth** — not the
   reference lockfile, not what `bun install` happens to resolve, not
   "current latest". Every pin in this document mirrors AFM resolution.
4. **Caniuse-lite is frozen at `1.0.30001766`.** Forever (until a coordinated
   rotation with AFM).
5. **No version bumps without a hash-rotation plan.** And that plan takes
   months, not days.
