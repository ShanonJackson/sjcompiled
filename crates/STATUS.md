# Phase 1/2/3 status — `crates/`

This document captures the state of the Rust port at the end of the initial
setup pass. Read together with `EXECUTION_PLAN.md` and `PARITY_VERSIONS.md`.

## Layout

The Cargo workspace lives at `crates/Cargo.toml` and currently has 12
members. Each crate's folder/file structure mirrors the upstream npm package
that it ports — opening upstream JS and the Rust port side-by-side should
show identical file layouts (camelCase preserved where upstream uses it).

Vendored upstream sources for the pinned versions are extracted under
`crates/_vendor/` (gitignored). They exist as the source-of-truth reference
during porting.

### Local-package crates (split: orchestrator vs plugins)

- **`crates/css/`** — Rust port of `@sjcompiled/css`'s public surface
  (`transformCss`, `sort`, `generateCompressionMap`). Mirrors
  `packages/css/src/{index,transform,sort,generate-compression-map}.ts`.
  **`transform_css` and `sort` signatures are locked here** — the parity
  contract is fixed even though the bodies are postcss-core identity
  passthrough until Phase 4-7 plugin work lands.
- **`crates/compiled-css/`** — Local plugin home; mirrors
  `packages/css/src/plugins/*` and `packages/css/src/utils/*` 1:1
  (atomicify-rules, discard-empty-rules, expand-shorthands/, at-rules/,
  etc.). Every plugin file exists as a Rust module with a typed shell
  (factory function + options struct) and an `unimplemented!()` body
  tagged with the phase that fills it in.

## Per-crate state

