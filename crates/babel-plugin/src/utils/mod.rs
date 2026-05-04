//! 1:1 port of `packages/babel-plugin/src/utils/`.
//!
//! Phase 4 §4.4 (this checkpoint) lands the small tractable deps
//! (`types`, `is_empty`, `is_compiled`, `ast`, `object_property_to_string`,
//! `manipulate_template_literal`) and the `css_builders` shell with
//! `unimplemented!()` stubs for evaluate/resolve/visitCssMap call sites
//! gated on Phases 5/6. Each module MUST be a 1:1 port of its `.ts`
//! sibling — see `plugins/PLAN.md` constraint 4.
//!
//! Still pending (Phase 5/6):
//! * `cache.rs` (§5.3)
//! * `resolve_binding.rs` (§5.4)
//! * `traverse_expression/` subtree (§5.5)
//! * `traversers/` subtree (§5.6)
//! * `evaluate_expression.rs` (§5.6)
//! * `transform_css_items.rs` (§4.5)
//! * `build_css_variables.rs` (§4.5)

pub mod ast;
pub mod constants;
pub mod css_builders;
pub mod is_compiled;
pub mod is_empty;
pub mod manipulate_template_literal;
pub mod object_property_to_string;
pub mod types;
