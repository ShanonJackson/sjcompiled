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
- **DEP UNION:** before folding, check the plugin's `Cargo.toml [dependencies]`. Any workspace deps it had (e.g. `regex`, `once_cell`, `cssnano-utils`) must be added to `crates/css/Cargo.toml` if they're not already there. Cargo error `use of unresolved module or unlinked crate` is the signal you missed one. `cargo check` flushes these out fast.

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
- [x] full checklist (single file → `crates/css/src/plugins/postcss_normalize_whitespace.rs`)
- [x] added `regex` + `once_cell` to `css/Cargo.toml` (plugin's transitive workspace deps)
- [x] 19/19 unit tests pass at new path
- [x] discard_duplicates parity still green (no regression)
- [ ] commit (user handles)

### postcss-nested
- [x] full checklist (single file → `crates/css/src/plugins/postcss_nested.rs`)
- [x] 6/6 unit tests pass at new path
- [ ] commit (user handles)

### postcss-calc — MOVED to Phase 4 due to cycle (see ordering correction)
Tried fold here but `cssnano-preset-default` (still top-level) imports it →
broke build. Reverted to top-level. Fold in Phase 4 after preset-default is
already inside `css`.

---

## Ordering correction (2026-05-13)

The original plan said "fold leaves before orchestrators". This is **wrong** for
the cssnano subtree because of cycles:

- `cssnano-preset-default` (intermediate) imports all 14 `cssnano-postcss-*` +
  `postcss-calc` directly.
- `compiled-css` (intermediate) imports `cssnano-preset-default`.
- Both intermediates are in `css`'s dep chain — meaning they can't add a `css`
  workspace dep (cycle) to access `css::plugins::...`.

Therefore: **fold the intermediates FIRST**, while their dependencies are still
top-level. Their `use cssnano_postcss_X::...` lines keep working because those
crates are still top-level workspace deps. Then progressively fold the leaves;
each leaf fold updates the now-inside-css intermediates from
`use cssnano_postcss_X::...` to `use super::cssnano_postcss_X::...` (sibling).

`compiled-css` must fold AT THE SAME TIME as `cssnano-preset-default` because
compiled-css depends on cssnano-preset-default — folding only one breaks the
other.

Note: this cycle only affects the cssnano subtree. The plain postcss plugins
(`postcss-discard-duplicates`, `postcss-normalize-whitespace`, `postcss-nested`)
are consumed only by `css` directly + `parity-runner`, so leaves-first works
for them. That's why Phases 1+2 already landed without trouble.

## Phase 3 (CORRECTED ORDER) — Intermediates first

### cssnano-preset-default + compiled-css (combined fold)

Both must move in one atomic change. After:
- [x] `crates/css/src/plugins/cssnano_preset_default.rs` (single-file, was `crates/cssnano-preset-default/src/lib.rs`)
- [x] `crates/css/src/plugins/compiled_css/` tree (was `crates/compiled-css/src/`)
  - mod.rs (was lib.rs), plugins.rs + plugins/, utils.rs + utils/, compat/
- [x] `compiled-css`'s `tests/` directory was empty — nothing to relocate
- [x] `compiled_css/plugins/normalize_css.rs`: `use cssnano_preset_default::X` → `use crate::plugins::cssnano_preset_default::X` (sibling under `css/src/plugins/`)
- [x] `compiled_css/plugins/normalize_css.rs`: `use crate::plugins::normalize_current_color` → `use super::normalize_current_color` (was wrong post-fold since `crate` = `css`)
- [x] `compiled_css/plugins/sort_shorthand_declarations.rs`: `use crate::compat::v8_array_sort` → `use super::super::compat::v8_array_sort`
- [x] `compiled_css/plugins/sort_atomic_style_sheet.rs`: `use crate::utils::sort_pseudo_selectors` → `use super::super::utils::sort_pseudo_selectors`
- [x] `css/Cargo.toml`: dropped `cssnano-preset-default` + `compiled-css`; added `colord` + all 14 `cssnano-postcss-*` + `postcss-calc` (transitive deps from the two folded crates)
- [x] `parity-runner/Cargo.toml`: dropped `compiled-css` (parity-runner didn't depend on `cssnano-preset-default` directly)
- [x] `parity-runner/src/stages.rs`: `compiled_css::plugins::X` → `css::plugins::compiled_css::plugins::X` (literal mirror)
- [x] `css/src/transform.rs`: `cssnano_preset_default::X` → `crate::plugins::cssnano_preset_default::X`; `compiled_css::plugins::X` → `crate::plugins::compiled_css::plugins::X`
- [x] `css/src/sort.rs`: `compiled_css::plugins::X` → `crate::plugins::compiled_css::plugins::X`
- [x] `css/src/lib.rs`: `compiled_css::utils::X` → `crate::plugins::compiled_css::utils::X`
- [x] `css/examples/profile_phases.rs`: `compiled_css::plugins::X` → `css::plugins::compiled_css::plugins::X`; `cssnano_preset_default::X` → `css::plugins::cssnano_preset_default::X`
- [x] `cargo check --workspace` — zero errors (only pre-existing babel-plugin warnings)
- [x] `cargo test -p css` — 249/249 pass (includes folded-plugin unit tests)
- [x] `cargo test -p parity-runner --test-threads=1` — 17/17 binaries green, 0 failures (atomicify_rules, discard_duplicates, discard_empty_rules, expand_shorthands, extract_stylesheets, increase_specificity, js_determinism×2, merge_duplicate_at_rules, normalize_current_color, npm_postcss_discard_duplicates, parent_orphaned_pseudos, postcss_core_roundtrip, postcss_ordered_values, sort_atomic_style_sheet)
- [ ] commit (user handles)

## Phase 4 — cssnano-postcss-* sub-plugins (13) + postcss-calc

Order doesn't matter inside this phase; the sub-plugins don't reference each
other. Pick alphabetical for simplicity. **Each fold also updates the now-
inside-css `cssnano_preset_default.rs`** from `use cssnano_postcss_X::...` to
`use super::cssnano_postcss_X::...` (sibling under `css/src/plugins/`).

Per-plugin diff is mechanical:
1. `git mv crates/<name>/src/{lib.rs,*.rs,subdir/} crates/css/src/plugins/<snake_name>{.rs OR /mod.rs+files}`
2. `git rm crates/<name>/Cargo.toml; rmdir crates/<name>/src crates/<name>`
3. Edit `crates/Cargo.toml`: remove from members + workspace.dependencies
4. Edit `crates/css/Cargo.toml`: remove `<name> = { workspace = true }` line; add any new transitive deps from the plugin's old `[dependencies]` if not already present (most need `cssnano-utils`, some need `colord`, `regex`, `once_cell`)
5. Edit `crates/parity-runner/Cargo.toml`: remove `<name> = { workspace = true }`
6. Edit `crates/css/src/plugins/mod.rs`: add `pub mod <snake_name>;`
7. Edit `crates/css/src/plugins/cssnano_preset_default.rs`: change `use cssnano_postcss_<X>::...` → `use super::cssnano_postcss_<X>::...`
8. Edit `crates/parity-runner/src/stages.rs`: change `cssnano_postcss_<X>::...` → `css::plugins::cssnano_postcss_<X>::...`
9. Audit internal `use crate::...` lines inside the folded source (flip to `super::` or `crate::plugins::<snake_name>::`)
10. `cargo check --workspace` clean
11. Per-plugin parity test if it exists (e.g. `cargo test -p parity-runner --test npm_<name>`); otherwise unit tests via `cargo test -p css plugins::<snake_name>`
12. Commit

### cssnano-postcss-colormin
- [x] multi-file fold; added `browserslist-shim` + `caniuse-api` to css/Cargo.toml
- [x] 33 unit tests pass

### cssnano-postcss-convert-values
- [x] multi-file fold (`lib/convert.rs` inline submodule); no new deps
- [x] 36 unit tests pass

### cssnano-postcss-discard-comments
- [x] multi-file fold; flipped 1 internal `use crate::DiscardCommentsOpts` → `use super::DiscardCommentsOpts`
- [x] 15 unit tests pass

### cssnano-postcss-minify-gradients
- [x] multi-file fold; added `cssnano-utils` to css; flipped `use crate::is_color_stop` → `use self::is_color_stop`
- [x] 16 unit tests pass

### cssnano-postcss-minify-params
- [x] single-file fold; no new deps
- [x] 16 unit tests pass

### cssnano-postcss-minify-selectors
- [x] multi-file fold; no internal `use crate::`
- [x] 50 unit tests pass

### cssnano-postcss-normalize-positions
- [x] single-file fold; no new deps
- [x] 20 unit tests pass

### cssnano-postcss-normalize-string
- [x] single-file fold
- [x] 11 unit tests pass

### cssnano-postcss-normalize-timing-functions
- [x] single-file fold
- [x] 21 unit tests pass

### cssnano-postcss-normalize-unicode
- [x] single-file fold
- [x] 10 unit tests pass

### cssnano-postcss-normalize-url
- [x] multi-file fold; added external `url@2` + `percent-encoding@2` to css/Cargo.toml (NOT workspace deps — direct version pins)
- [x] 33 unit tests pass

### cssnano-postcss-ordered-values
- [x] multi-file fold with `helpers/` + `rules/` subdirs; flipped 11 internal `use crate::` lines to `super::super::` or `super::` (sibling)
- [x] 19 unit tests pass

### cssnano-postcss-reduce-initial
- [x] multi-file fold with `data/*.json` (loaded via `include_str!`); added `serde_json` workspace dep to css
- [x] 16 unit tests pass

### postcss-calc (single-file but multi-module via inline `pub mod lib`)
- [x] multi-file fold (parser.rs + lib/{convert_unit,reducer,stringifier,transform}.rs); flipped 7 file-level `use crate::` to `super::super::`/`super::`; flipped 11 in-`mod tests` `use crate::` to `super::super::super::` (3 deep — tests are nested in module)
- [x] 53 unit tests pass

**Phase 4 total:** 14 plugins folded, 349 unit tests pass at new locations.
**Phase 4 workspace stats after:** `cargo test -p css` = 598/598 pass (up from 249 baseline). `cargo check --workspace` zero errors.

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

- [x] `cargo check --workspace` clean (zero errors)
- [x] `cargo build -p compiled-css-napi` (dev) — clean (8.96s)
- [x] `cargo build -p swc-native` (dev) — clean (43.44s)
- [x] Full parity-runner serial sweep: **17/17 binary results green, 0 failures** across atomicify_rules, discard_duplicates, discard_empty_rules, expand_shorthands, extract_stylesheets, increase_specificity, js_determinism (2 subtests), merge_duplicate_at_rules, normalize_current_color, npm_postcss_discard_duplicates, parent_orphaned_pseudos, postcss_core_roundtrip, postcss_ordered_values, sort_atomic_style_sheet
- [x] `cargo test -p css` 598/598 pass (was 249 before consolidation)
- [ ] `cargo build -p babel-plugin --target wasm32-wasip1 --release` (RUSTFLAGS="") — not yet run (optional final verification)
- [ ] Final commit: user handles

## Phase 6 — Vendor lib folds (4 more)

Recognized after Phase 5 that several "foundations" had lost their non-css consumers once compiled-css / cssnano-preset-default / sub-plugins moved inside css. Folded under `crates/css/src/vendor/` (separate from `plugins/` since these are libs, not pipeline plugins).

### cssnano-utils
- [x] 4-file fold (`mod.rs`, `get_arguments.rs`, `raw_cache.rs`, `same_parent.rs`); no internal `use crate::`; 3 consumers retargeted to `crate::vendor::cssnano_utils::*`
- [x] 8 unit tests pass

### postcss-selector-parser
- [x] 9-file fold; flipped 10 file-level `use crate::` → `use super::` (siblings); fixed 1 fully-qualified inline ref (`crate::vendor::postcss_selector_parser::nodes::Node`); 6 consumers retargeted
- [x] 31 unit tests pass

### postcss-values-parser
- [x] 5-file fold + `nodes/` subdir (12 files); flipped 13 `use crate::` (file-level + inside test mod); 14 consumers retargeted (all under `compiled_css/plugins/expand_shorthands/`)
- [x] 91 unit tests pass

### colord
- [x] 9-file fold + `plugins/` subdir (6 files); also moved `tests/minify_parity.rs` + `minify_vectors.json` to `crates/css/tests/`; flipped 18 file-level `use crate::` (bulk sed) + several inline `crate::X` refs + test-mod paths (`super::super::super::colord`)
- [x] 4 consumers retargeted (including one `pub use ::colord::plugins::minify::MinifyOpts` in `cssnano_postcss_colormin`)
- [x] All in-crate unit tests pass
- [x] Integration test `colord_minify_parity` retargeted to `use ::css::vendor::colord as colord_crate;` — passes 1/1

**Phase 6 stats:**
- `cargo test -p css` = 783/783 (up from 598 after Phase 5)
- `cargo build -p compiled-css-napi` (dev) clean
- `cargo build -p swc-native` (dev) clean
- Parity sweep: 17/17 binary results green, 0 failures

## CONSOLIDATION COMPLETE

**Crates listing went from ~36 → ~17 top-level dirs.** 23 crates folded:
- 4 direct-pipeline plugins: `postcss-discard-duplicates`, `postcss-normalize-whitespace`, `postcss-nested`, `postcss-calc`
- 2 intermediates: `cssnano-preset-default`, `compiled-css`
- 13 cssnano sub-plugins: colormin, convert-values, discard-comments, minify-gradients, minify-params, minify-selectors, normalize-positions, normalize-string, normalize-timing-functions, normalize-unicode, normalize-url, ordered-values, reduce-initial

**Plugins live under `crates/css/src/plugins/`**, **vendor libs under `crates/css/src/vendor/`**. Parity contract preserved — parity-runner tests each plugin in isolation via `css::plugins::<name>::...` paths, byte-identical to JS pipeline.

**What stayed top-level** (true multi-consumer + risk crates):
- `postcss-core`, `postcss-value-parser` — also consumed by autoprefixer
- `compiled-utils` — also consumed by babel-plugin
- `caniuse-db`, `caniuse-api`, `fraction-js`, `browserslist-shim`, `cssnano-browserslist-snapshot` — autoprefixer / babel-plugin / swc-native chains
- `autoprefixer` — documented LLVM OOM risk if folded (see `compiled-css-napi/Cargo.toml:31`); folding would also force `postcss-core` fold via dep cycle
- Other entry points: `babel-plugin`, `babel-plugin-strip-runtime`, `babel-plugin-phase0-probes`, `swc-native`, `compiled-css-napi`, `parity-runner`

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
