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
//! Phase 5 §5.5 PARTIAL (this checkpoint, parallel to §5.4): adds the
//! three §5.5 leaf traversers that DO NOT call into the resolver/scope
//! index — `traverse_binary_expression`, `traverse_unary_expression`,
//! `traverse_function` — plus their pure-data dependencies
//! `create_result_pair` and `has_numeric_value`. The remaining 11 files
//! in `traverse-expression/` (the resolve-binding-dependent half) wait
//! on §5.4. See `traverse_expression/mod.rs` module docs.
//!
//! Still pending (Phase 5/6):
//! * `resolve_binding.rs` (§5.4) — in progress, sequential
//! * `traverse_expression/` subtree (§5.5) — partial here; remaining
//!   leaves (`traverse-identifier`, `traverse-call-expression`,
//!   `traverse-member-expression/**`) gated on §5.4
//! * `traversers/` subtree (§5.6)
//! * `evaluate_expression.rs` (§5.6)

pub mod ast;
pub mod build_compiled_component;
pub mod build_css_variables;
pub mod cache;
pub mod compress_class_names_for_runtime;
pub mod constants;
pub mod create_result_pair;
pub mod css_builders;
pub mod get_jsx_attribute;
pub mod get_runtime_class_name_library;
pub mod has_numeric_value;
pub mod hoist_sheet;
pub mod is_compiled;
pub mod is_empty;
pub mod manipulate_template_literal;
pub mod object_property_to_string;
pub mod resolve_binding;
pub mod transform_css_items;
pub mod traverse_expression;
pub mod traversers;
pub mod types;
