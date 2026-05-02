# Status — `crates/`

End-of-session snapshot. Read with `EXECUTION_PLAN.md` and
`PARITY_VERSIONS.md`.

## Phase progress

| Phase | Description | Status |
|---|---|---|
| 0 | parity-runner + corpus + JS-vs-JS determinism | **DONE** |
| 1 | postcss-core / caniuse-db / colord / fraction-js | **DONE** |
| 2 | postcss-selector-parser / postcss-value-parser / postcss-values-parser / browserslist-shim / cssnano-utils | **DONE** |
| 3 | caniuse-api | **DONE** |
| 4a | discard-empty-rules / discard-duplicates (LOCAL) / extract-stylesheets | **DONE** — all byte-clean |
| 4b | parent-orphaned-pseudos / flatten-multiple-selectors / increase-specificity | **DONE** — all byte-clean |
| 4c | merge-duplicate-at-rules / normalize-current-color / sort-atomic-style-sheet (+ at-rules helpers, sort-pseudo-selectors, sort-shorthand-declarations) | **DONE** — all byte-clean |
| 4d | atomicify-rules (CRITICAL hash plugin) | **DONE** — byte-clean across 24-entry corpus |
| 4e | expand-shorthands (11 conversion functions) | **DONE** — byte-clean across 38-entry corpus |
| 5a | postcss-nested@5.0.6 | **DONE** — byte-clean across 38-entry corpus, deterministic JS oracle |
| 5b | postcss-normalize-whitespace@5.1.1 | **DONE** — byte-clean across 22-entry corpus, deterministic JS oracle |
| 5c | postcss-discard-duplicates@6.0.0 (npm — used by sort.ts) | **DONE** — byte-clean across 8-entry corpus |
| 6a | postcss-discard-comments@5.1.2 | **DONE** — byte-clean across 15-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-string@5.1.0 | **DONE** — byte-clean across 15-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-positions@5.1.1 | **DONE** — byte-clean across 20-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-timing-functions@5.1.0 | **DONE** — byte-clean across 21-entry corpus, deterministic JS oracle |
| 6b | postcss-normalize-url@5.1.0 | **DONE** — byte-clean across 60-entry corpus, deterministic JS oracle |
| 6c | postcss-minify-selectors@5.2.1 | **SCAFFOLDED** |
| 6d | postcss-ordered-values@5.1.3 | **SCAFFOLDED** |
| 6d | postcss-calc@8.2.4 | **SCAFFOLDED** — calc expression evaluator; high diff risk on float math. |
| 6e | postcss-normalize-unicode@5.1.1 | **SCAFFOLDED** — browserslist-aware. |
| 6e | postcss-reduce-initial@5.1.2 | **SCAFFOLDED** — caniuse-aware. |
| 6f | postcss-convert-values@5.1.3 | **SCAFFOLDED** — uses fraction-js. |
| 6f | postcss-minify-params@5.1.4 | **SCAFFOLDED** — caniuse-aware. |
| 6g | postcss-minify-gradients@5.1.1 | **SCAFFOLDED** — uses colord. |
| 6g | postcss-colormin@5.3.1 | **SCAFFOLDED** — highest-risk cssnano plugin. |
| 6h | cssnano-preset-default@5.2.14 (orchestrator) | **SCAFFOLDED** |
| 7 | autoprefixer@10.4.14 | **IN PROGRESS** — split between two parallel agents. Source vendored at `crates/_vendor/autoprefixer-10.4.14/`. Crate scaffolded at `crates/autoprefixer/` with module tree mirroring `lib/` 1:1 + 58 stubbed hack modules. **Fully ported (byte-clean):** `utils.rs`, `vendor.rs`, `brackets.rs`, `old_value.rs`, `old_selector.rs`, `prefixer.rs` (incl. `parent_prefix` via `walk_up_with` + `Node.attrs` cache + `clone_without` strip), `at_rule.rs` (full `add` + `process`). **20 unit tests passing.** Stubbed (signature-only): `browsers.rs`, `declaration.rs`, `value.rs`, `selector.rs`, `resolution.rs`, `supports.rs`, `transition.rs`, `prefixes.rs`, `processor.rs`, `info.rs`, `autoprefixer.rs`, `data/prefixes.rs`, all 58 hacks. Split contract: see "Phase 7 split contract" section below. |
| 8a | `sort()` NAPI bridge + sort.ts engine flag | **DONE** — 12/12 corpus byte-clean end-to-end on win32-x64-msvc. See "Phase 8a ship" section below. |
| 8b | `transformCss` NAPI bridge + transform.ts engine flag | **NOT STARTED** — blocks on Phase 5/6/7 plugin ports. |

## Test totals

`RUSTFLAGS="" cargo test --workspace --no-fail-fast`:
- **502 tests pass / 0 fail / 1 ignored / 0 failed suites.**
  (12 from Phase 5b `postcss-normalize-whitespace`,
  12 from Phase 6a `postcss-discard-comments`,
  7 from Phase 6b `postcss-normalize-string`,
  12 from Phase 6b `postcss-normalize-positions`,
  15 from Phase 6b `postcss-normalize-timing-functions`,
  33 from Phase 6b `postcss-normalize-url`,
  4 from Phase 5a `postcss-nested`,
  20 from Phase 7 `autoprefixer` foundation,
  3 new postcss-core round-trip tests pinning the
  `rawSemicolon` / `rawBeforeOpen` / `rawColon` fallbacks.)

## Phase 8a ship — `sort()` end-to-end byte-clean through NAPI

**The smaller of the two hashing entry points is now production-ready
behind a feature flag.** Consumers opt in via:

```bash
COMPILED_CSS_ENGINE=rust   # use Rust NAPI backend
# unset / any other value  # use existing JS pipeline (default, parity oracle)
```

The flag is read at module-load time of `packages/css/src/sort.ts:32`
(see `requireFromHere` lazy-load gate). Other env values fall through to
the unchanged JS pipeline — no behavior change for unflagged consumers.

### What landed this session

1. `crates/css/src/sort.rs` is no longer an identity passthrough — it
   composes the three real plugins in **postcss lifecycle order**, not
   array order (see "Lifecycle ordering — load-bearing" below).
2. `crates/compiled-css-napi/` — new crate. cdylib + rlib, napi-rs 2.x,
   exports `sort(stylesheet, opts?)`. Phase 8b will add `transformCss`.
3. `packages/css-native/` — new npm workspace package wrapping the
   prebuilt `.node` binary. Platform-binary loader follows napi-rs
   naming convention (`sjcompiled-css.<triple>.node`).
4. `packages/css/src/sort.ts` — env-flag gate via `createRequire` +
   lazy import. Default path unchanged; oracle JS pipeline preserved.
5. `Stage::Sort` added to parity-runner + JS bridge counterpart in
   `packages/css/scripts/parity-bridge.mjs`.
6. New corpus `crates/parity-runner/corpus/sort/` — 12 fixtures
   covering: blank input, single rule, dup decls, dup at-rules with
   pseudo sort, at-rule reordering, shorthand buckets, full-combo
   (all three stages), comments, decls-at-root, three min-width / two
   max-width, important-vs-not, realistic atomic CSS.
7. `merge_duplicate_at_rules` refactored: split into `visit()` (the
   AtRule visitor pass) + `finalize()` (the OnceExit pass) + a
   combined `merge_duplicate_at_rules()` wrapper. The split is required
   for `sort()` to interleave `postcss-discard-duplicates`'s OnceExit
   between merge's visit and merge's exit, matching postcss lifecycle.
