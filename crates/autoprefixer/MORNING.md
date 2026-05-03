## Today's session — Phase 7 SHIPPED

**Autoprefixer is end-to-end byte-clean for AFM.** Six delegated
subagents (AGENT_1..6) closed the engine, hack subset, parity-runner
stage, and NAPI binding in one wrap-up cycle.

### Triple-oracle byte-clean

| Gate | Result |
|---|---|
| `cargo test -p autoprefixer` | **231 active passing, 0 failing, 0 ignored** |
| `cargo run -p parity-runner -- --stage autoprefixer --corpus crates/parity-runner/corpus/autoprefixer` | **OK — 65/65 byte-clean (Rust direct vs JS oracle)** |
| `bun run packages/css/scripts/verify-napi-autoprefixer.mjs` | **OK — 65/65 byte-clean (Rust NAPI vs JS oracle)** |
| `cargo build -p autoprefixer` / `cargo check --workspace` | clean (1 pre-existing `supports.rs:384` warning) |

Floor jumped from 60 → 231 passing in this cycle. **You must keep this
floor or grow it.**

### What landed

| Agent | Unit | Done report |
|---|---|---|
| AGENT_1 | `Prefixes::new` + `cleaner` + `select` + `group` + `info.rs` + `autoprefixer.rs` shell | `AGENT_1_DONE.md` |
| AGENT_2 | `supports.rs` full port (302 LOC) | `AGENT_2_DONE.md` |
| AGENT_3 | `transition.rs` full port (329 LOC) | `AGENT_3_DONE.md` |
| AGENT_4 | `processor.rs` engine (Pass 1 helpers + Pass 2 add/remove walks + Pass 2.5 drift fixes) | `AGENT_4_DONE.md` |
| AGENT_5 | AFM hack instrumentation + 5 in-scope hacks ported + dispatch wiring | `AGENT_5_DONE.md`, `AFM_HACKS_INSTRUMENTATION.md` |
| AGENT_6 | `Stage::Autoprefixer` parity-runner + 65-entry corpus + JS bridge + NAPI binding + verify script | `AGENT_6_DONE.md` |

The full per-cycle write-up lives in `crates/STATUS.md` "Phase 7 ship —
autoprefixer end-to-end byte-clean (2026-05-03)" — that's the
single-source-of-truth, including the drift table, hack scope, and
follow-up list.

### Per-agent prompts (kept for reference)

`crates/autoprefixer/AGENT_{1..6}.md` — the prompts the controller
delegated to each subagent. Read these to understand HOW the work was
broken up. Don't re-invoke them as prompts — they're historical.
`crates/autoprefixer/AGENTS_INDEX.md` documents the dependency graph
and parallelism map.

## Two pieces of DRIFT observed and resolved this cycle

### Drift #1 — `Prefixes::values` return-type ripple (still outstanding)

`Prefixes::values` was changed from `Vec<String>` to
`Result<Vec<String>, NotYetImplemented>` mid-cycle without sweeping
callers. `crates/autoprefixer/src/supports.rs:384` still has
`for _checker in cleaner.values("remove", &unprefixed) { ... }` which
iterates the `Result` (one element on Ok, zero on Err) — produces a
`for_loops_over_fallibles` warning. **Same byte-output as JS today**
because `values` always returns Err until `preprocess()` populates the
table further. AGENT_2 follow-up to fix shape:

```rust
if let Ok(checkers) = cleaner.values("remove", &unprefixed) {
    for _checker in checkers { ... }
}
```

### Drift #2 — value-pass walk re-prefixed its own clones

AGENT_4 Pass 2 originally did the value-pass walk via direct
`insert_before_at_path` calls. The walker re-visited the just-inserted
clones and re-prefixed them, runaway. 13–19 GB OOM ~30 seconds in.
Fixed in AGENT_4 Pass 2.5 by switching to
`DeferredMutation::InsertBefore(clones)` so the cursor bumps past
inserts.

## Your unit for this session

**The autoprefixer port is wrapped up for AFM.** What remains is
either out-of-scope (AFM doesn't reach it) or downstream of work that
hasn't landed yet (Phase 8b needs the rest of the plugin chain
assembled). So your unit depends on what you're trying to advance:

### Option A — Phase 8b assembly (RECOMMENDED if Phase 4-7 is mostly done)

Wire all the Phase 4-7 plugins together in `crates/css/src/transform.rs`
behind the `COMPILED_CSS_ENGINE` flag. Autoprefixer's NAPI binding is
ready to slot in (parity-tested 65/65 standalone via
`verify-napi-autoprefixer.mjs`). Per `EXECUTION_PLAN.md` Phase 8b.

Pre-condition: every other Phase 4/5/6 plugin row in `STATUS.md` table
is DONE (most are; check). The autoprefixer NAPI binding is the
12th-or-so call in the chain.

DO NOT touch `packages/css/src/transform.ts` — that's on CLAUDE.md
IMMUTABLE list. The flag dispatch lives in `crates/css/src/transform.rs`
(currently identity-passthrough). When the full chain assembles, you
expose it via NAPI as `transformCss(css, opts)` mirroring the existing
`sort()` pattern from Phase 8a.

### Option B — Phase 8c release-mode NAPI build

