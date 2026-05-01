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
| 5a | postcss-nested@5.0.6 | **SCAFFOLDED** — `unimplemented!()`. Largest single port; budget multi-day. |
| 5b | postcss-normalize-whitespace@5.1.1 | **SCAFFOLDED** — `unimplemented!()`. Walks via postcss-value-parser. |
| 5c | postcss-discard-duplicates@6.0.0 (npm — used by sort.ts) | **DONE** — byte-clean across 8-entry corpus |
| 6a | postcss-discard-comments@5.1.2 | **SCAFFOLDED** |
| 6b | postcss-normalize-string@5.1.0 | **SCAFFOLDED** |
| 6b | postcss-normalize-positions@5.1.1 | **SCAFFOLDED** |
| 6b | postcss-normalize-timing-functions@5.1.0 | **SCAFFOLDED** |
| 6b | postcss-normalize-url@5.1.0 | **SCAFFOLDED** |
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
| 7 | autoprefixer@10.4.14 | **NOT STARTED** — largest single port (~50 files). |
| 8 | NAPI bridge + transformCss / sort assembly | **NOT STARTED** |

## Test totals

`RUSTFLAGS="" cargo test --workspace --no-fail-fast`:
- **354 tests pass / 0 fail / 1 ignored / 0 failed suites.**

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

`crates/Cargo.toml` has 32 members. Naming:
- `cssnano-postcss-*` — the 14 cssnano sub-plugins, prefixed to
  disambiguate from same-named npm packages (e.g. distinguishing
  `postcss-normalize-string` from any future v6/v7 fork).
- `postcss-*` — the 4 plugins consumed directly by `transform.ts` /
  `sort.ts`.
- `cssnano-preset-default` — the preset orchestrator.
- Foundation crates keep their upstream names where unambiguous.

## What's left to port (full source-faithful Rust ports)

15 crates. Listed in roughly ascending complexity:

1. `postcss-discard-comments` — ~100 LOC + 2 lib files. Comment-text
   predicate, inline-raws comment scrubbing.
2. `postcss-normalize-positions` — ~50 LOC. Position-keyword rewrite.
3. `postcss-normalize-string` — ~50 LOC. Quote-style normalization.
4. `postcss-normalize-timing-functions` — ~50 LOC. Easing-keyword
   compression.
5. `postcss-ordered-values` — moderate. Reorders multi-value
   shorthand parts.
6. `postcss-normalize-whitespace` (Phase 5b) — moderate. Walks decls
   via postcss-value-parser, normalizes raws.between/.semicolon, IE9
   hack regex.
7. `postcss-minify-selectors` — moderate. Selector minification using
   postcss-selector-parser.
8. `postcss-normalize-url` — moderate. URL parsing edge cases.
9. `postcss-normalize-unicode` — moderate, browserslist-aware.
10. `postcss-reduce-initial` — moderate, caniuse-aware.
11. `postcss-convert-values` — hard, uses fraction-js, browserslist.
12. `postcss-minify-params` — hard, caniuse-aware.
13. `postcss-minify-gradients` — hard, colord-heavy.
14. `postcss-calc` — VERY hard. Effectively a small expression compiler.
15. `postcss-colormin` — HARDEST cssnano plugin. Color downgrade
    decisions hinging on caniuse + colord rounding + byte-length
    comparison.
16. `postcss-nested` (Phase 5a) — VERY hard. Recursive selector
    merging with bubble/unwrap config.
17. `cssnano-preset-default` — moderate orchestrator (depends on
    1-15 being byte-clean first).

Plus Phase 7 (autoprefixer — 8+ weeks of its own) and Phase 8
(NAPI assembly + the `transformCss` / `sort` end-to-end gates).

## Recommended order for the next session

1. **Phase 5b** (`postcss-normalize-whitespace`) — runs in
   `transform.ts`'s pipeline; small but uses postcss-value-parser
   walking. Good warmup.
2. **Phase 5a** (`postcss-nested`) — gates everything that depends on
   nested rules being flattened. Multi-day commitment.
3. **Phase 6 simple band** (`discard-comments`,
   `normalize-positions`, `normalize-string`,
   `normalize-timing-functions`) — parallel-friendly small ports.
4. Then layer in the harder ones.

## Cardinal-rule conformance check

- ✅ Every Rust crate header names the JS package + version it ports.
- ✅ Every Rust file maps 1:1 to a JS source file in upstream.
- ✅ `IndexMap` used everywhere a HashMap would touch output bytes.
- ✅ No version bumps applied to any pinned package.
- ✅ JS pipeline in `packages/css/src/transform.ts` untouched — Rust
  is additive.
- ✅ Parity-runner harness wired for every implemented plugin.
- ✅ The CRITICAL hash plugin (`atomicify-rules`) is byte-clean.
