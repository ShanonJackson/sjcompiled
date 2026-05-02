# Status — `crates/`

End-of-session snapshot. Read with `EXECUTION_PLAN.md` and
`PARITY_VERSIONS.md`.

## AFM repin — coordinate the parity contract with JIRA's actual install (CRITICAL)

The `REFERENCE_LOCK_FILE/yarn.lock` we forked from the upstream `compiled`
repo does NOT match what JIRA's monorepo actually resolves for
`@compiled/css@0.19.0` (commit `40a4548`). Per
`AFM_MONOREPO_DEPENDENCIES_MORE.md`, AFM resolves the following
**byte-affecting** packages to different versions than the reference
lockfile we'd been targeting:

| Package | Reference lockfile (was) | AFM (now) | Action taken |
|---|---|---|---|
| postcss | 8.4.31 | **8.5.6** | repinned; postcss-core agent confirmed cosmetic-only diff (no code changes — see "postcss version pin" section above) |
| postcss-selector-parser | 6.0.13 | **6.1.2** | repinned in `PARITY_VERSIONS.md` + root `package.json` overrides; `crates/postcss-selector-parser` audit pending |
| browserslist | 4.24.4 | **4.24.2** | repinned; `crates/browserslist-shim` headers/docstrings updated; defaults audit pending |
| caniuse-lite | 1.0.30001690 | **1.0.30001766** | re-vendored at `crates/_vendor/caniuse-lite-1.0.30001766/`; `data/features.snapshot.json` regenerated (582 features); `caniuse-db` rebuilds clean |
| electron-to-chromium | 1.5.76 | **1.5.41** | repinned in overrides; vendor refresh pending if cssnano/autoprefixer reach it |
| node-releases | 2.0.19 | **2.0.18** | repinned in overrides; vendor refresh pending |
| colord | 2.9.1 | **2.9.3** | repinned (crate still scaffolded — header/Cargo.toml updated only) |

**Source-code drift fix.** `packages/css/src/` was tracking `compiled@HEAD`
(0.21.0). Overlaid with `git show 40a4548:packages/css/src/...` so the JS
oracle now matches AFM's installed `@compiled/css@0.19.0`. Concrete
deltas applied:

