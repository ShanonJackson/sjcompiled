//! 1:1 port of `packages/babel-plugin/src/utils/traversers/`.
//!
//! Three small visitor helpers used by `utils/resolve_binding.rs`
//! (Phase 5 §5.4e) when crossing module boundaries:
//!
//! - [`get_default_export`] — walks an imported module's AST and
//!   returns the default-export expression node + the AST scope it
//!   belongs to.
//! - [`get_named_export`] — walks an imported module's AST and
//!   returns the named-export expression node for a given name.
//! - [`get_object_property_value`] — walks an `ObjectExpression`
//!   for an object-property whose key matches a name; returns the
//!   value expression. Used by destructuring resolution paths.
//! - [`set_imported_compiled_imports`] — walks an imported module's
//!   AST and, if it imports `css` from `@compiled/react`, records
//!   the local binding name into `state.imported_compiled_imports`.
//!
//! ## Why this lives here
//!
//! These functions are described as `traversers/` in the JS plugin
//! and treated as `§5.6` deliverables in the original phase plan.
//! They land alongside `resolve_binding.rs` (§5.4e) because
//! `resolve-binding.ts` imports them directly — porting
//! `resolve_binding.rs` without them would force a stub layer that's
//! a strict subset of the actual port. STATUS.md §5.6 is updated to
//! reflect the bundling.
//!
//! ## Bug parity
//!
//! Each function preserves its upstream early-stop semantics — the
//! Babel `traverse` visitor calls `path.stop()` after the first
//! match, so we mirror that with a "first-write wins, subsequent
//! visits skipped" pattern in the SWC `Visit` impl. Multiple matches
//! in the same module are NOT collected; only the first is returned.

mod get_export;
mod object;
mod set_imported_compiled_imports;
mod types;

pub use get_export::{get_default_export, get_named_export};
pub use object::get_object_property_value;
pub use set_imported_compiled_imports::set_imported_compiled_imports;
pub use types::{ExportResult, ReexportHop};
