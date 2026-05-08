# `DEFINITIVE_BROWSERSLIST_PLAN.md` — host-resolved browserslist plumbing

> **Status:** active. Final plan.
> **Authors:** sjackson3 + AFM team (V1/V2/V3 empirical investigation).
> **Date:** 2026-05-08.
> **Scope:** close the WASI/SWC-only browserslist-resolution divergence
> reported by AFM in `plugins/BUG_REPORT.md` against AFM's 1000-file
> production sample (40 mismatches, ~96% parity → target ≥99.5% parity).
> **Prerequisite reading:** `CLAUDE.md`, `plugins/BUG_REPORT.md`,
> `plugins/DEV_LOOP.md`, `plugins/STATUS.md`,
> `crates/autoprefixer/src/precomputed.rs` (the architectural template
> we mirror).

---

## 1. Why this exists — empirical bug shape

### 1.1 Symptom

40 of 1000 AFM Jira files emit different CSS bytes between the Babel
pipeline (production today) and the SWC pipeline (migration target).
The divergences cluster on properties that hit cssnano's
`postcss-reduce-initial`, `postcss-colormin`, and autoprefixer:

| Property | Babel (correct) | SWC (divergent) |
|---|---|---|
| `text-decoration-color` | `initial` | `currentColor` |
| `background-color` | `initial` | `transparent` |
| `border-color` | `initial` | `currentColor` |
| `box-sizing` | `initial` | `content-box` |
| `outline-{color,style,width}` | `initial`/`initial`/`initial` | `currentColor`/`none`/`medium` |
| `width: fill` | (no `-moz-` prefix) | extra `-moz-available` prefix |

### 1.2 Root cause (verified end-to-end by AFM-PROBE traces)

Three layers:

1. **JS-side resolution path** — leaf cssnano plugins
   (`postcss-reduce-initial`, `postcss-colormin`, etc.) call
   `browserslist(null, { stats, path: __dirname, env })` internally.
   `__dirname` is the leaf plugin's installed directory under
   `jira/node_modules/postcss-{reduce-initial|colormin}/src/`.
   browserslist's `find_config_file` walks up from there → eventually
   hits `jira/.browserslistrc` (Yarn-classic-hoisted layout) → resolves
   to AFM's 14-entry modern browser list:

   ```
   ["and_chr 144", "chrome 144", "chrome 143", "chrome 142", "chrome 141", "chrome 140",
    "edge 144", "edge 143", "firefox 147", "firefox 146",
    "ios_saf 26.2", "ios_saf 26.1", "safari 26.2", "safari 26.1"]
   ```

   This list is byte-identical between the two leaf plugins (verified)
   and consistent across all 1000 files (no pnpm `__dirname` skew —
   AFM uses Yarn classic with hoisted node_modules). All 14 entries
   are CSS-`initial`-supporting, so
   `caniuse_api::is_supported("css-initial-value", joined) = true` and
   `initial` is preserved. Equivalent reasoning for the other features
   colormin / autoprefixer query.

2. **NAPI-side resolution path** — `browserslist_shim::resolve("", true)`
   inside the Rust port reads `BROWSERSLIST` / `BROWSERSLIST_CONFIG`
   env vars, falls back to a `find_config_file(cwd)` walk if neither
   is set. AFM's NAPI verification ran from `process.chdir(JIRA_ROOT)`
   so the cwd-walk lands on `jira/.browserslistrc` (same file, same
   14-entry list) → byte-equivalent to JS → byte-clean parity verified.

3. **WASI-side resolution path (the bug)** — SWC loads the babel-plugin
   WASM into a wasmtime sandbox. Two things break vs NAPI:

   - **Env vars don't cross the WASI boundary.** AFM verified empirically
     (V3 probe, 4 runs varying `BROWSERSLIST` / `BROWSERSLIST_CONFIG`,
     identical SWC output every time). `std::env::var(...)` returns
     `Err(NotPresent)` inside the plugin.
   - **The cwd walk anchors at `/cwd` (the WASI preopen), not
     `process.cwd()`.** Walking up from `/cwd` hits `/` → no
     `.browserslistrc` reachable → falls through to
     `browserslist@4.24.2` defaults: `> 0.5%, last 2 versions,
     Firefox ESR, not dead`. That list includes ancient browsers
     (IE 11 era pre-Firefox ESR cleanup) → `initial` not universally
     supported → reduce-initial substitutes `initial` → CSS
     bytes diverge.

