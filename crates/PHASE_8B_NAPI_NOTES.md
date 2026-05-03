# Phase 8b — NAPI agent write-up

> Wires the lifecycle-correct `crates/css::transform::transform_css` body
> (composed in Phase 8b's previous step) through `napi-rs` into
> `@sjcompiled/css-native`, splices it into `packages/css/src/transform.ts`
> behind a `COMPILED_CSS_ENGINE=rust` flag, and lands the byte-equality
> parity gate (`Stage::TransformCss` + corpus + verifier).
>
> Read with `crates/PHASE_8B_LIFECYCLE_AUDIT.md` (the spec),
> `crates/PHASE_8B_COMPOSE_NOTES.md` (the Rust composition shape),
> `crates/STATUS.md` (cross-phase context), and `crates/EXECUTION_PLAN.md`.

## NAPI surface added (`crates/compiled-css-napi/src/lib.rs`)

```ts
export interface TransformOpts {
  optimizeCss?: boolean;
  classNameCompressionMap?: Record<string, string>;
  increaseSpecificity?: boolean;
  sortAtRules?: boolean;
  sortShorthand?: boolean;
  classHashPrefix?: string;
}
export interface TransformResult {
  sheets: string[];
  classNames: string[];
}
export function transformCss(css: string, opts?: TransformOpts | null): TransformResult;
```

Mirrors the Phase 8a `sort()` and Phase 8b `autoprefixer()` exports
verbatim. New marshalling concerns:

- **`classNameCompressionMap` insertion order.** napi-rs's
  `#[napi(object)]` derive maps JS objects to `HashMap` by default —
  which would shuffle iteration order and silently break atomicify's
  for-in lookup semantics. The shim instead receives the map as
  `Option<JsObject>` and walks `get_property_names()` (V8-spec own-
  enumeration order, matches JS `Object.keys()`), building an
  `IndexMap<String, String>` that preserves insertion order. Verified
  byte-clean by the `Stage::TransformCss` corpus's
  `realistic_atomic_combo` and `full_combo` fixtures.
  See `jsobject_to_indexmap` helper in `crates/compiled-css-napi/src/lib.rs`.

- **Two callback channels collapse to two return-vec fields.**
  `atomicifyRules`'s `callback: (className) => classNames.push(...)` and
  `extractStyleSheets`'s `callback: (sheet) => sheets.push(...)` are
  private to JS `transformCss` — they never leak to callers. The Rust
  port collects both into `Vec<String>` on `TransformResult`. The NAPI
  shim returns these vecs as JS arrays directly (`Vec<String>` →
  `string[]` via napi-rs's standard marshalling). No callback marshalling
  needed; verified against open question #1 in `PHASE_8B_COMPOSE_NOTES.md`.

- **Error wrapping.** Rust returns `Result<_, String>`; the NAPI shim
  maps to `napi::Error::from_reason`. The JS-side `transform.ts` wrapper
  re-wraps thrown errors in the upstream `createError('css', 'Unhandled
  exception')` envelope, so consumers calling either engine see the
  same envelope format. (Open question #2 in the compose notes resolved
  by mirroring upstream's wrap-at-call-site pattern in `transform.ts`.)

Build artifact: `crates/target/debug/compiled_css_napi.dll` copied into
`packages/css-native/sjcompiled-css.win32-x64-msvc.node` (dev mode —
release mode OOMs per the `Cargo.toml` warning header). Build command:
`RUSTFLAGS="" cargo build -p compiled-css-napi`. The `RUSTFLAGS=""`
prefix neutralises a user-level
`RUSTFLAGS="-C lto=thin"` that breaks proc-macro compilation.

## `transform.ts` engine-flag patch

`packages/css/src/transform.ts:32-100` was the JS oracle and is the one
file under `packages/css/**` whose IMMUTABLE rule relaxes for Phase 8b
per `crates/autoprefixer/AGENT_6_DONE.md` and the audit. The patch:

- **Lines 32-71** (after the `transformCss` arrow-function opening at
  line 32): inserts a `process.env.COMPILED_CSS_ENGINE === 'rust'` gate
  that delegates to `require('@sjcompiled/css-native').transformCss`,
  threading the JS opts shape through verbatim. The Rust shim returns
  `{ sheets: string[]; classNames: string[] }` directly — no
  re-shape needed.
- **Lines 60-70** (inside the rust branch): a try/catch re-wraps any
  thrown Rust error in the same `createError('css', 'Unhandled
  exception')` envelope used by the JS pipeline below. Consumers see
  identical error shape on both engines.
- **The JS pipeline below the gate is UNCHANGED.** It stays as the
  parity oracle and emergency fallback for the next 12+ months per
  EXECUTION_PLAN Phase 10d. No deletes, no restructure.

The flag default is JS (gate only fires on exact string match against
`'rust'`), matching the Phase 8a `sort` precedent — except, see
"Drift detected" §1 below: Phase 8a's `sort.ts` actually never landed
the engine flag in production source. The Phase 8b transform.ts patch
is therefore the FIRST production-source engine-flag landing, even
though the verify-engine-flag.mjs harness referenced the flag earlier.

## Corpus (`crates/parity-runner/corpus/transform-css/`)

30 fixtures covering each major lifecycle hook plus realistic combos
plus edge cases:

| # | Fixture | Lifecycle hook(s) exercised |
|--:|---------|-----------------------------|
| 01 | blank | empty-input fast path |
| 02 | single_decl | atomicify single decl |
| 03 | single_rule | atomicify single rule |
| 04 | discard_duplicates | discardDuplicates.Once |
| 05 | parent_orphaned_pseudos | parentOrphanedPseudos.Once |
| 06 | postcss_nested_basic | postcss-nested.Rule (walk round) |
| 07 | postcss_nested_media_bubble | postcss-nested with @media bubble |
| 08 | expand_shorthand_margin | expandShorthands 4-longform expansion |
| 09 | expand_shorthand_padding | expandShorthands single-arg expansion |
| 10 | expand_shorthand_background | expandShorthands `background:` color path |
| 11 | atomicify_multi_decls | per-decl class emission |
| 12 | atomicify_with_selector | selector context preserved |
| 13 | atomicify_pseudo | atomicify `&:hover` |
| 14 | autoprefixer_user_select | autoprefixer prefix emission |
| 15 | normalize_whitespace | postcss-normalize-whitespace OnceExit |
| 16 | extract_multi_rules | extractStyleSheets multi-sheet split |
| 17 | at_media | @media at-rule passthrough |
| 18 | at_supports | @supports at-rule passthrough |
| 19 | at_layer | @layer (in postcss-nested bubble list) |
| 20 | var_bailout | expandShorthands var() bailout (audit Plugin 6 mutation) |
| 21 | currentcolor_canonicalization | normalize-current-color walk-round visitor |
| 22 | comments_at_positions | comments interleaved between decls |
| 23 | deeply_nested | postcss-nested 3-level depth |
| 24 | realistic_atomic_combo | colormin (#ff0000→red), shorthand expansion, autoprefix |
| 25 | empty_decl_dropped | discardEmptyRules walk-round visitor |
| 26 | flex_shorthand | expandShorthands `flex:` |
| 27 | text_decoration | expandShorthands `text-decoration:` |
| 28 | outline_shorthand | expandShorthands `outline:` |
| 29 | calc_value | postcss-calc OnceExit reduction |
| 30 | full_combo | atomic-CSS realistic combo with media + nesting |

No `.opts.json` files — every fixture runs against
`TransformOpts::default()` (matching the bridge's `transformCss(css, {})`
call). The conditional gates (`optimizeCss=false`, `increaseSpecificity=true`,
`AUTOPREFIXER=off`) are covered by the `crates/css::transform_css` unit
tests (`optimize_css_false_skips_*`, `increase_specificity_*`,
`autoprefixer_*` — see `crates/css/src/transform.rs:570-700`). The
parity-runner corpus is intentionally focused on the default-opts path
because that's the production AFM hot path; opts-toggle parity is
covered by the unit-test layer.

## Verifier script (`packages/css/scripts/verify-napi-transform-css.mjs`)

Mirrors `verify-napi-sort.mjs` and `verify-napi-autoprefixer.mjs`:
imports both engines, iterates the corpus, byte-compares
`JSON.stringify({sheets, classNames})` outputs, prints the smallest
divergent byte range with surrounding context, exits non-zero on any
divergence. Pins `BROWSERSLIST=chrome 100` and clears
`AUTOPREFIXER`/`COMPILED_CSS_ENGINE` for the duration of the run
(restored on exit).

Run via `bun run packages/css/scripts/verify-napi-transform-css.mjs`.

## Gate results

| Gate | Result |
|------|--------|
| `cargo test --workspace --no-fail-fast` | **1224 passed, 0 failed, 2 ignored** (no regressions; `--workspace` covers all 30+ crates) |
| `cargo run -p parity-runner -- --stage transform-css --corpus crates/parity-runner/corpus/transform-css` | **29/30 byte-clean (JS vs Rust)** — see "Drift detected" §2 below |
| `cargo run -p parity-runner -- --stage transform-css --corpus crates/parity-runner/corpus/transform-css --determinism` | **30/30 deterministic (JS oracle stable across two spawns)** |
| `bun run packages/css/scripts/verify-napi-transform-css.mjs` | **29/30 byte-clean (JS vs Rust NAPI)** — same fixture as above; consistent with the parity-runner result |

The 30/30 determinism is the load-bearing one for hash stability —
the JS oracle is byte-stable across spawns, so the parity contract is
"Rust must match this fixed oracle." The 29/30 byte-clean is the JS-vs-
Rust diff; the one drift is documented below and is OUTSIDE Phase 8b's
composition responsibility.

## Drift detected

### 1. `packages/css/src/sort.ts` never landed `COMPILED_CSS_ENGINE` flag (Phase 8a artifact)

`packages/css/scripts/verify-engine-flag.mjs` and
`packages/css/scripts/_engine-bridge.mjs` reference the
`COMPILED_CSS_ENGINE` flag and assume it gates `sort.ts`. Grep confirms
`packages/css/src/sort.ts` does NOT contain any
`process.env.COMPILED_CSS_ENGINE` check — the flag was wired into the
verify harness but never landed in production source. The `verify-
engine-flag.mjs` script therefore tests JS-vs-JS (since both env-var
settings hit the JS pipeline).

**Drift detected in packages/css/src/sort.ts** — Phase 8a appears to
have shipped the NAPI export and the verify harness but skipped the
production-source flag splice. Per CLAUDE.md the file is IMMUTABLE and
Phase 8b's relaxation only applies to `transform.ts`, so I did NOT
touch `sort.ts` here. Escalate to the user / next Phase 8a follow-up
to decide whether `sort.ts` should also receive the flag splice (would
require a CLAUDE.md exception or a Phase 8a re-open).

### 2. `crates/compiled-css::sort_atomic_style_sheet` mis-orders decls when comments interleave

`crates/parity-runner/corpus/transform-css/22_comments_at_positions.css`:

```css
/* leading */
color: red;
/* between */
background: blue;
/* trailing */
```

JS `sortAtomicStyleSheet.Once` reorders decls by shorthand-bucket
priority, producing `background, color` (and shoves comments to the
end of the catchAll bucket):

```text
/* leading */
background: blue;
color: red;
/* between */
/* trailing */
```

Rust `sort_atomic_style_sheet` keeps the original `color, background`
order. After the rest of the pipeline runs, the byte difference
surfaces as different sheet/className ordering in the JSON output.

I confirmed by running the existing `Stage::SortAtomicStyleSheet`
parity gate against this fixture in isolation (drove the fixture
through the bridge directly): the `sort-atomic-style-sheet` stage
also diverges at byte 14. So the drift is in
`crates/compiled-css/src/plugins/sort_atomic_style_sheet.rs` (not in
`crates/css/src/transform.rs`'s composition).

The existing
`crates/parity-runner/corpus/sort-atomic-style-sheet/` corpus does NOT
include any fixture mixing top-level comments with top-level decls —
the closest is `09_decls_at_root.css` which has decls but no comments.
**This is a previously-undiscovered drift case**, surfaced by Phase 8b's
broader corpus.

**Drift detected in `crates/compiled-css/src/plugins/sort_atomic_style_sheet.rs`** —
catchAll partition or sibling-reorder logic does not preserve JS's
behaviour when top-level comments interleave with top-level decls. The
visible symptom is decl ordering that does not match shorthand-bucket
priority when comments are interspersed.

Per CLAUDE.md drift-detection rules, I have NOT patched this in the
NAPI shim or in `transform.ts` — that would only add more drift.
Reporting and stopping for the user / a follow-up agent to decide
which crate to fix. The Phase 8b corpus retains the failing fixture
as evidence; the gate result therefore reads 29/30 honestly. When
the underlying drift is fixed in `sort_atomic_style_sheet`, the
fixture should pass without any change to Phase 8b's NAPI / verifier /
corpus layout.

## What's left for Phase 9

- **Corpus replay at scale.** The 30-fixture corpus is a hand-built
  sanity gate. Phase 9 should drive the AFM `__tests__` snapshot
  outputs (and/or a sample of production AFM atomic CSS bundles)
  through the same NAPI path and assert byte-clean against the JS
  oracle. The verifier script's structure scales: just point it at a
  larger corpus dir.
- **`cargo-fuzz` harness.** A randomised CSS generator + property-
  test harness (input → both engines → assert equal) would catch
  any drift the corpus misses. Adapt the existing parity-runner
  bridge to feed fuzz-generated CSS instead of file-based fixtures.
- **Shadow runs.** Once AFM consumes `@sjcompiled/css-native` behind
  the engine flag in production, run JS as the canonical engine and
  Rust as a shadow on every build, asserting byte equality on the
  generated `sheets` and `classNames` arrays. Any divergence emits
  a non-fatal warning until confidence is built; then the flag flips.
- **Resolve drift §1 (sort.ts engine flag missing).** Decide whether
  the Phase 8a relaxation should be retroactively applied to
  `packages/css/src/sort.ts`, or whether `verify-engine-flag.mjs`
  should be repurposed to test `transform.ts` instead.
- **Resolve drift §2 (sort_atomic_style_sheet mis-orders comment-
  interleaved decls).** Fix `crates/compiled-css/src/plugins/
  sort_atomic_style_sheet.rs` to match JS's catchAll-with-comments
  behaviour. Re-run the Phase 8b parity gate; expect 30/30 once
  the fix lands.
- **Release-mode build.** The `compiled-css-napi` Cargo.toml warning
  block calls out an OOM during release-mode codegen on Windows
  dev boxes < 32 GB RAM. The shipped `.node` is dev-mode; bytes-out
  are byte-identical between dev and release per Phase 8a's
  `verify-napi-autoprefixer` gate. Phase 8c should land the per-
  package release-profile overrides at the workspace root.
