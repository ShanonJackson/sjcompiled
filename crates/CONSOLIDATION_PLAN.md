# CSS Crate Consolidation Plan

**Goal:** Fold the 19 CSS-only plugin crates into `crates/css/src/plugins/` as Rust
modules. Reduces `crates/` top-level from ~39 to ~18, makes the codebase
reviewable without changing what ships.

**Non-negotiable:** byte-identical pipeline output (the corpus was backtested on
a 90 GB monorepo). After every plugin fold, `parity-runner` on the sample
corpus must stay green. If it goes red, revert that step before moving on.

**Strategy:** `pub mod` bridge (not `pub use` re-exports). Each plugin's source
moves under `crates/css/src/plugins/<name>/`. `parity-runner` retargets its
`use <name>::...` lines to `use css::plugins::<name>::...`. Compile-time
errors catch any missed import — no silent gap.

## What stays top-level (and why)

- `postcss-core`, `postcss-selector-parser`, `postcss-value-parser`,
  `postcss-values-parser` — parsers used by every folded plugin. Folding them
  would force `use crate::postcss_value_parser::...` rewrites across every
  plugin's source. Marginal win for huge churn.
- `autoprefixer` — multi-consumer (`css`, `compiled-css-napi`, `parity-runner`,
  `swc-native` dev). Also the LLVM-OOM-prone crate per
  `compiled-css-napi/Cargo.toml:31`. Folding worsens the existing release
  build problem.
- `compiled-utils` — also pulled by `babel-plugin`.
- `cssnano-browserslist-snapshot`, `browserslist-shim`, `colord`, `caniuse-*`,
  `cssnano-utils`, `fraction-js` — foundations / multi-consumer.

## Per-plugin checklist template

Copy this for each plugin. Tick as you go.

```
### <name>

- [ ] git mv crates/<name>/src crates/css/src/plugins/<snake_name>
- [ ] Delete crates/<name>/Cargo.toml; rmdir crates/<name>
- [ ] Remove "<name>" from `members` in crates/Cargo.toml
- [ ] Remove `<name> = { path = "<name>" }` from [workspace.dependencies]
- [ ] Remove `<name> = { workspace = true }` from crates/css/Cargo.toml (if present)
- [ ] Add `pub mod <snake_name>;` to crates/css/src/plugins/mod.rs
- [ ] Rewrite parity-runner imports: `<snake_name>::X` → `css::plugins::<snake_name>::X`
- [ ] Remove `<name> = { workspace = true }` from crates/parity-runner/Cargo.toml
- [ ] Audit `use crate::...` lines inside the folded source (they now reference `css` root, not the plugin root — flip to `use crate::plugins::<snake_name>::...` or `use super::...`)
- [ ] `cargo check -p css -p parity-runner` clean
- [ ] Run `parity-runner` on sample corpus, confirm 0 diffs
- [ ] Commit (one plugin per commit so revert is surgical)
```

Common gotchas:
- Crate name `postcss-discard-duplicates` → module name `postcss_discard_duplicates`. Use underscores for the directory name (`crates/css/src/plugins/postcss_discard_duplicates/`).
- Single-file plugins: rename `lib.rs` → `mod.rs` and put it in the new dir. Or use `crates/css/src/plugins/<snake_name>.rs` if it really is one file.
- `#[cfg(test)] mod tests` blocks keep working — they're already nested inside the plugin's modules.
- `tests/` directories (only `compiled-css` has one): move to `crates/css/tests/<snake_name>/` so the integration tests still compile against `css::plugins::<name>`.
- Watch for plugins that internally `use crate::helpers;` — after fold, `crate` is `css`, not the plugin. Flip to `use super::helpers;` or absolute `use crate::plugins::<name>::helpers;`.

---

## Phase 0 — Setup