`cargo build -p compiled-css-napi --release` OOMs LLVM (>32 GB working
memory) due to autoprefixer's ~5.5 KLOC + 58 hack files + codegen'd
data tables. Three failed attempts on the dev box; one crashed the
host. Currently shipping dev `.dll` (byte-identical output).

Fix paths laid out in `AGENT_6_DONE.md` "Release-mode build OOM" and
the WARNING block atop `crates/compiled-css-napi/Cargo.toml`:
1. `opt-level=z` (size-prioritized).
2. Split the 58 hack files into a separate sub-crate.
3. Strip caniuse-db to AFM-only entries before WASI compilation.
4. CI machine with ≥32 GB RAM.

This is a perf/binary-size unit, NOT a correctness unit. The Phase 7
parity gates pass with the dev binary.

### Option C — `PrefixesOptions::flexbox` / `grid` enum cleanup

Latent fix flagged by AGENT_2 + AGENT_4. Both options are tri-state in
JS (`true` / `false` / `"no-2009"` for flexbox; `true` / `false` /
`"autoplace"` / `"no-autoplace"` for grid). Current Rust
`Option<String>` collapses `false` and unset, which means certain
disable-paths can never fire. AFM doesn't set these so it's latent —
not a bug, but a soft-spot for any future caller.

5-minute change to introduce `FlexboxOption` + `GridOption` enums.
Then sweep `Supports::disabled` / `processor::grid_status` to drop
their string-coercion workarounds.

### Option D — drift fix for `supports.rs:384`

Drift #1 above. ~5 lines. Doesn't change byte-output today; cleanup of
a soft warning. Pair with Option C if you want a "hygiene" session.

### Option E — widen the hack scope (only if AFM browserslist changed)

Re-run AGENT_5 Phase A's instrumentation per the protocol in
`AFM_HACKS_INSTRUMENTATION.md` §7. If the in-scope hack set widened,
port the new entries with the same shape AGENT_5 used (read
`AGENT_5.md` + `AGENT_5_DONE.md` for the contract). Don't speculate-
port hacks — bounding scope is the whole point of the instrumentation.

## What you will NOT do (still applies)

1. Do NOT bump any pinned version (caniuse-lite, browserslist, postcss,
   autoprefixer, anything in `PARITY_VERSIONS.md`).
2. Do NOT "fix" upstream bugs. Replicate.
3. Do NOT use `format!("{}", f64)` for output bytes.
   `postcss_core::js_number_to_string`.
4. Do NOT use `HashMap` on the hashing path. `IndexMap` only.
5. Do NOT remove `oxc_browserslist` from `browserslist-shim`.
   Fallback is load-bearing for cssnano consumers.
6. Do NOT widen the AFM grammar in `browserslist-shim`. Out of scope.
7. Do NOT touch `packages/css/src/transform.ts` (CLAUDE.md IMMUTABLE).
8. Do NOT delete `crates/css/src/transform.rs`'s identity-passthrough
   without replacing it with the full Phase 4-7 chain. The fallback
   stays per cross-cutting policy #10.
9. Do NOT speculate-port hacks. Stay bounded by the AFM instrumentation
   report unless you re-run it.

## Sign-off checklist (every session, every commit)

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # ≥231 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean (supports.rs:384 warning is pre-existing)
RUSTFLAGS="" cargo check --workspace           # same

env -u RUSTFLAGS cargo run -p parity-runner -- --stage autoprefixer \
  --corpus parity-runner/corpus/autoprefixer
# OK — 65 inputs, all byte-clean (JS vs Rust)

cd ..
bun run packages/css/scripts/verify-napi-autoprefixer.mjs
# OK — 65/65 byte-clean (JS vs Rust NAPI)
```

If any of these regress and you can't fix in 10 minutes, ROLL BACK
your changes with `git restore` and document.

## When you're stuck

1. `crates/autoprefixer/HANDOVER.md` — exhaustive permanent reference,
   recently updated to reflect the SHIPPED state.
2. `crates/STATUS.md` "Phase 7 ship — autoprefixer end-to-end
   byte-clean (2026-05-03)" — single source of truth on what landed.
3. `crates/autoprefixer/AGENT_{1..6}_DONE.md` — per-agent close-out
   reports. Each documents what landed, JS quirks discovered, drift
   surfaced, and asks for follow-up agents.
4. `crates/autoprefixer/AFM_HACKS_INSTRUMENTATION.md` — empirical hack
   scoping report, including the protocol to widen.
5. `crates/browserslist-shim/AFM_PORT_NOTES.md` — architecture of the
   hybrid AFM-fast-path / oxc-fallback resolver.
6. Vendored upstream JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/`.
   NOT GitHub, NOT Stack Overflow. Pinned versions only.

## Final note

Phase 7 was the largest single port in the project (8+ weeks for one
engineer per the original `EXECUTION_PLAN.md` estimate). Compressed via
subagent fanout — AGENT_2 + AGENT_3 ran concurrently; AGENT_5 Phase A
ran concurrent with AGENT_4 Pass 1; the rest were sequential on the
critical path.

The contract held: every byte of Rust output matches the JS oracle for
every input AFM actually reaches. Three independent oracles (Rust
direct, NAPI marshalled, JS oracle) confirm 65/65 byte-clean.

Don't regress the floor. Good luck.
