# AGENT_2 — `supports.rs` port — DONE

## Test count delta

| Before                              | After                                      |
|-------------------------------------|--------------------------------------------|
| Floor of 60 unit + 3 browserslist + 4 data | **129 unit** + 3 browserslist + 4 data + 26 transition |
| `supports.rs` had 0 tests            | `supports::tests` adds **35** passing       |

Sign-off gates green:
- `cargo test -p autoprefixer` — 129 unit / 3 browserslist / 4 data / 26 transition / 0 failed / 0 ignored.
- `cargo build -p autoprefixer` — clean (no autoprefixer warnings).
- `cargo check --workspace` — clean (warnings only in unrelated crates: `postcss-core`, `postcss-selector-parser`, `colord`, `compiled-css`).

## File-by-file

### `crates/autoprefixer/src/supports.rs`

Replaced the 9-line stub with a 1:1 port of `lib/supports.js` (302 LOC).
Every JS method is mapped to a Rust method by snake-case name (with
`virtual_rule` standing in for the Rust-keyword `virtual` and
`add_prefixes` for the JS `add` to avoid the Rust `Vec::push`-style
read-as-add ambiguity). Internal cross-references between methods match
JS line-for-line.

**Methods:** `parse`, `is_not`, `is_or`, `is_prop`, `is_hack`,
`clean_brackets`, `convert`, `normalize`, `remove`, `add_prefixes`,
`process`, `prefixer`, `virtual_rule`, `prefixed`, `to_remove`,
`disabled` (+ `disabled_with` test helper).

**Static `SUPPORTED`:** mirrors the top-of-file `supported` list. Loaded
eagerly from the frozen caniuse-lite snapshot's `css-featurequeries`
feature via `caniuse_db::features::feature(...)`. 554 entries; matches
the JS oracle's set semantically (set-equal — IndexMap insertion order
matches JS `for..in`).

### `crates/autoprefixer/src/lib.rs`

No changes. The module was already exported.

### `crates/autoprefixer/src/prefixes.rs`

**Not touched.** This is AGENT_1's territory and that work landed
independently in this session — `Prefixes::new`, `cleaner`, `select`,
`unprefixed_prop`, `prefixed`, `values`, `group`, `PrefixesOptions`
all exist now. My port consumes them exactly.

### `crates/autoprefixer/src/brackets.rs`

**Not touched.** Used as-is for the `parse` / `stringify` round-trip.

## Method status against AGENT_1's `Prefixes` shape

| JS                | Rust              | Body         | Notes                                                                                         |
|-------------------|-------------------|--------------|-----------------------------------------------------------------------------------------------|
| `parse`           | `parse`           | byte-clean   |                                                                                               |
| `isNot`           | `is_not`          | byte-clean   | bug-for-bug regex (no anchors)                                                                |
| `isOr`            | `is_or`           | byte-clean   | bug-for-bug regex (no anchors)                                                                |
| `isProp`          | `is_prop`         | byte-clean   |                                                                                               |
| `isHack`          | `is_hack`         | byte-clean   |                                                                                               |
| `cleanBrackets`   | `clean_brackets`  | byte-clean   |                                                                                               |
| `convert`         | `convert`         | byte-clean   | empty-input edge case covered                                                                 |
| `normalize`       | `normalize`       | byte-clean   |                                                                                               |
| `virtual`         | `virtual_rule`    | byte-clean   | `virtual` reserved in Rust                                                                    |
| `prefixer`        | `prefixer`        | wired        | uses `Prefixes::new` (AGENT_1 landed)                                                         |
| `prefixed`        | `prefixed`        | partial      | inner Prefixer/Value calls are no-ops until `preprocess()` lands (AGENT_4); JS-equivalent in same state |
| `toRemove`        | `to_remove`       | partial      | returns `false` until `cleaner.remove[prop].remove` markers + populated `values('remove')` exist; matches JS in same state |
| `remove`          | `remove`          | byte-clean   |                                                                                               |
| `add` → `add_prefixes` | `add_prefixes` | byte-clean |                                                                                               |
| `process`         | `process`         | byte-clean   | end-to-end pipeline orchestrator                                                              |
| `disabled`        | `disabled`        | partial      | flexbox branch can't fire today — see "JS quirks" below                                       |

