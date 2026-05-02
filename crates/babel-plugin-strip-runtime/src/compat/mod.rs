//! Shims for Babel APIs SWC doesn't expose. Files here are paired with
//! a coverage manifest before any visitor logic depends on them — see
//! PLAN.md §3.8 / Cardinal rules conformance.

pub mod scope;
