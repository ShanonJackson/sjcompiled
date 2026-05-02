# Audit & Port: `node-releases` 2.0.19 → 2.0.18

> **DATA-ONLY PACKAGE.** This is a JSON/data dependency, not JS source. There is no `<file>.js` → `<file>.rs` mapping. The audit verifies the **vendored data tables** match AFM's installed snapshot, NOT that any Rust source matches an upstream JS file.

> **NOT YET CONSUMED BY ANY RUST CODE.** `grep -rn "node-releases\|node_releases" crates/` returns hits only in `Cargo.toml` description strings, not in any `*.rs` source. The Rust port has nothing to update today; this audit is a documentation/forward-pin sanity check. **Consider deferring this prompt entirely** unless a downstream port (autoprefixer, browserslist-aware cssnano plugins) is about to land that will consume it. If you proceed, the deliverable shrinks to (a) re-vendor under `crates/_vendor/node-releases-2.0.18/`, (b) bump pin docstrings, (c) write a one-page report, (d) flag the hand-off in STATUS.md. Skip the source diff. Skip the Rust port. Skip the corpus additions.


## Background — why this exists

The Rust ports under `crates/` were originally written against the
versions pinned in `REFERENCE_LOCK_FILE/yarn.lock` (the upstream
`compiled` repo's lockfile). We later discovered that the **AFM/JIRA
monorepo** — the actual consumer of the Rust port — installs
`@compiled/css@0.19.0` resolved against a different dependency graph.
AFM resolution wins; see `AFM_MONOREPO_DEPENDENCIES_MORE.md` and
`crates/PARITY_VERSIONS.md` "Source of Truth" section.

The contract for this project is **byte-equality** for hash output.
Any non-cosmetic source change between the two pinned versions must
be replicated in the Rust port. The 20-stage parity corpus in
`crates/parity-runner/corpus/` is a SMOKE gate (~430 hand-crafted
inputs total); the real consumer is **~60GB of AFM source** where
every selector/value/at-rule edge case will surface. **Do not assume
a change is cosmetic just because the existing corpus passes.** Bias
toward replicating upstream verbatim. The cost of a needless port is
low; the cost of a missed semantic change is a silent hash divergence
in production that is effectively impossible to debug.


## Specific to `node-releases`

We originally ported the Rust crate `crates/caniuse-db/` against
**2.0.19**. AFM resolves to **2.0.18**. The pin has been bumped
in `PARITY_VERSIONS.md`, root `package.json` overrides, and the
crate's docstrings — **but the port itself has not been audited against
2.0.18 source yet**.

**LOW PRIORITY** — `grep -rn "node-releases\|node_releases" crates/` returns ONE hit, in `crates/caniuse-db/Cargo.toml`'s description string. Nothing actually reads the data, AND Compiled's CSS pipeline rarely queries node versions (browserslist queries target browsers, not server runtimes). Audit reduces to: (1) re-vendor `crates/_vendor/node-releases-2.0.18/` for future use; (2) document in the report that no Rust consumer exists yet so no port is required; (3) flag the hand-off point for whichever future agent first reaches a `node N` query path. **Consider deferring this audit indefinitely** unless a real consumer appears.

### Where this is consumed in the Rust port

**Currently NOT consumed by any Rust code.** Mentioned only in `crates/caniuse-db/Cargo.toml` description string. Vendored as a forward-compatibility pin for browserslist `node N` query resolution; `oxc_browserslist` handles node version queries internally today.

### Known non-cosmetic deltas (anchors — find more in your full diff)

- Patch DOWN (2.0.19 → 2.0.18). One patch version removed.
- No Rust source consumes this data today. The audit is a documentation/forward-pin sanity check, not a port.

## Source locations

- **Old (2.0.19)**: `crates/_vendor/node-releases-2.0.19/`
- **New (2.0.18)**: `node_modules/.bun/node-releases@2.0.18/node_modules/node-releases/`
- **Rust port**: `crates/caniuse-db/`
- **Headline files** (in the `data/` subdirectory of the package): `data/processed/envs.json`, `data/release-schedule/release-schedule.json`

If `node_modules/.bun/node-releases@2.0.18*/` is missing, run
`bun install` from the workspace root first. If you want a vendored
copy for permanence:
`mkdir -p crates/_vendor/node-releases-2.0.18/package && cp -r node_modules/.bun/node-releases@2.0.18/node_modules/node-releases/. crates/_vendor/node-releases-2.0.18/package/`.

## Your task

### 1. Confirm the new vendored snapshot reflects the AFM pin

```bash
diff -r \
  crates/_vendor/node-releases-2.0.19/data/ \
  node_modules/.bun/node-releases@2.0.18/node_modules/node-releases/data/
```

Walk every file in the diff. Categorize each delta:

- **Data-only changes** (new browser version added, support flag flipped,
  feature added/removed) — record in your report. These reach output
  bytes ONLY through downstream consumers (autoprefixer, caniuse-api,
  the browserslist-aware cssnano plugins).
- **Schema changes** (field added/removed at the JSON level) — these
  break the unpacker / parser. Update `crates/caniuse-db/scripts/snapshot.js`
  and `crates/caniuse-db/src/features.rs` / `agents.rs` if hit.

### 2. Re-run the snapshot regeneration if needed

```bash
node crates/caniuse-db/scripts/snapshot.js
RUSTFLAGS="" cargo build --manifest-path crates/caniuse-db/Cargo.toml
```

The snapshot file (`crates/caniuse-db/data/features.snapshot.json`)
has already been regenerated as part of the AFM repin. Verify that file
contains the new version string at the head and the expected feature
count. Do NOT re-vendor on top of work already done.

### 3. Spot-check downstream consumers

For caniuse-lite specifically: pick 5–10 high-traffic features (flexbox,
grid, position-sticky, mask, aspect-ratio, container queries, :has,
transforms, gradients, css-variables) and confirm Rust-side
`caniuse_db::feature("X")` returns the same support matrix as
Node-side `require("caniuse-lite/data/features/X.js")` post-unpack.
Add unit tests under `crates/caniuse-db/src/lib.rs` or
`crates/caniuse-api/src/lib.rs` for any spot-check that surfaces
unexpected drift.

For electron-to-chromium / node-releases: there is no current Rust
consumer. Skip this step. Your report's "future hand-off" section
substitutes for it.

### 4. (skip if not-yet-consumed)

For `node-releases`, this step does not apply — no Rust code reads the data.


## Verification gates (must all pass before declaring done)

Run each command from the workspace root. If any fails, that's a
regression — investigate before declaring complete.

```bash
# Build the parity-runner if it isn't already built.
RUSTFLAGS="" cargo build --manifest-path crates/parity-runner/Cargo.toml

# Full Rust test suite — must stay green.
RUSTFLAGS="" cargo test --manifest-path crates/Cargo.toml --workspace --no-fail-fast

# Parity gates — ALL must remain byte-clean (JS-vs-Rust).
crates/target/debug/parity-runner --stage postcss-core-roundtrip --corpus crates/parity-runner/corpus/postcss-core-roundtrip

# NAPI sort + engine flag verifiers — must stay 12/12.
bun run packages/css/scripts/verify-napi-sort.mjs
bun run packages/css/scripts/verify-engine-flag.mjs

# Determinism on at least one stage you touched (JS-vs-JS oracle stability).
crates/target/debug/parity-runner --stage postcss-core-roundtrip --corpus crates/parity-runner/corpus/postcss-core-roundtrip --determinism
```

If `cargo build` complains about `lto cannot be used for proc-macro`,
prefix the command with `RUSTFLAGS=""`. The repo's user-level
RUSTFLAGS conflicts with proc-macro builds — clearing it is the standard
workaround.


## Report

Write a concise audit document at `crates/_vendor/NODE_RELEASES_2.0.19_TO_2.0.18_AUDIT.md` containing:

- A table of every file in the package source with a column for
  "cosmetic / non-cosmetic / no diff" and a one-line explanation per
  non-cosmetic entry.
- The list of Rust files you modified and a one-line description of
  what changed in each.
- The corpus entries you added and which code path each exercises.
- Verification gate results (paste the actual final-line output of
  each command in the verification block).

Update `crates/STATUS.md` "AFM repin" section: append a single
sub-section recording the change. **Do not** touch other STATUS sections.


## Constraints (do NOT break these)

- **Do not modify** `packages/css/src/`. That tree is the JS oracle
  pinned at `@compiled/css@0.19.0` (commit 40a4548) — touching it
  invalidates parity for every other agent.
- **Do not modify** the "Pinned Versions" tables in
  `crates/PARITY_VERSIONS.md`. The pin is already correct. You're
  closing the gap between the pin and the implementation, not changing
  the pin itself.
- **Do not modify** any `crates/_vendor/<pkg>-<old-version>/` directory.
  Those are read-only historical references.
- **Do not delete** any existing corpus entry, even if it looks
  redundant. Only add new ones.
- **Do not bypass** `RUSTFLAGS=""` by adjusting workspace
  `Cargo.toml` or `compiled-css-napi`'s `[profile.release]`.
  The clearing is the correct workaround for the proc-macro/LTO conflict.
- **Do not skip** any verification gate. Even gates that look unrelated
  to your package may exercise it transitively (e.g. `postcss-core-roundtrip`
  depends on every postcss-touching plugin's AST shape).
- If you find a delta that's ambiguous — could be cosmetic, could be
  semantic — **port it**. Bias toward replicating upstream verbatim.
- **Do not "improve" anything along the way.** Bugs are features. If the
  newer version has a regression vs the older one, port the regression.
- **HashMap is banned** in any code path that produces output bytes.
  Use `IndexMap` (insertion-order is byte-affecting downstream).
- **Do not run `bun install`** unless you genuinely need to refresh
  `node_modules`. The AFM-pinned versions are already resolved; an
  unprompted `bun install` can churn the lockfile and confuse other
  concurrent agents.