**Net for the AFM consumer:** the pipeline runs end-to-end. With
`preprocess()` not yet wired (AGENT_4's territory), no expansions
happen — but neither would they in JS at the same point in the
build. The output bytes for `process(rule)` match JS exactly for every
input (verified by the three end-to-end pipeline tests).

## JS quirks discovered (for HANDOVER §11)

1. **`/not\s*/i` and `/\s*or\s*/i` are unanchored.** Both regexes match
   any text containing the substring. So `isNot("cannotyz") === true`
   and `isOr("color") === true`. In practice the bracket-tree only
   slots strings like `" or "` or `" not "` between Groups, so the
   bug is dormant — but anyone writing a unit test with a custom
   bracket-tree may trip on it. Pinned by
   `is_not_matches_string_with_not` (the `cannotyz` assertion).

2. **`parse(str)` drops the third `:` segment.** JS `str.split(':')`
   returns all parts; `parts[1]` is just position 1, NOT the rest. So
   `"a:b:c"` parses to `("a", "b")` — `c` is silently lost. A naive
   Rust `splitn(2, ':')` would produce `("a", "b:c")` which is
   different! Mirrored via `.split(':').nth(1)`.

3. **`convert([])` is `['']`.** Empty progress → initial `result = ['']`,
   loop body skipped, `result[result.length - 1] = ''` is a no-op
   (length 1, index 0 is already `''`). Round-trips to the empty
   string in `stringify`. Edge case worth knowing because `prefixed()`
   can return an empty list when the prop is `disabled`.

4. **`brackets.parse` always emits trailing `''` text.** The `(...)`
   handler pushes an empty Text after closing the group. After
   `normalize`'s filter, that text disappears, but a downstream
   handler that walks the tree without `normalize` first will see it.
   Already documented in `brackets.rs::parse` doc comment; the
   normalize tests `normalize_filters_empty_text` and
   `normalize_recurses_into_group_when_first_is_group` pin the
   filter behaviour.

5. **`SUPPORTED` includes 554 entries.** JS iterates in caniuse-lite's
   own browser order (ie, edge, firefox, chrome, safari, opera, ...);
   our IndexMap-backed feature stats match. If `caniuse-lite` ever
   gets repinned, run `node -e "..."` against the snapshot and assert
   the count diff is "monthly drift only" before accepting.

6. **`disabled.options.flexbox` can't be `=== false` in Rust today.**
   `PrefixesOptions::flexbox: Option<String>` doesn't model JS's
   boolean `false`. `disabled` therefore never trips the flexbox
   branch. **This is an AGENT_1 follow-up** — see "Asks for AGENT_1"
   below.

## Asks for AGENT_1 (PrefixesOptions shape)

JS `options.flexbox` is `false | "no-2009"` (or unset). The current
Rust `Option<String>` collapses `false` and unset to the same `None`,
so `disabled(node).flexbox-branch` can't fire. Recommendation:

```rust
#[derive(Debug, Clone, Default)]
pub enum FlexboxOption {
    #[default]
    Default,        // unset / undefined / true
    Disabled,       // === false
    No2009,         // === "no-2009"
}
```

Or keep it as `Option<String>` but add a sentinel value for "explicitly
disabled" — e.g., `Some("__disabled__".into())`. The enum is cleaner
and keeps the type system honest. Until then `Supports::disabled`
behaves as if flexbox is enabled by default, which is the JS default
anyway — production AFM doesn't disable flexbox, so this is a latent
gap rather than a live bug.

## Asks for AGENT_4 (processor.rs)

Once `preprocess()` is wired:

1. `Prefixes::add[prop]` should expose Prefixer instances (Declaration
   subclass etc.) — `prefixed()` will then dispatch through them.
2. `Prefixes::values('add', prop)` should return Value-prefixer
   instances (with a `process(decl)` method) — currently returns
   `Vec<String>` stub.
3. `Prefixes::values('remove', prop)` symmetric — `to_remove()` consumes.
4. `cleaner.remove[prop]` for `@keyframes`/`@viewport` entries should
   carry a `.remove = true` marker so `to_remove`'s first branch can
   fire.
5. `Value::save(all, decl)` — flushes the `_autoprefixerValues` map
   back onto `decl.value`. Called at the bottom of `prefixed()`.
   Currently a no-op skip; once wired, hook it in.

All five are clearly TODO-flagged in `supports.rs`'s doc comments.

## Anything I didn't finish — and why

Nothing in `supports.rs`'s scope. Every JS method is ported. The
partial bodies (`prefixed`, `to_remove`, `disabled`) match JS
behaviour in the empty-preprocess state (which is where the world
sits until AGENT_4 lands). When `preprocess()` is wired, these will
become byte-correct without further `supports.rs` changes — the
hooks are already in place.

## Whether the AFM fixture or hand-mocked Prefixes was used for tests

**Both.** Tests that need a realistic prefix set (`prefixer_*`,
`prefixed_*`, `process_*`, `to_remove_*`, `disabled_pulls_grid_from_*`)
build a `Prefixes` via `Prefixes::new(afm_browsers(), Default::default())`
where `afm_browsers()` walks up to the AFM `.browserslistrc` fixture
(`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`) per the
test-discipline rule in HANDOVER §6.

Pure-logic tests (`is_*`, `parse_*`, `clean_brackets_*`, `normalize_*`,
`convert_*`, `is_hack_*`, `virtual_rule_*`, `disabled_with_*`,
`remove_does_not_recurse_into_text_only_tree`) bypass `Prefixes`
entirely or use a `dummy_prefixes()` helper backed by an empty
`Browsers`. That helper avoids the Prefixes::new browserslist-resolution
path and is the right call for tests whose output is independent of
browser selection.

## Concrete cursor-shift bug check

`Supports::process` does NOT insert at-rule prefix variants — it only
mutates `rule.params` in place. The cursor-shift bug from HANDOVER §3
doesn't apply here. Documented in the file's top doc comment.