8. `container::append` fixed: now applies `Root.normalize`'s
   raws-before transfer when appending to a Root with ≥2 existing
   children (mirrors `postcss/lib/root.js::normalize` lines 24-28).
   Without this, `finalize()` emitted concatenated rules with the
   wrong leading whitespace.

### Verification gates run

| Gate                                                                | Status |
|---------------------------------------------------------------------|--------|
| `cargo test --workspace --no-fail-fast`                             | 356/356 pass |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort` | 12/12 byte-clean |
| `parity-runner --stage sort ... --determinism`                      | 12/12 deterministic |
| `parity-runner --stage merge-duplicate-at-rules ...`                | 7/7 byte-clean (no regression from refactor) |
| `parity-runner --stage npm-postcss-discard-duplicates ...`          | 8/8 byte-clean |
| `parity-runner --stage sort-atomic-style-sheet ...`                 | 12/12 byte-clean |
| `bun run packages/css/scripts/verify-napi-sort.mjs`                 | 12/12 (JS sort.ts vs Rust NAPI direct) |
| `bun run packages/css/scripts/verify-engine-flag.mjs`               | 12/12 (sort.ts under both engines, subprocess-isolated) |

### Lifecycle ordering — load-bearing

**The plugins in `postcss([A, B, C])` do NOT run in array order.**
Postcss's actual execution lifecycle is:

1. **All `Once` hooks** fire first, in plugin array order.
2. **Per-node visitors** (`Rule`, `AtRule`, `Decl`, `Comment`) fire
   during a depth-first walk of root.
3. **All `OnceExit` hooks** fire, in plugin array order.

For `sort.ts`'s `[discardDuplicates, mergeDuplicateAtRules, sortAtomicStyleSheet]`:

| Plugin                       | Hooks                       | Lifecycle position |
|------------------------------|-----------------------------|--------------------|
| postcss-discard-duplicates@6 | OnceExit only               | step 3, first      |
| mergeDuplicateAtRules        | AtRule visitor + OnceExit   | step 2 + step 3, second |
| sortAtomicStyleSheet         | Once only                   | step 1             |

So the *actual* execution order is:
`sortAtomicStyleSheet.Once → mergeDuplicateAtRules.AtRule visitor → postcss-discard-duplicates.OnceExit → mergeDuplicateAtRules.OnceExit`.

Calling them naively in array order in Rust silently changes which
node sits at index 0 of root when discard-duplicates runs, which
changes which `Root.removeChild` raws-transfers fire — the bytes drift
without any obvious signal.

`crates/css/src/sort.rs:35` is the canonical example. **For Phase 8b
(`transformCss`), every plugin in `transform.ts` will need to be
classified the same way before composing them.** The mistake is
trivial to make and produces silent byte drift.

### Parity-contract drift — RESOLVED via root `package.json` overrides

Initial audit found bun's caret ranges had silently drifted past the
pins in `crates/PARITY_VERSIONS.md` and `REFERENCE_LOCK_FILE/yarn.lock`:

| Package                      | Reference pin | Pre-fix (bun)  | Post-fix |
|------------------------------|---------------|----------------|----------|
| postcss                      | 8.4.31        | 8.5.13         | 8.4.31 ✅ |
| postcss-selector-parser      | 6.0.13        | 6.1.2 (6.0.13 NOT INSTALLED) | 6.0.13 ✅ |
| postcss-discard-duplicates   | 6.0.0         | 6.0.3          | 6.0.0 ✅ |
| autoprefixer                 | 10.4.14       | 10.5.0         | 10.4.14 ✅ |
| postcss-nested               | 5.0.6         | 5.0.6          | 5.0.6 ✅ |
| postcss-normalize-whitespace | 5.1.1         | 5.1.1          | 5.1.1 ✅ |
| postcss-values-parser        | 6.0.2         | 6.0.2          | 6.0.2 ✅ |
| cssnano-preset-default       | 5.2.14        | 5.2.14         | 5.2.14 ✅ |

Fix: an `overrides` block in root `package.json` pinning every
byte-affecting dep to its EXACT reference version, followed by
`bun install`. **Bun does not support nested `overrides`**, so the
Yarn-style nested resolution for the transitive
`cssnano-preset-default → postcss-discard-duplicates@5.1.0` is not
expressible. The global override forces all instances to 6.0.0, but
per `PARITY_VERSIONS.md` Anomaly #5 the transitive 5.1.0 is filtered
out by `normalize-css.ts:62-72` before execution and never reaches
the hashing path — so this is harmless.

**Every parity stage was re-run against the pinned oracle and remains
byte-clean** (208 corpus inputs across 16 gates):

| Gate                                | Corpus size | Result |
|-------------------------------------|-------------|--------|
| postcss-core-roundtrip              | 12          | OK |
| discard-empty-rules                 | 16          | OK |
| discard-duplicates (local)          | 11          | OK |
| extract-stylesheets                 | 12          | OK |
| parent-orphaned-pseudos             | 13          | OK |
| flatten-multiple-selectors          | 11          | OK |
| increase-specificity                | 12          | OK |
| normalize-current-color             | 10          | OK |
| atomicify-rules                     | 24          | OK |
| expand-shorthands                   | 38          | OK |
| merge-duplicate-at-rules            | 7           | OK |
| sort-atomic-style-sheet             | 12          | OK |
| npm-postcss-discard-duplicates      | 8           | OK |
| sort (end-to-end)                   | 12          | OK |
| verify-napi-sort.mjs                | 12          | OK |
| verify-engine-flag.mjs              | 12          | OK |

The Rust ports were already byte-clean against 8.4.31 / 6.0.13 / 6.0.0
/ 10.4.14 — the previous "byte-clean" claim against drifted versions
held only because patch-level diffs happened to be byte-irrelevant on
the existing corpus. Future sessions: **always run `bun install` and
verify `node_modules/.bun/` resolves the reference pins before
declaring byte-clean.**

## Phase 7 split contract — autoprefixer parallel agents

`crates/autoprefixer/` is being ported by two agents in parallel. The
boundary is **physical** — each agent's tree is non-overlapping except
for one shared registration file. Read this before claiming Phase 7
work.

### Tree split

| Path                                          | Owner            |
|-----------------------------------------------|------------------|
| `crates/autoprefixer/src/*.rs` (top-level)    | foundation agent |
| `crates/autoprefixer/src/data/`               | foundation agent |
| `crates/autoprefixer/src/hacks/*.rs`          | hacks agent      |
| `crates/autoprefixer/src/hacks/HACKS_PORT.md` | hacks agent (checklist + progress tracker) |
| `crates/autoprefixer/src/prefixes.rs`         | **shared** — append-only `register_hacks()` block |

The hacks agent reads `crates/autoprefixer/src/hacks/HACKS_PORT.md`
for the per-hack parent-class table, the trait surface, and the
registration contract.

### Foundation agent's responsibilities (in order)

1. ✅ Vendor source under `crates/_vendor/autoprefixer-10.4.14/`.
2. ✅ Scaffold crate + module tree (compiles cleanly).
3. ✅ Port leaf utilities (`utils`, `vendor`, `brackets`, `old_value`,
   `old_selector`).
4. ✅ Port `prefixer.rs` — full. `parent_prefix` walks ancestors via
   `postcss_core::walk_up_with`, caches answers via `Node.attrs`
   (`_autoprefixerPrefix`), `clone_node` delegates to
   `Node::clone_without(CLONE_STRIP_KEYS)`. Tests pin all 5 cases.
5. ✅ Port `at_rule.rs` — full `add` + `process`. Uses
   `parent_some` for the sibling-existence guard,
   `insert_before_at_path` for the clone insert.
6. ⬜ Port `browsers.rs` (browserslist-shim integration).
7. ⬜ Port `data/prefixes.rs` (~1100 LOC static data table — codegen
   from `data/prefixes.js`).
8. ⬜ Port mid-tier base classes: `value.rs`, `selector.rs`,
   `resolution.rs` (signature-only stubs in place; trait surface
   locked).
9. ⬜ Port heavier base classes: `declaration.rs`, `supports.rs`,
   `transition.rs`.
10. ⬜ Port `prefixes.rs` (registry — wires hacks → declaration types).
11. ⬜ Port `processor.rs` + `info.rs` + `autoprefixer.rs` (entry
    point).
12. ⬜ Add `Stage::Autoprefixer` parity-runner gate (requires
    parity-runner edits + parity-bridge.mjs — re-ask permission).
13. ⬜ Wire into `crates/css/src/transform.rs` (re-ask permission).

### Path-shift gotcha — load-bearing for every base class

JS holds a node *reference* across `parent.insertBefore(node, cloned)`
calls — the reference auto-follows when the original's index shifts.
The Rust port uses *index paths*. Each successful
`insert_before_at_path(root, path, clone)` shifts the original's index
in its parent up by 1 because the clone is spliced at the original's
slot. **The path becomes stale** the moment the insert returns.

Fix pattern (see `at_rule.rs::process`):

```rust
let mut current_path = path.to_vec();
for prefix in &prefixes {
    if self.add(root, &current_path, prefix).is_some() {
        if let Some(last) = current_path.last_mut() { *last += 1; }
    }
}
```

**`value.js`, `selector.js`, `declaration.js` all do the same `parent
.insertBefore` pattern in a loop.** Apply the same path-bump on every
successful insert. The bug is silent: tests that only insert one
prefix won't catch it; tests that insert two or more will.

If the hacks agent ports a hack that does its own insert outside the
base class's `add`, follow the same pattern.

### Hacks agent's responsibilities

1. **Wait for foundation tasks #7 + #8 to land before starting.**
   The base-class trait surface isn't final until then; starting
   earlier guarantees rework.
2. Pick a hack from the table in `HACKS_PORT.md`. Take it 0 → 100%
   byte-clean (per cardinal rule).
3. Register it in `crates/autoprefixer/src/prefixes.rs::register_hacks`
   in alphabetical-by-JS-filename order.
4. Mark the row in `HACKS_PORT.md` Done.

### What the hacks agent must NOT do

- Edit anything outside `src/hacks/` (one exception: append-only
  registration in `src/prefixes.rs`).
- Add methods to base traits. If a hack needs a method that isn't on
  `Declaration`/`Value`/`Selector`/`AtRule`, file a note in
  `HACKS_PORT.md` and pause — the foundation agent owns base-class
  shape.
- Re-port `flex-spec.js` or `grid-utils.js` as classes. They're
  shared helpers; port as plain functions in
  `hacks/flex_spec.rs` / `hacks/grid_utils.rs`.

### Current handshake state

- Crate compiles. `cargo test -p autoprefixer` → 11/11 passing.
- Hacks agent **cannot start** until base classes (foundation tasks
  #7 + #8) land. The trait surface signature would change otherwise.
- Foundation agent will not finish in one session. Phase 7 is multi-
  session by design; STATUS.md tracks per-task completion.

## Foundational infrastructure (load-bearing for plugin ports)

These exist and are byte-tested. Plugin authors depend on them; do NOT
re-implement helpers. Add new ones in the appropriate crate:

### `postcss-core` (postcss@8.4.31 port)

- AST types (Root / AtRule / Rule / Declaration / Comment).
- Parser + tokenizer + stringifier with full `raws` preservation.
- **Stringifier raw-defaults**: `rawBeforeRule`, `rawBeforeDecl`,
  `rawBeforeComment`, `rawBeforeClose` scans cached on first use.
  Without these, plugin-driven replacements emit concatenated rules
  with no separator.
- **`container::remove_at`** — Root.removeChild override
  (postcss/lib/root.js): when removing the first child of root, the
  removed node's `raws.before` transfers to the new first child.
  ALL plugin-driven removals at root level MUST go through
  `remove_at`, not raw `Vec::remove`.
- **`container::replace_with_at`** — `node.replaceWith(...)` semantics
  (insertBefore-each-then-remove with Root.normalize override). Used
  internally by `each_mut` / `walk_mut`'s `Mutation::Replace` and
  `Mutation::ReplaceMany`.
- **`Rule::get_selectors` / `set_selectors`** — comma-split with
  `,\s*` separator preservation on join (`rule.selectors` get/set).
- **`list::comma` / `list::space`** — trimmed value-list splitters.
- **`stringify_node(node)`** — port of postcss `node.toString()` (no
  leading raws.before; first-child-of-root context).

### `postcss-selector-parser` (6.0.13)

- Tokenizer + parser + typed AST (ClassName, Identifier, Pseudo,
  Attribute, Combinator, etc.).
- **Compound-selector splitting** (`.foo.bar`, `tag.x#id`) into
  multiple typed nodes.
- **Pseudo arg storage**: prefix only (`:not`) on `value`, parens
  rebuilt from `nodes` at stringify time so plugin mutations to inner
  selectors flow through.
- **`walk_pseudos` / `walk_classes` / `walk_attributes`** mutating
  walkers with parent-context callbacks.
- **`Node::nesting()` / `Node::pseudo(value)`** factories.

### `postcss-values-parser` (6.0.2 plural — distinct from value-parser)

- Tokenize + parse + classify (Numeric, Word, Func, Quoted,
  Punctuation, Operator, UnicodeRange, AtWord, Comment).
- **`stringify_standalone(node)`** — port of `node.toString()` for the
  values-parser node hierarchy (skips outer `raws_before`; Funcs emit
  child `raws_before` inside parens).

### `sjcompiled-utils`

- `hash` — bit-identical to JS `murmurhash2_gc`. **Do not re-port.**
- `unique` / `flatten` / `kebab_case` / `to_boolean`.
- `INCREASE_SPECIFICITY_SELECTOR = ":not(#\\#)"`.
- `shorthand_buckets` (67 entries) / `shorthand_for` table.

### `colord` (2.9.1)

Full color parse / manipulation / minification surface. Phase 6g
(`postcss-colormin` / `postcss-minify-gradients`) consume this.

### `caniuse-db` / `caniuse-api` / `browserslist-shim`

Pinned data + query helpers for `autoprefixer` and the browserslist-
aware cssnano plugins.

## Workspace layout

`crates/Cargo.toml` has 33 members (32 + `compiled-css-napi` added in
Phase 8a). Naming:
- `cssnano-postcss-*` — the 14 cssnano sub-plugins, prefixed to
  disambiguate from same-named npm packages (e.g. distinguishing
  `postcss-normalize-string` from any future v6/v7 fork).
- `postcss-*` — the 4 plugins consumed directly by `transform.ts` /
  `sort.ts`.
- `cssnano-preset-default` — the preset orchestrator.
- `compiled-css-napi` — the Phase 8 NAPI bridge crate (cdylib + rlib).
- Foundation crates keep their upstream names where unambiguous.

`packages/css-native/` is the npm wrapper around the compiled `.node`
binary. Phase 8a ships `sjcompiled-css.win32-x64-msvc.node` only;
Phase 8b extends to linux-x64-gnu / linux-arm64-gnu / darwin-x64 /
darwin-arm64 once `transformCss` is byte-clean.

## What's left to port (full source-faithful Rust ports)

9 crates. Listed in roughly ascending complexity:

1. `postcss-ordered-values` — moderate. Reorders multi-value
   shorthand parts.
2. `postcss-minify-selectors` — moderate. Selector minification using
   postcss-selector-parser.
3. `postcss-normalize-unicode` — moderate, browserslist-aware.
4. `postcss-reduce-initial` — moderate, caniuse-aware.
5. `postcss-convert-values` — hard, uses fraction-js, browserslist.
6. `postcss-minify-params` — hard, caniuse-aware.
7. `postcss-minify-gradients` — hard, colord-heavy.
8. `postcss-calc` — VERY hard. Effectively a small expression compiler.
9. `postcss-colormin` — HARDEST cssnano plugin. Color downgrade
   decisions hinging on caniuse + colord rounding + byte-length
   comparison.
10. `postcss-nested` (Phase 5a) — VERY hard. Recursive selector
    merging with bubble/unwrap config.
11. `cssnano-preset-default` — moderate orchestrator (depends on
    1-9 being byte-clean first).

Plus Phase 7 (autoprefixer — 8+ weeks of its own) and Phase 8
(NAPI assembly + the `transformCss` / `sort` end-to-end gates).

## Recommended order for the next session

`sort()` is now byte-clean end-to-end through NAPI (Phase 8a) and
Phase 5b (`postcss-normalize-whitespace`) is byte-clean across a
20-entry corpus. The remaining work is all `transformCss`-bound. The
cardinal-rule guidance remains: **a session must take a unit from 0%
→ 100% byte-clean**. Half-done ports become silent byte-drift hazards
across agent handoffs.

The Phase 6b "simple band" (discard-comments, normalize-string,
normalize-positions, normalize-timing-functions, normalize-url) is **fully
byte-clean**. The remaining cssnano work is moderate-to-hard.

1. **Phase 6 moderate band** — pick one of:
   - `minify-selectors@5.2.1` (uses postcss-selector-parser; moderate).
   - `ordered-values@5.1.3` (multi-value reordering; moderate).
   Finish one before starting the next.
2. **Phase 5a** (`postcss-nested`) — multi-day commitment. Recursive
   selector merging. Don't start unless you have time to finish.
3. **Phase 6 hard band** — `ordered-values`, `minify-selectors`,
   `normalize-url`, `normalize-unicode`, `reduce-initial`,
   `convert-values`, `minify-params`, `minify-gradients`. Each
   multi-day.
4. **Phase 6h** — `postcss-calc` (small expression compiler) and
   `postcss-colormin` (HARDEST cssnano plugin). Each multi-week.
5. **Phase 7** — `autoprefixer@10.4.14`. ~8 weeks for one engineer.
7. **Phase 6h orchestrator** — `cssnano-preset-default` once 1–6
   are byte-clean.
8. **Phase 8b** — `transformCss` NAPI export + `transform.ts` engine
   flag, mirroring the Phase 8a pattern below.

## Phase 5a ship — `postcss-nested@5.0.6` byte-clean

The largest single pre-autoprefixer plugin port. Recursive selector
merging with `bubble`/`unwrap`/`at-root` semantics — runs inside
`transform.ts:48-61` and is on the hashing path for every nested rule
in every consumer. Now byte-clean.

### What landed this session

1. `crates/_vendor/postcss-nested-5.0.6/` — vendored upstream source
   (215 LOC `index.js` + LICENSE/README/package.json/index.d.ts).
2. `crates/postcss-nested/src/lib.rs` — full port. Single source file
   maps 1:1. Public surface: `postcss_nested(root, opts)` +
   `PostcssNestedOpts { bubble, unwrap, preserve_empty }`. Module-level
   helpers mirror upstream functions verbatim (`atrule_names`,
   `parse_selector`, `replace_nesting`, `selectors_of`,
   `build_wrapper_rule`, `atrule_childs`, `clone_rule_with_empty_nodes`,
   `insert_after_with_normalize`, `is_comment`, `visit_rule`,
   `walk_container`).
3. `crates/parity-runner/Cargo.toml` — added `postcss-nested` workspace
   dep so the stage handler can call into it.
4. `crates/parity-runner/src/stages.rs` — `Stage::PostcssNested`
   variant + handler with the production `bubble`/`unwrap` opts from
   `transform.ts:48-61` baked in (so the parity gate validates the
   exact configuration that ships).
5. `crates/parity-runner/src/main.rs` — CLI mapping for
   `postcss-nested`.
6. `packages/css/scripts/parity-bridge.mjs` — JS-side stage that runs
   `postcss([postcssNested({bubble, unwrap})]).process(css)` against
   the pinned 5.0.6 oracle, with the matching opts.
7. `crates/parity-runner/corpus/postcss-nested/` — 38 fixtures
   covering: blank, no-nesting, simple nest, `&` substitution,
   comma-list parent + comma-list child, deep nest (3-4 levels),
   bubble (`@media`/`@supports`/`@container`/`@starting-style`/`@layer`
   bubble-list), unwrap (`@keyframes`/`@font-face`/`@page` + vendor
   `@-webkit-keyframes`), `@at-root` (with and without params),
   comments interleaved between sibling rules, decls before AND after
   nested rules (split-decls), mixed decl/rule/decl shape, deep nest
   inside bubble at-rule, top-level rule with no decls + only nested,
   pseudo-nested (`:hover`/`::before`), `& + &`/`& ~ &`,
   `&--modifier` BEM-style suffix joining, `&:not(...)` pseudo-arg,
   `.b &` (descendant + nesting) for the spaces-transfer fix,
   tag-and-amp (`section { &.x; & > x }`), attribute selectors
   (`[type="text"]`), bubble + trailing decls, realistic atomic CSS
   with multiple `&` patterns inside `@media`.
8. Bug-for-bug fidelity replicated: `parse(str, rule)`'s "Missed
   semicolon" branch (when parse fails AND `str` contains `:`),
   `atruleNames` strip-`@` normalization, `bubble`/`unwrap`/`at-root`
   ordering of the at-rule sub-branches inside the visitor, the
   "dump pending declarations at top of EVERY at-rule sub-branch"
   detail (including the `copy_declarations` fall-through), the
   `replace`-recurses-only-into-nodes-with-children quirk, and the
   `if (j.length)` skip-empty-selector guard.

### Verification gates run

| Gate                                                                   | Status |
|------------------------------------------------------------------------|--------|
| `cargo test -p postcss-nested`                                         | 4/4 pass |
| `cargo test --workspace --no-fail-fast`                                | 462/462 pass |
| `parity-runner --stage postcss-nested --corpus ...`                    | 38/38 byte-clean |
| `parity-runner --stage postcss-nested ... --determinism`               | 38/38 deterministic |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort` | 12/12 (no regression) |
| `parity-runner --stage merge-duplicate-at-rules ...`                   | 7/7 (no regression) |
| `parity-runner --stage atomicify-rules ...`                            | 24/24 (no regression) |
| `parity-runner --stage expand-shorthands ...`                          | 38/38 (no regression) |
| `parity-runner --stage postcss-normalize-whitespace ...`               | 22/22 (no regression) |

### Walker design — single forward pass, no re-walk

Upstream's `Rule(rule, { Rule })` visitor is invoked on every Rule
discovered in document order, and re-visits Rules promoted to siblings
during the walk. The Rust port does NOT need a postcss-style re-walk
because postcss-nested's promotion always moves children OUT of the
rule (never back in). After visiting a rule, its remaining children
(decls and comments only — no rules) need no further processing.

The walker is a recursive `walk_container` that:

1. Iterates `parent.nodes` by index forward.
2. For each Rule encountered: invokes `visit_rule`, which removes the
   rule from its parent, processes its children, then re-inserts the
   rule plus any promoted siblings at the same position. The cursor
   advances 1 each iteration; promoted siblings end up at `i+1`,
   `i+2`, ... and are visited on subsequent iterations.
3. AtRules with bodies are recursed INTO (rules can be nested inside
   `@media`-style bubble at-rules — e.g. `.a { @media x { .b {} } }`
   ends up as `@media x { .a .b {} }` after `.a`'s visitor, and the
   walker descends into `@media` to visit `.a .b`).
4. Other node kinds skipped.

### `raws` defaults — handled by postcss-core stringifier

Fresh wrapper Rules created by `pickDeclarations` (and the at-root
params-wrapper branch) carry NO explicit `raws`. The postcss-core
stringifier derives `raws.between` (via `rawBeforeOpen` scanner),
`raws.semicolon` (via `rawSemicolon` scanner), and `raws.after` (via
`rawBeforeClose` scanner) from the surrounding tree at stringify time
— mirroring `postcss/lib/stringifier.js::raw`. This matches what JS
upstream emits for `new Rule({ selector, nodes: [] })` byte-for-byte.

(Earlier in this session those scanners weren't implemented and this
plugin hard-coded the inherited values. After the postcss-core fixes
landed — `rawSemicolon`, `rawBeforeOpen`, `rawColon`, plus the
`insert_before_with_normalize` Root-prepend strip-step guard — the
hard-coded values were stripped. All 21 parity stages remain
byte-clean and 465/465 workspace tests pass with no plugin-side
workarounds in this crate.)

### Postcss-selector-parser quirk — descendant-combinator emission

Upstream JS `postcss-selector-parser@6.0.13` emits an explicit
`Combinator{value: " "}` node for descendant whitespace combinators
(see `dist/parser.js::combinator` lines 480-568). The Rust port at
`crates/postcss-selector-parser/src/parser.rs` does NOT — it stores
descendant whitespace as the next node's `spaces.before`. This is a
parser-side divergence from upstream. To preserve byte-clean output
for `.b & { ... }`-style inputs, `replace_nesting` transfers the
Nesting's `spaces` onto the replacement node before splicing. In JS
this transfer isn't needed because the Combinator sits BETWEEN the
preceding selector and the Nesting; in our Rust port the space is
fused onto the Nesting itself, so it must be moved with the
replacement. **Filed as a postcss-selector-parser bug** — when that
bug is fixed, `replace_nesting`'s `new_node.spaces = nesting_spaces`
line should be removed (it'll become a double-space).

### Lessons from Phase 5a — apply to every future port

- **Postcss `rawCache` defaults are load-bearing.** Any plugin that
  creates fresh nodes (Rule/AtRule/Decl/Comment) inherits styling via
  `rawCache` walks at stringify time. The `postcss-core` stringifier
  now implements `rawBefore{Rule,Decl,Comment,Close,Open}`,
  `rawSemicolon`, and `rawColon`, mirroring upstream
  `stringifier.js::raw`. New plugins should construct fresh nodes with
  NO explicit `raws` and let the stringifier derive defaults — do not
  hard-code inherited values from a source node, that's a divergence
  trap.
- **Postcss-selector-parser doesn't emit descendant Combinators.**
  See the section above. Plugins that mutate selector ASTs around
  Nesting/Combinator nodes need to be aware of the
  spaces-on-next-node convention. Document this in any new
  selector-parser-touching plugin.
- **`atruleChilds(rule, child, false)` does NOT remove children from
  `child.nodes`.** Upstream JS pushes references into a local
  `children` list that's only consumed when `bubbling=true`. Our
  initial port removed unconditionally and broke unwrap (`@font-face`
  body went missing). Mirror upstream: only remove when `bubbling`.
- **Borrow-checker pattern: take-rule-out-of-parent.** Postcss visitors
  conceptually mutate `rule` AND `rule.parent` simultaneously. In Rust
  the ergonomic move is `parent.nodes_mut().unwrap().remove(rule_index)`
  to take ownership, then mutate `rule` and accumulate "promoted
  siblings" in a local Vec, then re-insert `rule` at `rule_index` and
  chain-insert promoted via `insert_after_with_normalize`. Avoids all
  borrow conflicts; matches upstream behavior including the
  Root.removeChild raws-transfer if `rule` is ultimately removed.
- **JS `child.prev()` / `pickComment` interaction with the iterator.**
  The JS iterator decrements when nodes are removed during the walk;
  combined with `pickComment` (which removes the prev-sibling comment
  before the current node) and `after.after(child)` (which removes
  the current node), the iterator lands on what was originally
  `index + 1`. In Rust the equivalent is "don't increment `i` when
  the current node was moved out (with optional pickComment of
  `nodes[i - 1]`)" — see `visit_rule` rule and at-rule branches.

## Phase 5b ship — `postcss-normalize-whitespace@5.1.1` byte-clean

Single OnceExit-only plugin that runs inside `transform.ts`'s pipeline
(blocking Phase 8b end-to-end `transformCss` parity). Now byte-clean.

### What landed this session

1. `crates/postcss-normalize-whitespace/src/lib.rs` — full port of
   `node_modules/postcss-normalize-whitespace@5.1.1/src/index.js`. The
   single source file maps 1:1 (file/folder shape preserved).
2. `crates/postcss-normalize-whitespace/Cargo.toml` — added `once_cell`
   and `indexmap` deps. Pre-existing `postcss-core`,
   `postcss-value-parser`, `regex` already declared.
3. `crates/parity-runner/Cargo.toml` — added
   `postcss-normalize-whitespace` workspace dep so the stage handler
   can call into it.
4. `crates/parity-runner/src/stages.rs` —
   `Stage::PostcssNormalizeWhitespace` variant + handler.
5. `crates/parity-runner/src/main.rs` — CLI mapping for
   `postcss-normalize-whitespace`.
6. `packages/css/scripts/parity-bridge.mjs` — JS-side stage that runs
   `postcss([postcssNormalizeWhitespace()]).process(css)` against the
   pinned 5.1.1 oracle.
7. `crates/parity-runner/corpus/postcss-normalize-whitespace/` — 22
   fixtures covering: blank, simple rule, multi-decl, calc inner WS,
   var/env exemption, IE9 hack (single-replace, no `g` flag), excess
   `!important` whitespace, `--*` empty value, atrule, nested atrule,
   url, comments, multi-calc cache hits, multi-value shorthand,
   quoted strings, mixed rule/decl neighbors, statement atrules
   (`@charset` / `@import`), pseudo selectors, nested calc, realistic
   atomic CSS, transform/translate function-chain spacing
   (translate3d / matrix / rotate / scale / perspective with
   pathological whitespace, multiline transform with embedded calc).

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p postcss-normalize-whitespace`                                    | 12/12 pass |
| `cargo test --workspace --no-fail-fast`                                         | 368/368 pass |
| `parity-runner --stage postcss-normalize-whitespace --corpus ...`               | 22/22 byte-clean |
| `parity-runner --stage postcss-normalize-whitespace ... --determinism`          | 22/22 deterministic |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

## Phase 6a ship — `postcss-discard-comments@5.1.2` byte-clean

cssnano sub-plugin. Removes comments from the AST and scrubs them out
of inline raws (`raws.between`, decl `raws.value.raw`,
rule `raws.selector.raw`, atrule `raws.afterName` / `raws.params.raw`).
Default opts keep `/*!` important comments.

### What landed this session

1. `crates/cssnano-postcss-discard-comments/src/lib.rs` — main port of
   `node_modules/postcss-discard-comments@5.1.2/src/index.js`.
2. `crates/cssnano-postcss-discard-comments/src/comment_parser.rs` —
   port of `src/lib/commentParser.js`. The upstream `lib/` parent
   directory is dropped because Rust's crate-root file is itself
   `lib.rs` and a child module literally named `lib` collides;
   behavior is unaffected. Includes the unclosed-comment quirk
   (`indexOf('*/') === -1` → upstream produces `pos = 1` and a
   sentinel slice — replicated via `UNCLOSED_END = usize::MAX`).
