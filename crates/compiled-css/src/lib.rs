//! crates/compiled-css
//! Rust home for the local plugins under `packages/css/src/plugins/`.
//!
//! This crate contains *only* plugins. The `transformCss` / `sort`
//! orchestrators live in `crates/css/` (which depends on this crate).
//! The split mirrors the way upstream organizes things: each `plugins/*.ts`
//! is a self-contained postcss plugin; `transform.ts` and `sort.ts` are
//! the orchestrators that compose plugins + third-party packages.
//!
//! Folder/file mapping (1:1 with `packages/css/src/plugins/`):
//!   - `atomicify-rules.ts`            -> `src/plugins/atomicify_rules.rs`
//!   - `discard-duplicates.ts`         -> `src/plugins/discard_duplicates.rs`
//!   - `discard-empty-rules.ts`        -> `src/plugins/discard_empty_rules.rs`
//!   - `extract-stylesheets.ts`        -> `src/plugins/extract_stylesheets.rs`
//!   - `flatten-multiple-selectors.ts` -> `src/plugins/flatten_multiple_selectors.rs`
//!   - `increase-specificity.ts`       -> `src/plugins/increase_specificity.rs`
//!   - `merge-duplicate-at-rules.ts`   -> `src/plugins/merge_duplicate_at_rules.rs`
//!   - `normalize-css.ts`              -> `src/plugins/normalize_css.rs`
//!   - `normalize-current-color.ts`    -> `src/plugins/normalize_current_color.rs`
//!   - `parent-orphaned-pseudos.ts`    -> `src/plugins/parent_orphaned_pseudos.rs`
//!   - `sort-atomic-style-sheet.ts`    -> `src/plugins/sort_atomic_style_sheet.rs`
//!   - `sort-shorthand-declarations.ts`-> `src/plugins/sort_shorthand_declarations.rs`
//!   - `at-rules/*.ts`                 -> `src/plugins/at_rules/*.rs`
//!   - `expand-shorthands/*.ts`        -> `src/plugins/expand_shorthands/*.rs`
//!
//! Plugin bodies are intentionally absent in this scaffold pass — they will
//! be filled in during Phase 4 (per `crates/EXECUTION_PLAN.md`).
//!
//! Utility helpers live in `src/utils/` and mirror `packages/css/src/utils/`.

pub mod plugins;
pub mod utils;