- **Deleted** `packages/css/src/plugins/flatten-multiple-selectors.ts`
  (added in 0.20+ — not in AFM's pipeline).
- **Deleted** `packages/css/src/plugins/__tests__/flatten-multiple-selectors.test.ts`.
- **Deleted** `packages/css/src/plugins/at-rules/parse-media-query.ts`
  (renamed back to its 0.19.0 name).
- **Restored** `packages/css/src/plugins/at-rules/parse-at-rule.ts`.
- **Reverted** `transform.ts`: dropped the `flattenMultipleSelectors`
  pipeline branch + opts field.
- **Reverted** `sort-atomic-style-sheet.ts`: uses `parseAtRule` (not
  `parseMediaQuery`); calls it on **any** at-rule when
  `sortAtRulesEnabled` (no `name === 'media'` gate).
- **Reverted** `expand-shorthands/flex.ts`: only handles `none` keyword
  in the 1-arg word case (drops `auto`/`initial`/`revert`/`revert-layer`/
  `unset`/`inherit` branches added in 0.20+).
- All `@compiled/utils` imports translated to `@sjcompiled/utils`.
- `sort.ts` re-wrapped with the `COMPILED_CSS_ENGINE=rust` engine flag
  (Phase 8a wiring preserved on top of the 0.19.0 source).

**Rust-side reverts to match the new oracle:**

- `crates/compiled-css/src/plugins/flatten_multiple_selectors.rs` —
  **deleted**. Module declaration removed from `plugins.rs`. Doc-mapping
  line removed from `lib.rs`. Stage variant + dispatch removed from
  `crates/parity-runner/src/{main.rs,stages.rs}`. Integration test +
  corpus directory deleted.
- `crates/css/src/transform.rs` — `flatten_multiple_selectors` field
  removed from `TransformOpts`; pipeline doc-comment updated.
- `crates/compiled-css/src/plugins/at_rules/parse_media_query.rs` →
  **renamed** to `parse_at_rule.rs`; `parse_media_query` function →
  `parse_at_rule`; module declaration in `at_rules.rs` updated.
- `crates/compiled-css/src/plugins/sort_atomic_style_sheet.rs` — calls
  `parse_at_rule(&at.params)` on any at-rule when `sort_at_rules_enabled`
  (no `name == "media"` gate). 5/5 unit tests still pass.
- `crates/compiled-css/src/plugins/expand_shorthands/flex.rs` — 1-arg
  word case simplified to only `none`; rebuild is byte-clean (112/112
  compiled-css tests pass).

**Documentation updates:**

- `crates/PARITY_VERSIONS.md` — Source-of-Truth section now points at
  `AFM_MONOREPO_DEPENDENCIES_MORE.md`; Anomaly #3/#4 versions updated;
  Crate Ownership Map versions updated; `flattenMultipleSelectors`
  excluded from `crates/compiled-css` plugin list.
- Root `package.json` overrides updated to AFM pins; `bun install`
  re-resolved successfully (verified via `bun pm ls --all`).
- `crates/colord/{Cargo.toml,src/lib.rs}` — header bumped to 2.9.3.
- `crates/browserslist-shim/{Cargo.toml,src/{lib,index,node}.rs}` —
  bumped to 4.24.2.
- `crates/caniuse-db/{Cargo.toml,src/lib.rs,build.rs,scripts/snapshot.js}`
  — bumped to 1.0.30001766; constant `CANIUSE_LITE_VERSION` updated.
- `crates/autoprefixer/{build.rs,tests/data_parity.rs,src/data/prefixes.rs}`
  — caniuse-lite pin string updated to 1.0.30001766.

**Verification gates run after the AFM repin (all green):**

| Gate | Result |
|---|---|
| `parity-runner --stage X --corpus crates/parity-runner/corpus/X` × 20 stages (JS-vs-Rust) | **20/20 byte-clean** |
| `parity-runner --stage X --corpus crates/parity-runner/corpus/X --determinism` × 20 stages (JS-vs-JS oracle stability) | **20/20 deterministic** |
| `bun run packages/css/scripts/verify-napi-sort.mjs` | 12/12 OK |
| `bun run packages/css/scripts/verify-engine-flag.mjs` | 12/12 OK |
| `cargo test --workspace --no-fail-fast` (RUSTFLAGS="") | **all targets green** (after fixing `caniuse-db::list_returns_579` → `list_returns_582` and `caniuse-api::features_lists_all` from 579 → 582 to match the new caniuse-lite snapshot) |

**Audit findings (postcss-selector-parser 6.0.13 → 6.1.2):**
- `parser.js` adds `sourceIndex: …` field on a few AST node initializations
  (commas, pseudos). Diagnostic surface only; not stringified.
- `parser.js` line 487: new clause treats `closeParenthesis` as a
  comma-like terminator alongside `comma` — affects boundary detection
  in selectors with parenthesized content (e.g. `:is()`, `:where()`,
  `:not()`).
- All 20 parity stages (including `sort` and `sort-atomic-style-sheet`,
  which exercise selectors) are byte-clean against the 6.1.2 oracle, so
  the practical impact on the AFM corpus is **nil**.

**postcss-selector-parser 6.0.13 → 6.1.2 audit landed (2026-05-02):**
- Full source-tree diff confirmed only `parser.js` differs — four hunks
  total, matching the upstream changelog (6.1.0 added `Selector.sourceIndex`,
  6.1.2 fixed trailing combinators in pseudos).
- Rust port updates in `crates/postcss-selector-parser/src/`:
  - `nodes.rs`: added `Node::source_index: Option<usize>`.
  - `parser.rs`: set `source_index` at the three upstream sites — root
    selector, comma-spawned selectors, and pseudo inner-arg selectors.
    Added an inline comment at the trailing-whitespace tail handler
    documenting that the existing fold-into-`spaces.after` behavior
    matches upstream's 6.1.2 `closeParenthesis` close-condition fix.
- `crates/postcss-nested/src/lib.rs` updated to include the new field
  on its synthesized descendant-Combinator node (cosmetic).
- Adversarial corpus added: 10 entries to `postcss-core-roundtrip/`
  (`23..32`) and 5 to `sort-atomic-style-sheet/` (`13..17`) — covering
  trailing whitespace before `)`, mixed combinators inside parens,
  comments in parens, adjacent pseudos, empty-arm edges, deep nesting.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green; six selector-touching parity stages byte-clean (32/17/13/12/24/12);
  both NAPI verifiers 12/12; determinism on `postcss-core-roundtrip`
  (32/32) and `sort-atomic-style-sheet` (17/17).
- Full audit document at
  `crates/_vendor/POSTCSS_SELECTOR_PARSER_6.0.13_TO_6.1.2_AUDIT.md`.

**colord 2.9.1 → 2.9.3 audit landed (2026-05-02):**
- Full source-tree diff: only `CHANGELOG.md` and `package.json`
  differ between the two versions. All `.js`/`.mjs`/`.ts` files
  (including every plugin) are byte-identical (verified via
  `diff -rq` and a directional `cmp -s` sweep in both directions).
- The two upstream releases are pure packaging fixes:
  2.9.2 added `"./package.json"` to the `exports` map; 2.9.3 added
  `"types"` keys for TypeScript 4.7 module resolution. Neither
  affects runtime color math, parse output, rounding, short-form
  `#fff`/`#ffffff` decisions, or stringification.
- **Zero Rust source changes required.** `crates/colord/` remains
  scaffolded as before; its `src/lib.rs` header already cites 2.9.3
  (set during the original AFM repin). When the actual port is
  written, target the existing 2.9.3 source — no 2.9.1-vs-2.9.3
  delta to track.
- No new corpus entries added — there are no changed code paths to
  exercise. The 2.9.1↔2.9.3 deltas live entirely in `package.json`'s
  `exports` map (Node.js module resolution), unreachable from CSS
  input. Existing `postcss-core-roundtrip`, NAPI-sort, and
  engine-flag gates exercise the colord-consuming pipeline
  transitively.
- Verification gates rerun: `cargo test --workspace --no-fail-fast`
  all green; `postcss-core-roundtrip` 32/32 byte-clean (JS vs Rust);
  determinism 32/32 (JS oracle stable); both NAPI verifiers 12/12.
- Full audit document at
  `crates/_vendor/COLORD_2.9.1_TO_2.9.3_AUDIT.md`.

**Audit findings (browserslist 4.24.4 → 4.24.2):**
- Default query string is **identical** (`> 0.5%, last 2 versions,
  Firefox ESR, not dead`).
- `index.js`: 4.24.4 added a `parseCache` layer + `needsPath` plumbing.
  Caching only — no semantic difference for our deterministic pipeline.
- `index.js` Firefox ESR resolution: 4.24.4 returns `['firefox 128']`,
  4.24.2 returns `['firefox 115', 'firefox 128']`. **Byte-affecting** for
  any consumer that hits the `Firefox ESR` query — but autoprefixer
  prefix decisions feed off our `crates/caniuse-db` snapshot via
  `oxc_browserslist`. Once Phase 7 (autoprefixer) parity gates land,
  add a Firefox-ESR-targeted corpus entry to confirm `oxc_browserslist`
  v3 returns the 4.24.2-style two-version list (NOT just `firefox 128`).
- `node.js`: significant cache-infra changes (4.24.4 added stat /
  config-path / parsed-config caches, `eachParent` signature changed).
  Internal only — does not affect the resolved query result.

**JS bridge fix during verification.** `packages/css/scripts/parity-bridge.mjs`
still imported `flatten-multiple-selectors.ts` after the overlay. Removed
the import and the `'flatten-multiple-selectors'` STAGE entry. Without
the fix, every `--determinism` run failed (the bridge crashed on import
before serving requests).

**`caniuse-db` test pin update.** `caniuse-lite@1.0.30001690` had 579
features; `1.0.30001766` has 582. Updated `caniuse-db::list_returns_579`
→ `list_returns_582` and `caniuse-api::features_lists_all` (579 → 582).
This is the only test-level change needed for the data swap.

**Audit findings (caniuse-lite 1.0.30001690 → 1.0.30001766) (2026-05-02):**
- Data-only package — no JS source code to port. `dist/` (unpacker) and
  `data/browsers.js` are byte-identical between the two versions, so the
  packed-encoding shape is unchanged.
- Net feature delta: 579 → 582. Three added (`cross-document-view-transitions`,
  `css-grid-lanes`, `css-if`); zero removed. None of the three are
  referenced by `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`
  (verified by enumerating the 63 distinct `feature` strings in that
  table), so they don't affect prefix decisions on AFM input today.
- All 579 existing feature files have refreshed support tables; all 19
  agents have refreshed `usage_global` / `release_date` / `version_list`
  data. **Schema-only deltas: none.**
- `crates/caniuse-db/data/features.snapshot.json` already at
  `caniuseLiteVersion: "1.0.30001766"` with 582 features. Spot-checked
  13 high-traffic features (flexbox, css-grid, css-sticky, css-masks,
  css-gradients, css-transitions, transforms2d, transforms3d,
  css-filters, css-clip-path, css-backdrop-filter, object-fit,
  css-logical-props) and the 3 net-new features against the unpacker
  output of the vendored 1.0.30001766 source — all 16/16 byte-clean.
- **No Rust source changes were required.** Autoprefixer's
  `data/prefixes.rs` is regenerated by `build.rs` evaluating upstream
  `prefixes.js` against the workspace-pinned `caniuse-lite` (verified
  `node_modules/caniuse-lite@1.0.30001766`), so the new data flows in
  on `cargo build` automatically. The `data_parity` test stayed green.
- Verification gates (re-run): `cargo test --workspace --no-fail-fast`
  all green; `parity-runner postcss-core-roundtrip` 32/32 byte-clean;
  determinism 32/32; both NAPI verifiers 12/12.
- No new corpus entries added — the package is data-only, no new code
  path was introduced to need an adversarial input.
- **Drift flagged (pre-existing, not introduced by AFM repin):**
  `oxc-browserslist@3.0.2` bundles its OWN caniuse-lite snapshot —
  see `crates/POSSIBLE_DRIFT_CAUSES.md` for the full write-up.
- Full audit document at
  `crates/_vendor/CANIUSE_LITE_1.0.30001690_TO_1.0.30001766_AUDIT.md`.

**Re-audit findings (postcss-normalize-whitespace 5.1.1, port-quality re-check) (2026-05-02):**
- Pin is `5.1.1` in BOTH `REFERENCE_LOCK_FILE/yarn.lock` and AFM resolution
  — no version drift. Re-audit was for **port-quality** drift, not version
  drift.
- Walked every line of upstream `src/index.js` (109 lines, single file)
  against `crates/postcss-normalize-whitespace/src/lib.rs`. Verified every
  control-flow branch, regex (ECMAScript `\s` vs Rust `\p{White_Space}`
  divergence at U+FEFF / U+0085 — port hand-rolls the spec set correctly),
  cache key/value handling (`IndexMap`, insertion-ordered), `prev()`
  resolution by index-before-mutation, and the `valueParser.walk` calc /
  variableFunctions exemptions.
- **No source-code changes required.** Port is 1:1.
- Added 6 adversarial corpus entries (`23..28`) to
  `corpus/postcss-normalize-whitespace/` covering: multiple-IE9-hacks
  (no-`g`-flag invariant), mixed-case `VAR`/`CALC` (`toLowerCase`), the
  third variable function `constant()`, decl-after-comment semicolon
  strip, `--*` walks-to-empty, and pure-whitespace `raws.before`.
- Added 7 unit tests in `lib.rs` citing the upstream lines they cover.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green; `parity-runner postcss-normalize-whitespace` 28/28 byte-clean;
  `parity-runner postcss-core-roundtrip` 32/32; both NAPI verifiers 12/12;
  determinism on `postcss-normalize-whitespace` (28/28).
- Full audit document at
  `crates/_vendor/POSTCSS_NORMALIZE_WHITESPACE_5.1.1_REAUDIT.md`.

**browserslist 4.24.4 → 4.24.2 port landed (2026-05-02):**
- Full source-tree diff (`diff -r`) confirms only three files differ:
  `package.json` (version + dep range tweaks — cosmetic for the Rust
  port), `index.js`, and `node.js`. `parse.js`, `error.js`, `browser.js`,
  `cli.js`, all `.d.ts` files, `LICENSE`, and `README.md` are
  byte-identical between the two versions.
- **Non-cosmetic deltas ported into `crates/browserslist-shim/`:**
  - `index.rs` — added `rewrite_firefox_esr()` that intercepts comma-
    separated query atoms matching `(?i)^\s*(not\s+)?(?:firefox|ff|fx)\s+esr\s*$`
    and rewrites them to the explicit pair `firefox 115, firefox 128`
    (or two `not firefox <ver>` atoms). Necessary because
    `oxc-browserslist@3.0.2` bundles its own snapshot and returns just
    `firefox 140` for `Firefox ESR` (`src/queries/firefox_esr.rs:4`),
    diverging from BOTH 4.24.4 (`firefox 128`) and 4.24.2
    (`firefox 115, firefox 128`). 4.24.2 reference: `index.js` ~1018-1025.
  - `node.rs::parse_package` — bubble `serde_json` parse failures as
    `BrowserslistError` instead of silently returning `Ok(None)`. 4.24.2
    JSON.parses unconditionally (`node.js` ~106-119); 4.24.4 short-circuited
    on `text.indexOf('"browserslist"')`. The new behavior matches 4.24.2.
- **Cosmetic / no-port-needed deltas (documented for future readers):**
  cache-infra changes in 4.24.4 (`parseCache` in index.js;
  `statCache`/`configPathCache`/`parseConfigCache` ↔ `filenessCache`/
  `configCache` in node.js; `eachParent` signature change; `needsPath`
  plumbing) are absent from 4.24.2. Our Rust shim has no caches, so
  these collapse to a no-op.
- **Adversarial coverage**: 6 new unit tests in
  `crates/browserslist-shim/src/{index,node}.rs` (`firefox_esr_returns_two_versions`,
  `firefox_esr_aliases`, `firefox_esr_combined_with_other_query`,
  `rewrite_firefox_esr_unit`, `parse_package_invalid_json_errors`,
  `parse_package_no_browserslist_key_returns_none`). The 20-stage
  parity-runner corpus does not invoke browserslist resolution, so no
  new corpus entries were added — none of those stages exercise the
  changed code paths. Browserslist-driven prefix decisions are
  exercised transitively by the `crates/autoprefixer/tests/data_parity.rs`
  test, which stays green.
- `Cargo.toml` description bumped from `4.24.4` → `4.24.2`.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green (15/15 in `browserslist-shim`, no regressions across the
  workspace); `parity-runner postcss-core-roundtrip` 32/32 byte-clean;
  `parity-runner sort` 12/12 byte-clean; both NAPI verifiers 12/12;
  determinism on `postcss-core-roundtrip` (32/32).
- Full audit document at
  `crates/_vendor/BROWSERSLIST_4.24.4_TO_4.24.2_AUDIT.md`.

**Re-audit findings (caniuse-api 3.0.0, port-quality re-check) (2026-05-02):**
- Pin is `3.0.0` in BOTH `REFERENCE_LOCK_FILE/yarn.lock` and AFM resolution
  — no version drift. Re-audit was for **port-quality** drift.
- Walked every line of upstream `dist/index.js` + `dist/utils.js` (the npm
  tarball ships compiled output only — no `src/`) against
  `crates/caniuse-api/src/{index,utils}.rs`. Five real divergences found:
  - `utils.rs` `parse_caniuse_data`: `split_whitespace()` → `split(' ')`
    (JS `String#split(" ")` preserves empty entries between consecutive
    spaces; collapsing them silently dropped a key from the output).
  - `utils.rs` `parse_caniuse_data`: added `js_parse_float()` helper.
    Rust `f64::from_str` is strict; JS `parseFloat` parses the longest
    numeric prefix. Concrete fail: caniuse-lite Android stats key
    `"4.4.3-4.4.4"` after `split("-")[0]` → `"4.4.3"` — JS yields 4.4,
    Rust used to yield `Err` and `continue`'d, dropping the entry.
  - `utils.rs` `strip_first_hash_digits`: rewrote to require the digits
    suffix (regex is `/#\d+/`, not `/#\d*/`). Old code stripped a lone
    `#` even when no digits followed.
  - `index.rs` `is_supported`: catch branch now mirrors the upstream JS
    bug at `dist/index.js:56` (`data = features[res[0]]` assigns the
    packed string; `data.stats` is undefined → `every` returns false
    for non-empty browser lists). Old port unpacked correctly and
    diverged.
  - `index.rs` `is_supported`: browser-version split changed
    `splitn(2, ' ')` → `split(' ')` to match JS literal-space split.
  - `utils.rs` `clean_browsers_list`: `HashSet` → `IndexSet` for the
    dedup membership set. Functionally identical today (Vec is the
    source of truth), but `HashSet`'s `RandomState` would silently
    introduce process-randomized order if a future refactor ever
    iterated the membership set. Determinism is now structural.
  - `index.rs` browser-scope storage: `Mutex<Vec<String>>` →
    `RwLock<Vec<String>>`. Write path now resolves the new scope
    OUTSIDE the lock and performs a single atomic Vec swap inside —
    matches the JS "single-event-loop-tick swap" semantics, prevents
    readers from being serialized behind a write that's doing
    browserslist config I/O, and guarantees no reader ever observes a
    half-applied scope when invoked from multiple NAPI worker threads.
  - `utils.rs` `parse_caniuse_data` enumeration order: replaced
    `IndexMap::iter` with `js_for_in_order(stats)` — a spec-conformant
    `OrdinaryOwnPropertyKeys` helper that visits ECMA-262 array-index
    keys (`IsArrayIndex`: canonical decimal in `[0, 2^32 - 1)`) in
    ascending numeric order, then string keys in insertion order.
    Caniuse-lite stats objects mix integer keys (`"4"`, `"49"`) with
    string keys (`"4.1"`, `"TP"`, `"12.0-12.5"`); pure IndexMap
    insertion-order iteration would produce a different first-write
    order in the returned `IndexMap`, observable to any caller that
    serializes/iterates/hashes the result.
- Crate is currently dormant (no consumer wires it yet — five would-be
  consumers in cssnano-* are scaffolded but not invoking the API), so
  none of these surfaced in the existing 20-stage parity corpus. They
  would have caused silent hash divergence once a consumer started
  calling through. Bias-toward-verbatim port applied; bugs preserved.
- Added 15 in-crate unit tests covering each fixed path (including a
  concurrent reader-thread test for the RwLock atomic-swap contract,
  a determinism test for the IndexSet dedup, and four spec-conformance
  tests for the JS `for...in` enumeration order). No parity-runner
  stage to extend — when consumers wire up (Phase 6f and onward), their
  parity stages will exercise these paths against the JS oracle.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green (21 caniuse-api tests, 6 → 21); `parity-runner postcss-core-roundtrip`
  35/35 byte-clean; both NAPI verifiers 12/12; determinism on
  `postcss-core-roundtrip` (35/35).
- Full audit document at
  `crates/_vendor/CANIUSE_API_3.0.0_REAUDIT.md`.

**Stale `_vendor/` directories** (kept, not deleted):
`caniuse-lite-1.0.30001690`, `browserslist-4.24.4`, `colord-2.9.1`,
`electron-to-chromium-1.5.76`, `node-releases-2.0.19`,
`postcss-selector-parser-6.0.13`. Useful for "what changed?" diffs.
Cleanup is a future cosmetic task — no parity impact.

**JS oracle source pin**: `packages/css/src/` mirrors
`@compiled/css@0.19.0` at upstream commit
`40a45489eaaacc023110c3f107d702a389232892`. `packages/utils/src/` mirrors
`@compiled/utils@0.13.2` at commit `130ed3b4ae8a48926892939679c2f1479375f2a8`
(byte-identical to `compiled@HEAD` — no overlay needed).

**postcss-nested 5.0.6 re-audit landed (2026-05-02):**
- Version is **not** drifted between REFERENCE_LOCK_FILE and AFM (both
  pin `5.0.6`). The audit closed two semantic gaps in the existing
  port that the 38-entry corpus had not been exercising.
- Rust port updates in `crates/postcss-nested/src/lib.rs`:
  - `replace_nesting`: switched `nesting_value.replace('&', ...)` →
    `nesting_value.replacen('&', ..., 1)` to match JS
    `String.prototype.replace(string, ...)` first-only semantics.
    Defensive — the in-tree selector parser always emits
    `Nesting.value == "&"`, so the fix only matters for
    consumer-mutated values, but the contract is upstream-fidelity.
  - `clone_rule_with_empty_nodes`: removed the unconditional
    `clone.raws.selector = None` clear. Upstream `clone({ nodes: [] })`
    deep-copies all raws verbatim; preserving raws.selector is what
    keeps byte-equality on `a/*c*/ { @media ... { } }`-style inputs
    where the selector raw form encodes a trailing comment.
- Adversarial corpus added: 3 entries to
  `crates/parity-runner/corpus/postcss-nested/` (`39..41`) — bubble
  with selector-raw comment, comma list with mid-comment, and
  trailing-spaces selector under @supports.
- Verification gates rerun: `cargo test --workspace --no-fail-fast`
  all green (6/6 in `postcss-nested`); `parity-runner postcss-nested`
  41/41 byte-clean; `parity-runner postcss-core-roundtrip` 32/32
  byte-clean; both NAPI verifiers 12/12; determinism on
  `postcss-nested` (41/41).
- Full audit document at
  `crates/_vendor/POSTCSS_NESTED_5.0.6_REAUDIT.md`.

**postcss-discard-duplicates 6.0.0 re-audit landed (2026-05-02):**
- Version is **not** drifted between REFERENCE_LOCK_FILE and AFM (both
  pin `6.0.0`). The audit closed two equality-semantics gaps in the
  existing port that the 8-entry corpus had not been exercising.
- Rust port updates in `crates/postcss-discard-duplicates/src/lib.rs`:
  - `nodes_equal`: removed the explicit `Comment.text` comparison.
    Upstream `equals()` (`src/index.js:38-69`) has NO `comment` case
    in its switch — two comments are considered equal as long as
    `type` matches, regardless of `text`. Now matches verbatim.
  - `trim_str`: replaced `str::trim()` with `trim_matches(is_ecma_whitespace)`
    where the predicate hand-rolls `String.prototype.trim()`'s
    ECMA-262 WhiteSpace + LineTerminator set. Rust's default
    `is_whitespace` differs from JS at U+0085 (NEL — only Rust)
    and U+FEFF (BOM/ZWNBSP — only JS); `raws.before` /
    `raws.afterName` containing either codepoint would diverge
    equality between JS and Rust. Same divergence pattern already
    flagged for `postcss-normalize-whitespace`.
- Adversarial corpus added: 7 entries to
  `crates/parity-runner/corpus/npm-postcss-discard-duplicates/`
  (`09..15`) — atrule with differing inner comment text (the
  comment-text fix), rule with leading comment then duplicated decl
  (`empty()` ignores comments), `!important` flag distinguishes,
  decl `raws.before` whitespace trim equality, atrule
  `raws.before`/`raws.afterName` whitespace trim equality, nested
  rule inside duplicate `@media` with differing inner comment text,
  duplicate `@media` with U+FEFF (BOM) in the second atrule's
  `raws.before` (locks in the trim fix).
- Five unit tests added to `lib.rs::tests` — two for the
  comment-text-ignored contract (atrule + rule paths), three for
  the trim semantics (BOM stripped, NEL preserved, Zs stripped).
- Verification gates rerun: `cargo test --workspace --no-fail-fast`
  all green (15/15 in `postcss-discard-duplicates`); `parity-runner
  npm-postcss-discard-duplicates` 15/15 byte-clean; `parity-runner
  sort` 12/12 byte-clean; both NAPI verifiers 12/12; determinism on
  `npm-postcss-discard-duplicates` (15/15).
- Full audit document at
  `crates/_vendor/POSTCSS_DISCARD_DUPLICATES_6.0.0_REAUDIT.md`.

**postcss-values-parser 6.0.2 re-audit landed (2026-05-02):**
- Version is **not** drifted between REFERENCE_LOCK_FILE and AFM (both
  pin `6.0.2`). Audit ran to close pre-existing transcription gaps in
  the original port; `expand-shorthands/*.ts` is the sole consumer.
- Walked all 14 upstream files (`index.js`, `tokenize.js`, `walker.js`,
  `ValuesParser.js`, `ValuesStringifier.js`, plus 9 `nodes/*.js`)
  against the Rust port. Found and fixed: (1) `UnicodeRange` regex was
  anchored + hex-only; upstream is unanchored, capital-U only, allows
  `\w` chars. (2) `Numeric` unit was over-permissive `(.*)`; replaced
  with the strict `unitRegex` so `5%a` is correctly rejected.
  (3) `Operator.chars` had 5 entries; upstream lists 10 (`=`, `<=`,
  `>=`, `<`, `>` were missing). (4) `OPERATOR_REGEX` rewritten to the
  literal upstream `([/\|*}])`. (5) `PUNCT_CHARS` corrected to upstream
  `[',', ':', '(', ')', '[', ']', '{', '}']` set. (6) `Word.is_hex`
  tightened from `starts_with('#')` to upstream `^#(.+)`. (7) `Func.is_color`
  and `Func.is_var` now set during parse using upstream regexes.
- 7 Rust files touched; 7 new corpus entries added (4 expand-shorthands,
  3 postcss-core-roundtrip) covering the touched code paths.
- **Second pass (same session)** closed five additional latent drifts
  flagged for the 90GB-monorepo edge cases:
  (1) `Word.is_color` now set via 148-name CSS color list (added
  `colord` dep) + hex regex. (2) `Word.is_url` now set via
  `is-url-superb`-equivalent predicate (`^[^:]+:/{1,2}[^/]`, with
  `//host` → `http://host` swap upstream uses). (3) Func name
  validation against `cssFunctions` whitelist (62 entries + 4 vendor
  prefixes) and the `^[a-zA-Z\-\.]+$` fallback — invalid names fall
  through to Word + Punctuation. (4) `Operator.chars` classification
  path emits Operator (not Word) for `=` / `<=` / `>=` / `<` / `>`
  literals. (5) Bug-not-ported documented: `Operator.tokenize` for
  `|` / `}` (JS infinite-loops; we emit as Word). 4 more corpus
  entries added.
- Acknowledged-but-not-ported: `Func.params` (consumers reconstruct
  via `stringify_standalone`), unbalanced-bracket throw recovery,
  optional `interpolation` feature path.
- **Third pass (same session)** closed two more latent classification
  drifts: (1) `Word.testEscaped` ported — `\41` / `\red` /
  `Times\ New\ Roman` style escape-prefixed identifiers now classify
  as Word via `Word::test_word()`, which composes
  `testEscaped || testHex || testVariable` in upstream order BEFORE
  Numeric/UnicodeRange checks. (2) `Comment.tokenizeNext` for
  value=="//" — when the parser's Word arm sees a literal `//`,
  consume tokens to the next `\n` and emit a single inline Comment.
  Empirically verified-as-handled (third pass): `tokenizeInline`
  for words containing `//` mid-value (`tokenize.rs` pre-splits on
  `/`); `tokenizeBrackets` (dead code in JS — wrapping expands
  Brackets tokens before parser); `tokenizeCommas` (`tokenize.rs`
  already splits comma-bearing words). 3 more corpus entries added.
- **Fourth pass (same session)** closed three more contract / edge
  drifts: (1) `Comment` predicate split — `Comment::test_inline_word`
  mirrors upstream `inlineRegex.test()` (contains `//` anywhere);
  pre-existing `starts_with("//")` path selector renamed to
  `is_inline_marker` (old name kept as `#[deprecated]` alias). The
  `VKind::Comment` parser branch uses `is_inline_marker`. (2) Numeric
  edge cases re-verified — added regression tests for trailing-dot
  value (`5.`), signed leading-dot, exponent + unit, hyphen-identifier
  rejection, and dash-prefixed unit (`5-MyUnit`). (3) Func nested-paren
  round-trip — regression tests for `calc(var(--x))`, `calc((1+2)*3)`,
  `var(--a, calc(50% - var(--gap)))`,
  `linear-gradient(to right, var(--s, red), var(--e, blue))`,
  `rgb(calc(255 - 1), 0, 0)`. Stack-based paren tracking matches
  upstream `Func.fromTokens` recursion for valid input. 2 more corpus
  entries added.
- Final verification gates: `cargo test --workspace --no-fail-fast`
  all green (91 tests in `postcss-values-parser` after four passes,
  was 15); `parity-runner expand-shorthands` 45/45 byte-clean;
  `parity-runner postcss-core-roundtrip` 41/41 byte-clean; both NAPI
  verifiers 12/12; determinism on `expand-shorthands` (45/45).
- Full audit document at
  `crates/_vendor/POSTCSS_VALUES_PARSER_6.0.2_REAUDIT.md`.

**cssnano-utils 3.1.0 re-audit landed (2026-05-02):**
- Version is **not** drifted between REFERENCE_LOCK_FILE and AFM (both
  pin `3.1.0`). Audit ran because this package is shared helpers used
  transitively by every cssnano plugin we run — any latent transcription
  gap would propagate widely.
- Walked all four upstream files in
  `node_modules/.bun/cssnano-utils@3.1.0+*/...src/` (`index.js`,
  `getArguments.js`, `rawCache.js`, `sameParent.js`) against
  `crates/cssnano-utils/src/{lib,get_arguments,raw_cache,same_parent}.rs`
  line-by-line. **No divergence found** — Rust port is already 1:1.
- No Rust files modified; no corpus entries added (per CLAUDE.md "do
  not improve along the way" rule).
- Caller-graph audit: three crates (`cssnano-postcss-ordered-values`,
  `-minify-gradients`, `-minify-params`) declare `cssnano-utils` as a
  Cargo dep but no source file actually imports `cssnano_utils::*`
  yet — declarations are currently vestigial, so there is no
  consumer-side divergence surface either.
- Verification gates rerun: `cargo test --workspace --no-fail-fast`
  all green (incl. `cssnano-utils` unit tests in `get_arguments.rs`
  and `same_parent.rs`); `parity-runner postcss-core-roundtrip` 32/32
  byte-clean; both NAPI verifiers 12/12; determinism on
  `postcss-core-roundtrip` (32/32).
- Full audit document at
  `crates/_vendor/CSSNANO_UTILS_3.1.0_REAUDIT.md`.

**Re-audit findings (postcss-value-parser 4.2.0, port-quality re-check) (2026-05-02):**
- Pin is `4.2.0` in BOTH `REFERENCE_LOCK_FILE/yarn.lock` and AFM resolution
  — no version drift. Re-audit was for **port-quality** drift.
- Walked every line of upstream `lib/{index,parse,stringify,walk,unit}.js`
  against `crates/postcss-value-parser/src/{lib,parse,stringify,walk,unit}.rs`.
  Validated each suspected divergence against the live JS oracle (running
  the AFM-pinned `postcss-value-parser@4.2.0` directly) before patching.
- **Four real divergences fixed in `parse.rs`:**
  - **Slash-divider whitespace at root after close-paren.** JS `parent`
    becomes the truthy-but-typeless root frame `{nodes: tokens}` after
    a close-paren executes (`parse.js:251`), flipping the `(!parent ||
    parent.type === "function" && parent.value !== "calc")` clause to
    false. Prior Rust port produced no space node before a top-level
    `/` after `(...)`/`var(...)`/`calc(...)`. Added a `parent_assigned`
    flag mirroring JS `parent` truthiness.
  - **Closed-url `sourceEndIndex` off-by-one.** `parse.js:230` writes
    `unclosed ? next : pos` where `pos = next + 1`. Prior Rust wrote
    `next` for the closed-with-trailing-whitespace case. Now mirrors
    the JS outer override exactly.
  - **Div `sourceIndex` read of moved-out `before`.** Prior port called
    `std::mem::take(&mut before)` before the struct literal then read
    `before.len()` (= 0). Captures `before_len` upfront now.
  - **Word slice panic on input ending with `\`.** JS `slice` clamps;
    Rust `value[pos..next]` panics when `next > value.len()`. Now
    clamps the slice end with `next.min(value.len())` while preserving
    raw `next` for `sourceEndIndex` to match JS exactly.
- Added 7 unit tests in `parse.rs::tests` citing the upstream lines they
  cover (4 regression, 3 sanity-pin). `crates/postcss-value-parser`
  total tests: 24 passed, 0 failed.
- Added 4 adversarial corpus entries (`29..32`) to
  `corpus/postcss-normalize-whitespace/` covering: slash-after-closed-
  function, multi-space div separators, url-with-trailing-whitespace,
  and trailing-backslash words.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green; `parity-runner postcss-core-roundtrip` 35/35 byte-clean;
  `parity-runner postcss-normalize-whitespace` 32/32 byte-clean
  (28 existing + 4 new); `parity-runner postcss-normalize-{string,
  positions,timing-functions,url}` 15/20/21/60 all byte-clean; both
  NAPI verifiers 12/12; determinism on `postcss-normalize-whitespace`
  (32/32).
- Full audit document at
  `crates/_vendor/POSTCSS_VALUE_PARSER_4.2.0_REAUDIT.md`.

**postcss-normalize-string 5.1.0 re-audit landed (2026-05-02):**
- Pin is `5.1.0` in BOTH `REFERENCE_LOCK_FILE/yarn.lock` and AFM
  resolution — no version drift. Re-audit was for **port-quality**
  drift; this plugin touches every quoted string value in the AFM
  corpus, so a latent gap would surface widely.
- **One real drift fixed.** `process_node` was unconditionally clearing
  `node.raws.{selector,value,params}` after writing — but the
  postcss-core stringifier already does the JS-equivalent
  `raws.X.value == node.X ? raws.X.raw : node.X` comparison, and JS
  upstream just assigns. Result of the drift: any selector / decl /
  atrule whose source contained a comment or trailing whitespace
  captured into `raws.X.raw` lost those source bytes on no-op
  normalization. Surfaced by adversarial fixture
  `28_raws_preserved_on_noop.css` (JS preserved trailing
  `/* comment */`, Rust dropped it). **Fixed:** removed the three
  `node.raws.* = None` lines.
- One micro-stylistic deviation tightened: `change_wrapping_quotes`
  now reads `node.quote` freshly between its two `if`s (was a
  `cur_quote` snapshot). Logically equivalent in all valid inputs but
  matches upstream verbatim.
- Four new regression tests added (`change_wrapping_quotes_post_flip_second_if_inert`,
  `backslash_followed_by_non_special_falls_through`,
  `cache_key_collision_resistant_with_pipe_in_value`,
  `preserves_raws_on_noop_normalization`) — crate now 11 tests, all green.
- Thirteen adversarial corpus entries added (`16..28`) covering:
  BACKSLASH fall-through, consecutive escapes, mixed-whitespace runs,
  `|`-in-value cache keys, single-class escape rewrap (both directions),
  nested function strings, `@supports`/`@media` params, attr-selector
  quotes, empty strings inside functions, sibling string nodes,
  Unicode (Latin-1, CJK, astral-plane emoji) bodies, and
  raws-comment-after-value/atrule/selector preservation.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green; `parity-runner postcss-normalize-string` 28/28 byte-clean
  (15 existing + 13 new); determinism 28/28; both NAPI verifiers 12/12.
- Full audit document at
  `crates/_vendor/POSTCSS_NORMALIZE_STRING_5.1.0_REAUDIT.md`.

**fraction.js 4.2.0 re-audit landed (2026-05-02):**
- Pin is `4.2.0` in BOTH `REFERENCE_LOCK_FILE/yarn.lock` and AFM
  resolution — no version drift. Re-audit was for **port-quality** drift
  in the existing `crates/fraction-js/` port. Two consumers today
  (`crates/autoprefixer/src/resolution.rs` and the scaffolded
  `crates/cssnano-postcss-convert-values`); neither is wired into a
  parity-runner stage yet, so latent gaps would only surface once those
  stages land.
- Walked the single 891-line upstream `fraction.js` source side-by-side
  with `crates/fraction-js/src/fraction.rs`. **Six real divergences
  fixed:** (1) `gcd(NaN, NaN)` infinite loop — JS `!a` is true for both
  `0` and `NaN`; Rust only checked `0.0`, so `Fraction::new(f64::NAN)`
  hung. (2) Str-branch sign tracker not chain-assigned — upstream
  `s = /* void */ n = ...` parses as `s = (n = ...)`, collapsing the
  sign to the just-computed numerator; without it, `Fraction::new("-0")`
  emitted `{s:-1, n:0, d:1}` and `toString()` returned `"-0"` while JS
  returned `"0"`. (3) `b[a+2]` / `b[a+4]` index panics on truncated
  fraction strings (`"1/"`, `"1:"`, `"1 1/"`); JS reads `undefined` and
  throws InvalidParameter via `assign(undefined, ...)`. (4) `b[a]` index
  panic in the decimal else-if for lone-sign input `"-"` / `"+"`. (5)
  `toString(0)` returned `"0"` instead of `"0.(3)"` for `1/3` — JS
  `dec || 15` collapses `0` to default. (6) `simplify` method was
  missing entirely (used by `postcss-convert-values@5.1.3` for
  percentage rounding); ported verbatim including the `eps || 0.001`
  truthiness fallback. Also mirrored JS regex `.`'s line-terminator
  skipping in `match_digits_or_char`.
- Eleven regression tests added inside `crates/fraction-js/src/fraction.rs`
  citing the upstream lines they cover. Total: 21 tests (was 10), all
  green. No corpus entries added — no parity-runner stage exercises
  fraction-js today; when the autoprefixer / convert-values stages are
  wired up, adversarial CSS inputs covering these paths should be added
  to those corpora.
- Verification gates rerun: `cargo test --workspace --no-fail-fast` all
  green (no regressions across the workspace); `parity-runner
  postcss-core-roundtrip` 37/37 byte-clean; both NAPI verifiers 12/12;
  determinism on `postcss-core-roundtrip` (37/37).
- Full audit document at
  `crates/_vendor/FRACTION_JS_4.2.0_REAUDIT.md`.

## postcss version pin: `8.4.31` → `8.5.6` (no code changes)

The consuming monorepo's actual postcss version is `8.5.6`, not `8.4.31`
as we'd been targeting. Empirical diff harness at
`crates/_vendor/test-postcss-versions/` confirmed byte-identical
`parse → stringify` output across both versions:

- 5 of 13 source files byte-identical (`stringifier.js`, `root.js`,
  `at-rule.js`, `comment.js`, `list.js`).
- Remaining 8 files differ only in declaration order, getters/setters
  reordered, defensive null-checks, sourcemap/diagnostic surface — none
  reach the hashing path.
- 26/26 raw round-trips byte-identical between versions.
- 30/30 plugin × input pairs byte-identical (covers visitor + OnceExit
  lifecycle, raws preservation, walks).

Pin bumped in `PARITY_VERSIONS.md`, `Cargo.toml` description, `lib.rs`
header. **No code changes required** — all 489 tests still green.

## Phase progress

| Phase | Description | Status |
|---|---|---|
| 0 | parity-runner + corpus + JS-vs-JS determinism | **DONE** |
| 1 | postcss-core / caniuse-db / colord / fraction-js | **DONE** |
| 2 | postcss-selector-parser / postcss-value-parser / postcss-values-parser / browserslist-shim / cssnano-utils | **DONE** |
| 3 | caniuse-api | **DONE** |
| 4a | discard-empty-rules / discard-duplicates (LOCAL) / extract-stylesheets | **DONE** — all byte-clean |
| 4b | parent-orphaned-pseudos / increase-specificity | **DONE** — byte-clean (provisional pending oracle re-bake). `flatten-multiple-selectors` was deleted in the AFM repin — not part of the 0.19.0 surface. |
| 4c | merge-duplicate-at-rules / normalize-current-color / sort-atomic-style-sheet (+ at-rules helpers, sort-pseudo-selectors, sort-shorthand-declarations) | **DONE** — all byte-clean |
| 4d | atomicify-rules (CRITICAL hash plugin) | **DONE** — byte-clean across 24-entry corpus |
| 4e | expand-shorthands (11 conversion functions) | **DONE** — byte-clean across 38-entry corpus |
| 5a | postcss-nested@5.0.6 | **DONE** — byte-clean across 38-entry corpus, deterministic JS oracle |
| 5b | postcss-normalize-whitespace@5.1.1 | **DONE** — byte-clean across 22-entry corpus, deterministic JS oracle |
| 5c | postcss-discard-duplicates@6.0.0 (npm — used by sort.ts) | **DONE** — byte-clean across 8-entry corpus |
| 6a | postcss-discard-comments@5.1.2 | **DONE** — byte-clean across 15-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-string@5.1.0 | **DONE** — byte-clean across 15-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-positions@5.1.1 | **DONE** — byte-clean across 20-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-timing-functions@5.1.0 | **DONE** — byte-clean across 21-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-url@5.1.0 | **DONE** — byte-clean across 60-entry corpus, deterministic JS oracle |
| 6c | postcss-minify-selectors@5.2.1 | **SCAFFOLDED** |
| 6d | postcss-ordered-values@5.1.3 | **SCAFFOLDED** |
| 6d | postcss-calc@8.2.4 | **SCAFFOLDED** — calc expression evaluator; high diff risk on float math. |
| 6e | postcss-normalize-unicode@5.1.1 | **SCAFFOLDED** — browserslist-aware. |
| 6e | postcss-reduce-initial@5.1.2 | **SCAFFOLDED** — caniuse-aware. |
| 6f | postcss-convert-values@5.1.3 | **SCAFFOLDED** — uses fraction-js. |
| 6f | postcss-minify-params@5.1.4 | **SCAFFOLDED** — caniuse-aware. |
| 6g | postcss-minify-gradients@5.1.1 | **SCAFFOLDED** — uses colord. |
| 6g | postcss-colormin@5.3.1 | **SCAFFOLDED** — highest-risk cssnano plugin. |
| 6h | cssnano-preset-default@5.2.14 (orchestrator) | **SCAFFOLDED** |
| 7 | autoprefixer@10.4.14 | **IN PROGRESS** — split between two parallel agents. Source vendored at `crates/_vendor/autoprefixer-10.4.14/`. Crate scaffolded at `crates/autoprefixer/` with module tree mirroring `lib/` 1:1 + 58 stubbed hack modules. **All base classes fully ported:** `utils.rs`, `vendor.rs`, `brackets.rs`, `old_value.rs`, `old_selector.rs`, `prefixer.rs`, `browsers.rs`, `at_rule.rs`, `value.rs`, `selector.rs`, `declaration.rs`, `resolution.rs`, plus `prefixes.rs` registry skeleton with `register_hacks(reg)` append-only block. **`data/prefixes.rs` byte-clean** — 183 entries, codegen via `build.rs` from vendored JS through `bun`, 4 parity gates (canonical-JSON byte-equal, entry count, key order, caniuse-lite version pin). **56 tests passing (52 unit + 4 parity).** Hacks agent **unblocked**. Still stubbed: `supports.rs`, `transition.rs` (heavy, hacks rarely subclass), `processor.rs`, `info.rs`, `autoprefixer.rs`, all 58 hacks, `Prefixes::new` orchestrator body. Split contract: see "Phase 7 split contract" section below. See also "Phase 7 ship — `data/prefixes.rs` codegen + caniuse-lite pin fix". |
| 8a | `sort()` NAPI bridge + sort.ts engine flag | **DONE** — 12/12 corpus byte-clean end-to-end on win32-x64-msvc. See "Phase 8a ship" section below. |
| 8b | `transformCss` NAPI bridge + transform.ts engine flag | **NOT STARTED** — blocks on Phase 5/6/7 plugin ports. |

## Test totals

`RUSTFLAGS="" cargo test --workspace --no-fail-fast`:
- **502 tests pass / 0 fail / 1 ignored / 0 failed suites.**
  (12 from Phase 5b `postcss-normalize-whitespace`,
  12 from Phase 6a `postcss-discard-comments`,
  7 from Phase 6b `postcss-normalize-string`,
  12 from Phase 6b `postcss-normalize-positions`,
  15 from Phase 6b `postcss-normalize-timing-functions`,
  33 from Phase 6b `postcss-normalize-url`,
  4 from Phase 5a `postcss-nested`,
  20 from Phase 7 `autoprefixer` foundation,
  3 new postcss-core round-trip tests pinning the
  `rawSemicolon` / `rawBeforeOpen` / `rawColon` fallbacks.)

## Phase 8a ship — `sort()` end-to-end byte-clean through NAPI

**The smaller of the two hashing entry points is now production-ready
behind a feature flag.** Consumers opt in via:

```bash
COMPILED_CSS_ENGINE=rust   # use Rust NAPI backend
# unset / any other value  # use existing JS pipeline (default, parity oracle)
```

The flag is read at module-load time of `packages/css/src/sort.ts:32`
(see `requireFromHere` lazy-load gate). Other env values fall through to
the unchanged JS pipeline — no behavior change for unflagged consumers.

### What landed this session

1. `crates/css/src/sort.rs` is no longer an identity passthrough — it
   composes the three real plugins in **postcss lifecycle order**, not
   array order (see "Lifecycle ordering — load-bearing" below).
2. `crates/compiled-css-napi/` — new crate. cdylib + rlib, napi-rs 2.x,
   exports `sort(stylesheet, opts?)`. Phase 8b will add `transformCss`.
3. `packages/css-native/` — new npm workspace package wrapping the
   prebuilt `.node` binary. Platform-binary loader follows napi-rs
   naming convention (`sjcompiled-css.<triple>.node`).
4. `packages/css/src/sort.ts` — env-flag gate via `createRequire` +
   lazy import. Default path unchanged; oracle JS pipeline preserved.
5. `Stage::Sort` added to parity-runner + JS bridge counterpart in
   `packages/css/scripts/parity-bridge.mjs`.
6. New corpus `crates/parity-runner/corpus/sort/` — 12 fixtures
   covering: blank input, single rule, dup decls, dup at-rules with
   pseudo sort, at-rule reordering, shorthand buckets, full-combo
   (all three stages), comments, decls-at-root, three min-width / two
   max-width, important-vs-not, realistic atomic CSS.
7. `merge_duplicate_at_rules` refactored: split into `visit()` (the
   AtRule visitor pass) + `finalize()` (the OnceExit pass) + a
   combined `merge_duplicate_at_rules()` wrapper. The split is required
   for `sort()` to interleave `postcss-discard-duplicates`'s OnceExit
   between merge's visit and merge's exit, matching postcss lifecycle.
8. `container::append` fixed: now applies `Root.normalize`'s
   raws-before transfer when appending to a Root with ≥2 existing
   children (mirrors `postcss/lib/root.js::normalize` lines 24-28).
   Without this, `finalize()` emitted concatenated rules with the
   wrong leading whitespace.

### Verification gates run

| Gate                                                                | Status |
|---------------------------------------------------------------------|--------|
| `cargo test --workspace --no-fail-fast`                             | 356/356 pass |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort` | 12/12 byte-clean |
| `parity-runner --stage sort ... --determinism`                      | 12/12 deterministic |
| `parity-runner --stage merge-duplicate-at-rules ...`                | 7/7 byte-clean (no regression from refactor) |
| `parity-runner --stage npm-postcss-discard-duplicates ...`          | 8/8 byte-clean |
| `parity-runner --stage sort-atomic-style-sheet ...`                 | 12/12 byte-clean |
| `bun run packages/css/scripts/verify-napi-sort.mjs`                 | 12/12 (JS sort.ts vs Rust NAPI direct) |
| `bun run packages/css/scripts/verify-engine-flag.mjs`               | 12/12 (sort.ts under both engines, subprocess-isolated) |

### Lifecycle ordering — load-bearing

**The plugins in `postcss([A, B, C])` do NOT run in array order.**
Postcss's actual execution lifecycle is:

1. **All `Once` hooks** fire first, in plugin array order.
2. **Per-node visitors** (`Rule`, `AtRule`, `Decl`, `Comment`) fire
   during a depth-first walk of root.
3. **All `OnceExit` hooks** fire, in plugin array order.

For `sort.ts`'s `[discardDuplicates, mergeDuplicateAtRules, sortAtomicStyleSheet]`:

| Plugin                       | Hooks                       | Lifecycle position |
|------------------------------|-----------------------------|--------------------|
| postcss-discard-duplicates@6 | OnceExit only               | step 3, first      |
| mergeDuplicateAtRules        | AtRule visitor + OnceExit   | step 2 + step 3, second |
| sortAtomicStyleSheet         | Once only                   | step 1             |

So the *actual* execution order is:
`sortAtomicStyleSheet.Once → mergeDuplicateAtRules.AtRule visitor → postcss-discard-duplicates.OnceExit → mergeDuplicateAtRules.OnceExit`.

Calling them naively in array order in Rust silently changes which
node sits at index 0 of root when discard-duplicates runs, which
changes which `Root.removeChild` raws-transfers fire — the bytes drift
without any obvious signal.

`crates/css/src/sort.rs:35` is the canonical example. **For Phase 8b
(`transformCss`), every plugin in `transform.ts` will need to be
classified the same way before composing them.** The mistake is
trivial to make and produces silent byte drift.

### Parity-contract drift — RESOLVED via root `package.json` overrides

Initial audit found bun's caret ranges had silently drifted past the
pins in `crates/PARITY_VERSIONS.md` and `REFERENCE_LOCK_FILE/yarn.lock`:

| Package                      | Reference pin | Pre-fix (bun)  | Post-fix |
|------------------------------|---------------|----------------|----------|
| postcss                      | 8.4.31        | 8.5.13         | 8.4.31 ✅ |
| postcss-selector-parser      | 6.0.13        | 6.1.2 (6.0.13 NOT INSTALLED) | 6.0.13 ✅ |
| postcss-discard-duplicates   | 6.0.0         | 6.0.3          | 6.0.0 ✅ |
| autoprefixer                 | 10.4.14       | 10.5.0         | 10.4.14 ✅ |
| postcss-nested               | 5.0.6         | 5.0.6          | 5.0.6 ✅ |
| postcss-normalize-whitespace | 5.1.1         | 5.1.1          | 5.1.1 ✅ |
| postcss-values-parser        | 6.0.2         | 6.0.2          | 6.0.2 ✅ |
| cssnano-preset-default       | 5.2.14        | 5.2.14         | 5.2.14 ✅ |

Fix: an `overrides` block in root `package.json` pinning every
byte-affecting dep to its EXACT reference version, followed by
`bun install`. **Bun does not support nested `overrides`**, so the
Yarn-style nested resolution for the transitive
`cssnano-preset-default → postcss-discard-duplicates@5.1.0` is not
expressible. The global override forces all instances to 6.0.0, but
per `PARITY_VERSIONS.md` Anomaly #5 the transitive 5.1.0 is filtered
out by `normalize-css.ts:62-72` before execution and never reaches
the hashing path — so this is harmless.

**Every parity stage was re-run against the pinned oracle and remains
byte-clean** (208 corpus inputs across 16 gates):

| Gate                                | Corpus size | Result |
|-------------------------------------|-------------|--------|
| postcss-core-roundtrip              | 12          | OK |
| discard-empty-rules                 | 16          | OK |
| discard-duplicates (local)          | 11          | OK |
| extract-stylesheets                 | 12          | OK |
| parent-orphaned-pseudos             | 13          | OK |
| flatten-multiple-selectors          | 11          | OK |
| increase-specificity                | 12          | OK |
| normalize-current-color             | 10          | OK |
| atomicify-rules                     | 24          | OK |
| expand-shorthands                   | 38          | OK |
| merge-duplicate-at-rules            | 7           | OK |
| sort-atomic-style-sheet             | 12          | OK |
| npm-postcss-discard-duplicates      | 8           | OK |
| sort (end-to-end)                   | 12          | OK |
| verify-napi-sort.mjs                | 12          | OK |
| verify-engine-flag.mjs              | 12          | OK |

The Rust ports were already byte-clean against 8.4.31 / 6.0.13 / 6.0.0
/ 10.4.14 — the previous "byte-clean" claim against drifted versions
held only because patch-level diffs happened to be byte-irrelevant on
the existing corpus. Future sessions: **always run `bun install` and
verify `node_modules/.bun/` resolves the reference pins before
declaring byte-clean.**

## Phase 7 ship — `data/prefixes.rs` codegen + caniuse-lite pin fix

`crates/autoprefixer/src/data/prefixes.rs` is the 183-entry static prefix
table that drives every `add_table` / `remove_table` decision in the
orchestrator. Now byte-clean against the JS oracle, codegen'd via
`build.rs`. Foundation task #12 from the autoprefixer split contract.

### What landed this session

1. `crates/autoprefixer/build.rs` — codegen. Spawns `bun <file>`
   (Windows shim-aware) on a tmp script that `require()`s the vendored
   `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`,
   dumps the resulting object as JSON, parses via `serde_json` with
   `preserve_order` (load-bearing — `Object.keys` insertion order
   reaches downstream `add_table` iteration), and emits a series of
   `m.insert(...)` statements wrapped in a single block expression at
   `$OUT_DIR/prefixes_table.rs`.
2. `crates/autoprefixer/src/data/prefixes.rs` — `PrefixEntry` struct
   with `#[serde(skip_serializing_if = "...")]` matching JS's
   "omit-when-falsy" convention (`mistakes`/`props`/`feature` skip
   when empty, `selector`/`transition` skip when false). Lazy
   `IndexMap<&'static str, PrefixEntry>` includes the codegen output.
3. `crates/autoprefixer/Cargo.toml` — `[build-dependencies]` and
   `[dev-dependencies]` declare `serde_json` with `preserve_order`.
4. `crates/autoprefixer/tests/data_parity.rs` — 4 parity gates:
   - `data_table_matches_js_oracle` — recursive canonical-JSON
     (sorted keys at every nesting level) byte-equal between Rust
     `PREFIXES` and the JS oracle.
   - `entry_count_matches_js_oracle` — `PREFIXES.len() ==
     Object.keys(prefixes).length`.
   - `key_order_matches_js_oracle` — `IndexMap` insertion order
     equals JS `Object.keys` order. Catches the `serde_json`
     alphabetization regression hit during port.
   - `caniuse_lite_pin_matches_parity_versions` — explicit assertion
     that `require('caniuse-lite/package.json').version ==
     "1.0.30001690"`. Belt-and-braces against future `bun.lock` drift.
5. **caniuse-lite pin fix in root `package.json`.** PARITY_VERSIONS.md
   Anomaly #3 + REFERENCE_LOCK_FILE/yarn.lock both pin caniuse-lite at
   1.0.30001690, but the previous "Parity-contract drift — RESOLVED"
   override block missed it (and electron-to-chromium / node-releases).
   Symptom: data/prefixes.js generated table contained chrome 135 /
   edge 135 / samsung 28 — versions that don't exist in the pinned
   snapshot. Fix landed:
   - Added `caniuse-lite: "1.0.30001690"`, `electron-to-chromium:
     "1.5.76"`, `node-releases: "2.0.19"` to root `package.json`
     `overrides`.
   - Added `caniuse-lite: "1.0.30001690"` to root `package.json`
     `devDependencies`. **Load-bearing** — without the direct-dep
     declaration, bun's isolated install layout leaves no top-level
     `node_modules/caniuse-lite/` symlink, and the vendored JS's
     `require('caniuse-lite')` walks UP the filesystem and resolves
     to whatever `node_modules/caniuse-lite` lives in a parent project
     (observed during port: a parent dir at 1.0.30001754).
   - `bun install` re-resolved. `node_modules/caniuse-lite` is now a
     symlink to the pinned `.bun/caniuse-lite@1.0.30001690/...` install.

### Verification gates run

| Gate | Status |
|---|---|
| `cargo test -p autoprefixer` | 56/56 (52 unit + 4 parity) |
| `cargo build -p autoprefixer` | clean |
| Generated table entry count | 183 (matches JS) |
| Last-entry max chrome | 134 (matches caniuse-lite 1.0.30001690) |
| Last-entry max samsung | 27 (matches caniuse-lite 1.0.30001690) |
| `samsung 28` in table | 0 occurrences (was 1+ pre-pin) |

### Lessons from Phase 7 `data/prefixes.rs` — apply to every future build.rs

- **Pin the runtime, not just the lockfile.** The previous "Parity-
  contract drift — RESOLVED" pinned 8 direct deps via root
  `package.json` overrides. But caniuse-lite is a transitive — and
  also the silent invariant per Anomaly #3. Override-only is not
  enough: if no workspace package has the transitive as a direct
  dep, bun's isolated layout leaves no top-level `node_modules/<pkg>`
  and parent-directory shadows can resolve. The fix is BOTH override
  AND direct devDependency.
- **Audit transitives flagged in PARITY_VERSIONS.md the same way.**
  electron-to-chromium / node-releases / picocolors / source-map-js /
  fraction.js / nanoid all sit in similar position. The autoprefixer
  port doesn't touch these directly yet, but `processor.rs` /
  `Browsers::new` / cssnano plugins will. The
  `caniuse_lite_pin_matches_parity_versions` test pattern can be
  copied for each (~5 LOC per dep).
- **`bun -e <script>` is fragile under Windows arg-quoting.** During
  port, `process.stdout.write(...)` was truncated to `ss.stdout.write`
  in the spawned subprocess because Windows mangled the JS string.
  Always write the script to a file and invoke `bun <file>`.
- **`serde_json` defaults to alphabetized object iteration.** Enable
  `preserve_order` for any codegen that translates JS `Object.keys`
  order into Rust `IndexMap` order. The codegen looked correct in
  initial review; the `key_order_matches_js_oracle` test caught it.
- **Test-script tmpdir matters for resolution.** Bun's CommonJS
  resolver walks UP from the script file's directory, NOT cwd. Tests
  that write dumpers to `std::env::temp_dir()` (outside the workspace)
  resolve `require('caniuse-lite')` against random parent-directory
  projects. Anchor under `target/` (inside the workspace).
- **`include!()` expects a single expression.** Wrap codegen output
  in a `{ ... }` block expression so the include site sees one
  expression containing N `m.insert(...)` statements.
- **`bun.cmd` shim resolution on Windows.** `Command::new("bun")`
  doesn't walk PATHEXT. Always try the bare name then fall back to
  `bun.cmd` / `bun.exe` candidates.

## Phase 7 split contract — autoprefixer parallel agents

`crates/autoprefixer/` is being ported by two agents in parallel. The
boundary is **physical** — each agent's tree is non-overlapping except
for one shared registration file. Read this before claiming Phase 7
work.

### Tree split

| Path                                          | Owner            |
|-----------------------------------------------|------------------|
| `crates/autoprefixer/src/*.rs` (top-level)    | foundation agent |
| `crates/autoprefixer/src/data/`               | foundation agent |
| `crates/autoprefixer/src/hacks/*.rs`          | hacks agent      |
| `crates/autoprefixer/src/hacks/HACKS_PORT.md` | hacks agent (checklist + progress tracker) |
| `crates/autoprefixer/src/prefixes.rs`         | **shared** — append-only `register_hacks()` block |

The hacks agent reads `crates/autoprefixer/src/hacks/HACKS_PORT.md`
for the per-hack parent-class table, the trait surface, and the
registration contract.

### Foundation agent's responsibilities (in order)

1. ✅ Vendor source under `crates/_vendor/autoprefixer-10.4.14/`.
2. ✅ Scaffold crate + module tree (compiles cleanly).
3. ✅ Port leaf utilities (`utils`, `vendor`, `brackets`, `old_value`,
   `old_selector`).
4. ✅ Port `prefixer.rs` — full.
5. ✅ Port `at_rule.rs` — full.
6. ✅ Port `browsers.rs` — full (caniuse-db agents + browserslist-shim).
7. ✅ Port `value.rs` — full.
8. ✅ Port `selector.rs` — full (incl. `already()` backward sibling walk
   via `parent_nodes` + `sibling_at`).
9. ✅ Port `declaration.rs` — full (incl. cascade via
   `_autoprefixerCascade`/`_autoprefixerMax` memos and `process` with
   path-shift cursor handling).
10. ✅ Port `resolution.rs` — full (uses `fraction_js`).
11. ✅ Port `prefixes.rs` skeleton — `HackRegistry` + `register_hacks`
    append-only block. `Prefixes` orchestrator method bodies still
    `unimplemented!()` pending `data/prefixes.rs` and `processor.rs`.
12. ✅ Port `data/prefixes.rs` — 183 entries codegen'd via `build.rs` +
    `bun`, 4 parity gates byte-clean. See "Phase 7 ship —
    `data/prefixes.rs`" section above.
13. ⬜ Port `supports.rs` (302 LOC — `@supports` query rewriting).
14. ⬜ Port `transition.rs` (329 LOC — `transition` shorthand).
15. ⬜ Fill `Prefixes` orchestrator body (depends on `data/prefixes.rs`).
16. ⬜ Port `processor.rs` (718 LOC — main walk).
17. ⬜ Port `info.rs` + `autoprefixer.rs` (entry point).
18. ⬜ Add `Stage::Autoprefixer` parity-runner gate (requires
    parity-runner edits + parity-bridge.mjs — re-ask permission).
19. ⬜ Wire into `crates/css/src/transform.rs` (re-ask permission).

### Path-shift gotcha — load-bearing for every base class

JS holds a node *reference* across `parent.insertBefore(node, cloned)`
calls — the reference auto-follows when the original's index shifts.
The Rust port uses *index paths*. Each successful
`insert_before_at_path(root, path, clone)` shifts the original's index
in its parent up by 1 because the clone is spliced at the original's
slot. **The path becomes stale** the moment the insert returns.

Fix pattern (see `at_rule.rs::process`):

```rust
let mut current_path = path.to_vec();
for prefix in &prefixes {
    if self.add(root, &current_path, prefix).is_some() {
        if let Some(last) = current_path.last_mut() { *last += 1; }
    }
}
```

**`value.js`, `selector.js`, `declaration.js` all do the same `parent
.insertBefore` pattern in a loop.** Apply the same path-bump on every
successful insert. The bug is silent: tests that only insert one
prefix won't catch it; tests that insert two or more will.

If the hacks agent ports a hack that does its own insert outside the
base class's `add`, follow the same pattern.

### Hacks agent's responsibilities

**Status: UNBLOCKED.** All five base classes (`Prefixer`,
`AtRuleBase`, `ValueBase`, `SelectorBase`, `DeclarationBase`,
`ResolutionBase`) plus `Browsers` plus the `HackRegistry` are
ported with full method bodies + passing unit tests. The trait
surface for hacks-by-composition is locked.

1. Pick a hack from the table in `HACKS_PORT.md`. Take it 0 → 100%
   byte-clean (per cardinal rule).
2. Register it in `crates/autoprefixer/src/prefixes.rs::register_hacks`
   in alphabetical-by-JS-filename order (BEGIN/END markers).
3. Mark the row in `HACKS_PORT.md` Done.

### What the hacks agent must NOT do

- Edit anything outside `src/hacks/` (one exception: append-only
  registration in `src/prefixes.rs`).
- Add methods to base traits. If a hack needs a method that isn't on
  `Declaration`/`Value`/`Selector`/`AtRule`, file a note in
  `HACKS_PORT.md` and pause — the foundation agent owns base-class
  shape.
- Re-port `flex-spec.js` or `grid-utils.js` as classes. They're
  shared helpers; port as plain functions in
  `hacks/flex_spec.rs` / `hacks/grid_utils.rs`.

### Current handshake state

- Crate compiles. `cargo test -p autoprefixer` → 11/11 passing.
- Hacks agent **cannot start** until base classes (foundation tasks
  #7 + #8) land. The trait surface signature would change otherwise.
- Foundation agent will not finish in one session. Phase 7 is multi-
  session by design; STATUS.md tracks per-task completion.

## Foundational infrastructure (load-bearing for plugin ports)

These exist and are byte-tested. Plugin authors depend on them; do NOT
re-implement helpers. Add new ones in the appropriate crate:

### `postcss-core` (postcss@8.4.31 port)

- AST types (Root / AtRule / Rule / Declaration / Comment).
- Parser + tokenizer + stringifier with full `raws` preservation.
- **Stringifier raw-defaults**: `rawBeforeRule`, `rawBeforeDecl`,
  `rawBeforeComment`, `rawBeforeClose` scans cached on first use.
  Without these, plugin-driven replacements emit concatenated rules
  with no separator.
- **`container::remove_at`** — Root.removeChild override
  (postcss/lib/root.js): when removing the first child of root, the
  removed node's `raws.before` transfers to the new first child.
  ALL plugin-driven removals at root level MUST go through
  `remove_at`, not raw `Vec::remove`.
- **`container::replace_with_at`** — `node.replaceWith(...)` semantics
  (insertBefore-each-then-remove with Root.normalize override). Used
  internally by `each_mut` / `walk_mut`'s `Mutation::Replace` and
  `Mutation::ReplaceMany`.
- **`Rule::get_selectors` / `set_selectors`** — comma-split with
  `,\s*` separator preservation on join (`rule.selectors` get/set).
- **`list::comma` / `list::space`** — trimmed value-list splitters.
- **`stringify_node(node)`** — port of postcss `node.toString()` (no
  leading raws.before; first-child-of-root context).

### `postcss-selector-parser` (6.0.13)

- Tokenizer + parser + typed AST (ClassName, Identifier, Pseudo,
  Attribute, Combinator, etc.).
- **Compound-selector splitting** (`.foo.bar`, `tag.x#id`) into
  multiple typed nodes.
- **Pseudo arg storage**: prefix only (`:not`) on `value`, parens
  rebuilt from `nodes` at stringify time so plugin mutations to inner
  selectors flow through.
- **`walk_pseudos` / `walk_classes` / `walk_attributes`** mutating
  walkers with parent-context callbacks.
- **`Node::nesting()` / `Node::pseudo(value)`** factories.

### `postcss-values-parser` (6.0.2 plural — distinct from value-parser)

- Tokenize + parse + classify (Numeric, Word, Func, Quoted,
  Punctuation, Operator, UnicodeRange, AtWord, Comment).
- **`stringify_standalone(node)`** — port of `node.toString()` for the
  values-parser node hierarchy (skips outer `raws_before`; Funcs emit
  child `raws_before` inside parens).

### `sjcompiled-utils`

- `hash` — bit-identical to JS `murmurhash2_gc`. **Do not re-port.**
- `unique` / `flatten` / `kebab_case` / `to_boolean`.
- `INCREASE_SPECIFICITY_SELECTOR = ":not(#\\#)"`.
- `shorthand_buckets` (67 entries) / `shorthand_for` table.

### `colord` (2.9.1)

Full color parse / manipulation / minification surface. Phase 6g
(`postcss-colormin` / `postcss-minify-gradients`) consume this.

### `caniuse-db` / `caniuse-api` / `browserslist-shim`

Pinned data + query helpers for `autoprefixer` and the browserslist-
aware cssnano plugins.

## Workspace layout

`crates/Cargo.toml` has 33 members (32 + `compiled-css-napi` added in
Phase 8a). Naming:
- `cssnano-postcss-*` — the 14 cssnano sub-plugins, prefixed to
  disambiguate from same-named npm packages (e.g. distinguishing
  `postcss-normalize-string` from any future v6/v7 fork).
- `postcss-*` — the 4 plugins consumed directly by `transform.ts` /
  `sort.ts`.
- `cssnano-preset-default` — the preset orchestrator.
- `compiled-css-napi` — the Phase 8 NAPI bridge crate (cdylib + rlib).
- Foundation crates keep their upstream names where unambiguous.

`packages/css-native/` is the npm wrapper around the compiled `.node`
binary. Phase 8a ships `sjcompiled-css.win32-x64-msvc.node` only;
Phase 8b extends to linux-x64-gnu / linux-arm64-gnu / darwin-x64 /
darwin-arm64 once `transformCss` is byte-clean.

## What's left to port (full source-faithful Rust ports)

9 crates. Listed in roughly ascending complexity:

1. `postcss-ordered-values` — moderate. Reorders multi-value
   shorthand parts.
2. `postcss-minify-selectors` — moderate. Selector minification using
   postcss-selector-parser.
3. `postcss-normalize-unicode` — moderate, browserslist-aware.
4. `postcss-reduce-initial` — moderate, caniuse-aware.
5. `postcss-convert-values` — hard, uses fraction-js, browserslist.
6. `postcss-minify-params` — hard, caniuse-aware.
7. `postcss-minify-gradients` — hard, colord-heavy.
8. `postcss-calc` — VERY hard. Effectively a small expression compiler.
9. `postcss-colormin` — HARDEST cssnano plugin. Color downgrade
   decisions hinging on caniuse + colord rounding + byte-length
   comparison.
10. `postcss-nested` (Phase 5a) — VERY hard. Recursive selector
    merging with bubble/unwrap config.
11. `cssnano-preset-default` — moderate orchestrator (depends on
    1-9 being byte-clean first).

Plus Phase 7 (autoprefixer — 8+ weeks of its own) and Phase 8
(NAPI assembly + the `transformCss` / `sort` end-to-end gates).

## Recommended order for the next session

`sort()` is now byte-clean end-to-end through NAPI (Phase 8a) and
Phase 5b (`postcss-normalize-whitespace`) is byte-clean across a
20-entry corpus. The remaining work is all `transformCss`-bound. The
cardinal-rule guidance remains: **a session must take a unit from 0%
→ 100% byte-clean**. Half-done ports become silent byte-drift hazards
across agent handoffs.

The Phase 6b "simple band" (discard-comments, normalize-string,
normalize-positions, normalize-timing-functions, normalize-url) is **fully
byte-clean**. The remaining cssnano work is moderate-to-hard.

1. **Phase 6 moderate band** — pick one of:
   - `minify-selectors@5.2.1` (uses postcss-selector-parser; moderate).
   - `ordered-values@5.1.3` (multi-value reordering; moderate).
   Finish one before starting the next.
2. **Phase 5a** (`postcss-nested`) — multi-day commitment. Recursive
   selector merging. Don't start unless you have time to finish.
3. **Phase 6 hard band** — `ordered-values`, `minify-selectors`,
   `normalize-url`, `normalize-unicode`, `reduce-initial`,
   `convert-values`, `minify-params`, `minify-gradients`. Each
   multi-day.
4. **Phase 6h** — `postcss-calc` (small expression compiler) and
   `postcss-colormin` (HARDEST cssnano plugin). Each multi-week.
5. **Phase 7** — `autoprefixer@10.4.14`. ~8 weeks for one engineer.
7. **Phase 6h orchestrator** — `cssnano-preset-default` once 1–6
   are byte-clean.
8. **Phase 8b** — `transformCss` NAPI export + `transform.ts` engine
   flag, mirroring the Phase 8a pattern below.

## Phase 5a ship — `postcss-nested@5.0.6` byte-clean

The largest single pre-autoprefixer plugin port. Recursive selector
merging with `bubble`/`unwrap`/`at-root` semantics — runs inside
`transform.ts:48-61` and is on the hashing path for every nested rule
in every consumer. Now byte-clean.

### What landed this session

1. `crates/_vendor/postcss-nested-5.0.6/` — vendored upstream source
   (215 LOC `index.js` + LICENSE/README/package.json/index.d.ts).
2. `crates/postcss-nested/src/lib.rs` — full port. Single source file
   maps 1:1. Public surface: `postcss_nested(root, opts)` +
   `PostcssNestedOpts { bubble, unwrap, preserve_empty }`. Module-level
   helpers mirror upstream functions verbatim (`atrule_names`,
   `parse_selector`, `replace_nesting`, `selectors_of`,
   `build_wrapper_rule`, `atrule_childs`, `clone_rule_with_empty_nodes`,
   `insert_after_with_normalize`, `is_comment`, `visit_rule`,
   `walk_container`).
3. `crates/parity-runner/Cargo.toml` — added `postcss-nested` workspace
   dep so the stage handler can call into it.
4. `crates/parity-runner/src/stages.rs` — `Stage::PostcssNested`
   variant + handler with the production `bubble`/`unwrap` opts from
   `transform.ts:48-61` baked in (so the parity gate validates the
   exact configuration that ships).
5. `crates/parity-runner/src/main.rs` — CLI mapping for
   `postcss-nested`.
6. `packages/css/scripts/parity-bridge.mjs` — JS-side stage that runs
   `postcss([postcssNested({bubble, unwrap})]).process(css)` against
   the pinned 5.0.6 oracle, with the matching opts.
7. `crates/parity-runner/corpus/postcss-nested/` — 38 fixtures
   covering: blank, no-nesting, simple nest, `&` substitution,
   comma-list parent + comma-list child, deep nest (3-4 levels),
   bubble (`@media`/`@supports`/`@container`/`@starting-style`/`@layer`
   bubble-list), unwrap (`@keyframes`/`@font-face`/`@page` + vendor
   `@-webkit-keyframes`), `@at-root` (with and without params),
   comments interleaved between sibling rules, decls before AND after
   nested rules (split-decls), mixed decl/rule/decl shape, deep nest
   inside bubble at-rule, top-level rule with no decls + only nested,
   pseudo-nested (`:hover`/`::before`), `& + &`/`& ~ &`,
   `&--modifier` BEM-style suffix joining, `&:not(...)` pseudo-arg,
   `.b &` (descendant + nesting) for the spaces-transfer fix,
   tag-and-amp (`section { &.x; & > x }`), attribute selectors
   (`[type="text"]`), bubble + trailing decls, realistic atomic CSS
   with multiple `&` patterns inside `@media`.
8. Bug-for-bug fidelity replicated: `parse(str, rule)`'s "Missed
   semicolon" branch (when parse fails AND `str` contains `:`),
   `atruleNames` strip-`@` normalization, `bubble`/`unwrap`/`at-root`
   ordering of the at-rule sub-branches inside the visitor, the
   "dump pending declarations at top of EVERY at-rule sub-branch"
   detail (including the `copy_declarations` fall-through), the
   `replace`-recurses-only-into-nodes-with-children quirk, and the
   `if (j.length)` skip-empty-selector guard.

### Verification gates run

| Gate                                                                   | Status |
|------------------------------------------------------------------------|--------|
| `cargo test -p postcss-nested`                                         | 4/4 pass |
| `cargo test --workspace --no-fail-fast`                                | 462/462 pass |
| `parity-runner --stage postcss-nested --corpus ...`                    | 38/38 byte-clean |
| `parity-runner --stage postcss-nested ... --determinism`               | 38/38 deterministic |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort` | 12/12 (no regression) |
| `parity-runner --stage merge-duplicate-at-rules ...`                   | 7/7 (no regression) |
| `parity-runner --stage atomicify-rules ...`                            | 24/24 (no regression) |
| `parity-runner --stage expand-shorthands ...`                          | 38/38 (no regression) |
| `parity-runner --stage postcss-normalize-whitespace ...`               | 22/22 (no regression) |

### Walker design — single forward pass, no re-walk

Upstream's `Rule(rule, { Rule })` visitor is invoked on every Rule
discovered in document order, and re-visits Rules promoted to siblings
during the walk. The Rust port does NOT need a postcss-style re-walk
because postcss-nested's promotion always moves children OUT of the
rule (never back in). After visiting a rule, its remaining children
(decls and comments only — no rules) need no further processing.

The walker is a recursive `walk_container` that:

1. Iterates `parent.nodes` by index forward.
2. For each Rule encountered: invokes `visit_rule`, which removes the
   rule from its parent, processes its children, then re-inserts the
   rule plus any promoted siblings at the same position. The cursor
   advances 1 each iteration; promoted siblings end up at `i+1`,
   `i+2`, ... and are visited on subsequent iterations.
3. AtRules with bodies are recursed INTO (rules can be nested inside
   `@media`-style bubble at-rules — e.g. `.a { @media x { .b {} } }`
   ends up as `@media x { .a .b {} }` after `.a`'s visitor, and the
   walker descends into `@media` to visit `.a .b`).
4. Other node kinds skipped.

### `raws` defaults — handled by postcss-core stringifier

Fresh wrapper Rules created by `pickDeclarations` (and the at-root
params-wrapper branch) carry NO explicit `raws`. The postcss-core
stringifier derives `raws.between` (via `rawBeforeOpen` scanner),
`raws.semicolon` (via `rawSemicolon` scanner), and `raws.after` (via
`rawBeforeClose` scanner) from the surrounding tree at stringify time
— mirroring `postcss/lib/stringifier.js::raw`. This matches what JS
upstream emits for `new Rule({ selector, nodes: [] })` byte-for-byte.

(Earlier in this session those scanners weren't implemented and this
plugin hard-coded the inherited values. After the postcss-core fixes
landed — `rawSemicolon`, `rawBeforeOpen`, `rawColon`, plus the
`insert_before_with_normalize` Root-prepend strip-step guard — the
hard-coded values were stripped. All 21 parity stages remain
byte-clean and 465/465 workspace tests pass with no plugin-side
workarounds in this crate.)

### Postcss-selector-parser quirk — descendant-combinator emission

Upstream JS `postcss-selector-parser@6.0.13` emits an explicit
`Combinator{value: " "}` node for descendant whitespace combinators
(see `dist/parser.js::combinator` lines 480-568). The Rust port at
`crates/postcss-selector-parser/src/parser.rs` does NOT — it stores
descendant whitespace as the next node's `spaces.before`. This is a
parser-side divergence from upstream. To preserve byte-clean output
for `.b & { ... }`-style inputs, `replace_nesting` transfers the
Nesting's `spaces` onto the replacement node before splicing. In JS
this transfer isn't needed because the Combinator sits BETWEEN the
preceding selector and the Nesting; in our Rust port the space is
fused onto the Nesting itself, so it must be moved with the
replacement. **Filed as a postcss-selector-parser bug** — when that
bug is fixed, `replace_nesting`'s `new_node.spaces = nesting_spaces`
line should be removed (it'll become a double-space).

### Lessons from Phase 5a — apply to every future port

- **Postcss `rawCache` defaults are load-bearing.** Any plugin that
  creates fresh nodes (Rule/AtRule/Decl/Comment) inherits styling via
  `rawCache` walks at stringify time. The `postcss-core` stringifier
  now implements `rawBefore{Rule,Decl,Comment,Close,Open}`,
  `rawSemicolon`, and `rawColon`, mirroring upstream
  `stringifier.js::raw`. New plugins should construct fresh nodes with
  NO explicit `raws` and let the stringifier derive defaults — do not
  hard-code inherited values from a source node, that's a divergence
  trap.
- **Postcss-selector-parser doesn't emit descendant Combinators.**
  See the section above. Plugins that mutate selector ASTs around
  Nesting/Combinator nodes need to be aware of the
  spaces-on-next-node convention. Document this in any new
  selector-parser-touching plugin.
- **`atruleChilds(rule, child, false)` does NOT remove children from
  `child.nodes`.** Upstream JS pushes references into a local
  `children` list that's only consumed when `bubbling=true`. Our
  initial port removed unconditionally and broke unwrap (`@font-face`
  body went missing). Mirror upstream: only remove when `bubbling`.
- **Borrow-checker pattern: take-rule-out-of-parent.** Postcss visitors
  conceptually mutate `rule` AND `rule.parent` simultaneously. In Rust
  the ergonomic move is `parent.nodes_mut().unwrap().remove(rule_index)`
  to take ownership, then mutate `rule` and accumulate "promoted
  siblings" in a local Vec, then re-insert `rule` at `rule_index` and
  chain-insert promoted via `insert_after_with_normalize`. Avoids all
  borrow conflicts; matches upstream behavior including the
  Root.removeChild raws-transfer if `rule` is ultimately removed.
- **JS `child.prev()` / `pickComment` interaction with the iterator.**
  The JS iterator decrements when nodes are removed during the walk;
  combined with `pickComment` (which removes the prev-sibling comment
  before the current node) and `after.after(child)` (which removes
  the current node), the iterator lands on what was originally
  `index + 1`. In Rust the equivalent is "don't increment `i` when
  the current node was moved out (with optional pickComment of
  `nodes[i - 1]`)" — see `visit_rule` rule and at-rule branches.

## Phase 5b ship — `postcss-normalize-whitespace@5.1.1` byte-clean

Single OnceExit-only plugin that runs inside `transform.ts`'s pipeline
(blocking Phase 8b end-to-end `transformCss` parity). Now byte-clean.

### What landed this session

1. `crates/postcss-normalize-whitespace/src/lib.rs` — full port of
   `node_modules/postcss-normalize-whitespace@5.1.1/src/index.js`. The
   single source file maps 1:1 (file/folder shape preserved).
2. `crates/postcss-normalize-whitespace/Cargo.toml` — added `once_cell`
   and `indexmap` deps. Pre-existing `postcss-core`,
   `postcss-value-parser`, `regex` already declared.
3. `crates/parity-runner/Cargo.toml` — added
   `postcss-normalize-whitespace` workspace dep so the stage handler
   can call into it.
4. `crates/parity-runner/src/stages.rs` —
   `Stage::PostcssNormalizeWhitespace` variant + handler.
5. `crates/parity-runner/src/main.rs` — CLI mapping for
   `postcss-normalize-whitespace`.
6. `packages/css/scripts/parity-bridge.mjs` — JS-side stage that runs
   `postcss([postcssNormalizeWhitespace()]).process(css)` against the
   pinned 5.1.1 oracle.
7. `crates/parity-runner/corpus/postcss-normalize-whitespace/` — 22
   fixtures covering: blank, simple rule, multi-decl, calc inner WS,
   var/env exemption, IE9 hack (single-replace, no `g` flag), excess
   `!important` whitespace, `--*` empty value, atrule, nested atrule,
   url, comments, multi-calc cache hits, multi-value shorthand,
   quoted strings, mixed rule/decl neighbors, statement atrules
   (`@charset` / `@import`), pseudo selectors, nested calc, realistic
   atomic CSS, transform/translate function-chain spacing
   (translate3d / matrix / rotate / scale / perspective with
   pathological whitespace, multiline transform with embedded calc).

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p postcss-normalize-whitespace`                                    | 12/12 pass |
| `cargo test --workspace --no-fail-fast`                                         | 368/368 pass |
| `parity-runner --stage postcss-normalize-whitespace --corpus ...`               | 22/22 byte-clean |
| `parity-runner --stage postcss-normalize-whitespace ... --determinism`          | 22/22 deterministic |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

## Phase 6a ship — `postcss-discard-comments@5.1.2` byte-clean

cssnano sub-plugin. Removes comments from the AST and scrubs them out
of inline raws (`raws.between`, decl `raws.value.raw`,
rule `raws.selector.raw`, atrule `raws.afterName` / `raws.params.raw`).
Default opts keep `/*!` important comments.

### What landed this session

1. `crates/cssnano-postcss-discard-comments/src/lib.rs` — main port of
   `node_modules/postcss-discard-comments@5.1.2/src/index.js`.
2. `crates/cssnano-postcss-discard-comments/src/comment_parser.rs` —
   port of `src/lib/commentParser.js`. The upstream `lib/` parent
   directory is dropped because Rust's crate-root file is itself
   `lib.rs` and a child module literally named `lib` collides;
   behavior is unaffected. Includes the unclosed-comment quirk
   (`indexOf('*/') === -1` → upstream produces `pos = 1` and a
   sentinel slice — replicated via `UNCLOSED_END = usize::MAX`).
3. `crates/cssnano-postcss-discard-comments/src/comment_remover.rs` —
   port of `src/lib/commentRemover.js`. Tri-state predicate matching
   upstream (`Some(true)` remove / `Some(false)` keep / `None` for
   upstream `undefined` fall-through which JS treats as falsy → keep).
4. Stage + bridge wiring: `Stage::PostcssDiscardComments`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-discard-comments: 5.1.2`),
   `packages/css/package.json` devDependency.
5. 15-fixture corpus covering: blank, no-comments, top-level comment,
   `/*!` important kept, comments inside rule bodies, comments inside
   decl values, comments inside selectors and selector lists, comments
   in atrule params and afterName, comments in `raws.between` (decl
   prop/colon split), comments around `!important`, mixed
   important/normal, atrules without bodies (`@charset`/`@import`),
   nested atrules, consecutive comments, realistic atomic CSS.

### Verification gates run

| Gate                                                                    | Status |
|-------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-discard-comments`                        | 12/12 pass |
| `parity-runner --stage postcss-discard-comments --corpus ...`           | 15/15 byte-clean |
| `parity-runner --stage postcss-discard-comments ... --determinism`      | 15/15 deterministic |

## Phase 6b ship — `postcss-normalize-string@5.1.0` byte-clean

cssnano sub-plugin. Walks rule selectors, decl values, and atrule
params; rewraps string literals to the preferred quote style (default
`'double'`) when the swap reduces escapes, and collapses `\\\n`
(escaped newline) inside string bodies.

### What landed this session

1. `crates/cssnano-postcss-normalize-string/src/lib.rs` — full port of
   `node_modules/postcss-normalize-string@5.1.0/src/index.js`.
   Single source file maps 1:1.
2. The bespoke string-AST parser (`ast_parse`) is a hand-rolled
   byte-scan with the same `[ \n\t\r\f'"\\]` word-end character class
   as upstream's `WORD_END` regex, including the intentional
   fall-through from the backslash branch into the default word
   branch (upstream's "missing `break`" bug — replicated verbatim).
3. Stage + bridge wiring: `Stage::PostcssNormalizeString`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-string: 5.1.0`),
   `packages/css/package.json` devDependency.
4. 15-fixture corpus covering: blank, no-strings, single/double-quoted
   plain values, escaped-double-in-double, escaped-single-in-single,
   mixed escapes, bare quote inside opposite wrap, empty strings,
   attribute selectors with both quote styles, `url(...)` with
   quotes, font-family lists, atrule string params
   (`@charset`/`@import`), escaped newline collapse, realistic atomic
   CSS.

### Verification gates run

| Gate                                                                  | Status |
|-----------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-string`                      | 7/7 pass |
| `parity-runner --stage postcss-normalize-string --corpus ...`         | 15/15 byte-clean |
| `parity-runner --stage postcss-normalize-string ... --determinism`    | 15/15 deterministic |

## Phase 6b ship — `postcss-normalize-positions@5.1.1` byte-clean

cssnano sub-plugin. Walks decls matching
`/^(background(-position)?|(-\w+-)?perspective-origin)$/i` and
rewrites position-keyword pairs to length values per upstream rules
(`left top` → `0 0`, `right bottom` → `100% 100%`, `top right` →
`100% 0`, etc.). `var()`/`env()`/`constant()` short-circuits the
current background entry; `/` defers to background-size.

### What landed this session

1. `crates/cssnano-postcss-normalize-positions/src/lib.rs` — full port
   of `node_modules/postcss-normalize-positions@5.1.1/src/index.js`.
   Single source file maps 1:1.
2. `crates/cssnano-postcss-normalize-positions/Cargo.toml` — added
   `indexmap`, `regex`, `once_cell` workspace deps. Pre-existing
   `postcss-core` and `postcss-value-parser` already declared.
3. Stage + bridge wiring: `Stage::PostcssNormalizePositions`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-positions: 5.1.1`),
   `packages/css/package.json` devDependency.
4. Sparse-array semantics replicated via `Vec<Option<Range>>` so
   `forEach` skips holes (matches the JS sparse-array case where a
   layer's range slot is never populated, e.g. when an entry hits `/`
   before any position keyword).
5. `parseFloat(value)` not-NaN equivalence implemented as
   `parse_unit(value).is_some()` — `like_number`'s prefix check
   matches JS `parseFloat`'s "valid leading numeric" semantics
   exactly.
6. 20-fixture corpus covering: blank, no-position decls, `left top`,
   `right bottom`, vertical-first swap, center pair collapse, single
   keyword, `var()` short-circuit, `/` background-size guard, comma
   layer reset, three-value-skip rule, dimensions/numbers, calc/min/
   max math fns, vendor `-webkit-`/`-moz-perspective-origin`,
   `!important`, atrule nesting, uppercase keywords, mixed
   keyword+dimension, realistic atomic CSS, empty value.

### Verification gates run

| Gate                                                                    | Status |
|-------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-positions`                     | 12/12 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)     | 410/410 pass |
| `parity-runner --stage postcss-normalize-positions --corpus ...`        | 20/20 byte-clean |
| `parity-runner --stage postcss-normalize-positions ... --determinism`   | 20/20 deterministic |
| `parity-runner --stage postcss-normalize-string ...`                    | 15/15 (no regression) |
| `parity-runner --stage postcss-discard-comments ...`                    | 15/15 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`  | 12/12 (no regression) |

## Phase 6b ship — `postcss-normalize-timing-functions@5.1.0` byte-clean

cssnano sub-plugin. Walks decls matching
`/^(-\w+-)?(animation|transition)(-timing-function)?$/i` and rewrites
timing functions to keyword equivalents:

- `cubic-bezier(0.25, 0.1, 0.25, 1)` → `ease`.
- `cubic-bezier(0, 0, 1, 1)` → `linear`.
- `cubic-bezier(0.42, 0, 1, 1)` → `ease-in`.
- `cubic-bezier(0, 0, 0.58, 1)` → `ease-out`.
- `cubic-bezier(0.42, 0, 0.58, 1)` → `ease-in-out`.
- `steps(1, start | jump-start)` → `step-start`.
- `steps(1, end | jump-end)` → `step-end`.
- `steps(N, end | jump-end)` → `steps(N)` (browser default).

### What landed this session

1. `crates/cssnano-postcss-normalize-timing-functions/src/lib.rs` —
   full port of `node_modules/postcss-normalize-timing-functions@5.1.0/
   src/index.js`. Single source file maps 1:1.
2. `Cargo.toml` — added `indexmap`, `regex`, `once_cell` workspace
   deps. Pre-existing `postcss-core` and `postcss-value-parser` already
   declared.
3. JS `parseFloat(s)` parity implemented as
   `parse_unit(s).map(|u| u.number.parse::<f64>().ok()).flatten()` —
   `like_number`'s prefix check matches parseFloat's "valid leading
   numeric" semantics exactly (handles `"0.25"`, `".25"`, `"1px"`,
   `"1e-2"`, etc.).
4. cubic-bezier conversion-table key built via
   `js_number_to_string(v)` joined by `,` — exact 1:1 with JS
   `[a,b,c,d].toString()`. Uses the existing postcss-core helper
   instead of a bespoke formatter.
5. Stage + bridge wiring: `Stage::PostcssNormalizeTimingFunctions`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-timing-functions: 5.1.0`),
   `packages/css/package.json` devDependency.
6. 21-fixture corpus covering: blank, no-timing decls, all 5
   cubic-bezier→keyword conversions, unknown-bezier passthrough,
   `steps(1, start|jump-start)` → step-start, `steps(1, end|jump-end)`
   → step-end, `steps(N, end|jump-end)` → `steps(N)`,
   `steps(N)` (already minimal), keyword passthrough (ease/linear/
   step-start), comma-list (multiple bezier+steps), transition
   shorthand inside `transition` / `animation`, vendor prefixes
   (`-webkit-`/`-moz-`/`-ms-`), uppercase keywords/property names,
   `!important`, atrule nesting (`@media`/`@keyframes`), realistic
   atomic CSS, decimal variants (`.25` vs `0.25`, `1.0` vs `1`).

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-timing-functions`                      | 15/15 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)             | 425/425 pass |
| `parity-runner --stage postcss-normalize-timing-functions --corpus ...`         | 21/21 byte-clean |
| `parity-runner --stage postcss-normalize-timing-functions ... --determinism`    | 21/21 deterministic |
| `parity-runner --stage postcss-normalize-positions ...`                         | 20/20 (no regression) |
| `parity-runner --stage postcss-normalize-string ...`                            | 15/15 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

## Phase 6b ship — `postcss-normalize-url@5.1.0` byte-clean

cssnano sub-plugin. Walks every Decl value and `@namespace` AtRule params;
rewrites `url(...)` calls. Absolute / protocol-relative URLs route through
`normalize-url@6.1.0` (vendored — WHATWG URL canonicalization, default-port
strip, `utm_*` query removal, etc.). Relative paths route through Node's
`path.normalize` (host-OS dependent — see "Lessons from Phase 6b normalize-url"
below). `data:`/`*-extension:/` short-circuit conversion.

The 5 postcss-side overrides on top of normalize-url's defaults:
`normalizeProtocol` / `sortQueryParameters` / `stripHash` / `stripWWW` /
`stripTextFragment` all `false`.

### What landed this session

1. `crates/cssnano-postcss-normalize-url/src/lib.rs` — main port of
   `node_modules/postcss-normalize-url@5.1.0/src/index.js`. Single source
   file maps 1:1.
2. `crates/cssnano-postcss-normalize-url/src/normalize_url.rs` — vendored
   port of `node_modules/normalize-url@6.1.0/index.js`. The `normalize-url`
   npm package is a single-consumer dep (only `postcss-normalize-url`
   uses it on our hashing path) so it lives inside this crate as a sibling
   module rather than a single-consumer crate. WHATWG URL parsing
   delegates to the Rust `url@2.5` crate (also WHATWG-compliant).
3. `crates/cssnano-postcss-normalize-url/src/path.rs` — vendored subset of
   Node `lib/path.js` (`path.posix.normalize` plus a Win32 wrapper). The
   `\` → `/` separator unification on Windows replicates upstream
   `path.win32.normalize(...).replace(/\\/g, '/')` for inputs without
   drive letters / UNC prefixes (which upstream's `WINDOWS_PATH_REGEX`
   filters out before convert() runs).
4. `Cargo.toml` — added `url@2`, `percent-encoding@2`, plus the standard
   `indexmap`/`regex`/`once_cell` workspace deps.
5. Stage + bridge wiring: `Stage::PostcssNormalizeUrl`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-url: 5.1.0`), `packages/css/package.json` devDep.
6. 60-fixture corpus covering: blank, no-url decls, unquoted/quoted
   relative paths, root-relative, absolute http/https, default-port
   strip (`:80`/`:443`), `utm_*` query strip, data URIs (svg+xml,
   base64, default mime/charset), `chrome-extension:`/`moz-extension:`
   short-circuit, empty `url()`, `..` collapse in relative + absolute
   paths, `@namespace url(...)` rewrite (single + multi-quoted),
   protocol-relative (`//cdn`), spaces in quoted URLs, escaped newline
   inside string, escaped quote inside string (Win32 `\` → `/` path),
   parens in quoted URLs, multiple `url()` per decl, `url()` inside
   `@media`/`@keyframes`/`@supports`, uppercase `URL`/`DATA:`/
   `MOZ-EXTENSION://`, single quotes, percent-encoded path,
   text-fragment (`#:~:text=...`) — kept since stripTextFragment=false,
   `www.` host — kept since stripWWW=false, hash-only fragments,
   `?`-only query, comment-in-value neighbors, no-protocol/no-quotes
   (`url(example.com/path)`), realistic atomic CSS combo.

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-url`                                   | 33/33 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)             | 458/458 pass |
| `parity-runner --stage postcss-normalize-url --corpus ...`                      | 60/60 byte-clean |
| `parity-runner --stage postcss-normalize-url ... --determinism`                 | 60/60 deterministic |
| `parity-runner --stage postcss-normalize-timing-functions ...`                  | 21/21 (no regression) |
| `parity-runner --stage postcss-normalize-positions ...`                         | 20/20 (no regression) |
| `parity-runner --stage postcss-normalize-string ...`                            | 15/15 (no regression) |
| `parity-runner --stage postcss-discard-comments ...`                            | 15/15 (no regression) |
| `parity-runner --stage postcss-normalize-whitespace ...`                        | 22/22 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

### Lessons from Phase 6b `normalize-url` — apply to every future port

- **Upstream "moderate" is misleading when transitive deps matter.** The
  upstream JS file is ~150 LOC. But it pulls in `normalize-url@6.1.0`
  (~220 LOC + WHATWG URL parser) AND Node `path.normalize`. Real port
  size: ~3 source files, ~750 LOC total. **Always audit transitive deps
  before declaring "moderate".**
- **`require('path')` is OS-dependent.** `path.normalize('foo\\"bar.png')`
  on Windows returns a `\`-replaced output that the upstream then
  forward-slash-converts. POSIX returns the input unchanged. Replicating
  bug-for-bug means the Rust port also splits via `cfg(windows)`. Same
  CSS input → different bytes on Linux vs Windows. The user's monorepo
  builds on Linux, so production hashes are POSIX-shaped; the Windows
  build is for dev parity testing only. **Document host-OS dependencies
  prominently** because they propagate to consumer hashes.
- **Rust `url@2.5` ≈ JS `new URL(...)` for our inputs.** Both implement
  WHATWG URL canonicalization. For 60 corpus entries spanning realistic
  CSS URL inputs, byte-equal serialization. Edge cases with unusual
  schemes or invalid characters MAY differ — corpus coverage is the
  only safety net. Add new fixtures for any URL pattern the consuming
  monorepo produces.
- **`Lazy<Regex>` for ALL upstream regexes — including ones that look
  trivial.** The escape-chars regex `/([\s\(\)"'])/g` is hot path; a
  per-call `Regex::new` would re-compile per `url()`. Caching via
  `once_cell::Lazy` is mandatory for performance, but also clarifies
  the regex source location for byte-parity audits.
- **Lookbehind/lookahead patterns require manual emulation.** Rust
  `regex` rejects `(?<!...)` and `(?!...)`. The `collapse_path_slashes`
  function and the `^(?!(?:\w+:)?\/\/)|^\/\/` protocol-prepend regex
  are both hand-implemented. Cover these explicitly in unit tests.
- **`url::Url::set_query(None)` vs `set_query(Some(""))` matter.** The
  former drops the `?` entirely; the latter keeps it. Upstream's
  `searchParams` mutations can leave a trailing `?` when all params are
  filtered. Match upstream behavior carefully — `searchParams.delete`
  removes pairs but leaves the `?`; assigning `urlObj.search = ''`
  drops it. Both arise in our port.

### Lessons from Phase 6a / 6b — apply to every future port

- **`lib/` subdirectories collide with Rust crate-root naming.** When
  upstream nests sources under `src/lib/<name>.js`, the only
  Rust-legal layout that keeps a 1:1 file map is to flatten the
  `lib/` parent into the crate root and document the deviation in
  each module header.
- **Bug-for-bug fall-through bugs are real.** `postcss-normalize-string`
  has an intentional missing `break` in the switch statement
  (backslash + non-quote → falls through into the default word
  branch). Replicate verbatim, do not "clean up" — class hashes
  downstream depend on byte-equivalent string output.
- **The `replaceComments` separator argument is load-bearing.** For
  rule selectors, separator is `''` (empty), not the default `' '`.
  This can cause previously-separated selector tokens to JOIN when a
  comment lived between them. Replicate exactly — minified selectors
  rely on this.
- **JS bridge dependency resolution:** plugins not in
  `@sjcompiled/css`'s direct deps must be added to its
  `devDependencies` (not just root `overrides`) so
  `parity-bridge.mjs` can `import` them. Without the package-level
  dep, bun's `node_modules` doesn't expose the package to the bridge
  script and `JS bridge closed unexpectedly` errors every fixture.

### Lessons from Phase 5b — apply to every future port

- **ECMAScript `\s` ≠ Rust regex `\s` ≠ Unicode `White_Space`.** JS
  `\s` includes U+FEFF (BOM) but excludes U+0085 (NEL); Rust `\s`
  (Unicode mode) is `\p{White_Space}` which is the opposite. The
  IE9-hack regex hand-rolls the JS character class explicitly so
  parity holds for any input that uses BOMs/NEL inside CSS values.
  Same for the per-character `replace(/\s/g, '')` calls — we ship a
  bespoke `is_es_whitespace` predicate, not `char::is_whitespace`.
- **`variableFunctions` exemption is shallow.** `var()` / `env()` /
  `constant()` keep their *Function*-level `before`/`after` raws, but
  the default reducer recursion still descends and clears Div nodes'
  `before`/`after` inside. So `var( --x , red )` becomes
  `var( --x,red )`, not `var( --x , red )`. Test for this exact
  shape.
- **The IE9-hack regex has NO `g` flag.** First match only. Fixtures
  that have two `\9` occurrences in the same value would only normalize
  the first — replicate the bug, do not "fix" it.
- **OnceExit-only plugins compose into single-plugin pipelines as a
  direct function call.** No per-node visitor / no `Once` hook means
  postcss runs the function once on the parsed root. The Phase 8a
  lifecycle warning still applies: when this plugin lands in the same
  pipeline as another plugin's `Once` / per-node visitor, the
  OnceExit ordering matters and array order ≠ execution order.

### Lessons from Phase 8a — apply to every future port

- **Lifecycle ordering matters.** Read `Phase 8a ship` →
  "Lifecycle ordering — load-bearing" before composing plugins in
  any orchestrator. Array order ≠ execution order. The bug is silent
  and only manifests when multiple plugins share root.
- **`container::append` on Root applies Root.normalize.** Direct
  `Vec::push` on `root.nodes` skips the raws-transfer and produces
  missing-newline drift. Always go through `container::append`.
- **`bun.lock` silently floats past `^X.Y.Z` pins.** Audit every
  pinned version in `PARITY_VERSIONS.md` against the actual
  `node_modules/.bun/` contents before declaring byte-clean. The
  `postcss-discard-duplicates` `^6.0.0 → 6.0.3` drift is one example;
  there are likely others.
- **A new stage needs three coordinated additions:** the Rust
  `Stage::*` variant in `crates/parity-runner/src/stages.rs`, the
  CLI mapping in `main.rs`, and the JS counterpart in
  `packages/css/scripts/parity-bridge.mjs`. Forgetting the JS side
  silently produces "no diff" because both sides hit the unknown-stage
  error path.

## Cardinal-rule conformance check

- ✅ Every Rust crate header names the JS package + version it ports.
- ✅ Every Rust file maps 1:1 to a JS source file in upstream.
- ✅ `IndexMap` used everywhere a HashMap would touch output bytes.
- ✅ No version bumps applied to any pinned package.
- ✅ JS pipeline in `packages/css/src/transform.ts` untouched — Rust
  is additive.
- ⚠️ JS pipeline in `packages/css/src/sort.ts` — modified additively
  to add the `COMPILED_CSS_ENGINE=rust` env-flag gate. Default path
  (flag unset, JS pipeline) is byte-identical to pre-modification
  behavior. JS code stays as the parity oracle.
- ✅ Parity-runner harness wired for every implemented plugin.
- ✅ The CRITICAL hash plugin (`atomicify-rules`) is byte-clean.
- ✅ JS oracle versions match `REFERENCE_LOCK_FILE/yarn.lock` exactly
  for every byte-affecting dep (postcss 8.4.31, postcss-selector-parser
  6.0.13, postcss-discard-duplicates 6.0.0, autoprefixer 10.4.14, etc.)
  via root `package.json` `overrides` block. See "Parity-contract drift
  — RESOLVED" above for the audit table.