- [x] Create `crates/css/src/plugins/` directory
- [x] Create `crates/css/src/plugins/mod.rs` (empty — modules added per-plugin)
- [x] Add `pub mod plugins;` to `crates/css/src/lib.rs`
- [x] `cargo check -p css` clean (note: requires `RUSTFLAGS=""` — user's global RUSTFLAGS has `lto=thin` which breaks proc-macro builds)

---

## Phase 1 — Pilot fold (validates playbook)

### postcss-discard-duplicates
*Smallest leaf, only depends on `postcss-core`. Single-file likely. Best canary.*

- [x] git mv crates/postcss-discard-duplicates/src/lib.rs → crates/css/src/plugins/postcss_discard_duplicates.rs (single file, no need for dir/mod.rs)
- [x] Delete crates/postcss-discard-duplicates/Cargo.toml; rmdir
- [x] Update crates/Cargo.toml (members + workspace.dependencies)
- [x] Update crates/css/Cargo.toml
- [x] Add `pub mod postcss_discard_duplicates;` to plugins/mod.rs
- [x] Rewrite parity-runner imports (`stages.rs:403`) + css `sort.rs:22`
- [x] Audit internal `use crate::` lines (none present; only `use super::*` in tests, still correct)
- [x] cargo check -p css -p parity-runner — clean
- [x] cargo check --workspace — clean (babel-plugin, swc-native, compiled-css-napi all still link)
- [x] Unit tests at new location: 17/17 passing
- [x] parity-runner `discard_duplicates` — green
- [x] parity-runner `npm_postcss_discard_duplicates` (full npm corpus) — green
- [ ] Commit

**Env fix landed:** Added `@babel/preset-typescript` + `@babel/plugin-transform-modules-commonjs` to root `package.json` devDependencies (required by `packages/css/scripts/parity-bridge-cjs-hook.cjs`). Both bridge tests now green.

**Pilot status:** ✅ Byte-identical confirmed. Playbook validated. Cleared to scale.

---

## Phase 2 — Remaining direct-pipeline plugins

### postcss-normalize-whitespace
- [ ] full checklist
- [ ] commit

### postcss-nested
- [ ] full checklist
- [ ] commit

### postcss-calc
- [ ] full checklist
- [ ] commit

---

## Phase 3 — cssnano-postcss-* sub-plugins (13)

Order doesn't matter inside this phase; they don't reference each other. Pick
alphabetical for simplicity.

### cssnano-postcss-colormin
- [ ] full checklist
- [ ] commit

### cssnano-postcss-convert-values
- [ ] full checklist
- [ ] commit

### cssnano-postcss-discard-comments
- [ ] full checklist
- [ ] commit

### cssnano-postcss-minify-gradients
- [ ] full checklist
- [ ] commit

### cssnano-postcss-minify-params
- [ ] full checklist
- [ ] commit

### cssnano-postcss-minify-selectors
- [ ] full checklist
- [ ] commit

### cssnano-postcss-normalize-positions
- [ ] full checklist
- [ ] commit

### cssnano-postcss-normalize-string
- [ ] full checklist
- [ ] commit

### cssnano-postcss-normalize-timing-functions
- [ ] full checklist
- [ ] commit

### cssnano-postcss-normalize-unicode
- [ ] full checklist
- [ ] commit

### cssnano-postcss-normalize-url
- [ ] full checklist
- [ ] commit

### cssnano-postcss-ordered-values
- [ ] full checklist
- [ ] commit

### cssnano-postcss-reduce-initial
- [ ] full checklist
- [ ] commit

---

## Phase 4 — Orchestrators (must come AFTER Phase 3)

These wrap the sub-plugins. Their internal source has
`use cssnano_postcss_<x>::...` which must flip to
`use crate::plugins::cssnano_postcss_<x>::...`.

### cssnano-preset-default
- [ ] full checklist + extra: flip internal sub-plugin imports
- [ ] commit

### compiled-css
*Has `tests/` dir — also move to `crates/css/tests/compiled_css_*/`.*
*Imports `cssnano-preset-default` internally — flip that import too.*
- [ ] full checklist
- [ ] Move `tests/` to `crates/css/tests/compiled_css_*/`
- [ ] commit

---

## Phase 5 — Final verification

- [ ] `cargo check --workspace` clean
- [ ] `cargo build -p compiled-css-napi` (dev — NOT release, OOM) clean
- [ ] `cargo build -p swc-native` (dev) clean
- [ ] Full parity-runner corpus run: 0 diffs
- [ ] `cargo build -p babel-plugin --target wasm32-wasip1 --release` (RUSTFLAGS="") clean
- [ ] Final commit: cleanup any stragglers, update top-level Cargo.toml comments

---

## Revert protocol

If parity-runner goes red after a fold:
1. `git revert <that-commit>` — the per-plugin commits make this surgical.
2. Don't reroll the plugin until you've identified what changed (likely a
   `use crate::` path that resolves differently now that `crate` is `css`).
3. Tick the plugin's `commit` box back to `[ ]` and retry.

## When NOT to revert mid-phase

If a plugin compiles + parity-runner green but you spot something cosmetically
ugly (e.g. ambiguous module name), keep moving — log it for cleanup at the end.
The fold is mechanical; aesthetic improvements come after the dust settles.
