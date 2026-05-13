//! 1:1 vendor ports of npm packages (`cssnano-utils`, `postcss-selector-parser`,
//! `postcss-values-parser`, `colord`) folded in from former top-level crates.
//! See `crates/CONSOLIDATION_PLAN.md` (Phase 6). Kept separate from `plugins/`
//! because these are libraries, not pipeline plugins.

pub mod colord;
pub mod cssnano_utils;
pub mod postcss_selector_parser;
pub mod postcss_values_parser;