The bug is **not in `postcss-reduce-initial`** (it's a faithful 1:1
port of the JS plugin). It's at the **deployment-environment
boundary**: the JS pipeline implicitly inherits the host's filesystem
view + env vars; the WASI pipeline doesn't, and we never explicitly
threaded a substitute through.

### 1.3 Why this didn't show up earlier

`crates/css::transform_css` was verified byte-clean against AFM's
monorepo via NAPI, with `process.chdir(JIRA_ROOT)` in the test
bootstrap. That bootstrap happens to make the Rust cwd-walk land on
the same `.browserslistrc` as the JS `__dirname`-walk by coincidence
of cwd. Neither side was actually exercising the production-shaped
resolution path; they coincided by luck.

### 1.4 Cluster size — all 6 browserslist-aware plugins are affected

The same broken pattern (`browserslist_shim::resolve("", true)` →
falls through to defaults inside WASI) exists in every browserslist-
consuming Rust port:

| Plugin | Code pattern | 1000-file divergence count |
|---|---|---|
| `postcss-reduce-initial` | `caniuse_api::is_supported("css-initial-value", "")` | 25 |
| `postcss-colormin` | (signature accepts query, caller passes `""`) | 9 |
| `postcss-convert-values` | `browserslist_shim::resolve("", true)` | 0 (untriggered in sample) |
| `postcss-minify-params` | `browserslist_shim::resolve("", true)` | 0 (untriggered) |
| `postcss-normalize-unicode` | `browserslist_shim::resolve("", true)` | 0 (untriggered) |
| `autoprefixer` | `Browsers::new(...)` cwd-walk | 1 (`width: fill` `-moz-available`) |

Three plugins are silently latent — they have the same broken
resolution path but their inputs in this 1000-file sample don't trip
a property whose minification decision flips between modern-vs-wide
browser lists. The fix MUST cover all 6 to close the systemic bug;
patching only the surfaced 3 would leave latent regressions to
discover later.

### 1.5 Hardening over a JS-side fragility — declared explicitly

The JS-side resolution that AFM-prod uses today is fragile by design:

- It depends on `node_modules/postcss-{plugin}/` being installed in
  a directory tree whose walk-up reaches a `.browserslistrc`.
- Yarn classic with hoisted node_modules makes this work today.
- pnpm isolated layout would NOT work — `__dirname` walks up through
  `.pnpm/postcss-X@Y/node_modules/postcss-X/src/` which doesn't share
  ancestors with `jira/.browserslistrc`.
- A flat node_modules layout in some future tooling change would
  also break it.

Our fix replaces this implicit, layout-dependent resolution with an
explicit, deployment-environment-independent precomputed snapshot.
The fix accidentally hardens AFM's JS pipeline against future layout
changes too. Document this in the AFM bootstrap comment so a future
maintainer doesn't undo it thinking it's redundant.

---

## 2. Constraints

These are non-negotiable. Any deviation requires re-reading
`CLAUDE.md` and explicit user authorisation:

- **C-1 — `packages/*` is immutable.** No edits to
  `packages/babel-plugin`, `packages/babel-plugin-strip-runtime`,
  `packages/css`, `packages/utils`. The fix lands entirely in
  `crates/*` + `parity-harness/*` + the AFM-side bootstrap (which is
  in their monorepo, not this repo).
- **C-2 — Existing byte-equality contracts must hold.**
  - `crates/css::transform_css` NAPI byte-equality (verified against
    AFM monorepo) — **no regression.**
  - `crates/babel-plugin/tests/transform_css_integration.rs` (AFM-
    fixture-pinned) — **stays green.**
  - `bun parity-harness/fixtures-triage.mjs` parity count —
    **does not decrease.**
  - `bun parity-harness/babel-plugin/triage.mjs` parity count —
    **does not decrease (currently 476/477).**
- **C-3 — `Option<_>::None` defaults must mean "current behaviour."**
  Every new field on every struct defaults to `None` and `None`
  preserves today's resolution path bit-for-bit. Existing callers
  that don't update get identical behaviour.
- **C-4 — No environment-variable resolution support in the WASM
  plugin.** Per AFM team alignment: `BROWSERSLIST` /
  `BROWSERSLIST_CONFIG` env vars are intentionally NOT plumbed
  through the WASI boundary. The precomputed snapshot is the only
  supported path. Documented at the WASM plugin entry and in the
  precompute module's docstring.