3. `crates/cssnano-postcss-discard-comments/src/comment_remover.rs` —
   port of `src/lib/commentRemover.js`. Tri-state predicate matching
   upstream (`Some(true)` remove / `Some(false)` keep / `None` for
   upstream `undefined` fall-through which JS treats as falsy → keep).
4. Stage + bridge wiring: `Stage::PostcssDiscardComments`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-discard-comments: 5.1.2`),
   `packages/css/package.json` devDependency.
5. 15-fixture corpus covering: blank, no-comments, top-level comment,
   `/*!` important kept, comments inside rule bodies, comments inside
   decl values, comments inside selectors and selector lists, comments
   in atrule params and afterName, comments in `raws.between` (decl
   prop/colon split), comments around `!important`, mixed
   important/normal, atrules without bodies (`@charset`/`@import`),
   nested atrules, consecutive comments, realistic atomic CSS.

### Verification gates run

| Gate                                                                    | Status |
|-------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-discard-comments`                        | 12/12 pass |
| `parity-runner --stage postcss-discard-comments --corpus ...`           | 15/15 byte-clean |
| `parity-runner --stage postcss-discard-comments ... --determinism`      | 15/15 deterministic |

## Phase 6b ship — `postcss-normalize-string@5.1.0` byte-clean

cssnano sub-plugin. Walks rule selectors, decl values, and atrule
params; rewraps string literals to the preferred quote style (default
`'double'`) when the swap reduces escapes, and collapses `\\\n`
(escaped newline) inside string bodies.

### What landed this session

1. `crates/cssnano-postcss-normalize-string/src/lib.rs` — full port of
   `node_modules/postcss-normalize-string@5.1.0/src/index.js`.
   Single source file maps 1:1.
2. The bespoke string-AST parser (`ast_parse`) is a hand-rolled
   byte-scan with the same `[ \n\t\r\f'"\\]` word-end character class
   as upstream's `WORD_END` regex, including the intentional
   fall-through from the backslash branch into the default word
   branch (upstream's "missing `break`" bug — replicated verbatim).
3. Stage + bridge wiring: `Stage::PostcssNormalizeString`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-string: 5.1.0`),
   `packages/css/package.json` devDependency.
4. 15-fixture corpus covering: blank, no-strings, single/double-quoted
   plain values, escaped-double-in-double, escaped-single-in-single,
   mixed escapes, bare quote inside opposite wrap, empty strings,
   attribute selectors with both quote styles, `url(...)` with
   quotes, font-family lists, atrule string params
   (`@charset`/`@import`), escaped newline collapse, realistic atomic
   CSS.

### Verification gates run

| Gate                                                                  | Status |
|-----------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-string`                      | 7/7 pass |
| `parity-runner --stage postcss-normalize-string --corpus ...`         | 15/15 byte-clean |
| `parity-runner --stage postcss-normalize-string ... --determinism`    | 15/15 deterministic |

## Phase 6b ship — `postcss-normalize-positions@5.1.1` byte-clean

cssnano sub-plugin. Walks decls matching
`/^(background(-position)?|(-\w+-)?perspective-origin)$/i` and
rewrites position-keyword pairs to length values per upstream rules
(`left top` → `0 0`, `right bottom` → `100% 100%`, `top right` →
`100% 0`, etc.). `var()`/`env()`/`constant()` short-circuits the
current background entry; `/` defers to background-size.

### What landed this session

1. `crates/cssnano-postcss-normalize-positions/src/lib.rs` — full port
   of `node_modules/postcss-normalize-positions@5.1.1/src/index.js`.
   Single source file maps 1:1.
2. `crates/cssnano-postcss-normalize-positions/Cargo.toml` — added
   `indexmap`, `regex`, `once_cell` workspace deps. Pre-existing
   `postcss-core` and `postcss-value-parser` already declared.
3. Stage + bridge wiring: `Stage::PostcssNormalizePositions`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-positions: 5.1.1`),
   `packages/css/package.json` devDependency.
4. Sparse-array semantics replicated via `Vec<Option<Range>>` so
   `forEach` skips holes (matches the JS sparse-array case where a
   layer's range slot is never populated, e.g. when an entry hits `/`
   before any position keyword).
5. `parseFloat(value)` not-NaN equivalence implemented as
   `parse_unit(value).is_some()` — `like_number`'s prefix check
   matches JS `parseFloat`'s "valid leading numeric" semantics
   exactly.
6. 20-fixture corpus covering: blank, no-position decls, `left top`,
   `right bottom`, vertical-first swap, center pair collapse, single
   keyword, `var()` short-circuit, `/` background-size guard, comma
   layer reset, three-value-skip rule, dimensions/numbers, calc/min/
   max math fns, vendor `-webkit-`/`-moz-perspective-origin`,
   `!important`, atrule nesting, uppercase keywords, mixed
   keyword+dimension, realistic atomic CSS, empty value.

### Verification gates run

| Gate                                                                    | Status |
|-------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-positions`                     | 12/12 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)     | 410/410 pass |
| `parity-runner --stage postcss-normalize-positions --corpus ...`        | 20/20 byte-clean |
| `parity-runner --stage postcss-normalize-positions ... --determinism`   | 20/20 deterministic |
| `parity-runner --stage postcss-normalize-string ...`                    | 15/15 (no regression) |
| `parity-runner --stage postcss-discard-comments ...`                    | 15/15 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`  | 12/12 (no regression) |

## Phase 6b ship — `postcss-normalize-timing-functions@5.1.0` byte-clean

cssnano sub-plugin. Walks decls matching
`/^(-\w+-)?(animation|transition)(-timing-function)?$/i` and rewrites
timing functions to keyword equivalents:

- `cubic-bezier(0.25, 0.1, 0.25, 1)` → `ease`.
- `cubic-bezier(0, 0, 1, 1)` → `linear`.
- `cubic-bezier(0.42, 0, 1, 1)` → `ease-in`.
- `cubic-bezier(0, 0, 0.58, 1)` → `ease-out`.
- `cubic-bezier(0.42, 0, 0.58, 1)` → `ease-in-out`.
- `steps(1, start | jump-start)` → `step-start`.
- `steps(1, end | jump-end)` → `step-end`.
- `steps(N, end | jump-end)` → `steps(N)` (browser default).

### What landed this session

1. `crates/cssnano-postcss-normalize-timing-functions/src/lib.rs` —
   full port of `node_modules/postcss-normalize-timing-functions@5.1.0/
   src/index.js`. Single source file maps 1:1.
2. `Cargo.toml` — added `indexmap`, `regex`, `once_cell` workspace
   deps. Pre-existing `postcss-core` and `postcss-value-parser` already
   declared.
3. JS `parseFloat(s)` parity implemented as
   `parse_unit(s).map(|u| u.number.parse::<f64>().ok()).flatten()` —
   `like_number`'s prefix check matches parseFloat's "valid leading
   numeric" semantics exactly (handles `"0.25"`, `".25"`, `"1px"`,
   `"1e-2"`, etc.).
4. cubic-bezier conversion-table key built via
   `js_number_to_string(v)` joined by `,` — exact 1:1 with JS
   `[a,b,c,d].toString()`. Uses the existing postcss-core helper
   instead of a bespoke formatter.
5. Stage + bridge wiring: `Stage::PostcssNormalizeTimingFunctions`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-timing-functions: 5.1.0`),
   `packages/css/package.json` devDependency.
6. 21-fixture corpus covering: blank, no-timing decls, all 5
   cubic-bezier→keyword conversions, unknown-bezier passthrough,
   `steps(1, start|jump-start)` → step-start, `steps(1, end|jump-end)`
   → step-end, `steps(N, end|jump-end)` → `steps(N)`,
   `steps(N)` (already minimal), keyword passthrough (ease/linear/
   step-start), comma-list (multiple bezier+steps), transition
   shorthand inside `transition` / `animation`, vendor prefixes
   (`-webkit-`/`-moz-`/`-ms-`), uppercase keywords/property names,
   `!important`, atrule nesting (`@media`/`@keyframes`), realistic
   atomic CSS, decimal variants (`.25` vs `0.25`, `1.0` vs `1`).

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-timing-functions`                      | 15/15 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)             | 425/425 pass |
| `parity-runner --stage postcss-normalize-timing-functions --corpus ...`         | 21/21 byte-clean |
| `parity-runner --stage postcss-normalize-timing-functions ... --determinism`    | 21/21 deterministic |
| `parity-runner --stage postcss-normalize-positions ...`                         | 20/20 (no regression) |
| `parity-runner --stage postcss-normalize-string ...`                            | 15/15 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

## Phase 6b ship — `postcss-normalize-url@5.1.0` byte-clean

cssnano sub-plugin. Walks every Decl value and `@namespace` AtRule params;
rewrites `url(...)` calls. Absolute / protocol-relative URLs route through
`normalize-url@6.1.0` (vendored — WHATWG URL canonicalization, default-port
strip, `utm_*` query removal, etc.). Relative paths route through Node's
`path.normalize` (host-OS dependent — see "Lessons from Phase 6b normalize-url"
below). `data:`/`*-extension:/` short-circuit conversion.

The 5 postcss-side overrides on top of normalize-url's defaults:
`normalizeProtocol` / `sortQueryParameters` / `stripHash` / `stripWWW` /
`stripTextFragment` all `false`.

### What landed this session

1. `crates/cssnano-postcss-normalize-url/src/lib.rs` — main port of
   `node_modules/postcss-normalize-url@5.1.0/src/index.js`. Single source
   file maps 1:1.
2. `crates/cssnano-postcss-normalize-url/src/normalize_url.rs` — vendored
   port of `node_modules/normalize-url@6.1.0/index.js`. The `normalize-url`
   npm package is a single-consumer dep (only `postcss-normalize-url`
   uses it on our hashing path) so it lives inside this crate as a sibling
   module rather than a single-consumer crate. WHATWG URL parsing
   delegates to the Rust `url@2.5` crate (also WHATWG-compliant).
3. `crates/cssnano-postcss-normalize-url/src/path.rs` — vendored subset of
   Node `lib/path.js` (`path.posix.normalize` plus a Win32 wrapper). The
   `\` → `/` separator unification on Windows replicates upstream
   `path.win32.normalize(...).replace(/\\/g, '/')` for inputs without
   drive letters / UNC prefixes (which upstream's `WINDOWS_PATH_REGEX`
   filters out before convert() runs).
4. `Cargo.toml` — added `url@2`, `percent-encoding@2`, plus the standard
   `indexmap`/`regex`/`once_cell` workspace deps.
5. Stage + bridge wiring: `Stage::PostcssNormalizeUrl`,
   `parity-bridge.mjs` import, root `package.json` `overrides` pin
   (`postcss-normalize-url: 5.1.0`), `packages/css/package.json` devDep.
6. 60-fixture corpus covering: blank, no-url decls, unquoted/quoted
   relative paths, root-relative, absolute http/https, default-port
   strip (`:80`/`:443`), `utm_*` query strip, data URIs (svg+xml,
   base64, default mime/charset), `chrome-extension:`/`moz-extension:`
   short-circuit, empty `url()`, `..` collapse in relative + absolute
   paths, `@namespace url(...)` rewrite (single + multi-quoted),
   protocol-relative (`//cdn`), spaces in quoted URLs, escaped newline
   inside string, escaped quote inside string (Win32 `\` → `/` path),
   parens in quoted URLs, multiple `url()` per decl, `url()` inside
   `@media`/`@keyframes`/`@supports`, uppercase `URL`/`DATA:`/
   `MOZ-EXTENSION://`, single quotes, percent-encoded path,
   text-fragment (`#:~:text=...`) — kept since stripTextFragment=false,
   `www.` host — kept since stripWWW=false, hash-only fragments,
   `?`-only query, comment-in-value neighbors, no-protocol/no-quotes
   (`url(example.com/path)`), realistic atomic CSS combo.

### Verification gates run

| Gate                                                                            | Status |
|---------------------------------------------------------------------------------|--------|
| `cargo test -p cssnano-postcss-normalize-url`                                   | 33/33 pass |
| `cargo test --workspace --no-fail-fast` (excl. `compiled-css-napi`)             | 458/458 pass |
| `parity-runner --stage postcss-normalize-url --corpus ...`                      | 60/60 byte-clean |
| `parity-runner --stage postcss-normalize-url ... --determinism`                 | 60/60 deterministic |
| `parity-runner --stage postcss-normalize-timing-functions ...`                  | 21/21 (no regression) |
| `parity-runner --stage postcss-normalize-positions ...`                         | 20/20 (no regression) |
| `parity-runner --stage postcss-normalize-string ...`                            | 15/15 (no regression) |
| `parity-runner --stage postcss-discard-comments ...`                            | 15/15 (no regression) |
| `parity-runner --stage postcss-normalize-whitespace ...`                        | 22/22 (no regression) |
| `parity-runner --stage sort --corpus crates/parity-runner/corpus/sort`          | 12/12 (no regression) |

### Lessons from Phase 6b `normalize-url` — apply to every future port

- **Upstream "moderate" is misleading when transitive deps matter.** The
  upstream JS file is ~150 LOC. But it pulls in `normalize-url@6.1.0`
  (~220 LOC + WHATWG URL parser) AND Node `path.normalize`. Real port
  size: ~3 source files, ~750 LOC total. **Always audit transitive deps
  before declaring "moderate".**
- **`require('path')` is OS-dependent.** `path.normalize('foo\\"bar.png')`
  on Windows returns a `\`-replaced output that the upstream then
  forward-slash-converts. POSIX returns the input unchanged. Replicating
  bug-for-bug means the Rust port also splits via `cfg(windows)`. Same
  CSS input → different bytes on Linux vs Windows. The user's monorepo
  builds on Linux, so production hashes are POSIX-shaped; the Windows
  build is for dev parity testing only. **Document host-OS dependencies
  prominently** because they propagate to consumer hashes.
- **Rust `url@2.5` ≈ JS `new URL(...)` for our inputs.** Both implement
  WHATWG URL canonicalization. For 60 corpus entries spanning realistic
  CSS URL inputs, byte-equal serialization. Edge cases with unusual
  schemes or invalid characters MAY differ — corpus coverage is the
  only safety net. Add new fixtures for any URL pattern the consuming
  monorepo produces.
- **`Lazy<Regex>` for ALL upstream regexes — including ones that look
  trivial.** The escape-chars regex `/([\s\(\)"'])/g` is hot path; a
  per-call `Regex::new` would re-compile per `url()`. Caching via
  `once_cell::Lazy` is mandatory for performance, but also clarifies
  the regex source location for byte-parity audits.
- **Lookbehind/lookahead patterns require manual emulation.** Rust
  `regex` rejects `(?<!...)` and `(?!...)`. The `collapse_path_slashes`
  function and the `^(?!(?:\w+:)?\/\/)|^\/\/` protocol-prepend regex
  are both hand-implemented. Cover these explicitly in unit tests.
- **`url::Url::set_query(None)` vs `set_query(Some(""))` matter.** The
  former drops the `?` entirely; the latter keeps it. Upstream's
  `searchParams` mutations can leave a trailing `?` when all params are
  filtered. Match upstream behavior carefully — `searchParams.delete`
  removes pairs but leaves the `?`; assigning `urlObj.search = ''`
  drops it. Both arise in our port.

### Lessons from Phase 6a / 6b — apply to every future port

- **`lib/` subdirectories collide with Rust crate-root naming.** When
  upstream nests sources under `src/lib/<name>.js`, the only
  Rust-legal layout that keeps a 1:1 file map is to flatten the
  `lib/` parent into the crate root and document the deviation in
  each module header.
- **Bug-for-bug fall-through bugs are real.** `postcss-normalize-string`
  has an intentional missing `break` in the switch statement
  (backslash + non-quote → falls through into the default word
  branch). Replicate verbatim, do not "clean up" — class hashes
  downstream depend on byte-equivalent string output.
- **The `replaceComments` separator argument is load-bearing.** For
  rule selectors, separator is `''` (empty), not the default `' '`.
  This can cause previously-separated selector tokens to JOIN when a
  comment lived between them. Replicate exactly — minified selectors
  rely on this.
- **JS bridge dependency resolution:** plugins not in
  `@sjcompiled/css`'s direct deps must be added to its
  `devDependencies` (not just root `overrides`) so
  `parity-bridge.mjs` can `import` them. Without the package-level
  dep, bun's `node_modules` doesn't expose the package to the bridge
  script and `JS bridge closed unexpectedly` errors every fixture.

### Lessons from Phase 5b — apply to every future port

- **ECMAScript `\s` ≠ Rust regex `\s` ≠ Unicode `White_Space`.** JS
  `\s` includes U+FEFF (BOM) but excludes U+0085 (NEL); Rust `\s`
  (Unicode mode) is `\p{White_Space}` which is the opposite. The
  IE9-hack regex hand-rolls the JS character class explicitly so
  parity holds for any input that uses BOMs/NEL inside CSS values.
  Same for the per-character `replace(/\s/g, '')` calls — we ship a
  bespoke `is_es_whitespace` predicate, not `char::is_whitespace`.
- **`variableFunctions` exemption is shallow.** `var()` / `env()` /
  `constant()` keep their *Function*-level `before`/`after` raws, but
  the default reducer recursion still descends and clears Div nodes'
  `before`/`after` inside. So `var( --x , red )` becomes
  `var( --x,red )`, not `var( --x , red )`. Test for this exact
  shape.
- **The IE9-hack regex has NO `g` flag.** First match only. Fixtures
  that have two `\9` occurrences in the same value would only normalize
  the first — replicate the bug, do not "fix" it.
- **OnceExit-only plugins compose into single-plugin pipelines as a
  direct function call.** No per-node visitor / no `Once` hook means
  postcss runs the function once on the parsed root. The Phase 8a
  lifecycle warning still applies: when this plugin lands in the same
  pipeline as another plugin's `Once` / per-node visitor, the
  OnceExit ordering matters and array order ≠ execution order.

### Lessons from Phase 8a — apply to every future port

- **Lifecycle ordering matters.** Read `Phase 8a ship` →
  "Lifecycle ordering — load-bearing" before composing plugins in
  any orchestrator. Array order ≠ execution order. The bug is silent
  and only manifests when multiple plugins share root.
- **`container::append` on Root applies Root.normalize.** Direct
  `Vec::push` on `root.nodes` skips the raws-transfer and produces
  missing-newline drift. Always go through `container::append`.
- **`bun.lock` silently floats past `^X.Y.Z` pins.** Audit every
  pinned version in `PARITY_VERSIONS.md` against the actual
  `node_modules/.bun/` contents before declaring byte-clean. The
  `postcss-discard-duplicates` `^6.0.0 → 6.0.3` drift is one example;
  there are likely others.
- **A new stage needs three coordinated additions:** the Rust
  `Stage::*` variant in `crates/parity-runner/src/stages.rs`, the
  CLI mapping in `main.rs`, and the JS counterpart in
  `packages/css/scripts/parity-bridge.mjs`. Forgetting the JS side
  silently produces "no diff" because both sides hit the unknown-stage
  error path.

## Cardinal-rule conformance check

- ✅ Every Rust crate header names the JS package + version it ports.
- ✅ Every Rust file maps 1:1 to a JS source file in upstream.
- ✅ `IndexMap` used everywhere a HashMap would touch output bytes.
- ✅ No version bumps applied to any pinned package.
- ✅ JS pipeline in `packages/css/src/transform.ts` untouched — Rust
  is additive.
- ⚠️ JS pipeline in `packages/css/src/sort.ts` — modified additively
  to add the `COMPILED_CSS_ENGINE=rust` env-flag gate. Default path
  (flag unset, JS pipeline) is byte-identical to pre-modification
  behavior. JS code stays as the parity oracle.
- ✅ Parity-runner harness wired for every implemented plugin.
- ✅ The CRITICAL hash plugin (`atomicify-rules`) is byte-clean.
- ✅ JS oracle versions match `REFERENCE_LOCK_FILE/yarn.lock` exactly
  for every byte-affecting dep (postcss 8.4.31, postcss-selector-parser
  6.0.13, postcss-discard-duplicates 6.0.0, autoprefixer 10.4.14, etc.)
  via root `package.json` `overrides` block. See "Parity-contract drift
  — RESOLVED" above for the audit table.
