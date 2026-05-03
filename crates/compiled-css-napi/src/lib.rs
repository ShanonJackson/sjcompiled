//! crates/compiled-css-napi
//!
//! NAPI bridge exposing the Rust `sort()` orchestrator from `crates/css`,
//! the autoprefixer port from `crates/autoprefixer`, and the full
//! `transformCss` pipeline from `crates/css` to Node/Bun. Phase 8a
//! (sort) + Phase 8b (autoprefixer + full transform) per
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
//!   from?: string;
//! }
//! export function autoprefixer(stylesheet: string, opts?: AutoprefixerOpts | null): string;
//!
//! export interface TransformOpts {
//!   optimizeCss?: boolean;
//!   classNameCompressionMap?: Record<string, string>;
//!   increaseSpecificity?: boolean;
//!   sortAtRules?: boolean;
//!   sortShorthand?: boolean;
//!   classHashPrefix?: string;
//! }
//! export interface TransformResult {
//!   sheets: string[];
//!   classNames: string[];
//! }
//! export function transformCss(css: string, opts?: TransformOpts | null): TransformResult;
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
//! `sort()` / `transformCss()` in Rust return `Result<_, String>`. The
//! NAPI shim maps the error string to a JS `Error` thrown back to the
//! caller, matching the behavior of upstream postcss in JS (which throws
//! on parse error). For `transformCss`, the JS-side wrapper in
//! `packages/css/src/transform.ts:84-99` re-wraps any thrown error in a
//! `createError('css', 'Unhandled exception')` envelope; reproducing that
//! envelope is the JS wrapper's responsibility, not the NAPI shim's,
//! because consumers calling the JS engine directly hit the same path.
//!
//! ## `classNameCompressionMap` insertion order — drift gate
//!
//! Per `PHASE_8B_LIFECYCLE_AUDIT.md` Plugin 1 DRIFT RISK, the JS object
//! `for-in` order is insertion order for string keys. The Rust port
//! threads `IndexMap<String, String>` to preserve that order. The NAPI
//! shim CANNOT use a plain `HashMap` — it would shuffle keys and emit
//! different class names on consumers that rely on insertion-order map
//! iteration during atomicify lookup. We therefore receive the JS map
//! as a `JsObject`, walk its property names via `get_property_names()`
//! (which returns names in JS-spec own-enumeration order — matching
//! `Object.keys()` semantics, V8 spec), and build an `IndexMap` in that
//! order. Verified by the `classnamecompressionmap-insertion-order`
//! corpus fixture.

use indexmap::IndexMap;
use napi::bindgen_prelude::*;
use napi::JsObject;
use napi_derive::napi;

use ::css::sort::{sort as rust_sort, SortOpts as RustSortOpts};
use ::css::transform::{transform_css as rust_transform_css, TransformOpts as RustTransformOpts};
use ::autoprefixer::autoprefixer::{build_prefixes_default, AutoprefixerOptions as RustAutoprefixerOptions};
use ::autoprefixer::precomputed::{encode_precomputed, precompute_prefixes};
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

/// JS-shaped `TransformOpts` mirroring `packages/css/src/transform.ts:17-24`
/// field-for-field. `classNameCompressionMap` is intentionally NOT a
/// `HashMap<String, String>` — see the crate-level docblock for the
/// insertion-order rationale. We receive it as `Option<JsObject>` and
/// walk its keys in JS-spec order to preserve insertion semantics.
#[napi(object)]
pub struct TransformOpts {
    pub optimize_css: Option<bool>,
    pub class_name_compression_map: Option<JsObject>,
    pub increase_specificity: Option<bool>,
    pub sort_at_rules: Option<bool>,
    pub sort_shorthand: Option<bool>,
    pub class_hash_prefix: Option<String>,
    /// **Optional perf knob — NOT part of the upstream `TransformOpts`
    /// surface.** Pass postcard bytes produced by
    /// `precomputePrefixesDefault()` to skip the per-call autoprefixer
    /// filesystem walk + browserslist resolution + full PREFIXES table
    /// iteration. Byte-equal to omitting it.
    ///
    /// `Buffer` round-trips zero-copy through NAPI. The Rust side
    /// passes the bytes opaquely to `autoprefixer::precomputed`.
    pub precomputed_prefixes: Option<Buffer>,
}

/// JS-shaped `TransformResult`. Field naming matches the JS contract in
/// `transform.ts:35` exactly: `{ sheets: string[]; classNames: string[] }`.
/// `napi-rs` lowercases-camelizes Rust field names; `class_names` becomes
/// `classNames` on the JS side.
#[napi(object)]
pub struct TransformResult {
    pub sheets: Vec<String>,
    pub class_names: Vec<String>,
}

