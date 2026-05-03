//! crates/compiled-css-napi
//!
//! NAPI bridge exposing the Rust `sort()` orchestrator from `crates/css`
//! and the autoprefixer port from `crates/autoprefixer` to Node/Bun.
//! Phase 8a (sort) + Phase 8b (autoprefixer) per
//! `crates/EXECUTION_PLAN.md`.
//!
//! ## Surface
//!
//! ```ts
//! export interface SortOpts {
//!   sortAtRulesEnabled?: boolean;
//!   sortShorthandEnabled?: boolean;
//! }
//! export function sort(stylesheet: string, opts?: SortOpts | null): string;
//!
//! export interface AutoprefixerOpts {
//!   /// Mirrors `result.opts.from` from postcss — autoprefixer reads
//!   /// `path.dirname(from)` and passes it to browserslist's `path`
//!   /// option for the directory walk-up. Pass an absolute file path
//!   /// inside the directory whose `.browserslistrc` should be picked
//!   /// up. AFM passes the source `.css` path here in production.
//!   from?: string;
//! }
//! export function autoprefixer(stylesheet: string, opts?: AutoprefixerOpts | null): string;
//! ```
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
use ::autoprefixer::autoprefixer::build_prefixes_default;
use ::autoprefixer::processor::Processor as AutoprefixerProcessor;
use ::postcss_core::{parse as postcss_parse, stringify as postcss_stringify};

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

/// JS-shaped autoprefixer options. `from` mirrors postcss's
/// `result.opts.from`. AFM passes the source `.css` file path so
/// autoprefixer's internal `browserslist(reqs, { path: dirname(from) })`
/// walks up to the project's pinned `.browserslistrc`. The Rust port
/// threads the same value into `BrowsersOptions::from`.
#[napi(object)]
pub struct AutoprefixerOpts {
    pub from: Option<String>,
}

/// `autoprefixer(stylesheet, opts?)` — byte-for-byte port of
/// `autoprefixer()` from `autoprefixer@10.4.14`. Mirrors
/// `autoprefixer.js`'s `OnceExit` hook: `prefixes.processor.remove(root)`
/// then `prefixes.processor.add(root)`. Output is parity-tested via the
/// `Stage::Autoprefixer` corpus in `crates/parity-runner/`.
///
/// `opts.from` is passed to `build_prefixes_default` which threads it
/// through `BrowsersOptions::from`. When `None`, browserslist resolves
/// from `std::env::current_dir()` matching `browserslist@4.24.2`'s
/// `prepareOpts` defaulting (HANDOVER.md §6).
#[napi]
pub fn autoprefixer(stylesheet: String, opts: Option<AutoprefixerOpts>) -> Result<String> {
    let from = opts.and_then(|o| o.from);
    let mut root = postcss_parse(&stylesheet)
        .map_err(|e| Error::from_reason(format!("parse error: {e}")))?;
    let prefixes = build_prefixes_default(from)
        .map_err(|e| Error::from_reason(format!("autoprefixer build error: {e}")))?;
    let proc = AutoprefixerProcessor::new(&prefixes);
    let mut warnings: Vec<String> = Vec::new();
    proc.remove(&mut root.root, &mut warnings);
    proc.add(&mut root.root, &mut warnings);
    Ok(postcss_stringify(&root))
}
