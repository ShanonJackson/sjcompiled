//! Stages: each value names ONE pipeline configuration to diff.
//!
//! As plugins land, add a variant here, wire it into `rust_run_stage`,
//! and add the matching JS counterpart in `scripts/js-pipeline.mjs`.

use postcss_core::{parse, stringify};

/// U+001E record separator — the JS bridge joins sheets with this byte
/// so a single-line response can carry an array. Must match the
/// `SHEET_SEP` constant in `packages/css/scripts/parity-bridge.mjs`.
const SHEET_SEP: char = '\x1e';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `parse(css).toString()` — the postcss-core round-trip oracle.
    /// Used to confirm the parser+stringifier are byte-clean before any
    /// plugin layers it.
    PostcssCoreRoundtrip,

    /// `parse → discardEmptyRules → stringify`. Phase 4a.
    DiscardEmptyRules,

    /// `parse → discardDuplicates (local) → stringify`. Phase 4a.
    /// Distinct from the npm `postcss-discard-duplicates@6` used by
    /// `sort.ts` (that's a separate stage, separate Rust crate).
    DiscardDuplicates,

    /// `parse → extractStyleSheets → join sheets with U+001E`. Phase 4a.
    /// The plugin is read-only; we diff the per-child sheet strings.
    ExtractStylesheets,

    /// `parse → parentOrphanedPseudos → stringify`. Phase 4b.
    ParentOrphanedPseudos,

    /// `parse → increaseSpecificity → stringify`. Phase 4b.
    IncreaseSpecificity,

    /// `parse → mergeDuplicateAtRules → stringify`. Phase 4c.
    MergeDuplicateAtRules,

    /// `parse → normalizeCurrentColor → stringify`. Phase 4c.
    NormalizeCurrentColor,

    /// `parse → sortAtomicStyleSheet → stringify`. Phase 4c.
    SortAtomicStyleSheet,

    /// `parse → atomicifyRules → stringify`. Phase 4d. Default opts —
    /// no compression map, no class-hash prefix, no callback. The
    /// generated class names are an *output side-effect* in the AST
    /// (selectors); the bridge diff captures them via the stringified
    /// CSS bytes.
    AtomicifyRules,

    /// `parse → expandShorthands → stringify`. Phase 4e.
    ExpandShorthands,

    /// `parse → postcss-discard-duplicates@6 → stringify`. Phase 5c.
    /// Distinct from `Stage::DiscardDuplicates` (the LOCAL plugin).
    NpmPostcssDiscardDuplicates,

    /// `parse → postcss-nested@5.0.6 → stringify`. Phase 5a.
    /// Run with the same `bubble`/`unwrap` opts that `transform.ts:48-61`
    /// uses, so the parity gate validates the exact configuration baked
    /// into the production pipeline (no v5 → v6 drift, `starting-style`
    /// in bubble list, `color-profile`/`counter-style`/`font-palette-values`/
    /// `page`/`property` in unwrap list).
    PostcssNested,

    /// `parse → postcss-normalize-whitespace@5.1.1 → stringify`. Phase 5b.
    /// Single OnceExit hook — runs once on the parsed root.
    PostcssNormalizeWhitespace,

    /// `parse → postcss-discard-comments@5.1.2 (default opts) → stringify`.
    /// Phase 6a. Default keeps `/*!` important comments, drops the rest.
    PostcssDiscardComments,

    /// `parse → postcss-normalize-string@5.1.0 (default opts) → stringify`.
    /// Phase 6b. Default `preferredQuote: 'double'`.
    PostcssNormalizeString,

    /// `parse → postcss-normalize-positions@5.1.1 → stringify`. Phase 6b.
    /// Rewrites `background-position` / `*-perspective-origin` keyword pairs
    /// (left/top → 0 0, etc.). No options.
    PostcssNormalizePositions,

    /// `parse → postcss-normalize-timing-functions@5.1.0 → stringify`. Phase 6b.
    /// Compresses `cubic-bezier(...)` / `steps(...)` to keyword equivalents
    /// (ease/linear/ease-in/ease-out/ease-in-out/step-start/step-end), and
    /// strips the redundant trailing `, end` from `steps(N, end)`. No options.
    PostcssNormalizeTimingFunctions,

    /// `parse → postcss-normalize-url@5.1.0 (default opts) → stringify`. Phase 6b.
    /// Walks every Decl value and `@namespace` AtRule params; rewrites the
    /// inner of `url(...)` calls. Absolute/protocol-relative URLs pass through
    /// `normalize-url@6.1.0`. Relative paths pass through `path.posix.normalize`.
    /// `data:`/`*-extension:/` short-circuit the conversion. The 5
    /// postcss-side overrides hold (`normalizeProtocol`/`sortQueryParameters`/
    /// `stripHash`/`stripWWW`/`stripTextFragment` all `false`).
    PostcssNormalizeUrl,

    /// `parse → postcss-normalize-unicode@5.1.1 (no opts) → stringify`. Phase 6e.
    /// Browserslist-aware: `prepare(result)` resolves
    /// `browsers = browserslist(null, { path: __dirname, ... })` once,
    /// computes `isLegacy = browsers.some(hasLowerCaseUPrefixBug)` where
    /// `hasLowerCaseUPrefixBug(b)` ↔ `b ∈ browserslist('ie <=11, edge <= 15')`.
    /// Under the workspace's locked 4.24.2 defaults (no IE, no Edge ≤15) →
    /// `isLegacy = false`. `OnceExit` walks every Decl matching
    /// `/^unicode-range$/i`; lowercases each `unicode-range` value-parser
    /// token, attempts wildcard collapse via `mergeRangeBounds` (`0`/`f`
    /// pairs become `?`, max 5), and re-uppercases the leading `u` only
    /// when `isLegacy` is true. Per-call cache keyed on raw decl value.
    PostcssNormalizeUnicode,

    /// `parse → postcss-minify-selectors@5.2.1 (no opts) → stringify`. Phase 6c.
    /// `OnceExit` walks every Rule, runs each selector through a
    /// postcss-selector-parser pipeline that clears spaces, dispatches
    /// per-kind reducers (attribute / combinator / pseudo / tag / universal),
    /// dedupes top-level Selector arms (when their stringified forms match
    /// post-clear), and lex-sorts the surviving Selectors.
    PostcssMinifySelectors,

    /// `parse → postcss-minify-params@5.1.4 (no opts) → stringify`. Phase 6f.
    /// `OnceExit` walks every AtRule. Filters to `@media` / `@supports`
    /// (case-insensitive). Bubble-walks the value-parsed params to
    /// normalize whitespace around Div tokens, function `before`/`after`,
    /// collapse Space tokens, drop the `all` keyword for media queries
    /// (legacy-IE-aware via browserslist), and reduce aspect-ratio pairs
    /// by integer GCD. Then `getArguments(params).map(stringify)` →
    /// `[...new Set(...)].sort().join(',')`. Empty result clears
    /// `raws.afterName`. Default opts; browserslist resolves to
    /// `4.24.2` defaults — no `ie 10` / `ie 11` → `legacy = false`.
    PostcssMinifyParams,

    /// `parse → postcss-ordered-values@5.1.3 (no opts) → stringify`. Phase 6d.
    /// OnceExit walker. Reorders multi-value parts of `border` /
    /// `box-shadow` / `animation` / `transition` / `flex-flow` / `outline`
    /// / `column-rule` / `columns` / `list-style` / `grid-auto-flow` /
    /// `grid-{column,row,column-start,row-start,column-end,row-end,column-gap,row-gap}`
    /// for shorthand-deduplication consistency. Variable functions
    /// (`var`/`env`/`constant`), comments, and `___CSS_LOADER_IMPORT___`
    /// markers short-circuit the transformation.
    PostcssOrderedValues,

    /// `parse → postcss-reduce-initial@5.1.2 (default opts) → stringify`.
    /// Phase 6e. Browserslist+caniuse-aware: `prepare(result)` resolves
    /// `isSupported('css-initial-value', browsers)` once at instantiation,
    /// then `OnceExit` walks every Decl. `toInitial[prop] === value` →
    /// `value = "initial"` (gated on caniuse). `value === "initial"` AND
    /// `fromInitial[prop]` exists → `value = fromInitial[prop]`.
    /// `defaultIgnoreProps = ['writing-mode', 'transform-box']` are
    /// always skipped (cssnano#905). `opts.ignore` extends that set.
    PostcssReduceInitial,

    /// `parse → postcss-colormin@5.3.1 (default opts, browserslist
    /// `chrome 100`) → stringify`. Phase 6g — **highest-risk cssnano
    /// plugin**. Browserslist+caniuse-aware: `addPluginDefaults` resolves
    /// `transparent` (true unless IE 8/9 in target) and `alphaHex`
    /// (caniuse `css-rrggbbaa`). Walks every Decl, skips
    /// `composes`/`font*`/`src`/`filter*`/`-webkit-tap-highlight-color`,
    /// then value-parser-walks each value rewriting rgb/rgba/hsl/hsla
    /// functions and bare-word colors via `colord(input).minify(opts)`
    /// with the `< input.length` strict-shorter check (else
    /// `input.toLowerCase()`). Math functions opaque. Caches by
    /// (value, options, browsers).
    PostcssColormin,

    /// `parse → postcss-minify-gradients@5.1.1 (no opts) → stringify`. Phase 6g.
    /// OnceExit walks every Decl. Bails when value is empty / contains
    /// `var(` / `env(` / lacks `gradient`. Otherwise value-parses and walks
    /// top-level Functions: linear-gradient (incl. `-webkit-` and
    /// `repeating-` variants) rewrites `to <side>` to angles, strips a
    /// non-deg `0<unit>` first stop and a final `100%` stop; radial-gradient
    /// (with optional `at` skip) and `-webkit-radial-gradient` (uses
    /// `isColorStop` predicate via `colord` + length-unit/calc check)
    /// renormalize each stop to `0` when `lastStop.unit` matches AND
    /// `lastStop.number >= thisStop.number` (upstream's misnamed
    /// `isLessThan` actually returns ≥; replicated verbatim).
    PostcssMinifyGradients,

    /// `parse → postcss-convert-values@5.1.3 (default opts) → stringify`.
    /// Phase 6f. Browserslist-aware — `pluginCreator` resolves
    /// `browsers = browserslist(null, { path: __dirname })` once. Under
    /// the workspace's locked 4.24.2 defaults the result does not include
    /// `'ie 11'`, so the `keepZeroPercent` IE-11 branch never fires.
    /// `OnceExit` walks every Decl, skipping flex / `--*` / `notALength`
    /// props; for each Word inside (excluding `url()` args), parses the
    /// number+unit, converts to the shortest equivalent across length /
    /// time / angle conv tables (ties favor LATER candidate per
    /// upstream's strict-`<` reduce), and clamps `opacity` /
    /// `shape-image-threshold` to `[0, 1]`. Default opts:
    /// `precision: false` — px-precision rounding disabled.
    PostcssConvertValues,

    /// `parse → postcss-calc@8.2.4 (default opts) → stringify`. Phase 6d.
    /// OnceExit walks every Decl, transforms `value` through value-parser
    /// looking for `(-vendor-)?calc(...)` function nodes, parses the inner
    /// expression with the jison-grammar parser, reduces it (constant
    /// folding, unit conversion, distributed mul/div), and re-stringifies.
    /// CSS variables (`var(...)`, `env(...)`, etc. — anything tokenized
    /// as a Function) are preserved opaquely. Default opts:
    /// `precision: 5`, `preserve: false`, `warnWhenCannotResolve: false`,
    /// `mediaQueries: false`, `selectors: false`.
    PostcssCalc,

    /// The full `sort()` entry point — `packages/css/src/sort.ts`. Runs
    /// `postcss-discard-duplicates@6 → mergeDuplicateAtRules → sortAtomicStyleSheet`
    /// with default opts (both `Option<bool>` flags `None`, mirroring the
    /// `undefined` defaults in the JS signature). This is the byte-parity
    /// gate for the smaller of the two hashing entry points.
    Sort,

    /// Phase 6 band exit gate — `packages/css/src/plugins/normalize-css.ts`
    /// in isolation. Runs `postcss(normalizeCSS({optimizeCss: true}))
    /// .process(css)` end-to-end: 14 cssnano sub-plugins (in preset
    /// source order) plus `normalize-current-color`'s Declaration visitor,
    /// composed via the postcss lifecycle (walk → OnceExit). This proves
    /// every Phase 6 sub-plugin port composes byte-clean against the JS
    /// pipeline as a unit, not just individually.
    ///
    /// Browserslist is pinned to `chrome 100` for the gate (env var
    /// `BROWSERSLIST=chrome 100` set by both bridges). Otherwise the 5
    /// browserslist-aware plugins (colormin, convert-values, minify-params,
    /// normalize-unicode, reduce-initial) would resolve to the workspace
    /// default which can drift across caniuse-lite versions.
    CssnanoBand,

    /// Phase 8b end-to-end gate — `packages/css/src/transform.ts` in full.
    /// Runs `transformCss(css, { optimizeCss: true })` end-to-end, returning
    /// `{ sheets, classNames }`. The bridge serializes the result via
    /// `JSON.stringify({ sheets, classNames })` so the byte-comparison
    /// covers BOTH the per-sheet stringification AND the class-name
    /// emission order. This is the strongest parity gate in the project —
    /// every Phase 4-7 plugin must compose byte-clean as a unit through
    /// the lifecycle-correct ordering documented in
    /// `crates/PHASE_8B_LIFECYCLE_AUDIT.md`.
    ///
    /// Browserslist is pinned to `chrome 100` for the gate (env var
    /// `BROWSERSLIST=chrome 100` set by the bridge). `AUTOPREFIXER` is
    /// left unset — the autoprefixer step runs on both engines (env var
    /// equality check `=== 'off'` is false for unset). The Rust side
    /// reads the same env var via `std::env::var("AUTOPREFIXER")`.
    ///
    /// Note: AUTOPREFIXER's browserslist resolution still walks from
    /// `current_dir()` when no `from:` is given (matching JS where
    /// `transform.ts:74` hardcodes `from: undefined`). Both engines run
    /// in the same cwd (the parity-runner process cwd), so the walk-up
    /// produces identical results.
    TransformCss,

    /// `parse → autoprefixer@10.4.14 (AFM browserslist) → stringify`. Phase 7.
    /// Runs `Processor::remove(root) → Processor::add(root)` against a
    /// `Prefixes` built from AFM's pinned `.browserslistrc` fixture
    /// (`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` —
    /// SHA256 `08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`,
    /// see HANDOVER.md §6). Both engines pin to the same fixture: Rust
    /// via `BrowsersOptions::from`, JS via `BROWSERSLIST_CONFIG` env var.
    /// AFM never drifts away from this fixture, so the parity gate
    /// validates the exact production resolution path.
    ///
    /// Pre-condition: AGENT_4 Pass 2 landed — `Processor::add` /
    /// `Processor::remove` are real. Pre-AFM-hack-subset coverage of the
    /// 5 in-scope hacks (cross-fade / intrinsic / text-decoration /
    /// text-decoration-skip-ink / user-select) lands via AGENT_5's Pass B.
    Autoprefixer,
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Stage::PostcssCoreRoundtrip => "postcss-core-roundtrip",
            Stage::DiscardEmptyRules => "discard-empty-rules",
            Stage::DiscardDuplicates => "discard-duplicates",
            Stage::ExtractStylesheets => "extract-stylesheets",
            Stage::ParentOrphanedPseudos => "parent-orphaned-pseudos",
            Stage::IncreaseSpecificity => "increase-specificity",
            Stage::MergeDuplicateAtRules => "merge-duplicate-at-rules",
            Stage::NormalizeCurrentColor => "normalize-current-color",
            Stage::SortAtomicStyleSheet => "sort-atomic-style-sheet",
            Stage::AtomicifyRules => "atomicify-rules",
            Stage::ExpandShorthands => "expand-shorthands",
            Stage::NpmPostcssDiscardDuplicates => "npm-postcss-discard-duplicates",
            Stage::PostcssNested => "postcss-nested",
            Stage::PostcssNormalizeWhitespace => "postcss-normalize-whitespace",
            Stage::PostcssDiscardComments => "postcss-discard-comments",
            Stage::PostcssNormalizeString => "postcss-normalize-string",
            Stage::PostcssNormalizePositions => "postcss-normalize-positions",
            Stage::PostcssNormalizeTimingFunctions => "postcss-normalize-timing-functions",
            Stage::PostcssNormalizeUrl => "postcss-normalize-url",
            Stage::PostcssNormalizeUnicode => "postcss-normalize-unicode",
            Stage::PostcssMinifySelectors => "postcss-minify-selectors",
            Stage::PostcssMinifyParams => "postcss-minify-params",
            Stage::PostcssOrderedValues => "postcss-ordered-values",
            Stage::PostcssReduceInitial => "postcss-reduce-initial",
            Stage::PostcssColormin => "postcss-colormin",
            Stage::PostcssMinifyGradients => "postcss-minify-gradients",
            Stage::PostcssCalc => "postcss-calc",
            Stage::PostcssConvertValues => "postcss-convert-values",
            Stage::Sort => "sort",
            Stage::CssnanoBand => "cssnano-band",
            Stage::TransformCss => "transform-css",
            Stage::Autoprefixer => "autoprefixer",
        }
    }
}

