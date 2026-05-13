//! Folded plugin crates — see `crates/CONSOLIDATION_PLAN.md`.
//!
//! Each submodule was its own workspace crate; folded in to flatten the
//! `crates/` listing for review without changing the dep graph behaviour.
//! Parity-runner consumes these via `css::plugins::<name>::...` instead of
//! the former `<name>::...` top-level import.

pub mod postcss_discard_duplicates;
