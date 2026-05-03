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
        }
    }
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
            postcss_discard_duplicates::postcss_discard_duplicates(&mut root)
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
    }
}
