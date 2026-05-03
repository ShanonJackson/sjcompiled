## Today's session — what landed

**Closed the browserslist-shim parity gate** (the OPEN gate from the
previous MORNING.md / HANDOVER §6 / TaskList #2). The gate is now active
and passing for AFM's actual surface; `Prefixes::new` is unblocked.

Approach picked: **option (b), descoped to AFM's actual query surface**
— the "Recommended" option from the prior planning chain. AFM's
dependency engineer ran runtime instrumentation through the actual
`jira/` build pipeline and reported (in `BROWSER_LIST_FROM_AFM.md`) that:

- `@compiled/css@0.19.0` calls `autoprefixer()` with no args
- Autoprefixer calls `browserslist(null, { path: cwd })`
- That walks up to `jira/.browserslistrc` (SHA256
  `08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`)
- The file contains six lines, all `last N <browser> version[s]?` atoms
- Resolved output: a frozen 14-entry list

That tiny surface let the port collapse from "multi-day full-resolver
re-port" into one session of focused work.

What I did, in order:
1. Read the AFM agent's `BROWSER_LIST_FROM_AFM.md`, the prior
   `MORNING.md`, `HANDOVER.md` §1 §6 §11 §12, and verified pin
   alignment between `crates/PARITY_VERSIONS.md` and AFM (caught and
   resolved a `@compiled/css@0.21.1 vs 0.19.0` false alarm — AFM
   re-confirmed 0.19.0 from the nested install path).
2. Verified floor: `cargo test -p autoprefixer` → 58 passing, 1 ignored
   (53 unit + 4 data parity + 1 active browserslist + 1 ignored omnibus).
3. Implemented hybrid resolver in `crates/browserslist-shim/`:
   - `src/parse.rs` — `QueryAtom` enum, `try_parse_atom_afm` with
     two AFM atoms (`LastNBrowserVersions`, `BrowserVersion`).
   - `src/index.rs::resolve_with` — fast-path on full AFM grammar
     match, oxc fallback otherwise. Per-atom Firefox ESR expansion
     replaces the comma-string rewrite for internal use; the original
     `rewrite_firefox_esr` helper is kept as a `pub fn` for backwards
     compat with the existing test contract.
4. Added `tests/fixtures/afm/.browserslistrc` (byte-copy from AFM,
   SHA256-verified) and `tests/afm_parity.rs` with two integration
   tests: SHA256 fixture-integrity check (with inline pure-Rust
   SHA-256 to avoid a dev-dep) and end-to-end resolver byte-test
   against the frozen 14-entry oracle.
5. Plumbed `path` through `crates/autoprefixer/src/browsers.rs::Browsers::parse_static`
   — defaults to `std::env::current_dir()` when `BrowsersOptions::from`
   is unset (matches `browserslist@4.24.2`'s `prepareOpts` defaulting).
6. Rewrote `crates/autoprefixer/tests/browserslist_parity.rs` — the
   `#[ignore]`'d canonical-queries omnibus is replaced by an
   AFM-fixture-driven omnibus that spawns bun against the SAME fixture
   and compares element-by-element. Active, passing.
7. Wrote `crates/browserslist-shim/AFM_PORT_NOTES.md` capturing
   architecture, "what NOT to remove" (per the user's explicit
   guidance), and the protocol for adding a new atom when AFM's
   `.browserslistrc` evolves.
8. Updated `HANDOVER.md` §1 (floor count), §6 (gate closure), §2 + §5
   (stale `caniuse-lite: 1.0.30001690` → `1.0.30001766`).
9. Updated `crates/STATUS.md` with the Phase 7 closure entry.

**New floor:** `cargo test -p autoprefixer` → **60 passing, 0 ignored**
(53 unit + 4 data parity + 3 browserslist parity active).
**Browserslist-shim floor:** `cargo test -p browserslist-shim` → **29
passing, 0 ignored** (was 15).

## Two pieces of DRIFT I observed and addressed

### Drift #1 — `oxc_browserslist` bundled snapshot (THE GATE)

**Status: CLOSED for AFM's surface.** Hybrid resolver routes AFM atoms
through caniuse-db directly (byte-correct); routes everything else
through oxc as before (drift-tolerant, unchanged for cssnano consumers).
See `AFM_PORT_NOTES.md`.

### Drift #2 — stale caniuse-lite version in HANDOVER.md

`HANDOVER.md` §2 line 73-77 + §5 line 165 referenced
`caniuse-lite: 1.0.30001690` — but the workspace pin moved to
`1.0.30001766` per AFM repin (captured in `PARITY_VERSIONS.md` line
124 + root `package.json`). Fixed in this session. If you regenerate
`data/prefixes.rs` and the pin assertion test fails, check those docs
match `caniuse_db::CANIUSE_LITE_VERSION`.

## Your unit for this session

**Read `HANDOVER.md` §1 + §6 + §11 + §12 + `AFM_PORT_NOTES.md` first.**
Same cardinal rule: take ONE unit 0 → 100% byte-clean. Stop.

### Option A (RECOMMENDED) — `Prefixes::new` body

Now unblocked. `Browsers::new(...)` returns byte-correct `selected` for
the AFM surface; `Prefixes::new` can consume that and build the
`add_table` / `remove_table` against the AFM-pinned data. No mocking
needed for the AFM call site.

Caveat: if you write tests that pass arbitrary browserslist queries
(e.g. `defaults`, `> 1%`), those still go through the oxc fallback
and drift. Either (a) pin tests to the AFM `.browserslistrc` fixture
via `Browsers::new` with `from = Some("...crates/browserslist-shim/tests/fixtures/afm")`,
or (b) hand-curate `selected` for non-AFM tests and bypass `Browsers::new`.

**ALWAYS set `from` explicitly in tests — DO NOT rely on cwd.** The
gate-closure agent's `parse_static` change defaults `path` to
`std::env::current_dir()` when `BrowsersOptions::from` is unset. Cargo's
test-binary cwd varies between invocations (sometimes the crate dir,
sometimes the workspace root, sometimes wherever a CI runner launches
from). A non-AFM cwd silently lands on a different `.browserslistrc`
walk result — or none, falling through to the oxc-fallback `defaults`
which drifts ~2 chrome versions. Your byte-test will then fail for
reasons that have nothing to do with `Prefixes::new`. Use:

```rust
let opts = BrowsersOptions {
    from: Some(workspace_root().join("crates").join("browserslist-shim")
        .join("tests").join("fixtures").join("afm")
        .to_string_lossy().into_owned()),
    ..Default::default()
};
let browsers = Browsers::new(query, opts);
```

Mirror the `workspace_root()` helper from
`crates/autoprefixer/tests/browserslist_parity.rs`.

The data shape: `Prefixes::new(browsers: &Browsers, data: &PREFIXES)`
walks the 183-entry `PREFIXES` table built by Phase 7's `data/prefixes.rs`
codegen, intersects each entry's `browsers` list with `browsers.selected`,
and emits `add_table` / `remove_table` keyed on those intersections. The
JS source is at `crates/_vendor/autoprefixer-10.4.14/package/lib/prefixes.js`.

Per `HANDOVER.md` §4: `Prefixes::group(decl)` lives on this same struct
and shares the constructor's data shape — fill them in together.

### Option B — `supports.rs` standalone

302 LOC `@supports` rewriting. Depends on `Prefixes::new` for the
data table only — can be ported in parallel by reading the data shape
out of the constructor's design. Lower critical-path leverage than A.

### Option C — `transition.rs` standalone

329 LOC transition shorthand handling. Same shape as B — independent
unit, lower critical-path leverage than A.

### Options D-F — hacks / `info.rs` / parity-runner stage

Same as the previous MORNING.md. Don't pick these without confirming
with the user.

## What you will NOT do

Same list as before; transcribed for visibility:

1. Do not port any hacks (parallel agent's territory).
2. Do not edit `parity-runner/src/stages.rs`,
   `parity-runner/src/main.rs`, or
   `packages/css/scripts/parity-bridge.mjs` without asking.
3. Do not edit `crates/css/src/transform.rs` (final wire-up; out of
   scope until everything else is done).
4. Do not bump any pinned version (`autoprefixer`, `caniuse-lite`,
   `browserslist`, `postcss`, anything in `PARITY_VERSIONS.md`).
5. Do not "fix" upstream bugs.
6. Do not write your own tree walk — use
   `postcss_core::walk_*_mut_with_parent` family.
7. Do not `format!("{}", f64)` for any output bytes — use
   `postcss_core::js_number_to_string`.
8. Do not use `HashMap` on the hashing path. `IndexMap` only.
9. Do not skip the verification gates. Before claiming done:
   `cargo test -p autoprefixer` (≥60 passing, 0 failing, 0 ignored),
   `cargo build -p autoprefixer` (clean), `cargo check --workspace` (clean).
10. **Do not remove `oxc_browserslist` from `browserslist-shim/Cargo.toml`.**
    The fallback is load-bearing for cssnano consumers. See
    `AFM_PORT_NOTES.md` "What NOT to remove".
11. **Do not widen the AFM grammar opportunistically.** Each new
    `QueryAtom` variant pins more surface to caniuse-db semantics.
    Only port what AFM actually consumes — confirm via runtime
    instrumentation (the protocol in `AFM_PORT_NOTES.md`).

## Sign-off checklist

1. `RUSTFLAGS="" cargo test -p autoprefixer` — must show ≥60 passing, 0 ignored.
2. `RUSTFLAGS="" cargo test -p browserslist-shim` — must show ≥29 passing, 0 ignored.
3. `RUSTFLAGS="" cargo build -p autoprefixer` — must be clean.
4. `RUSTFLAGS="" cargo check --workspace` — must be clean.
5. If you wired a new test for byte-equality vs JS oracle — run that
   test specifically and read its output.
6. Update `crates/STATUS.md` Phase 7 row + test count + the
   "Foundation agent's responsibilities" checklist.
7. Update `crates/autoprefixer/HANDOVER.md` §1 (test count floor) +
   §11 (any new JS quirk).
8. Write a `MORNING.md`-style handoff for the NEXT agent — overwrite
   this file.
9. Mark TaskList items completed; create new ones for follow-up work.

## When you're stuck

Vendored sources at:
```
crates/_vendor/autoprefixer-10.4.14/package/lib/<file>.js
crates/_vendor/postcss-8.5.6/package/lib/<file>.js
crates/_vendor/browserslist-4.24.4/package/<file>.js   (closest to 4.24.2 we have)
crates/_vendor/caniuse-lite-1.0.30001766/package/
```

The `BROWSER_LIST_FROM_AFM.md` runtime-instrumentation report and
`AFM_PORT_NOTES.md` architecture doc are your friends if you have to
extend the browserslist surface.

## Final note

The browserslist gate was the single biggest unblocker for the rest
of Phase 7. With it closed, every downstream byte-test against the JS
oracle has a trustworthy foundation FOR AFM-SHAPED QUERIES. Don't
extend that scope to non-AFM queries unless you also extend the AFM
fast path — otherwise you'll re-introduce the drift through the oxc
fallback.

Good luck.