/// `transformCss(css, opts?)` — byte-for-byte port of `transformCss()` in
/// `packages/css/src/transform.ts:32`. Composes the 12-plugin pipeline
/// in postcss-lifecycle-correct order (Once → walk → OnceExit) per
/// `crates/PHASE_8B_LIFECYCLE_AUDIT.md` and `crates/PHASE_8B_COMPOSE_NOTES.md`.
///
/// The two internal callbacks (atomicifyRules's class-name push,
/// extractStyleSheets's sheet push) are private to the JS function and
/// resolved before return; the Rust port collects them into the
/// `TransformResult` `Vec<String>` fields. The JS-side wrapper in
/// `packages/css/src/transform.ts` reads those vecs back and returns the
/// `{ sheets, classNames }` shape directly to the caller — so the public
/// API contract is preserved with no callback marshalling needed.
///
/// Output is parity-tested via the `Stage::TransformCss` corpus in
/// `crates/parity-runner/`.
#[napi]
pub fn transform_css(css: String, opts: Option<TransformOpts>) -> Result<TransformResult> {
    let rust_opts = match opts {
        Some(o) => RustTransformOpts {
            optimize_css: o.optimize_css,
            class_name_compression_map: jsobject_to_indexmap(o.class_name_compression_map)?,
            increase_specificity: o.increase_specificity,
            sort_at_rules: o.sort_at_rules,
            sort_shorthand: o.sort_shorthand,
            class_hash_prefix: o.class_hash_prefix,
            // `Buffer.as_ref()` borrows; `to_vec()` copies. We copy
            // because `RustTransformOpts` owns the bytes — keeps
            // lifetime management trivial. The cost is one alloc +
            // memcpy per call (the snapshot is small, kilobyte-range).
            precomputed_prefixes: o.precomputed_prefixes.map(|b| b.as_ref().to_vec()),
        },
        None => RustTransformOpts::default(),
    };
    let result = rust_transform_css(&css, &rust_opts)
        .map_err(|e| Error::from_reason(e))?;
    Ok(TransformResult {
        sheets: result.sheets,
        class_names: result.class_names,
    })
}

/// `precomputePrefixesDefault(from?)` — produce the postcard bytes a
/// caller can pass back to `transformCss` via `opts.precomputedPrefixes`.
///
/// Runs the slow construction path (`Browsers::new` + `select` +
/// snapshot) ONCE, returns the encoded blob as a `Buffer`. Hand that
/// `Buffer` to subsequent `transformCss` calls and they skip the
/// per-call setup cost.
///
/// `from` mirrors `result.opts.from` — pass the project root or any
/// path under it so `.browserslistrc` resolution lands in the right
/// scope. When `None`, `current_dir()` is the resolution anchor.
///
/// **WASI consumers:** the host (Node) calls this once, reads the
/// returned bytes, and passes them through `plugin_config` on every
/// subsequent SWC plugin invocation. The plugin sees a fresh
/// `Buffer` per call but the bytes are constant.
#[napi]
pub fn precompute_prefixes_default(from: Option<String>) -> Result<Buffer> {
    let snapshot = precompute_prefixes(RustAutoprefixerOptions {
        from,
        ..Default::default()
    });
    let bytes = encode_precomputed(&snapshot);
    Ok(Buffer::from(bytes))
}

/// Walks a JS object's own enumerable string-keyed properties in JS-spec
/// order (matching `Object.keys()`), producing an `IndexMap<String, String>`
/// whose iteration order equals the JS insertion order. Keys whose values
/// are not strings are skipped (mirrors JS where `Object.entries(obj)`
/// returns the value as-is and the consumer code in atomicify expects a
/// string lookup — non-string values would error at use-site under JS
/// equality, so dropping them here matches the byte-equivalent fault-on-
/// use semantics).
///
/// Returns `None` when the input is `None`. Returns `Err` only on NAPI
/// access failure (which would be a programming error in the JS caller).
fn jsobject_to_indexmap(
    jsobj: Option<JsObject>,
) -> Result<Option<IndexMap<String, String>>> {
    let Some(jsobj) = jsobj else { return Ok(None) };
    let names = jsobj.get_property_names()?;
    let len: u32 = names.get_array_length()?;
    let mut map: IndexMap<String, String> = IndexMap::with_capacity(len as usize);
    for i in 0..len {
        let key_val: napi::JsString = names.get_element(i)?;
        let key_utf8 = key_val.into_utf8()?;
        let key = key_utf8.into_owned()?;
        // Only string-typed values land in the map. Other types are
        // silently skipped to match V8's coercion behaviour at the JS
        // call site (`compressedClassName[longClassName]` returns
        // `undefined` for non-string entries; atomicify treats undefined
        // as "no compression", which is also what skipping does here).
        if let Ok(value_js) = jsobj.get_named_property::<napi::JsString>(&key) {
            let value_utf8 = value_js.into_utf8()?;
            let value = value_utf8.into_owned()?;
            map.insert(key, value);
        }
    }
    Ok(Some(map))
}
