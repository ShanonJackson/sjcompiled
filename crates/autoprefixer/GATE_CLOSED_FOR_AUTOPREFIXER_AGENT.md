# Gate closed — read this before your next session

> **TL;DR for the impatient:** the `oxc_browserslist` snapshot drift gate
> from your prior `HANDOVER.md` §6 / `MORNING.md` Drift #1 is **closed
> for AFM's actual surface**. `Browsers::new(...)` now returns
> byte-correct `selected` for AFM's `.browserslistrc`. `Prefixes::new`
> is unblocked. New floor is **60 passing, 0 ignored** (was 58 + 1
> ignored). Read §6 of the updated `HANDOVER.md` and
> `crates/browserslist-shim/AFM_PORT_NOTES.md` before touching anything.

---

## Why this exists

You opened the previous session with the gate `#[ignore]`'d at
`crates/autoprefixer/tests/browserslist_parity.rs::browserslist_shim_matches_js_oracle_for_canonical_queries`.
Your `MORNING.md` recommended Option A (close the shim parity gate)
because every downstream byte-test against the JS oracle was effectively
poisoned: `Browsers::new` consumed `browserslist_shim::resolve` which
delegated to `oxc_browserslist`, and oxc's bundled caniuse-lite snapshot
ran ~2 chrome releases ahead of our `1.0.30001766` pin. For "current
versions" queries (`defaults`, `> 1%`, `last 2 versions`, `chrome >= 50`)
the lists silently drifted.

That work has now landed. This document tells you what changed, why it
was scoped the way it was, what you can rely on, and what you must NOT
do. It's a delta on top of `HANDOVER.md` — not a replacement.

---

## What we knew before starting

The closure had three options on the table (your `HANDOVER.md` §6):

- **(a)** Inject our `caniuse-db` snapshot into `oxc_browserslist`. Multi-day, requires upstream PR or fork. Bundled `.bin.deflate` blobs make data injection painful. Rejected.
- **(b)** Re-port `browserslist@4.24.2`'s `index.js::resolve` against `caniuse-db` directly. Cleanest deterministic path. Originally estimated multi-day.
- **(c)** Downgrade `oxc-browserslist` to a version (`2.3.0`) whose bundled snapshot has `chrome 144`. Looks closer but still drifts in caniuse-lite usage percentages, which percentage-thresholded queries (`> 1%`, `defaults`) read. Rejected as false progress.

We picked **(b)**, then descoped it dramatically once AFM's dependency
engineer ran runtime instrumentation through the actual `jira/` build
pipeline. The instrumentation report lives at
`BROWSER_LIST_FROM_AFM.md` (workspace root). The actionable content:

- `@compiled/css@0.19.0` calls `autoprefixer()` with no args.
- Autoprefixer calls `browserslist(null, { path: cwd })`.
- cwd at build time is `/home/ubuntu/atlassian-frontend-monorepo/jira/`.
- The walk lands on `jira/.browserslistrc`, SHA256
  `08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`.
- That file contains six lines of `last N <browser> version[s]?` atoms
  AND NOTHING ELSE — no `> X%`, no `defaults`, no `dead`, no
  `mobileToDesktop` opt, no `[production]` / `[development]` sections,
  no env vars.

So the byte-correct port surface for AFM was effectively **one query
clause type plus browser-name aliasing**. That collapsed option (b) from
a multi-day full-resolver port into a one-session focused unit.

---

## The architecture we landed

**Hybrid AFM-fast-path / oxc-fallback resolver** in
`crates/browserslist-shim/`. Two paths, picked automatically per call:

1. **AFM fast path** — every atom in the resolved query parses against
   the AFM grammar (`crates/browserslist-shim/src/parse.rs::try_parse_atom_afm`).
   Resolves against `caniuse-db@1.0.30001766` directly. Byte-correct
   against the AFM-pinned snapshot. This is the path AFM's runtime call
   takes.
2. **`oxc_browserslist` fallback** — at least one atom is outside the
   AFM grammar. Defers to `browserslist::resolve` (re-exported by
   `oxc-browserslist@3.0.2`). Drift-tolerant; unchanged from
   pre-closure behaviour. This is the path Phase 6 cssnano consumers
   take (`postcss-normalize-unicode`'s `> 0.5%` defaults +
   `ie <=11, edge <= 15` legacy query, `caniuse-api::clean_browsers_list`'s
   user queries, etc.) — their output reduces to drift-stable booleans
   so the snapshot drift never reaches hash bytes.