| Crate | Phase | Status |
|---|---|---|
| `fraction-js` | 1d | **Ported.** All public methods (`add`/`sub`/`mul`/`div`/`mod`/`gcd`/`lcm`/`ceil`/`floor`/`round`/`pow`/`equals`/`compare`/`divisible`/`toString`/`toFraction`/`toLatex`/`toContinued`). 10 unit tests pass. Open: `js_number_to_string` non-integer formatting deferred to autoprefixer integration. |
| `colord` | 1c | **Ported.** Full color parse (hex 3/4/6/8 digit, rgb legacy + modern + percent, hsl legacy + modern + grad/turn/rad units, 148 named colors + `transparent`, case-insensitive). Color math: `toHex` / `toRgb(String)` / `toHsl(String)` / `toHsv` / `invert` / `saturate` / `desaturate` / `grayscale` / `lighten` / `darken` / `rotate` / `alpha` / `hue` / `isEqual` / `brightness` / `isDark` / `isLight` / `toName` (exact + closest). Plugins: `a11y` (contrast, luminance, isReadable AA/AAA), `hwb`, `lab` (CIE LAB via D50 XYZ), `mix` (LAB-space lerp), `harmonies` (analogous, complementary, double-split, rectangle, split-complementary, tetradic, triadic), `minify` (shortest-string serializer used by `postcss-colormin`). 39 unit tests. |
| `cssnano-utils` | 2e | **Ported.** `getArguments`, `rawCache`, `sameParent`. 5 unit tests. Caller-side adapter takes a `is_div` predicate so this crate stays AST-agnostic (couples to value-parser via the consumer). |
| `caniuse-db` | 1b | **Ported.** Vendored `caniuse-lite@1.0.30001690` JSON snapshot at `data/features.snapshot.json` (3.5 MB, 579 features, all agents). The snapshot is produced once-off by `scripts/snapshot.js` (Node) using upstream's bundled unpacker against `crates/_vendor/caniuse-lite-1.0.30001690`. `build.rs` copies the JSON into `OUT_DIR`; `lib.rs` `include_str!`s it and parses lazily via `once_cell`. 5 unit tests verify version pin, flexbox/css-grid feature lookups, chrome agent, list size. **Next:** vendor `electron-to-chromium@1.5.76` and `node-releases@2.0.19` (only matters once `browserslist-shim` consults them — `oxc-browserslist` ships its own copies for now). |
| `caniuse-api` | 3a | **Ported.** `features`, `find`, `getSupport`, `isSupported`, `setBrowserScope`, `getBrowserScope` end-to-end against real caniuse-lite data. 6 integration tests (find, fuzzy substring, ie6 unsupported features, full feature list count). |
| `browserslist-shim` | 2d | **Ported (config + defaults).** `parseConfig` (`.browserslistrc` body parser, comment-strip, `[section]` headers), `parsePackage` (package.json `browserslist` field — array / string / env-object / `browserlist` typo detection), `findConfigFile` (ancestor walk), `loadConfig` (`BROWSERSLIST` > `BROWSERSLIST_CONFIG` > path discovery), `pickEnv` (`opts.env` > `BROWSERSLIST_ENV` > `NODE_ENV` > `production`). `DEFAULT_QUERIES = ["> 0.5%", "last 2 versions", "Firefox ESR", "not dead"]`. 9 unit tests. **Next:** match the *resolution* semantics byte-for-byte against JS — `oxc-browserslist` may differ from JS browserslist on edge cases (deadlist semantics, region queries). |
| `postcss-core` | 1a | **Ported.** Tokenizer (`tokenize.js`), parser (`parser.js`) and stringifier (`stringifier.js`) are now real. AST: Root / AtRule / Rule / Declaration / Comment / Container with full `raws` (before/after/between/afterName/important/left/right/value/selector/params/ownSemicolon/semicolon). Round-trip `stringify(parse(css)) == css` passes for: simple decl, no-trailing-semi, multiple decls, nested at-rules, comment-in-value, statement at-rules, empty rules, `!important`, url values, leading-underscore-hack. **Next:** corpus-scale round-trip, edge-case raws (semicolon decision in body, missed-semicolon error path, `_` / `*` decl-prop hacks), `from_offset`-driven source positions. |
| `postcss-selector-parser` | 2a | **Ported (level 1).** Real `tokenize.js` (with `consumeWord` / `consumeEscape`). Parser splits on top-level `,` to build Root → Selector(s). 8 round-trip parity tests pass: class, id, descendant, child, comma list, attribute with string, pseudo-function, nested pseudo. **Next:** parser-level Node type breakdown (ClassName / Combinator / Pseudo / Attribute) so `flattenMultipleSelectors` and `increaseSpecificity` plugins can introspect. |
| `postcss-value-parser` | 2b | **Ported.** Real `parse.js` (Function / String / Div / Space / Word / Comment / UnicodeRange), `stringify.js`, `walk.js`, `unit.js`. 11 round-trip parity tests pass: keyword, px value, space-separated list, comma list, function call, calc, url unquoted, url with spaces, quoted string, comment, nested function. |
| `postcss-values-parser` | 2c | **Ported (classification).** Real `tokenize.js` (wraps postcss-core tokenizer, splits brackets/operators/commas). Parser classifies each token into Numeric/Word/Func/Quoted/Punctuation/Operator/UnicodeRange/AtWord/Comment. 6 unit tests pass. **Next:** raws bookkeeping that lets `expand-shorthands` mutate-and-re-stringify cleanly. |
| `css` | (local) | **Scaffolded.** `transform_css(css, opts) -> { sheets, classNames }` and `sort(stylesheet, opts) -> string` signatures locked. `TransformOpts` / `SortOpts` / `TransformResult` types are `serde`-roundtrippable. Bodies are postcss-core identity passthrough today (3 transform round-trip tests + 1 sort-passthrough test). Plugin pipeline gets wired in here in Phase 4-7 in upstream order — module doc has the canonical sequence. |
| `compiled-css` | (local) | **Scaffolded.** Every `packages/css/src/plugins/*.ts` (and the nested `at-rules/*` and `expand-shorthands/*` trees) maps 1:1 to a Rust module declaring the plugin's typed surface (factory function + options struct) with an `unimplemented!()` body tagged with the phase that fills it in. Phase 4-6 fills the bodies. |