/// Absolute path to AFM's pinned `.browserslistrc` fixture directory.
/// Both engines (Rust `BrowsersOptions::from`, JS `BROWSERSLIST_CONFIG`)
/// pin to this directory so the resolution path is identical end-to-end.
/// HANDOVER.md §6 documents the closure rationale.
fn afm_browserslist_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = `<workspace>/crates/parity-runner`. Walk up to
    // workspace root, then into the AFM fixture subtree.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates")
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm")
}

/// Run the Rust counterpart of `stage` against `css` and return the
/// stringified output. `Err` carries a description of why the Rust side
/// couldn't produce output (parse error, plugin error, etc.).
pub fn rust_run_stage(stage: Stage, css: &str) -> Result<String, String> {
    match stage {
        Stage::PostcssCoreRoundtrip => {
            let root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            Ok(stringify(&root))
        }
        Stage::DiscardEmptyRules => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::discard_empty_rules::discard_empty_rules(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::DiscardDuplicates => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::discard_duplicates::discard_duplicates(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::ExtractStylesheets => {
            let root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let mut opts = compiled_css::plugins::extract_stylesheets::ExtractStyleSheetsOpts::default();
            compiled_css::plugins::extract_stylesheets::extract_stylesheets(&root, &mut opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(opts.sheets.join(&SHEET_SEP.to_string()))
        }
        Stage::ParentOrphanedPseudos => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::parent_orphaned_pseudos::parent_orphaned_pseudos(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::IncreaseSpecificity => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::increase_specificity::increase_specificity(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::MergeDuplicateAtRules => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::merge_duplicate_at_rules::merge_duplicate_at_rules(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::NormalizeCurrentColor => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::normalize_current_color::normalize_current_color(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::SortAtomicStyleSheet => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = compiled_css::plugins::sort_atomic_style_sheet::SortAtomicStyleSheetOpts {
                sort_at_rules_enabled: None,
                sort_shorthand_enabled: None,
            };
            compiled_css::plugins::sort_atomic_style_sheet::sort_atomic_style_sheet(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::AtomicifyRules => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let mut opts = compiled_css::plugins::atomicify_rules::AtomicifyRulesOpts::default();
            compiled_css::plugins::atomicify_rules::atomicify_rules(&mut root, &mut opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::ExpandShorthands => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::expand_shorthands::expand_shorthands(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::NpmPostcssDiscardDuplicates => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            css::plugins::postcss_discard_duplicates::postcss_discard_duplicates(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNested => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            // Mirror `packages/css/src/transform.ts:48-61` opts verbatim.
            let opts = postcss_nested::PostcssNestedOpts {
                bubble: vec![
                    "container".to_string(),
                    "-moz-document".to_string(),
                    "layer".to_string(),
                    "else".to_string(),
                    "when".to_string(),
                    "starting-style".to_string(),
                ],
                unwrap: vec![
                    "color-profile".to_string(),
                    "counter-style".to_string(),
                    "font-palette-values".to_string(),
                    "page".to_string(),
                    "property".to_string(),
                ],
                preserve_empty: false,
            };
            postcss_nested::postcss_nested(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizeWhitespace => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            postcss_normalize_whitespace::postcss_normalize_whitespace(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssDiscardComments => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = cssnano_postcss_discard_comments::DiscardCommentsOpts::default();
            cssnano_postcss_discard_comments::postcss_discard_comments(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizeString => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = cssnano_postcss_normalize_string::NormalizeStringOpts::default();
            cssnano_postcss_normalize_string::postcss_normalize_string(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizePositions => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_normalize_positions::postcss_normalize_positions(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizeTimingFunctions => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_normalize_timing_functions::postcss_normalize_timing_functions(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizeUrl => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = cssnano_postcss_normalize_url::NormalizeUrlOpts::default();
            cssnano_postcss_normalize_url::postcss_normalize_url(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssNormalizeUnicode => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_normalize_unicode::postcss_normalize_unicode(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssMinifySelectors => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_minify_selectors::postcss_minify_selectors(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssMinifyParams => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_minify_params::postcss_minify_params(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssOrderedValues => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_ordered_values::postcss_ordered_values(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssReduceInitial => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = cssnano_postcss_reduce_initial::PostcssReduceInitialOpts::default();
            cssnano_postcss_reduce_initial::postcss_reduce_initial(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssColormin => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            // Pin the browserslist query to "chrome 100" for the parity
            // gate. Both engines must see the same browsers — the JS
            // bridge passes the same string to `browserslist()`. Default
            // (empty) query would resolve to the workspace default which
            // can drift; an explicit pin makes the contract explicit.
            cssnano_postcss_colormin::postcss_colormin_with_query(&mut root, None, "chrome 100")
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssMinifyGradients => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            cssnano_postcss_minify_gradients::postcss_minify_gradients(&mut root)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssCalc => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = postcss_calc::Options::default();
            postcss_calc::postcss_calc(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::PostcssConvertValues => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = cssnano_postcss_convert_values::ConvertValuesOpts::default();
            cssnano_postcss_convert_values::postcss_convert_values(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::Sort => {
            // Default opts: both flags `None`, matching the `undefined`
            // defaults in `sort.ts:18-26`. The plugin defaults (true/true)
            // take effect inside sort_atomic_style_sheet.
            css::sort::sort(css, &css::sort::SortOpts::default())
        }
        Stage::CssnanoBand => {
            // Phase 6 band exit gate. `optimize_css = None` defaults to
            // `true` per JS line 59 — runs all 14 cssnano sub-plugins +
            // normalize-current-color. BROWSERSLIST env var is set by
            // the caller (parity-runner main / test harness) to keep both
            // engines on the same browser list.
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let opts = compiled_css::plugins::normalize_css::NormalizeCssOpts {
                optimize_css: None,
            };
            compiled_css::plugins::normalize_css::normalize_css(&mut root, &opts)
                .map_err(|e| format!("rust plugin error: {e:?}"))?;
            Ok(stringify(&root))
        }
        Stage::TransformCss => {
            // Phase 8b end-to-end gate. Run the lifecycle-correct
            // `css::transform::transform_css` and serialise the
            // `{ sheets, classNames }` result to a canonical JSON
            // shape that matches what the JS bridge emits via
            // `JSON.stringify({ sheets, classNames })`.
            //
            // Field order is `sheets` first then `classNames`, matching
            // the JS object-literal construction order — JSON.stringify
            // walks own-enumerable string keys in insertion order in V8.
            // We hand-build the JSON via serde_json::Value so the field
            // order is pinned regardless of struct-field ordering.
            //
            // The opts are `TransformOpts::default()` which mirrors the
            // bridge call site `transformCss(css, {})`: `optimizeCss`
            // unset → defaults to `true`, all other flags unset.
            //
            // Browserslist is pinned to `chrome 100` for both engines.
            // The JS bridge sets `process.env.BROWSERSLIST = 'chrome 100'`
            // around its call; the Rust call here mirrors that pin via
            // `std::env::set_var` (restored after). Both autoprefixer
            // and the 5 browserslist-aware cssnano sub-plugins read
            // this env var at call time.
            //
            // AUTOPREFIXER is explicitly removed so autoprefixer DOES
            // run (the check is `!= "off"`; unset → runs). Both engines
            // see identical env state.
            let prev_browserslist = std::env::var("BROWSERSLIST").ok();
            let prev_autoprefixer = std::env::var("AUTOPREFIXER").ok();
            std::env::set_var("BROWSERSLIST", "chrome 100");
            std::env::remove_var("AUTOPREFIXER");
            let opts = css::transform::TransformOpts::default();
            let result = css::transform::transform_css(css, &opts);
            // Restore env state before unwinding the result, so the
            // env mutation is scoped strictly to the call.
            match prev_browserslist {
                Some(v) => std::env::set_var("BROWSERSLIST", v),
                None => std::env::remove_var("BROWSERSLIST"),
            }
            match prev_autoprefixer {
                Some(v) => std::env::set_var("AUTOPREFIXER", v),
                None => std::env::remove_var("AUTOPREFIXER"),
            }
            let result = result?;
            // Hand-build the JSON string in `sheets` → `classNames`
            // order. `serde_json::json!` uses `serde_json::Map` which
            // requires the `preserve_order` feature to honour
            // insertion order; the workspace's serde_json is built
            // without it, so the macro alphabetises keys (`classNames`
            // → `sheets`). Using `serde_json::to_string` on each Vec
            // separately and concatenating with the literal field
            // markers guarantees the JS-engine field order without
            // adding a workspace-wide feature flag.
            let sheets_arr = serde_json::to_string(&result.sheets)
                .map_err(|e| format!("transform-css json error: {e}"))?;
            let class_names_arr = serde_json::to_string(&result.class_names)
                .map_err(|e| format!("transform-css json error: {e}"))?;
            Ok(format!(
                "{{\"sheets\":{sheets_arr},\"classNames\":{class_names_arr}}}"
            ))
        }
        Stage::Autoprefixer => {
            // Mirror `autoprefixer.js`'s `OnceExit(root)` hook:
            // `prefixes.processor.remove(root, result)` then
            // `prefixes.processor.add(root, result)`. Both gated by the
            // `options.add` / `options.remove` toggles — defaults are
            // both true (`AutoprefixerOptions::default()`).
            //
            // Browserslist is pinned to AFM's `.browserslistrc` fixture
            // via `BrowsersOptions::from`. The JS bridge pins the same
            // file via `BROWSERSLIST_CONFIG`. Both engines see the
            // identical 14-entry resolution.
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            let from = afm_browserslist_dir().to_string_lossy().into_owned();
            let prefixes = autoprefixer::autoprefixer::build_prefixes_default(Some(from))
                .map_err(|e| format!("rust autoprefixer build error: {e}"))?;
            let proc = autoprefixer::processor::Processor::new(&prefixes);
            let mut warnings: Vec<String> = Vec::new();
            // `Processor::{add, remove}` operate on the root Node, not the
            // Root wrapper. `Root` holds the root Node at `.root`.
            proc.remove(&mut root.root, &mut warnings);
            proc.add(&mut root.root, &mut warnings);
            // Warnings are diagnostic-only (cf. autoprefixer.js result.warn)
            // and don't affect output bytes; the JS bridge's
            // `result.css` doesn't include them either.
            Ok(stringify(&root))
        }
    }
}
