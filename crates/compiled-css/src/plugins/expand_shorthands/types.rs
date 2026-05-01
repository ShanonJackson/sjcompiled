//! Port of `packages/css/src/plugins/expand-shorthands/types.ts`.
//!
//! Each conversion function takes a parsed values-parser Root and
//! returns a list of [`Longform`] entries. The plugin entry maps each
//! Longform to a cloned Declaration (prop+value) and replaces the
//! original via `decl.replaceWith(...)`.
//!
//! Upstream:
//! ```ts
//! type ConversionFunction = (value: Root) => { prop?: string; value: string | number }[];
//! ```
//!
//! `value` is `string | number` upstream; we always carry it as `String`
//! (numbers get formatted at the call site — JS template literal
//! `${val.value}` produces the same `"1"` string from `1`).

use postcss_values_parser::Root;

#[derive(Debug, Clone)]
pub struct Longform {
    /// `None` ↔ upstream `prop: undefined`. The single special case
    /// `[{ value: "..." }]` (one entry, prop None) signals "leave the
    /// decl unchanged" — see `expand-shorthands/index.ts`'s early-exit
    /// branch.
    pub prop: Option<String>,
    pub value: String,
}

impl Longform {
    /// Convenience: build a `Longform { prop: Some(p), value: v }`.
    pub fn new(prop: impl Into<String>, value: impl Into<String>) -> Self {
        Longform { prop: Some(prop.into()), value: value.into() }
    }

    /// Convenience: the "leave decl unchanged" sentinel.
    pub fn no_op(value: impl Into<String>) -> Self {
        Longform { prop: None, value: value.into() }
    }
}

/// Conversion function signature mirrors upstream
/// `(value: Root) => Longform[]`.
pub type ConversionFunction = fn(&Root) -> Vec<Longform>;