- **C-5 — No FS I/O inside the WASM plugin's CSS pipeline for
  browserslist resolution.** The snapshot is decoded from in-memory
  bytes (or read from a single host-supplied file path that's
  preopened in WASI). No tree-walks, no `find_config_file`, no
  `caniuse-db` scans per transform.
- **C-6 — No cross-transform state.** SWC tears down the WASI
  instance between calls (CLAUDE.md). No `Lazy<Mutex<Option<...>>>`,
  no module-level caching. State that needs to survive must live in
  `Vec<u8>` snapshots delivered via plugin opts on every call.
- **C-7 — Performance is a side-goal but the precompute pattern is
  strictly better than the naive alternative** (per perf-test.ts /
  CSS_PERF.md). Use the precompute pattern, not a per-call resolution.
- **C-8 — No emoji, no warnings emitted to stderr from the plugin.**
  Per session ruling: silent fallback to current behaviour when the
  snapshot is absent on the WASM path. (Any consumer that forgets the
  bootstrap experiences today's bug; we accept this trade for build-log
  cleanliness.)

---

## 3. Architecture — `PrecomputedBrowserslist` snapshot, parallel to `PrecomputedPrefixes`

### 3.1 Mirror the autoprefixer pattern exactly

`crates/autoprefixer/src/precomputed.rs` already solves an identical
problem for autoprefixer (FS walk + browserslist resolution + downstream
table-iteration done once on the host, snapshotted to postcard bytes,
ingested by the WASI plugin without re-running the slow path). The
cssnano fix follows the same shape:

```
[Host (Node)]                          [WASI plugin]
─────────────                          ─────────────
precomputeBrowserslistDefault(opts)
  → Browserslist resolve (FS walk)
  → caniuse_api::is_supported(F, list)   ──────►   build_caniuse_lookup_from_precomputed(bytes)
    for every feature any cssnano plugin             → IndexMap<&str, bool> in-memory
    queries (small fixed set)                        → leaf plugins do O(1) lookups
  → postcard-encode → Vec<u8>                        → no FS I/O, no env reads,
                                                        no caniuse-db scans
```

### 3.2 New module — `crates/cssnano-browserslist-snapshot/`

Mint a new crate (or new module inside an existing crate; see §3.3)
that exposes:

```rust
/// Layout version. Bump on any field change.
pub const PRECOMPUTED_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct PrecomputedBrowserslist {
    pub format_version: u32,
    /// Resolved "name version" entries (mirrors
    /// `Browsers.selected` in the autoprefixer snapshot).
    /// Kept for callers that need the raw list (currently none in
    /// cssnano, but cheap to ship and useful for debugging).
    pub selected: Vec<String>,
    /// Pre-evaluated `caniuse_api::is_supported(feature, &joined)`
    /// for every feature any cssnano plugin queries. Strictly an
    /// optimisation — byte-equivalent to running `is_supported` live
    /// against `selected`.
    pub feature_support: indexmap::IndexMap<String, bool>,
}

pub fn precompute_browserslist(
    opts: BrowserslistOpts,
) -> PrecomputedBrowserslist { ... }

pub fn precompute_browserslist_default() -> PrecomputedBrowserslist { ... }

pub fn encode_precomputed(snapshot: &PrecomputedBrowserslist) -> Vec<u8> { ... }
pub fn decode_precomputed(bytes: &[u8]) -> Result<PrecomputedBrowserslist, ...> { ... }
```

