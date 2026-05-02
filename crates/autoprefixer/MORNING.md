## Today's session — what landed

**Picked Option D from the previous MORNING.md** (closing the
browserslist-shim parity gate). The gate is now landed, with one slice
green and the omnibus `#[ignore]`'d documenting real divergence.

What I did, in order:
1. Read `PARITY_VERSIONS.md`, `PLUGIN_IMPLEMENTATION_GUIDE.md`,
   `STATUS.md` Phase 7 sections, this crate's `HANDOVER.md`, the prior
   `MORNING.md`. ~45 min.
2. Verified floor: `cargo test -p autoprefixer` → 57 passing
   (53 unit + 4 parity).
3. Wrote `crates/autoprefixer/tests/browserslist_parity.rs` — two tests:
   - `browserslist_shim_firefox_esr_matches_js_oracle` — active, passes.
     Pins the `rewrite_firefox_esr` shim path against the JS oracle.
   - `browserslist_shim_matches_js_oracle_for_canonical_queries` —
     `#[ignore]`'d. Compares 6 canonical queries element-by-element.
     Surfaces the open drift (see below).
4. Documented findings in `STATUS.md` ("Phase 7 ship — browserslist-shim
   parity gate"), `HANDOVER.md` §1 / §6 / §11, and TaskList (#2 + #3).

**New floor:** `cargo test -p autoprefixer` → **58 passing, 1 ignored**.

## Two pieces of DRIFT I observed

Both flagged in HANDOVER + STATUS + TaskList:

### Drift #1 — browserslist-shim caniuse-lite snapshot (THE OPEN GATE)

`oxc_browserslist`'s bundled caniuse-lite snapshot is ~2 chrome
releases newer than the workspace pin (1.0.30001766). Concrete numbers
the `#[ignore]`'d test surfaced:

| Query                          | JS oracle                       | Rust shim                       |
|--------------------------------|---------------------------------|---------------------------------|
| `chrome >= 50`                 | `chrome 144 ... chrome 50` (94) | `chrome 146, chrome 145, ...` (96) |
| `last 2 versions` (first 5)    | `and_chr 144, and_ff 147, ...`  | `and_chr 146, and_ff 149, ...`  |
| `defaults` (first 5)           | `and_chr 144, and_ff 147, ...`  | `and_chr 146, and_ff 149, ...`  |

Closure: TaskList #2 — three options laid out in HANDOVER §6 and
STATUS "Phase 7 ship — browserslist-shim parity gate". All multi-day.

### Drift #2 — workspace `browserslist` floats to 4.28.2

`require('browserslist')` from workspace root resolves to **4.28.2**,
not the pinned 4.24.2. Root cause: `package.json` lists browserslist in
`overrides` but not `devDependencies`, so bun's isolated layout leaves
no top-level symlink and resolution lands on whatever `.bun/` install
wins (4.28.2 is pulled by `update-browserslist-db`).

The new browserslist parity test bypasses this by globbing
`node_modules/.bun/browserslist@4.24.2+*/` for the pinned hash dir.
Proper fix: TaskList #3 — add `"browserslist": "4.24.2"` to root
`package.json` `devDependencies` + `bun install`. ~10-min change.

## Your unit for this session

**Read MORNING-PREVIOUS.md (if archived) or HANDOVER.md §1 + §6 + §11
+ §12 first.** Same cardinal rule: take ONE unit 0 → 100% byte-clean.
Stop.

Recommended order, given today's findings:

### Option A (RECOMMENDED) — close browserslist-shim parity gate (Drift #1 above)

This is the cleanest pre-condition for `Prefixes::new`. Multi-day
unit. Three approaches in HANDOVER §6 — pick (b) "re-port
`browserslist@4.24.2`'s `index.js::resolve` line-by-line against
`caniuse-db`" if you want to maximize byte-control and minimize
upstream-fork risk. The vendored source is at
`crates/_vendor/browserslist-4.24.4/package/` (4.24.4, not 4.24.2;
the diff between the two is captured in
`crates/_vendor/BROWSERSLIST_4.24.4_TO_4.24.2_AUDIT.md`).

Land the closure as: omnibus parity test flips from `#[ignore]` to
active and passes. That's the gate.

### Option B — `Prefixes::new` body, with the gate left open

Per HANDOVER §6 closing paragraph: write `Prefixes::new` against
mock `selected` lists (hand-curated to match JS oracle output for a
specific query). Pins the constructor's transform logic without
needing the gate closed. Don't claim the unit byte-clean — it isn't,
but it's logic-clean. The next agent who closes the gate then re-runs
your tests with `Browsers::new(...)` providing real `selected` and
gets the byte-clean confirmation for free.

This is a fallback if Option A's scope feels too big. The risk is
that the constructor's transform might have its own latent bugs that
only surface against real `selected` data — mock tests can't catch
that class of drift.

### Option C — Drift #2 fix (workspace browserslist devDep)

10 minutes of work but **trivial enough that you can pair it with
either A or B**. Just don't make it your only unit — the project's
hash output isn't affected today (data tables are codegen'd from
vendored autoprefixer source, not from a `require('browserslist')`
call), so this is hygiene for FUTURE oracle tests, not a hash-bytes
fix. Add `"browserslist": "4.24.2"` to root `package.json`
`devDependencies`, `bun install`, then verify
`require('browserslist/package.json').version === '4.24.2'`.

