//! 1:1 port of `packages/babel-plugin/src/utils/`.
//!
//! Phase 4 §4.4 (prior checkpoint) lands the small tractable deps
//! (`types`, `is_empty`, `is_compiled`, `ast`, `object_property_to_string`,
//! `manipulate_template_literal`) and the `css_builders` shell with
//! `unimplemented!()` stubs for evaluate/resolve/visitCssMap call sites
//! gated on Phases 5/6.
//!
//! Phase 4 §4.5 (this checkpoint) adds the two pure-data adapters that
//! consume the shell's `CSSOutput` / `CssItem` / `Variable` shape:
//! `transform_css_items` and `build_css_variables`, plus the small
//! `compress_class_names_for_runtime` helper that
//! `transform_css_items` depends on. Each module MUST be a 1:1 port
//! of its `.ts` sibling — see `plugins/PLAN.md` constraint 4.
//!
//! Still pending (Phase 5/6):
//! * `cache.rs` (§5.3)
//! * `resolve_binding.rs` (§5.4)
//! * `traverse_expression/` subtree (§5.5)
//! * `traversers/` subtree (§5.6)
//! * `evaluate_expression.rs` (§5.6)

pub mod ast;
pub mod build_compiled_component;
pub mod build_css_variables;
pub mod cache;
pub mod compress_class_names_for_runtime;
pub mod constants;
pub mod css_builders;
pub mod get_jsx_attribute;
pub mod get_runtime_class_name_library;
pub mod hoist_sheet;
pub mod is_compiled;
pub mod is_empty;
pub mod manipulate_template_literal;
pub mod object_property_to_string;
pub mod transform_css_items;
pub mod types;
