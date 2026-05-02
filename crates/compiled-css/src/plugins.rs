//! Plugin module index. Each declaration mirrors a `packages/css/src/plugins/*.ts`
//! file. Bodies land in Phase 4 — until then each module is a typed shell
//! exposing the plugin's public surface (factory function + options struct)
//! so `crates/css/transform.rs` can wire the call site without churn later.

pub mod atomicify_rules;
pub mod discard_duplicates;
pub mod discard_empty_rules;
pub mod extract_stylesheets;
pub mod increase_specificity;
pub mod merge_duplicate_at_rules;
pub mod normalize_css;
pub mod normalize_current_color;
pub mod parent_orphaned_pseudos;
pub mod sort_atomic_style_sheet;
pub mod sort_shorthand_declarations;

pub mod at_rules;
pub mod expand_shorthands;