The fixed feature set (initial value of `feature_support`'s keys) is
the union of every feature name any of the 5 cssnano plugins passes
to `caniuse_api::is_supported`. Discovered by `rg`-grep across the
plugin sources — locked at plan time, regenerated automatically by a
unit test that fails if any plugin queries a feature not in the set.

### 3.3 Crate layout decision

Two options:

- **(a)** New crate `crates/cssnano-browserslist-snapshot/`, mirrors
  the autoprefixer split. Cleanest; keeps cssnano-specific snapshot
  logic out of `browserslist-shim` (which is supposed to be a faithful
  port of `browserslist@4.24.2`).
- **(b)** New module `crates/browserslist-shim/src/precomputed.rs`,
  reuses existing crate. Smaller diff but conflates the
  upstream-faithful shim with new optimisation logic.

**Decision: (a).** Drift hygiene — `browserslist-shim` stays a
faithful port of the JS package; the precompute snapshot is a new
abstraction we're adding for our deployment, not present upstream.
Same reasoning autoprefixer used (its precomputed module is a
sibling, not a fork of upstream-faithful code).

### 3.4 Consumer integration — leaf plugin Opts

Every cssnano leaf plugin (`postcss-{colormin,convert-values,minify-
params,normalize-unicode,reduce-initial}`) gets one new field on its
`Opts` struct:

```rust
pub struct PostcssReduceInitialOpts {
    pub ignore: Vec<String>,
    pub env: Option<String>,
    /// Pre-evaluated browserslist + feature-support snapshot.
    /// When `Some`, replaces the per-call
    /// `caniuse_api::is_supported(feature, "")` call with an
    /// `IndexMap` lookup. When `None`, falls back to the per-call
    /// path — preserves today's NAPI behaviour exactly.
    pub browserslist_snapshot: Option<Arc<PrecomputedBrowserslist>>,
}
```

`Arc<>` so the snapshot can be cheaply shared across the 5 plugin
invocations within a single transform without cloning the
`IndexMap`. Decoded once in `transform_css`, threaded down.

### 3.5 NAPI surface

`crates/compiled-css-napi/src/lib.rs` exposes:

```rust
#[napi]
pub fn precompute_browserslist_default(
    path: Option<String>,  // mirrors browserslist's `path:` opt
) -> Result<Buffer> { ... }
```

Consumed JS-side via `require('@compiled/css-native')
.precomputeBrowserslistDefault({ path: require.resolve(...) })`,
returning a postcard `Buffer` ready to ship to the WASM plugin.

---

## 4. The AFM bootstrap — final form

`jira/dev-tooling/packages/variants/default/babel.js`:

```js
const native = require('@compiled/css-native');

// Resolve from the leaf plugin's installation directory — provably
// byte-equivalent-by-construction to the JS pipeline's existing
// __dirname walk (which lands on jira/.browserslistrc via Yarn-classic
// hoisting). DO NOT replace with `__dirname` of this file or
// `process.cwd()` — both happen to give the right answer today on
// AFM's layout but not by construction; this anchor is the only one
// that's provably equivalent regardless of future tree reorgs or
// dev-tooling/ .browserslistrc additions.
const precomputedBrowserslist = native.precomputeBrowserslistDefault({
    path: require.resolve('postcss-reduce-initial/package.json'),
});

config.add(config.PLUGIN, '@compiled/babel-plugin', {
    parserBabelPlugins: ['typescript', 'jsx'],
    resolver: '@jira-dev/compiled-resolver',
    precomputedBrowserslist,
});
```

---

## 5. Per-crate change list

Files in execution order. Each row = one logical commit. Diffs are
sketched, not exact.

### Phase A — additive scaffolding (no behaviour change)

| # | File | Change |
|---|---|---|
| A1 | `crates/cssnano-browserslist-snapshot/Cargo.toml` (new) | New crate. Deps: `caniuse-api`, `browserslist-shim`, `indexmap`, `serde`, `postcard`, `once_cell`. |
| A2 | `crates/cssnano-browserslist-snapshot/src/lib.rs` (new) | `PrecomputedBrowserslist`, `precompute_browserslist[_default]`, `encode_precomputed`, `decode_precomputed`. ~150 LOC. |
| A3 | `crates/cssnano-browserslist-snapshot/src/feature_set.rs` (new) | `pub const CANIUSE_FEATURES: &[&str] = &["css-initial-value", ...];` — the locked feature set. ~20 LOC. |
| A4 | `crates/cssnano-postcss-reduce-initial/src/lib.rs` | Add `browserslist_snapshot: Option<Arc<PrecomputedBrowserslist>>` to `Opts`. No consumer wiring yet. |
| A5 | Same for `colormin`, `convert-values`, `minify-params`, `normalize-unicode`. |
| A6 | `crates/cssnano-preset-default/src/lib.rs` | Add `browserslist_snapshot: Option<Arc<...>>` to `PresetOpts`. Thread to each `apply_postcss_*` (NOT yet to leaf plugin Opts — that's Phase C). |
| A7 | `crates/css/src/transform.rs` | Add `precomputed_browserslist: Option<Vec<u8>>` and `precomputed_browserslist_path: Option<PathBuf>` to `TransformOpts`. **Mirror autoprefixer's existing surface exactly** (lines 222–255). `#[serde(skip)]` — Rust-internal control knob, not part of JS-side `TransformOpts`. |
| A8 | `crates/babel-plugin/src/types.rs` | Add `precomputed_browserslist: Option<Vec<u8>>` to `PluginOptions`. Wire shape: `Buffer` round-tripped as base64 string OR `Vec<u8>` via serde (verify SWC plugin-config JSON shape supports byte arrays — TBD in §6). |

### Phase B — leaf-plugin consumer wiring

| # | File | Change |
|---|---|---|
| B1 | `crates/cssnano-postcss-reduce-initial/src/lib.rs` | Replace line 81 `let initial_support = caniuse_api::is_supported("css-initial-value", "");` with: `let initial_support = match &opts.browserslist_snapshot { Some(s) => s.feature_support.get("css-initial-value").copied().unwrap_or(false), None => caniuse_api::is_supported("css-initial-value", ""), };`. |
| B2–B5 | Same shape for each of the other 4 cssnano plugins. |

### Phase C — producer threading

| # | File | Change |
|---|---|---|
| C1 | `crates/cssnano-preset-default/src/lib.rs` | Each `apply_postcss_*` for a browserslist-aware plugin reads `PresetOpts.browserslist_snapshot` and passes it into the leaf plugin's Opts via `Arc::clone`. |
| C2 | `crates/css/src/transform.rs` | At entry: if `precomputed_browserslist.is_some()` (or path is Some), decode → `Arc::new` → thread into `PresetOpts.browserslist_snapshot`. **Path semantics mirror autoprefixer's** (§3 of its docstring): inline bytes > path > slow build (→ `None`-snapshot path → leaf plugins fall through to current behaviour). |
| C3 | `crates/babel-plugin/src/utils/build_styled_component.rs` (line 899) | Thread `opts.precomputed_browserslist` from `PluginOptions` into `TransformOpts.precomputed_browserslist`. Currently passes `None`; preserve `None` when plugin opt absent. |
| C4 | `crates/babel-plugin/src/utils/transform_css_items.rs` (line 71) | Same. |
| C5 | `crates/babel-plugin/src/lib.rs` (or wherever `Config` is parsed) | Read `precomputed_browserslist` from `PluginOptions`, hand to the call sites in C3/C4. |
| C6 | `crates/compiled-css-napi/src/lib.rs` | Add `#[napi] pub fn precompute_browserslist_default(...) -> Result<Buffer>` exposing the new precompute function to NAPI consumers. |
| C7 | `packages/css-native/index.d.ts` | TypeScript declaration for `precomputeBrowserslistDefault({ path?: string }): Buffer`. |

### Phase D — parity-harness alignment

| # | File | Change |
|---|---|---|
| D1 | `parity-harness/babel-plugin/engines.ts` | At module top: compute `precomputedBrowserslist` once via `native.precomputeBrowserslistDefault({ path: AFM_FIXTURE_BROWSERSLISTRC_PATH })`. Pass to `swcEngine` plugin opts. (Babel side: the plugin runs natively in Node, current cwd-walk works — no change needed unless tests fail.) Document anchor choice in code comment. |
| D2 | `parity-harness/transform-css/oracle.mjs` | If this oracle is also exercising the browserslist-aware plugin path (verify), apply the same precompute mirror so its parity stays green. |

### Phase E — tests (write FIRST, then run as gate)

Per C3 (test-first ordering), write these BEFORE Phase B consumer
wiring so we discover any mismatch in the precompute round-trip
before encoding the wrong assumption in 5 plugin changes:

| # | File | Change |
|---|---|---|
| E1 | `crates/cssnano-browserslist-snapshot/src/lib.rs` (`#[cfg(test)] mod tests`) | `precompute_then_decode_roundtrip_byte_identical` — encode + decode preserves all fields. |
| E2 | Same | `feature_support_matches_live_for_canonical_afm_list` — every feature in `CANIUSE_FEATURES` produces the same `bool` as `caniuse_api::is_supported(feature, joined_canonical_list)`. |
| E3 | Same | `legacy_version_byte_rejected` (mirror autoprefixer's V2 test). |
| E4 | `crates/cssnano-postcss-reduce-initial/tests/snapshot_parity.rs` (new) | `with_modern_snapshot_keeps_initial` — plugin with `browserslist_snapshot = Some(modern_canonical)` produces `initial` for `text-decoration-color: initial`. `with_wide_snapshot_substitutes` — plugin with a snapshot whose `feature_support["css-initial-value"] = false` substitutes to `currentColor`. `without_snapshot_falls_back_to_live` — plugin with `None` produces same bytes as today's behaviour (regression gate). |
| E5 | Equivalent for each of the 4 other cssnano leaf plugins. |
| E6 | `crates/babel-plugin/tests/transform_css_browserslist_snapshot_integration.rs` (new) | End-to-end: feed `transform_css` AFM canonical input, with `precomputed_browserslist: Some(canonical_snapshot_bytes)`, assert byte-equal to the existing `BROWSERSLIST_CONFIG=afm_fixture` env-pinned `transform_css_integration` test output. **This is THE gate** that proves the new path agrees with the verified env-pinned path. |
| E7 | `crates/cssnano-browserslist-snapshot/src/lib.rs` (test) | `feature_set_covers_all_plugin_queries` — `rg`-grep across the 5 plugin sources for `is_supported("...")` literal calls, assert every feature name found is in `CANIUSE_FEATURES`. Catches the case where a future plugin port adds a query we forgot to precompute. |

### Phase F — docs

| # | File | Change |
|---|---|---|
| F1 | `crates/compiled-css/src/plugins/normalize_css.rs` (around line 34) | Replace stale env-var-driven-parity comment with the actual fix shape. Cite this plan doc. |
| F2 | `plugins/STATUS.md` | New "Phase X — browserslist plumbing" section documenting the bug shape, the fix, the empirical AFM-PROBE evidence, and the latent JS-side fragility we hardened over. |
| F3 | `plugins/FIXTURES_STATUS.md` | If any local-harness fixtures were diverging on browserslist-related properties, move from "Open" to "Closed" with the standard template. |
| F4 | `plugins/BUG_REPORT.md` | Append "Resolved YYYY-MM-DD" footer with link to this plan + the AFM 1000-file re-run result. |
| F5 | `plugins/BROWSERLIST_PLAN.md` (the typo'd predecessor file) | **Delete.** |

---

## 6. Open questions to resolve during execution

These don't block starting; they get answered during Phase A:

- **Q-6.1 — SWC plugin-config JSON byte-array shape.** Does
  SWC's `experimental.plugins[i][1]` JSON config support a
  `Vec<u8>` field directly, or do we need to base64-encode? Check by
  reading `@swc/core`'s plugin-config validation. If base64, use a
  small custom serde (de)serializer; if raw bytes, use `Vec<u8>`
  directly. Either way the consumer of the deserialized field works
  the same.
- **Q-6.2 — `Arc<PrecomputedBrowserslist>` vs `&PrecomputedBrowserslist`
  threading.** `Arc` if the snapshot needs to be shared across
  multiple plugin invocations; `&` if all uses are within one stack
  frame. Decide after reading the `apply_postcss_*` call sites in
  Phase A6.
- **Q-6.3 — Crate name.** Going with
  `crates/cssnano-browserslist-snapshot/` per §3.3. If a shorter name
  is preferable on review, candidates: `crates/cssnano-bl-snapshot/`,
  `crates/browserslist-snapshot/`. Lock at PR time.

---

## 7. Verification ladder

In order. Each must pass before the next runs. Failure at any rung
returns to the corresponding phase above.

| # | Command | Gates |
|---|---|---|
| V1 | `cargo build --workspace --release` | Phase A compiles. |
| V2 | `cargo test -p cssnano-browserslist-snapshot --release` | E1, E2, E3, E7 pass. **Run BEFORE Phase B wiring.** |
| V3 | `cargo test -p cssnano-postcss-reduce-initial --release` (and the other 4) | E4, E5 pass. |
| V4 | `cargo test -p browserslist-shim --release` | No regression in shim itself. |
| V5 | `cargo test -p babel-plugin --release` | E6 passes. `transform_css_integration` (existing) stays green. |
| V6 | `cargo test --workspace --release` | Full workspace, no regressions. |
| V7 | `( cd crates && cargo build -p babel-plugin --target wasm32-wasip1 --release )` | WASM build clean. |
| V8 | `bun parity-harness/fixtures-triage.mjs` | Local fixtures parity ≥ pre-fix baseline. |
| V9 | `bun parity-harness/babel-plugin/triage.mjs` | Phase 6 §6.5 unit-test corpus stays at 476/477. |
| V10 | Ship WASM artefact to AFM. They re-run 1000-file gate with the new bootstrap. | Target: 40 → ≤ 5 divergences. ≤ 5 acceptable because some divergences may have unrelated root causes. |

---

## 8. Rollback strategy

If V10 regresses (parity worse than pre-fix) or doesn't reach target:

1. **Bisect.** Run the 1000-file gate with `precomputedBrowserslist`
   omitted from the AFM bootstrap. Plugin falls back to the
   `None` path — should equal pre-fix behaviour exactly. If it
   doesn't, the bug is in Phase B wiring, not the snapshot itself.
2. **Spot-check the canonical list.** Have AFM dump the
   `precomputedBrowserslist.selected` array post-decode inside the
   plugin (one-line `eprintln!`, gated behind a debug feature).
   Verify it byte-equals the AFM-PROBE-confirmed 14-entry list.
   If it doesn't, the bootstrap's `path:` arg is resolving to the
   wrong `.browserslistrc`.
3. **Spot-check feature_support.** Verify
   `feature_support["css-initial-value"] = true` etc. for the
   modern list. If false, the precompute function has a bug —
   probably in the canonical-list-to-query-string join or the
   pre-evaluation loop.
4. **Worst case — revert the AFM bootstrap.** Plugin sees `None`
   snapshot → falls back to current behaviour → divergence
   restored to 40. We're back to where we started, no permanent
   damage. Plan for a v2 fix.

---

## 9. Out of scope

Intentionally NOT in this plan, to keep scope tight:

- **Threading browserslist into other Rust crates** that may consume
  it (e.g. any future PostCSS plugin port we add). They follow the
  same pattern; precedent established.
- **Dynamic browserslist resolution from SWC config root.** Could be a
  future enhancement (e.g. SWC plugin reads a path the host preopens
  to `/cwd/.browserslistrc-snapshot`); deferred until a consumer asks.
- **Migrating `BROWSERSLIST_CONFIG` env-var support to the WASM
  path.** Per §C-4 not a goal; explicitly unsupported.
- **Fixing the JS-side fragility in upstream cssnano.** That's
  upstream's problem; we just stop depending on it.
- **Performance benchmarking the cssnano plugins.** The autoprefixer
  precompute saved ~345 µs/call. Cssnano's per-call cost is smaller
  (handful of `is_supported` queries vs full PREFIXES iteration). The
  win here is byte-correctness; perf is a side-effect.

---

## 10. Empirical reference — AFM-PROBE traces (verbatim)

Preserved for future maintainers auditing this fix:

```
AFM-PROBE reduce-initial {
  "cwd": "/home/ubuntu/atlassian-frontend-monorepo/jira",
  "__dirname": "/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/postcss-reduce-initial/src",
  "BROWSERSLIST_CONFIG": "/home/ubuntu/atlassian-frontend-monorepo/jira/.browserslistrc",
  "BROWSERSLIST": undefined,
  "browsers": ["and_chr 144","chrome 144","chrome 143","chrome 142","chrome 141","chrome 140",
               "edge 144","edge 143","firefox 147","firefox 146",
               "ios_saf 26.2","ios_saf 26.1","safari 26.2","safari 26.1"],
  "initialSupport": true
}

AFM-PROBE colormin { (same browsers, same cwd, options: {transparent: true, alphaHex: true, name: true}) }
```

WASI behaviour (verified empirically across 4 env-var permutations):
all produce identical SWC output → env vars don't cross WASI boundary
→ snapshot is the only correct delivery mechanism.

---

## 11. Sign-off

When V10 hits target, append below:

- [ ] Phase A complete (date, commit hash)
- [ ] Phase B complete
- [ ] Phase C complete
- [ ] Phase D complete
- [ ] Phase E tests added & green
- [ ] Phase F docs updated
- [ ] V1–V9 green (workspace + harness + cargo)
- [ ] V10 green (AFM 1000-file → ≤ 5 divergences, with property-level
      analysis of any remaining divergences)
- [ ] `plugins/BROWSERLIST_PLAN.md` deleted
- [ ] `plugins/BUG_REPORT.md` resolution footer added
