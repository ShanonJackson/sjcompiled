//! crates/compiled-css-napi
//!
//! NAPI bridge exposing the Rust `sort()` orchestrator from `crates/css`
//! to Node/Bun. Phase 8a per `crates/EXECUTION_PLAN.md`.
//!
//! ## Surface (Phase 8a — sort only)
//!
//! ```ts
//! export interface SortOpts {
//!   sortAtRulesEnabled?: boolean;
//!   sortShorthandEnabled?: boolean;
//! }
//! export function sort(stylesheet: string, opts?: SortOpts | null): string;
//! ```
//!
//! `transformCss` follows in Phase 8b once Phase 5/6/7 plugin ports land.
//!
//! ## Why a separate crate?
//!
//! Keeping the NAPI shim out of `crates/css` means the orchestrator stays
//! a pure Rust library — usable from `parity-runner`, future fuzz
//! targets, and any non-Node consumer (WASM, CLI) without dragging in
//! napi runtime deps.
//!
//! ## Errors
//!
//! `sort()` in Rust returns `Result<String, String>`. The NAPI shim maps
//! the error string to a JS `Error` thrown back to the caller, matching
//! the behavior of upstream postcss in JS (which throws on parse error).

use napi::bindgen_prelude::*;
use napi_derive::napi;

use ::css::sort::{sort as rust_sort, SortOpts as RustSortOpts};

/// JS-shaped sort options. `undefined`/missing → `None`, mirroring the
/// `undefined`-default semantics in `packages/css/src/sort.ts:18-26`
/// (deferring defaults to the underlying plugin).
#[napi(object)]
pub struct SortOpts {
    pub sort_at_rules_enabled: Option<bool>,
    pub sort_shorthand_enabled: Option<bool>,
}

/// `sort(stylesheet, opts?)` — byte-for-byte port of `sort()` in
/// `packages/css/src/sort.ts`. The output is parity-tested via the
/// `Stage::Sort` corpus in `crates/parity-runner/`.
#[napi]
pub fn sort(stylesheet: String, opts: Option<SortOpts>) -> Result<String> {
    let rust_opts = match opts {
        Some(o) => RustSortOpts {
            sort_at_rules_enabled: o.sort_at_rules_enabled,
            sort_shorthand_enabled: o.sort_shorthand_enabled,
        },
        None => RustSortOpts::default(),
    };
    rust_sort(&stylesheet, &rust_opts).map_err(|e| Error::from_reason(e))
}
