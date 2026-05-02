# Audit & Port: `colord` 2.9.1 → 2.9.3


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


## Specific to `colord`

We originally ported the Rust crate `crates/colord/` against
**2.9.1**. AFM resolves to **2.9.3**. The pin has been bumped
in `PARITY_VERSIONS.md`, root `package.json` overrides, and the
crate's docstrings — **but the port itself has not been audited against
2.9.3 source yet**.

`crates/colord/` is currently SCAFFOLDED — the port itself hasn't been written. This audit can flow into the initial port: target 2.9.3 source from the start, do not port 2.9.1 first. Mark all upstream-2.9.1-equivalent code paths with comments noting the 2.9.3 deltas.

### Known non-cosmetic deltas (anchors — find more in your full diff)

- Two patch versions of color-math changes. Diff `parse.js` and `colord.js` carefully (HSL/RGB rounding, alpha format, `#fff` vs `#ffffff` short-form decisions).
- Used by `postcss-colormin` (color minification — the highest-risk cssnano plugin) and `postcss-minify-gradients`.

## Source locations

- **Old (2.9.1)**: `crates/_vendor/colord-2.9.1/`
- **New (2.9.3)**: `node_modules/.bun/colord@2.9.3/node_modules/colord/`
- **Rust port**: `crates/colord/`
- **Headline files** (in the root of the package): `index.mjs`, `colord.js`, `helpers.js`, `parse.js`, `random.js`, `constants.js`, `plugins/names.js`, `plugins/a11y.js`, `plugins/harmonies.js`, `plugins/hwb.js`, `plugins/lab.js`, `plugins/minify.js`, `plugins/mix.js`

If `node_modules/.bun/colord@2.9.3*/` is missing, run
`bun install` from the workspace root first. If you want a vendored
copy for permanence:
`mkdir -p crates/_vendor/colord-2.9.3/package && cp -r node_modules/.bun/colord@2.9.3/node_modules/colord/. crates/_vendor/colord-2.9.3/package/`.

## Your task

### 1. Full source-tree diff

Run `diff -r` on the entire package tree between the two versions.
Categorize every change:

- **Cosmetic** (whitespace, comment edits, variable renames with no
  semantic effect) — list but do not port.
- **Non-cosmetic** (control flow, output, AST shape, regex, sort, raws
  handling, default options, error messages) — port into the Rust crate.

Starting command:

```bash
diff -r \
  crates/_vendor/colord-2.9.1/ \
  node_modules/.bun/colord@2.9.3/node_modules/colord/
```

Walk **every file** under the package.
Don't stop at the first file. Don't trust headline files only.

### 2. Port every non-cosmetic delta into `crates/colord/`

For each non-cosmetic change you identify:

- Locate the corresponding Rust file (mapping is 1:1 by filename
  — `parser.js` → `parser.rs`, etc.).
- Apply the equivalent change in Rust.
- Add a **brief** comment citing the upstream change:
  `// 2.9.3: <one-line summary> (file.js line ~N)`. Do not write
  paragraphs.
- Hashmap → `IndexMap` always.

### 3. Add adversarial corpus entries

The current corpus does not necessarily cover the changed code paths.
Add files to the parity-runner corpora for the verification stages
listed below. Every changed code path needs at least one input that
exercises it. File naming: `corpus/<stage>/NN_<short_label>.css`.
Pick `NN` numbers that don't collide with existing entries.


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

Write a concise audit document at `crates/_vendor/COLORD_2.9.1_TO_2.9.3_AUDIT.md` containing:

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

