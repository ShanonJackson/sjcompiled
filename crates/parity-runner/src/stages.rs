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

    /// `parse → flattenMultipleSelectors → stringify`. Phase 4b.
    FlattenMultipleSelectors,

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
            Stage::FlattenMultipleSelectors => "flatten-multiple-selectors",
            Stage::IncreaseSpecificity => "increase-specificity",
            Stage::MergeDuplicateAtRules => "merge-duplicate-at-rules",
            Stage::NormalizeCurrentColor => "normalize-current-color",
            Stage::SortAtomicStyleSheet => "sort-atomic-style-sheet",
            Stage::AtomicifyRules => "atomicify-rules",
            Stage::ExpandShorthands => "expand-shorthands",
            Stage::NpmPostcssDiscardDuplicates => "npm-postcss-discard-duplicates",
            Stage::PostcssNormalizeWhitespace => "postcss-normalize-whitespace",
            Stage::PostcssDiscardComments => "postcss-discard-comments",
            Stage::PostcssNormalizeString => "postcss-normalize-string",
            Stage::PostcssNormalizePositions => "postcss-normalize-positions",
            Stage::PostcssNormalizeTimingFunctions => "postcss-normalize-timing-functions",
            Stage::PostcssNormalizeUrl => "postcss-normalize-url",
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
        Stage::FlattenMultipleSelectors => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            compiled_css::plugins::flatten_multiple_selectors::flatten_multiple_selectors(&mut root)
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
        Stage::Sort => {
            // Default opts: both flags `None`, matching the `undefined`
            // defaults in `sort.ts:18-26`. The plugin defaults (true/true)
            // take effect inside sort_atomic_style_sheet.
            css::sort::sort(css, &css::sort::SortOpts::default())
        }
    }
}
