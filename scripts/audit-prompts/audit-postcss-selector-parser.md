# Audit & Port: `postcss-selector-parser` 6.0.13 → 6.1.2


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


## Specific to `postcss-selector-parser`

We originally ported the Rust crate `crates/postcss-selector-parser/` against
**6.0.13**. AFM resolves to **6.1.2**. The pin has been bumped
in `PARITY_VERSIONS.md`, root `package.json` overrides, and the
crate's docstrings — **but the port itself has not been audited against
6.1.2 source yet**.

A separate agent may already be working on this. If you see an in-progress audit doc at the report path, append to it rather than overwriting.

### Known non-cosmetic deltas (anchors — find more in your full diff)

- `parser.js` line ~488: new clause treats `closeParenthesis` as a comma-like terminator alongside `tokens.comma`. Affects boundary detection inside `:is()`, `:where()`, `:not()`, `:has()`, `:matches()`. Different boundary → different raws attachment → different stringified bytes → different hash.
- `parser.js`: `sourceIndex: …` field added on a few node initializations (commas, pseudos). Diagnostic surface only; not stringified today. Mirror the addition anyway — downstream plugin ports may read the field later.

## Source locations

- **Old (6.0.13)**: `crates/_vendor/postcss-selector-parser-6.0.13/package/`
- **New (6.1.2)**: `node_modules/.bun/postcss-selector-parser@6.1.2/node_modules/postcss-selector-parser/`
- **Rust port**: `crates/postcss-selector-parser/`
- **Headline files** (in the `dist/` subdirectory of the package): `parser.js`, `processor.js`, `tokenize.js`, `index.js`, `sortAscending.js`, `tokenTypes.js`, `selectors/*.js`, `util/*.js`

If `node_modules/.bun/postcss-selector-parser@6.1.2*/` is missing, run
`bun install` from the workspace root first. If you want a vendored
copy for permanence:
`mkdir -p crates/_vendor/postcss-selector-parser-6.1.2/package && cp -r node_modules/.bun/postcss-selector-parser@6.1.2/node_modules/postcss-selector-parser/. crates/_vendor/postcss-selector-parser-6.1.2/package/`.

## Your task

### 1. Full source-tree diff

Run `diff -r` on the entire `dist/` tree between the two versions.
Categorize every change:

- **Cosmetic** (whitespace, comment edits, variable renames with no
  semantic effect) — list but do not port.
- **Non-cosmetic** (control flow, output, AST shape, regex, sort, raws
  handling, default options, error messages) — port into the Rust crate.

Starting command:

```bash
diff -r \
  crates/_vendor/postcss-selector-parser-6.0.13/package/ \
  node_modules/.bun/postcss-selector-parser@6.1.2/node_modules/postcss-selector-parser/
```

Walk **every file** under `dist/`.
Don't stop at the first file. Don't trust headline files only.

### 2. Port every non-cosmetic delta into `crates/postcss-selector-parser/`

For each non-cosmetic change you identify:

- Locate the corresponding Rust file (mapping is 1:1 by filename
  — `parser.js` → `parser.rs`, etc.).
- Apply the equivalent change in Rust.
- Add a **brief** comment citing the upstream change:
  `// 6.1.2: <one-line summary> (file.js line ~N)`. Do not write
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
crates/target/debug/parity-runner --stage sort-atomic-style-sheet --corpus crates/parity-runner/corpus/sort-atomic-style-sheet
crates/target/debug/parity-runner --stage parent-orphaned-pseudos --corpus crates/parity-runner/corpus/parent-orphaned-pseudos
crates/target/debug/parity-runner --stage increase-specificity --corpus crates/parity-runner/corpus/increase-specificity
crates/target/debug/parity-runner --stage atomicify-rules --corpus crates/parity-runner/corpus/atomicify-rules
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

Write a concise audit document at `crates/_vendor/POSTCSS_SELECTOR_PARSER_6.0.13_TO_6.1.2_AUDIT.md` containing:

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