## Test summary

`RUSTFLAGS="" cargo test --workspace` — **130 unit tests passing, 0 failing.**

Breakdown:
- `colord`: 39 (parse: hex 3/4/6/8, rgb legacy/modern/%, hsl legacy/modern/grad/alpha, named, transparent, case-insensitive, whitespace; manipulate: invert, lighten, darken, saturate, grayscale, rotate, alpha, hue, brightness, isEqual; plugins: a11y contrast, harmonies, minify shortest-string)
- `cssnano-utils`: 5
- `fraction-js`: 10
- `postcss-core`: 16 (10 round-trip parity tests on the parser+stringifier)
- `postcss-value-parser`: 17 (11 round-trip parity tests on the value AST)
- `postcss-selector-parser`: 13 (8 round-trip parity tests + 5 tokenize tests)
- `postcss-values-parser`: 6 (kind-classification: Numeric, Func, Word, Quoted, Variable, UnicodeRange)
- `browserslist-shim`: 9 (parseConfig variants, parsePackage variants, defaults string, query resolution)
- `caniuse-db`: 5 (snapshot version pin, flexbox + css-grid feature lookups, chrome agent, 579-feature list)
- `caniuse-api`: 6 (find exact + fuzzy, ie6 unsupported, feature list count)
- `css`: 4 (transform_css passthrough simple/nested/empty + sort passthrough)

## Cardinal-rule conformance check

- ✅ Every Rust crate header names the JS package + version it ports.
- ✅ Every Rust file maps 1:1 to a JS source file in upstream.
- ✅ `IndexMap` used in `caniuse-db` / `caniuse-api` / `Raws::other`. `HashMap` is currently only used inside `caniuse-api::utils::clean_browsers_list` for de-dup; **TODO**: switch to `IndexSet` to honour the cardinal rule.
- ✅ No version bumps applied to any pinned package.
- ✅ JS pipeline in `packages/css/src/transform.ts` untouched — Rust is additive.

## Next milestones (gating Phase 4)

1. Stand up `crates/parity-runner/` (Phase 0 deliverable, prerequisite for any plugin diff testing) and run the postcss-core port against a 1000-input corpus.
2. Deepen `postcss-selector-parser` to expose ClassName/Combinator/Pseudo/Attribute typed nodes (gates `atomicifyRules` & `flattenMultipleSelectors` plugin ports).
3. Vendor `electron-to-chromium@1.5.76` + `node-releases@2.0.19` snapshots into `caniuse-db` (only needed if `browserslist-shim` ever stops delegating to `oxc-browserslist`'s own bundled copies).
4. Confirm `oxc-browserslist`'s query resolution matches `browserslist@4.24.4` byte-for-byte across the 1000-input config corpus from the EXECUTION_PLAN.

## Known parity hazards still un-addressed

- `caniuse-db` carries the pinned 1.0.30001690 snapshot; `caniuse-api::is_supported` works end-to-end. Edge case still open: support values with notes (`"y #1"`) — upstream `caniuse-api` strictly compares `=== "y"`, our port does the same; documented and exercised by tests.
- `browserslist-shim` resolves queries through `oxc-browserslist` (which bundles its own caniuse-lite). Need a corpus diff vs JS browserslist@4.24.4 to confirm parity on edge queries.
- `postcss-core::tokenize` ports the regex character classes but the `RE_WORD_END` JS lookahead `\/(?=\*)` is approximated via a separate `/*` byte scan — needs a corpus diff against the JS tokenizer to confirm equivalence.
- `f64::round` vs JS `Math.round`: handled in `fraction-js::round`. Other crates don't yet emit numbers; revisit as `colord`/`fraction-js` flow into autoprefixer.
