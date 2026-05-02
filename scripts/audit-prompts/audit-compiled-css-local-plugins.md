# Re-audit: `compiled-css` local plugins vs @compiled/css@0.19.0 (40a4548)


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


## Specific to local plugins

`packages/css/src/plugins/` was OVERLAID during the AFM repin with
the source tree from upstream commit `40a45489eaaacc023110c3f107d702a389232892` (i.e.
`@compiled/css@0.19.0`). The overlay reverted three files (the
`flatten-multiple-selectors` deletion, the `expand-shorthands/flex.ts`
simplification, and the `parse-at-rule.ts` rename) and the corresponding
Rust ports in `crates/compiled-css/src/plugins/` were patched.

This audit does a **fresh diff between the AFM commit's plugins/ tree
and our current Rust ports**, to catch any other drift the overlay
missed.

`packages/css/src/` was overlaid with `@compiled/css@0.19.0` source from commit 40a4548 during the AFM repin. Three files were known to revert (sort-atomic-style-sheet, expand-shorthands/flex, parse-at-rule rename) and the corresponding Rust ports were patched. This audit does a fresh diff between the AFM commit's plugins/ tree and our current Rust ports to catch ANY other drift that was missed during the overlay.

## Source locations

- **AFM-pinned JS source**: `packages/css/src/plugins/` (this directory IS the
  AFM commit\'s plugins/ tree — already overlaid).
- **Reference checkout** (read-only): `/c/Users/shanon/Documents/projects/compiled`
  at commit `40a45489eaaacc023110c3f107d702a389232892`. Use
  `git -C /c/Users/shanon/Documents/projects/compiled show 40a4548:packages/css/src/plugins/<file>`
  to fetch the canonical source if you suspect `packages/css/src/plugins/`
  was edited.
- **Rust port**: `crates/compiled-css/src/plugins/`

## Your task

### 1. Verify the JS oracle hasn't drifted

Confirm `packages/css/src/plugins/` is byte-identical to the AFM commit's
`packages/css/src/plugins/` tree:

```bash
diff -r \
  <(git -C /c/Users/shanon/Documents/projects/compiled archive 40a4548 packages/css/src/plugins | tar -t) \
  <(find packages/css/src/plugins/ -type f | sort | sed 's|^packages/css/src/plugins/|packages/css/src/plugins/|')
```

If files differ, flag them — someone may have re-edited the JS oracle.
DO NOT fix them yourself; surface the drift in your report.

### 2. Walk every plugin file

For each file under `packages/css/src/plugins/`, locate the corresponding
Rust port and verify line-by-line as in the no-drift template
(control flow, regex, sort comparators, default options, numeric
stringification, iteration order, raws preservation).

The mapping is 1:1 by filename:
- `atomicify-rules.ts` → `atomicify_rules.rs`
- `discard-duplicates.ts` → `discard_duplicates.rs`
- `discard-empty-rules.ts` → `discard_empty_rules.rs`
- `expand-shorthands/<file>.ts` → `expand_shorthands/<file>.rs`
- `extract-stylesheets.ts` → `extract_stylesheets.rs`
- `increase-specificity.ts` → `increase_specificity.rs`
- `merge-duplicate-at-rules.ts` → `merge_duplicate_at_rules.rs`
- `normalize-css.ts` → `normalize_css.rs`
- `normalize-current-color.ts` → `normalize_current_color.rs`
- `parent-orphaned-pseudos.ts` → `parent_orphaned_pseudos.rs`
- `sort-atomic-style-sheet.ts` → `sort_atomic_style_sheet.rs`
- `sort-shorthand-declarations.ts` → `sort_shorthand_declarations.rs`
- `at-rules/<file>.ts` → `at_rules/<file>.rs`

### 3. Pay special attention to:

- **`atomicify-rules.ts`**: the CRITICAL hash plugin. Class-name hash
  output reaches every consumer — bit-identical hashing is a hard
  invariant. Re-verify the hash function port (`crates/sjcompiled-utils`)
  too.
- **`sort-atomic-style-sheet.ts`**: was reverted during the AFM repin
  (now uses `parseAtRule` not `parseMediaQuery`, no name=="media"
  gate). Confirm the Rust matches.
- **`expand-shorthands/flex.ts`**: was reverted during the AFM repin
  (only handles `none` keyword, drops `auto`/`initial`/`revert`/etc.
  branches that were added in 0.20+). Confirm the Rust matches.
- **`normalize-css.ts`**: the BASE_PLUGINS / PROD_PLUGINS filter list.
  Plugin set is 14 + normalizeCurrentColor. Confirm cssnano-preset-default
  source order is preserved (NOT the order in normalize-css.ts arrays —
  Anomaly #7 in PARITY_VERSIONS.md).


## Verification gates (must all pass before declaring done)

Run each command from the workspace root. If any fails, that's a
regression — investigate before declaring complete.

```bash
# Build the parity-runner if it isn't already built.
RUSTFLAGS="" cargo build --manifest-path crates/parity-runner/Cargo.toml

# Full Rust test suite — must stay green.
RUSTFLAGS="" cargo test --manifest-path crates/Cargo.toml --workspace --no-fail-fast

# Parity gates — ALL must remain byte-clean (JS-vs-Rust).
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
crates/target/debug/parity-runner --stage sort --corpus crates/parity-runner/corpus/sort

# NAPI sort + engine flag verifiers — must stay 12/12.
bun run packages/css/scripts/verify-napi-sort.mjs
bun run packages/css/scripts/verify-engine-flag.mjs

# Determinism on at least one stage you touched (JS-vs-JS oracle stability).
crates/target/debug/parity-runner --stage discard-empty-rules --corpus crates/parity-runner/corpus/discard-empty-rules --determinism
```

If `cargo build` complains about `lto cannot be used for proc-macro`,
prefix the command with `RUSTFLAGS=""`. The repo's user-level
RUSTFLAGS conflicts with proc-macro builds — clearing it is the standard
workaround.


## Report

Write a concise audit document at `crates/_vendor/COMPILED_CSS_LOCAL_PLUGINS_AFM_REAUDIT.md` containing:

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

