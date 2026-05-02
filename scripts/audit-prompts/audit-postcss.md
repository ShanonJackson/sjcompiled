# Audit & Port: `postcss` 8.4.31 → 8.5.6


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


## Specific to `postcss`

We originally ported the Rust crate `crates/postcss-core/` against
**8.4.31**. AFM resolves to **8.5.6**. The pin has been bumped
in `PARITY_VERSIONS.md`, root `package.json` overrides, and the
crate's docstrings — **but the port itself has not been audited against
8.5.6 source yet**.

postcss-core is the load-bearing foundation. Every plugin port depends on its AST shape, raws preservation, stringifier output, and number formatting being byte-identical. A missed change here invalidates EVERY downstream parity claim. This audit is the highest priority of the bunch.

### Known non-cosmetic deltas (anchors — find more in your full diff)

- postcss-core agent previously claimed "cosmetic only" — claim was made before the audit standard was tightened. RE-VERIFY with full file-by-file walk; do not take the prior claim as ground truth.
- Empirical diff harness at `crates/_vendor/test-postcss-versions/` (built by the postcss-core agent) compared `parse → stringify` round-trips and found byte-identical output across 26 raw round-trips and 30 plugin × input pairs. Use that harness as ONE input to your audit; do not use it as the SOLE input.

## Source locations

- **Old (8.4.31)**: NOT vendored. To obtain: `npm pack postcss@8.4.31` and extract, OR check `node_modules/.bun/postcss@8.4.31/` for a stale leftover from before the repin.
- **New (8.5.6)**: `node_modules/.bun/postcss@8.5.6/node_modules/postcss/`
- **Rust port**: `crates/postcss-core/`
- **Headline files** (in the `lib/` subdirectory of the package): `parser.js`, `tokenize.js`, `stringifier.js`, `container.js`, `root.js`, `atrule.js`, `rule.js`, `declaration.js`, `comment.js`, `node.js`, `list.js`, `css-syntax-error.js`, `lazy-result.js`

If `node_modules/.bun/postcss@8.5.6*/` is missing, run
`bun install` from the workspace root first. If you want a vendored
copy for permanence:
`mkdir -p crates/_vendor/postcss-8.5.6/package && cp -r node_modules/.bun/postcss@8.5.6/node_modules/postcss/. crates/_vendor/postcss-8.5.6/package/`.

## Your task

### 1. Full source-tree diff

Run `diff -r` on the entire `lib/` tree between the two versions.
Categorize every change:

- **Cosmetic** (whitespace, comment edits, variable renames with no
  semantic effect) — list but do not port.
- **Non-cosmetic** (control flow, output, AST shape, regex, sort, raws
  handling, default options, error messages) — port into the Rust crate.

Starting command:

```bash
diff -r \
  <old-source-path> \
  node_modules/.bun/postcss@8.5.6/node_modules/postcss/
```

Walk **every file** under `lib/`.
Don't stop at the first file. Don't trust headline files only.

### 2. Port every non-cosmetic delta into `crates/postcss-core/`

For each non-cosmetic change you identify:

- Locate the corresponding Rust file (mapping is 1:1 by filename
  — `parser.js` → `parser.rs`, etc.).
- Apply the equivalent change in Rust.
- Add a **brief** comment citing the upstream change:
  `// 8.5.6: <one-line summary> (file.js line ~N)`. Do not write
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
crates/target/debug/parity-runner --stage discard-empty-rules --corpus crates/parity-runner/corpus/discard-empty-rules
crates/target/debug/parity-runner --stage discard-duplicates --corpus crates/parity-runner/corpus/discard-duplicates
crates/target/debug/parity-runner --stage extract-stylesheets --corpus crates/parity-runner/corpus/extract-stylesheets
crates/target/debug/parity-runner --stage parent-orphaned-pseudos --corpus crates/parity-runner/corpus/parent-orphaned-pseudos
crates/target/debug/parity-runner --stage increase-specificity --corpus crates/parity-runner/corpus/increase-specificity
crates/target/debug/parity-runner --stage merge-duplicate-at-rules --corpus crates/parity-runner/corpus/merge-duplicate-at-rules
crates/target/debug/parity-runner --stage normalize-current-color --corpus crates/parity-runner/corpus/normalize-current-color
crates/target/debug/parity-runner --stage sort-atomic-style-sheet --corpus crates/parity-runner/corpus/sort-atomic-style-sheet
crates/target/debug/parity-runner --stage atomicify-rules --corpus crates/parity-runner/corpus/atomicify-rules
crates/target/debug/parity-runner --stage expand-shorthands --corpus crates/parity-runner/corpus/expand-shorthands
crates/target/debug/parity-runner --stage npm-postcss-discard-duplicates --corpus crates/parity-runner/corpus/npm-postcss-discard-duplicates
crates/target/debug/parity-runner --stage postcss-nested --corpus crates/parity-runner/corpus/postcss-nested
crates/target/debug/parity-runner --stage postcss-normalize-whitespace --corpus crates/parity-runner/corpus/postcss-normalize-whitespace
crates/target/debug/parity-runner --stage postcss-discard-comments --corpus crates/parity-runner/corpus/postcss-discard-comments
crates/target/debug/parity-runner --stage postcss-normalize-string --corpus crates/parity-runner/corpus/postcss-normalize-string
crates/target/debug/parity-runner --stage postcss-normalize-positions --corpus crates/parity-runner/corpus/postcss-normalize-positions
crates/target/debug/parity-runner --stage postcss-normalize-timing-functions --corpus crates/parity-runner/corpus/postcss-normalize-timing-functions
crates/target/debug/parity-runner --stage postcss-normalize-url --corpus crates/parity-runner/corpus/postcss-normalize-url
crates/target/debug/parity-runner --stage sort --corpus crates/parity-runner/corpus/sort

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

Write a concise audit document at `crates/_vendor/POSTCSS_8.4.31_TO_8.5.6_AUDIT.md` containing:

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