### Options D-F — `supports.rs` / `transition.rs` / hacks

Same as the previous MORNING.md. **Do NOT pick these without
confirming with the user that you want this rather than the engine
path.** They are independent of the gate, but they're not on the
critical path for `Prefixes::new` either.

## What you will NOT do

Same list as the previous MORNING.md, transcribed for visibility:

1. Do not port any hacks (parallel agent's territory).
2. Do not edit `parity-runner/src/stages.rs`,
   `parity-runner/src/main.rs`, or
   `packages/css/scripts/parity-bridge.mjs` without asking.
3. Do not edit `crates/css/src/transform.rs` (final wire-up; out of
   scope until everything else is done).
4. Do not bump any pinned version (`autoprefixer`, `caniuse-lite`,
   `browserslist`, `postcss`, anything in `PARITY_VERSIONS.md`).
   "Do not bump" applies to BOTH the override AND the devDep value
   — but adding a NEW devDep entry that matches the existing
   override (the Drift #2 fix) is not a bump.
5. Do not "fix" upstream bugs.
6. Do not write your own tree walk — use
   `postcss_core::walk_*_mut_with_parent` family.
7. Do not `format!("{}", f64)` for any output bytes — use
   `postcss_core::js_number_to_string`.
8. Do not use `HashMap` on the hashing path. `IndexMap` only.
9. Do not skip the verification gates. Before claiming done:
   `cargo test -p autoprefixer` (≥58 active passing, 0 failing,
   any `#[ignore]` count justified in HANDOVER), `cargo build -p
   autoprefixer` (clean), `cargo check --workspace` (clean).

## Sign-off checklist (same as previous MORNING)

1. `RUSTFLAGS="" cargo test -p autoprefixer` — must show ≥58 passing.
   If you closed the omnibus gate, that becomes ≥59 (Option A) and
   the `#[ignore]` count drops from 1 to 0.
2. `RUSTFLAGS="" cargo build -p autoprefixer` — must be clean.
3. `RUSTFLAGS="" cargo check --workspace` — must be clean.
4. If you wired a new test for byte-equality vs JS oracle — run that
   test specifically and read its output. Don't trust an aggregate
   "≥58 passing" that hides a non-running parity test.
5. Update `crates/STATUS.md` Phase 7 row + test count + the
   "Foundation agent's responsibilities" checklist.
6. Update `crates/autoprefixer/HANDOVER.md` §1 (test count floor) +
   §6 (browserslist gate status) + §11 (any new JS quirk).
7. Write a `MORNING.md`-style handoff for the NEXT agent — overwrite
   this file with what you completed, what you observed about other
   agents' work, and what the next-highest-leverage unit is.
8. Mark TaskList items completed (#1 done; #2/#3 pending → in_progress
   if you picked them up).

## When you're stuck

Vendored sources at:
```
crates/_vendor/autoprefixer-10.4.14/package/lib/<file>.js
crates/_vendor/postcss-8.5.6/package/lib/<file>.js
crates/_vendor/browserslist-4.24.4/package/<file>.js   (closest to 4.24.2 we have)
```

The 4.24.4 → 4.24.2 audit at
`crates/_vendor/BROWSERSLIST_4.24.4_TO_4.24.2_AUDIT.md` is your friend
if you take Option A — it surfaces every line-level diff between the
two versions so you know what to back-port.

## Final note

Drift #1 (the open gate) is the single biggest unblocker for the rest
of Phase 7. Closing it gives EVERY downstream byte-test a trustworthy
foundation. Don't skip it just because it's bigger than the previous
units — the alternative is shipping `Prefixes::new` + `processor.rs`
with mock-test only, then discovering the drift at full-pipeline gate
when there's a 720-LOC walker between the bug and your repro. Pay it
now.

Good luck.
