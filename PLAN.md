● Plan: Port packages/css/src/transform.ts to Rust with byte-exact parity

Guiding principle

Every byte of the JS pipeline output must match. The hash isn't a checksum we compute — it's the entire downstream invariant. So this plan is organized around differential testing first, code second. Nothing ships until a     
corpus of millions of inputs produces byte-identical output between JS and Rust.

  ---
Phase 0 — Differential test harness (do this BEFORE writing any Rust)

This is the highest-leverage step. If the oracle is wrong, everything downstream is wrong.

1. Capture corpus
   - Snapshot inputs from packages/babel-plugin test fixtures.
   - Instrument transformCss in a side branch to dump (css, opts) → (sheets, classNames) tuples. Run it across:
    - The compiled repo's own test suite.
    - The Atlassian product code that consumes compiled (or whatever your largest internal consumer is).
    - npm's top ~1000 packages that use @compiled/react (synthesized).
      - Store as corpus/*.json — one tuple per file, content-addressed.
2. Pin every transitive dep
   - package-lock.json exact pin for: postcss, postcss-nested, postcss-normalize-whitespace, postcss-selector-parser, postcss-value-parser, autoprefixer, caniuse-lite, browserslist.
   - Record versions in crates/PARITY_VERSIONS.md. The Rust port targets these specific versions, not "current" anything. Caniuse-lite is the silent killer — it changes monthly and autoprefixer's output changes with it.
3. Diff harness
   - crates/parity-runner/ — Rust binary that loads the corpus, runs Rust pipeline, runs Node pipeline (subprocess), byte-compares.
   - Failure mode reports the smallest divergent byte range with surrounding context.
4. Seed regression baseline
   - Run the JS pipeline against itself across N machines / N OS versions. Any non-determinism here (e.g. browserslist resolving differently in CI vs local) is a blocker we must understand before we add Rust into the mix.

  ---
Phase 1 — Crate scaffolding

crates/
postcss-core/              # AST, tokenizer, parser, stringifier
postcss-selector-parser/   # selector AST
postcss-value-parser/      # value tokenizer
postcss-nested/            # plugin port
postcss-normalize-whitespace/
autoprefixer/              # the elephant
caniuse-db/                # bundled JSON, build.rs codegen → static tables
browserslist-shim/         # thin wrapper around oxc_browserslist for parity
compiled-css/              # local plugins (1:1 with packages/css/src/plugins)
compiled-css-napi/         # NAPI exports
parity-runner/             # diff harness
parity-fuzz/               # cargo-fuzz targets

Tooling: napi-rs for bindings, napi build produces .node artifacts per platform. Use IndexMap everywhere postcss uses Object (insertion order matters). Ban HashMap from anything that touches output.

  ---
Phase 2 — PostCSS core (parser + stringifier)

This is the load-bearing piece. The output bytes come from the Stringifier walking the AST + raws. Everything depends on getting it byte-exact.

Port targets (read these, port line-for-line):
- node_modules/postcss/lib/tokenize.js → postcss-core/src/tokenize.rs
- node_modules/postcss/lib/parser.js → postcss-core/src/parser.rs
- node_modules/postcss/lib/stringifier.js → postcss-core/src/stringify.rs
- node_modules/postcss/lib/{root,atrule,rule,declaration,comment,container,node}.js → AST modules

The raws object is everything. Every node carries raws.before, raws.after, raws.between, raws.semicolon, raws.afterName, raws.left, raws.right, raws.value.raw, etc. These are the whitespace and comment fragments that get      
verbatim re-emitted. A faithful port stores them as &str slices into the original source (or owned Strings with the exact bytes).

Pitfalls to watch:
- UTF-16 vs UTF-8. JS counts in code units; Rust in bytes. Column numbers in source.start aren't user-visible, but if any plugin reads them, drift happens. Track positions in code-unit space if any plugin uses them.
- Number formatting. JS String(0.1+0.2) = "0.30000000000000004". If any plugin does math on declaration values, the Rust port must match f64 → string via the JS double-to-string algorithm (ryū won't always agree on edge cases
  — verify each call site).
- Regex. Replace JS regexes with regex crate equivalents; some Unicode classes differ. Audit each regex site.
- Iteration order. PostCSS uses array indices and walks; Rust must mirror exactly (forward index walk, mutation-during-walk semantics included — postcss decrements/skips on insert/remove).

Validation gate: before any plugin work, port parse(css).toString() === css invariant. Run that round-trip on the entire corpus. Zero divergence required before moving on.

  ---
Phase 3 — Selector-parser and value-parser

- postcss-selector-parser → port verbatim. Used by postcss-nested, autoprefixer, and several local plugins (flattenMultipleSelectors, parentOrphanedPseudos, increaseSpecificity).
- postcss-value-parser → port verbatim. Used by autoprefixer and expandShorthands.

Round-trip parity test: parse(sel).toString() === sel over a selector corpus extracted from the CSS corpus.

  ---
Phase 4 — Local plugins (in packages/css/src/plugins/)

Port one at a time, in ascending complexity, gating each on byte-exact diff against JS. Order:

┌─────┬──────────────────────────────────────────────────────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐   
│  #  │                                    Plugin                                    │                                                                 Notes                                                                  │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 1   │ discard-empty-rules                                                          │ Trivial.                                                                                                                               │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 2   │ discard-duplicates                                                           │ Watch the equality semantics — postcss compares stringified forms.                                                                     │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 3   │ parent-orphaned-pseudos                                                      │ Selector traversal.                                                                                                                    │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 4   │ extract-stylesheets                                                          │ Iteration order matters — the callback receives sheets in a specific order, and sheets.push ordering feeds into downstream hashing.    │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 5   │ flatten-multiple-selectors                                                   │ Selector splitting; tie-break ordering.                                                                                                │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 6   │ expand-shorthands/*                                                          │ Several files; uses value-parser. Match exact insertion points.                                                                        │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 7   │ normalize-css                                                                │ Wrapper composing other plugins.                                                                                                       │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 8   │ increase-specificity                                                         │ Selector rewriting.                                                                                                                    │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 9   │ atomicify-rules                                                              │ Critical for hashing. Whatever hash function it uses (likely murmur or similar via @compiled/utils), the Rust port must use a          │   
│     │                                                                              │ bit-identical implementation. The class name compression map iteration order matters.                                                  │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 10  │ sort-atomic-style-sheet                                                      │ Stable sort. Rust sort_by is stable; verify the tie-break comparator matches JS exactly (JS Array.prototype.sort is stable since       │   
│     │                                                                              │ ES2019).                                                                                                                               │   
├─────┼──────────────────────────────────────────────────────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤   
│ 11  │ normalize-current-color, merge-duplicate-at-rules, sort-pseudo-selectors,    │ Touched by the above.                                                                                                                  │   
│     │ sort-shorthand-declarations (utils)                                          │                                                                                                                                        │   
└─────┴──────────────────────────────────────────────────────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

Per-plugin gate: corpus run with this plugin spliced into the JS pipeline (rest JS, this one Rust via a hybrid harness) must produce zero diff before moving on.

  ---
Phase 5 — Third-party plugins

postcss-nested

- Port node_modules/postcss-nested/index.js line-for-line.
- The bubble/unwrap config in transform.ts:48-61 includes 'starting-style' as an explicit bubble — that's a workaround comment indicating a version-specific behavior. Pin the JS version, port that exact behavior.
- Recursive selector merging is the bug-prone area; uses selector-parser heavily.

postcss-normalize-whitespace

- Small plugin; port verbatim. The whole point is whitespace normalization — every space matters here, doubly.

  ---
Phase 6 — Autoprefixer + browserslist (the hardest)

This is 60% of the total work. Budget accordingly.

6a. Browserslist with oxc_browserslist

oxc_browserslist exists and resolves queries → browser list. But parity isn't guaranteed across:
- Default query (> 0.5%, last 2 versions, Firefox ESR, not dead) — the meaning depends on caniuse-lite version.
- Config resolution: package.json browserslist field, .browserslistrc, BROWSERSLIST, BROWSERSLIST_CONFIG, BROWSERSLIST_ENV, BROWSERSLIST_DISABLE_CACHE, BROWSERSLIST_STATS. Each precedence rule must match
  node_modules/browserslist/node.js exactly.
- Cache file behavior. JS browserslist memoizes; we must either disable caching or match the cache key derivation.

Approach:
- Use oxc_browserslist for query parsing.
- Write browserslist-shim that owns config resolution, replicating node.js precedence verbatim.
- Pin caniuse-lite to the version present in package-lock.json. Vendor it into crates/caniuse-db/data/ and codegen Rust static tables via build.rs.
- Differential test: take 1000 real-world package.json + .browserslistrc combos and verify the resolved browser list matches Node's browserslist() byte-for-byte.

6b. Autoprefixer port

- Source: node_modules/autoprefixer/lib/.
- It's ~50 files. Port each verbatim.
- Uses caniuse-lite data lookups — feed those from crates/caniuse-db.
- Vendor prefix decision tables, hack files (flexbox, grid), value transformations — all 1:1.
- The process.env.AUTOPREFIXER === 'off' switch in transform.ts:75 stays — Rust just respects it.

Validation gate: corpus run with autoprefixer-rust, rest JS. Zero diff. This is where most regressions will surface; budget multiple weeks of iteration.

  ---
Phase 7 — Wire into transform.ts via NAPI

// crates/compiled-css-napi/src/lib.rs
#[napi]
pub fn transform_css(css: String, opts: TransformOpts) -> TransformResult { ... }

Update packages/css/src/transform.ts:

import { transformCss as transformCssRust } from '@compiled/css-native';

export const transformCss = (css, opts) => {
if (process.env.COMPILED_CSS_ENGINE === 'rust') {
return transformCssRust(css, opts);
}
// existing JS pipeline unchanged
};

Keep the JS pipeline alive permanently as the fallback + diff oracle. Don't delete it until rollout is complete.

  ---
Phase 8 — Differential testing at scale

1. Corpus replay: every PR runs the full corpus through both engines. Any byte diff = block.
2. Coverage-guided fuzzing: cargo-fuzz targets that take arbitrary CSS bytes, run both engines, assert byte-equality. Run on dedicated infra for weeks.
3. Shadow mode in CI of the consuming codebase: run JS engine for the build (still trusted), run Rust engine in parallel, compare hashes, alarm on divergence — no production impact.
4. Property tests for invariants we know must hold: round-trip parse → stringify, idempotency of normalize-whitespace, etc.

  ---
Phase 9 — Rollout

1. Engine flag default = js.
2. Ship Rust artifacts via napi build for linux-x64-gnu, linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc. Each platform binary is a separate parity surface — float formatting and regex behavior should be identical,  
   but verify.
3. Internal opt-in via env var.
4. Hash-shadow in production: compute Rust output, hash it, compare to JS hash, log divergence — don't use Rust output yet.
5. After N weeks of zero divergence on production traffic, flip default to rust.
6. JS engine stays in the tree as the parity oracle for at least a year.

  ---
Risk register

┌────────────────────────────────────────────────────┬──────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                        Risk                        │                                                Mitigation                                                │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Caniuse-lite version drift                         │ Vendor + pin; codegen static tables.                                                                     │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Float-to-string differences                        │ Audit every numeric stringification; use JS double-to-string algorithm where any plugin formats numbers. │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Object iteration order                             │ IndexMap everywhere; ban HashMap in output paths via lint.                                               │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Regex semantics divergence                         │ Per-regex audit; differential test each.                                                                 │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ UTF-16 vs UTF-8 column counting                    │ Track positions in JS-equivalent units anywhere positions reach output.                                  │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Browserslist config resolution edge cases          │ Hand-port node.js precedence; corpus-test resolution.                                                    │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ postcss raws drift                                 │ Round-trip identity test on full corpus before any plugin work.                                          │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Hash function in atomicify-rules not bit-identical │ Read @compiled/utils hash impl, port with byte-level test vectors.                                     │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Stable-sort tie-breaks                             │ Audit each sort call; explicit tie-break comparators.                                                    │
├────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Plugin re-entry semantics                          │ Postcss runs plugins in passes; verify our pass loop matches node_modules/postcss/lib/lazy-result.js.    │
└────────────────────────────────────────────────────┴──────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  ---
Effort estimate (rough, calendar weeks, one strong engineer)

- Phase 0: 2
- Phase 1: 1
- Phase 2 (postcss core): 4
- Phase 3 (selector + value parsers): 3
- Phase 4 (local plugins): 4
- Phase 5 (nested + whitespace): 2
- Phase 6 (browserslist + autoprefixer): 8
- Phase 7 (NAPI): 1
- Phase 8 (fuzz + diff at scale): 4 (overlaps)
- Phase 9 (rollout): 6 (calendar, mostly waiting)

Total: ~6–9 months. Anyone quoting less is underestimating autoprefixer.

  ---
What I need from you before writing any code

1. Confirm the JS dep versions to pin (current package-lock.jso n snapshot, or do you want to bump first?).
2. Access (or representative samples) of your largest internal consumer's CSS for the corpus.
3. Are there CSS inputs whose hashes are already committed somewhere that we must preserve? If so, they're the highest-priority corpus entries.
4. Are you OK with the "JS stays as fallback for ≥1 year" rollout shape, or do you need an earlier delete?

I'd recommend we start with Phase 0 only as a standalone deliverable — the diff harness is independently valuable (it'll catch JS-side regressions today) and proves we can detect divergence before we commit to the larger port.