The `try_parse_all_afm` predicate is **unanimous-or-none**: if any
atom is unrecognised, the whole query routes to the fallback. Partial
mixes would silently drift — we don't allow them.

### The AFM grammar (today)

Two `QueryAtom` variants in `parse.rs`:

| Variant | Matches | Used by |
|---|---|---|
| `LastNBrowserVersions { n, browser }` | `last N <browser> version[s]?` (case-insensitive) | AFM's `.browserslistrc` (the only atom present) |
| `BrowserVersion { browser, version }` | `<browser> <version>` literal (e.g. `firefox 115`) | Output of the Firefox ESR rewrite |

Browser names are canonicalised at parse time per `browserslist@4.24.2`'s
`aliases` map: `Edge → edge`, `iOS → ios_saf`, `ChromeAndroid → and_chr`,
`FF/fx → firefox`, `explorer → ie`, etc.

### The released-versions semantics

`LastNBrowserVersions` resolves against caniuse-db's `Agent.versions`
filtered two ways:
1. Strip `None` placeholders (caniuse-lite stores future planned
   slots as nulls).
2. Strip entries whose `release_date` is `None` (versions that exist in
   the table but haven't shipped yet).

This mirrors what browserslist's JS does internally via its unpacker's
`data.released` field. Without step 2, `last 5 chrome version` would
return `chrome 147..143` instead of `chrome 144..140` (the snapshot
tracks future versions 145, 146, 147 with `release_date: null`).

### Sort order

Final sort mirrors `browserslist@4.24.2` `index.js:431-444`:
- Same browser → descending semver (compareSemver).
- Different browser → ascending name lexicographic.
- Range entries (e.g. `"18.5-18.7"`) compare by lower bound.

### The Firefox ESR override

`Firefox ESR` (and aliases `ff esr`, `fx esr`) is rewritten BEFORE the
fast-path-vs-fallback decision into the literal pair `firefox 115,
firefox 128` (per `browserslist@4.24.2` `select()` at index.js:1024).
The literals then route through the fast path's `BrowserVersion` atom.

This override exists because both `oxc_browserslist@2.x` and `@3.x`
hardcode `firefox_esr → firefox 140` — different from BOTH 4.24.4 and
4.24.2. Without the rewrite, `Firefox ESR` would silently return the
wrong version.

Two implementations exist (both `pub`):
- `expand_firefox_esr_atom(&str) -> Vec<String>` — per-atom, used
  internally.
- `rewrite_firefox_esr(&str) -> String` — string-level (comma-joined
  input → comma-joined output). Kept for backwards-compat with the
  existing `rewrite_firefox_esr_unit` test contract and any external
  string-based callers.

---

## What changed in autoprefixer specifically

### `crates/autoprefixer/src/browsers.rs::Browsers::parse_static`

Before: ignored `BrowsersOptions::from`, called
`browserslist_shim::resolve_with(query, &Default::default())`.

After: plumbs `from` through as `ResolveOpts::path`, defaulting to
`std::env::current_dir()` when `from` is unset. This mirrors
`browserslist@4.24.2`'s `prepareOpts` (`index.js:366`) which defaults
`opts.path` to `path.resolve('.')`. AFM's call site
(`browserslist(null, { path: cwd })`) now walks up to AFM's
`.browserslistrc` and resolves byte-correctly through the fast path.

The `BrowsersOptions::from` field type didn't change (`Option<String>`).
The struct's other fields didn't change. Constructor signature didn't
change. Public API is unchanged — only the internal `parse_static`
helper grew the path-plumbing.

### `crates/autoprefixer/tests/browserslist_parity.rs`

Three tests, all active, all passing:

| Test | What it does |
|---|---|
| `workspace_browserslist_pin_is_424_2` | Bun loads `browserslist@4.24.2` (pin contract) |
| `browserslist_shim_firefox_esr_matches_js_oracle` | Firefox ESR rewrite agreement bun ↔ Rust shim |
| `browserslist_shim_matches_js_oracle_for_afm_browserslistrc` | **The closure** — bun reads AFM's `.browserslistrc` fixture, Rust shim does the same, lists match element-for-element |

The previously `#[ignore]`'d canonical-queries omnibus is gone. The new
omnibus reads from `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`
(the SAME fixture the shim's own integration test uses) so JS and Rust
are looking at byte-identical input. If the fixture file ever drifts,
the shim's `tests/afm_parity.rs::afm_browserslistrc_fixture_sha256_matches`
catches it independently with a SHA256 assertion.

### `crates/autoprefixer/HANDOVER.md`

- §1 — floor count updated: 58 + 1 ignored → **60 + 0 ignored**.
- §6 — gate-open description rewritten as gate-closed description.
  Architecture summary, what NOT to remove, references to `AFM_PORT_NOTES.md`.
- §2 — corrected stale `caniuse-lite: 1.0.30001690` references to
  `1.0.30001766` (the actual workspace pin per `PARITY_VERSIONS.md`).
  Same in §5. This was pre-existing doc drift, fixed in passing.

### `crates/autoprefixer/MORNING.md`

Overwritten with a fresh handoff. Recommends `Prefixes::new` as Option A
(now unblocked). Documents what NOT to remove. Lists the new floor.

---

## What you can rely on as the autoprefixer agent

1. **`Browsers::new(query, opts)` returns byte-correct `selected` for the
   AFM call site.** When you pass an empty query and `opts.from` either
   set to AFM's path or unset (cwd-defaulted), the shim walks to AFM's
   `.browserslistrc`, parses it, and resolves against `caniuse-db`.
   Output is the frozen 14-entry list AFM's runtime instrumentation
   captured.
2. **Drift gate is active.** If anyone (you, a parallel agent, an
   upstream caniuse-lite repin) breaks the resolver, the parity tests
   panic on the next CI run. You don't have to remember to check it.
3. **Phase 6 cssnano consumers are unaffected.** The fallback path is
   identical to pre-closure behaviour. If something in that area is
   suddenly red, it's not from this work.
4. **The Firefox ESR rewrite still defends against oxc's bundled
   `firefox_esr → firefox 140` divergence.** Don't worry about it
   regressing silently.

## What you CANNOT rely on

1. **Generic `Prefixes::new` against arbitrary queries is NOT byte-clean.**
   Only AFM-shaped queries (`last N <browser> version`) hit the fast path.
   If you write a `Prefixes::new` test that passes `defaults`, `> 1%`,
   `chrome >= 50`, etc. you'll get the oxc fallback's drifted `selected`
   and your byte-test will fail for reasons unrelated to your code.
2. **The fast path doesn't cover region queries, percentage queries, or
   any operator-style atom.** Don't try to extend `Prefixes::new` to
   exercise those — the gate is closed for AFM, not universally.
3. **`mobileToDesktop: true` is NOT supported.** AFM doesn't set it.
   If a future caller needs it, it has to be plumbed through
   `ResolveOpts` and into the resolver explicitly. Don't add it
   speculatively.

---

## What you must NOT do

The user's explicit guidance during landing was to keep the existing
scaffolding intact in case AFM's browserslist changes later. Concretely:

1. **Do not remove `oxc_browserslist` from `crates/browserslist-shim/Cargo.toml`.**
   The fallback is load-bearing for cssnano consumers. Removing it breaks
   `cssnano-postcss-normalize-unicode`, `postcss-colormin`,
   `postcss-minify-params`, `caniuse-api::clean_browsers_list`, and
   anything that hits `browserslist_shim::resolve("", true)` for defaults.

2. **Do not delete `expand_firefox_esr_atom` OR `rewrite_firefox_esr`.**
   Both are `pub`. The string-form is used by the existing
   `rewrite_firefox_esr_unit` test and is the API external string-based
   callers may depend on.

3. **Do not delete `node::default_query`, `node::find_config_file`,
   `node::pick_env`, or `node::parse_package`.** AFM doesn't use the
   defaults / `package.json#browserslist` / sectioned config paths today,
   but they're exercised by the `defaults_resolve` smoke test and may
   be reached by future AFM repo migrations.

4. **Do not widen the AFM grammar opportunistically.** Each new
   `QueryAtom` variant pins more surface to caniuse-db semantics and
   has to be byte-tested. Only port what AFM actually consumes — the
   protocol is in `crates/browserslist-shim/AFM_PORT_NOTES.md` "Adding
   a new atom".

5. **Do not byte-test `Prefixes::new` against arbitrary queries.** Pin
   tests to the AFM `.browserslistrc` fixture via
   `Browsers::new(...)` with `from = Some("...crates/browserslist-shim/tests/fixtures/afm".into())`,
   OR hand-curate `selected` lists for tests that need non-AFM queries
   and bypass `Browsers::new` entirely.

6. **Do not bump `caniuse-lite`.** If `caniuse_db::CANIUSE_LITE_VERSION`
   moves, the AFM-fixture parity test will likely break (it's pinned
   to specific browser versions in the snapshot). That's a hash-rotation
   event for every consumer — not a session-scope change.

7. **Do not edit `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`.**
   It's a byte-copy of AFM's pinned file. Editing it locally fails the
   SHA256 assertion immediately. If AFM updates their config, that's a
   coordinated repin (re-run the protocol in `BROWSER_LIST_FROM_AFM.md`
   to capture the new bytes + new resolved oracle list).

---

## What this DOES NOT close

The closure unblocks `Prefixes::new` for AFM-shaped queries. It does NOT
ship:

- `Prefixes::new` body itself — still `unimplemented!()` in
  `crates/autoprefixer/src/prefixes.rs`. This is your next session's
  unit (your `MORNING.md` Option A).
- `processor.rs` main walk — depends on `Prefixes::new`. ~720 LOC.
- `supports.rs` / `transition.rs` — independent units, lower
  critical-path leverage.
- Parity-runner `Stage::Autoprefixer` — not wired (your `MORNING.md`
  defers this until everything else is done).
- NAPI wire-in to `transform.ts` — same.

---

## Where to read

In priority order:

1. **`crates/autoprefixer/MORNING.md`** — your fresh handoff for next
   session, including recommended unit + sign-off checklist.
2. **`crates/autoprefixer/HANDOVER.md` §1 + §6** — updated floor count
   and gate-closure description.
3. **`crates/browserslist-shim/AFM_PORT_NOTES.md`** — full architecture
   doc for the shim. Read this before touching anything in
   `crates/browserslist-shim/`.
4. **`BROWSER_LIST_FROM_AFM.md`** (workspace root) — AFM dependency
   engineer's runtime-instrumentation report. The source of truth for
   what AFM actually passes to autoprefixer.
5. **`crates/STATUS.md`** — Phase 7 closure entry at the top, includes
   the test-count delta table and source-of-truth pointers.

---

## Files touched in this session

For your audit / blame walks:

| File | Change |
|---|---|
| `crates/browserslist-shim/src/parse.rs` | Replaced scaffold with `QueryAtom` enum + `try_parse_atom_afm` + `try_parse_all_afm` + `canonical_browser_name` + tests |
| `crates/browserslist-shim/src/index.rs` | Added hybrid resolver: `resolve_with` checks AFM grammar, fast-path via caniuse-db or fallback to oxc; new `resolve_last_n_browser_versions`, `resolve_browser_version`, `sort_distribs`, `compare_semver`, `expand_firefox_esr_atom` helpers; preserved `rewrite_firefox_esr` string-form for backwards compat; new `afm_fast_path_*` byte-test units |
| `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` | NEW — byte-copy of AFM's `jira/.browserslistrc`, SHA256-verified |
| `crates/browserslist-shim/tests/afm_parity.rs` | NEW — fixture SHA256 integrity check + end-to-end resolver byte-test against frozen 14-entry oracle. Includes inline pure-Rust SHA-256 |
| `crates/browserslist-shim/AFM_PORT_NOTES.md` | NEW — architecture doc, what NOT to remove, add-an-atom protocol |
| `crates/autoprefixer/src/browsers.rs` | `parse_static` plumbs `BrowsersOptions::from` → `ResolveOpts::path`; defaults to `current_dir()` when unset |
| `crates/autoprefixer/tests/browserslist_parity.rs` | Rewrote: removed `#[ignore]`'d canonical-queries omnibus, added AFM-fixture-driven omnibus; kept `workspace_browserslist_pin_is_424_2` and Firefox ESR drift monitor |
| `crates/autoprefixer/HANDOVER.md` | §1 floor count, §6 gate-closure, §2 + §5 caniuse-lite version typo fix |
| `crates/autoprefixer/MORNING.md` | Overwrote with fresh handoff |
| `crates/STATUS.md` | Added Phase 7 closure entry at top |
| `OUTSTANDING_SHANON.md` | Marked the gate as CLOSED |

Net: **0 failed tests, 0 ignored tests in browserslist-shim or autoprefixer; 93/93 workspace test binaries green.**

Good luck with `Prefixes::new`.
