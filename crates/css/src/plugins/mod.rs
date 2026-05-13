//! Folded plugin crates — see `crates/CONSOLIDATION_PLAN.md`.
//!
//! Each submodule was its own workspace crate; folded in to flatten the
//! `crates/` listing for review without changing the dep graph behaviour.
//! Parity-runner consumes these via `css::plugins::<name>::...` instead of
//! the former `<name>::...` top-level import.

pub mod compiled_css;
pub mod cssnano_postcss_colormin;
pub mod cssnano_postcss_convert_values;
pub mod cssnano_postcss_discard_comments;
pub mod cssnano_postcss_minify_gradients;
pub mod cssnano_postcss_minify_params;
pub mod cssnano_postcss_minify_selectors;
pub mod cssnano_postcss_normalize_positions;
pub mod cssnano_postcss_normalize_string;
pub mod cssnano_postcss_normalize_timing_functions;
pub mod cssnano_postcss_normalize_unicode;
pub mod cssnano_postcss_normalize_url;
pub mod cssnano_postcss_ordered_values;
pub mod cssnano_postcss_reduce_initial;
pub mod cssnano_preset_default;
pub mod postcss_calc;
pub mod postcss_discard_duplicates;
pub mod postcss_nested;
pub mod postcss_normalize_whitespace;
