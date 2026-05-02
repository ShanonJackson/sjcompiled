# Re-audit: `postcss-normalize-positions@5.1.1` (no drift)


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


## Specific to `postcss-normalize-positions`

`postcss-normalize-positions@5.1.1` is **NOT** drifted between the
REFERENCE_LOCK_FILE and AFM resolution — both pin the same version.
This audit exists because the original port may have introduced
mistakes that the existing 20-stage parity corpus doesn't catch.
The risk model: "we ported imperfectly to begin with," not "version
changed under us."

Rewrites `background-position` and `*-perspective-origin` keyword pairs (left/top → 0 0, etc.). No options.

## Source locations

- **AFM-pinned source (5.1.1)**: `node_modules/.bun/postcss-normalize-positions@5.1.1/node_modules/postcss-normalize-positions/`
- **Rust port**: `crates/cssnano-postcss-normalize-positions/`
- **Headline files** (in the `src/` subdirectory of the package): `*.js`

## Your task

### 1. Full source-tree walk

Walk every file in `node_modules/.bun/postcss-normalize-positions@5.1.1/node_modules/postcss-normalize-positions/src/`.
For each file, locate the corresponding Rust port and verify line-by-line
that:

- Every control-flow branch matches.
- Every regex matches (audit Unicode classes — JS regex semantics differ
  from Rust's `regex` crate in subtle places).
- Every sort comparator matches, including tie-break ordering. Rust's
  `sort_by` is stable (matches JS since ES2019), but the comparator
  must produce identical orderings even for "equal" elements.
- Every default option value matches.
- Every numeric stringification matches. JS's `String(0.1+0.2)` =
  `"0.30000000000000004"`; Rust's `format!("{}", ...)` may not agree
  on edge cases. Use a JS-double-to-string algorithm where any output
  path stringifies a number.
- Every iteration order matches. Banned: `HashMap` in output paths.
- Every raws field is preserved 1:1.

### 2. Apply fixes for any divergence found

For each fix:
- Cite the upstream file + line in a brief comment.
- Add a regression test under `#[cfg(test)] mod tests`.
- Bias toward replicating upstream verbatim — do not "improve."

### 3. Add adversarial corpus entries for any code path you touched

Same approach as the version-drift template. Files go to the
parity-runner corpora for the verification stages below.


## Verification gates (must all pass before declaring done)

Run each command from the workspace root. If any fails, that's a
regression — investigate before declaring complete.

```bash
# Build the parity-runner if it isn't already built.
RUSTFLAGS="" cargo build --manifest-path crates/parity-runner/Cargo.toml

# Full Rust test suite — must stay green.
RUSTFLAGS="" cargo test --manifest-path crates/Cargo.toml --workspace --no-fail-fast

# Parity gates — ALL must remain byte-clean (JS-vs-Rust).
crates/target/debug/parity-runner --stage postcss-normalize-positions --corpus crates/parity-runner/corpus/postcss-normalize-positions

# NAPI sort + engine flag verifiers — must stay 12/12.
bun run packages/css/scripts/verify-napi-sort.mjs
bun run packages/css/scripts/verify-engine-flag.mjs

# Determinism on at least one stage you touched (JS-vs-JS oracle stability).
crates/target/debug/parity-runner --stage postcss-normalize-positions --corpus crates/parity-runner/corpus/postcss-normalize-positions --determinism
```

If `cargo build` complains about `lto cannot be used for proc-macro`,
prefix the command with `RUSTFLAGS=""`. The repo's user-level
RUSTFLAGS conflicts with proc-macro builds — clearing it is the standard
workaround.


## Report

Write a concise audit document at `crates/_vendor/POSTCSS_NORMALIZE_POSITIONS_5.1.1_REAUDIT.md` containing:

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

