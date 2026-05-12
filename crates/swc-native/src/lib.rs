//! crates/swc-native — native-host wrapper around `swc_core::base::Compiler`
//! that runs the `babel-plugin` port as an in-process Rust pass instead of
//! a wasm32-wasip1 module.
//!
//! ## Why
//!
//! Our `crates/babel-plugin` build-out emits a `wasm32-wasip1` `.wasm`
//! that SWC's plugin runtime loads per-file. The WASI sandbox tears down
//! between transforms (no cross-call cache), every host call crosses a
//! VM boundary, and the JIT can't inline through it. Profiling against
//! Babel shows the WASI build is ~65× slower than upstream Babel running
//! the plain JS plugin. Native execution (no WASI, no per-call linear-
//! memory init, full inlining through `swc_core` + `babel_plugin`) is
//! the upper-bound benchmark for what the existing port can deliver.
//!
//! ## Shape
//!
//! Mirrors `C:/Users/shanon/Documents/projects/swc-rust/src` (the
//! upstream sketch this package was modelled on):
//!
//!   * `transform_sync(src, is_module, opts_buf) -> JSON string` —
//!     one-shot transform. Handed to `index.js` which wraps it as a
//!     `transformSync(src, options)` that's drop-in-compatible with
//!     `@swc/core::transformSync` for our parity-harness needs.
//!   * `native_plugins::build(plugin_configs)` — maps the
//!     `jsc.experimental.plugins` array (the same one the WASI build
//!     consumes via `@swc/core`) onto the in-process Rust passes that
//!     correspond to each plugin entry. See `native_plugins.rs`.
//!
//! ## Output parity
//!
//! Both the WASI plugin and this native wrapper call into
//! `babel_plugin::apply_native` (the same function `process()` delegates
//! to). The only behavioural delta is `unresolved_mark`: WASI gets it
//! from `meta.unresolved_mark` (set by SWC's pipeline); native callers
//! pre-resolve via `Compiler::run`'s default mark wiring, threaded
//! through the `&PluginContext` passed to the custom-pass closure. The
//! React-import-rename harness reconciler covers the `React1`-shape
//! drift if a native run lands in a fixture that exercises that path
//! without an upstream binding.

#![deny(warnings)]

// Global allocator override — mimalloc is faster than the system
// allocator across all three production platforms, though the margin
// varies a lot:
//   - Windows (HeapAlloc): ~2–3× on small-alloc-heavy workloads — the
//     largest single OS-level lever.
//   - Linux glibc (ptmalloc2): ~10–25%.
//   - macOS (libsystem malloc, Apple Silicon): ~5–15% — smallest
//     margin; Apple's allocator is well-tuned for the platform.
// SWC's AST + babel-plugin's per-node visitor work allocates very
// frequently, so we expect to land near the upper end of each band.
//
// Gated on `cfg(not(target_os = "wasi"))` because this cdylib only
// ever targets native; the cfg keeps the file honest if someone ever
// runs `cargo check --target wasm32-wasip1` against it.
#[cfg(not(target_os = "wasi"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};

use anyhow::{anyhow, Context, Error};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use swc_core::{
    base::{config::Options, try_with_handler, Compiler, HandlerOpts, TransformOutput},
    common::{
        comments::SingleThreadedComments, errors::Handler, sync::Lrc, FileName, FilePathMapping,
        Mark, SourceMap, GLOBALS,
    },
    ecma::ast::noop_pass,
};

mod native_plugins;

/// Fresh `Compiler` per transform — matches `@swc/core`'s shape (it
/// also constructs a new `Compiler` per `transformSync` call) and
/// avoids cross-call mutable state. The `SourceMap` lives only for
/// this one transform's duration; spans beyond that are invalid.
fn fresh_compiler() -> Compiler {
    let cm = Arc::new(SourceMap::new(FilePathMapping::empty()));
    Compiler::new(cm)
}

fn filename_for_options(options: &Options) -> FileName {
    if options.filename.is_empty() {
        FileName::Anon
    } else {
        FileName::Real(options.filename.clone().into())
    }
}

/// `try_with_handler` + panic catch — a thin wrapper that gives every
/// `c.run(|| ...)` block the same error-coercion shape (panic → anyhow
/// error with the panic payload). Lifted from swc-rust's pattern.
fn run_with_handler<F, Ret>(cm: Lrc<SourceMap>, skip_filename: bool, op: F) -> Result<Ret, Error>
where
    F: FnOnce(&Handler) -> Result<Ret, Error>,
{
    GLOBALS
        .set(&Default::default(), || {
            try_with_handler(
                cm,
                HandlerOpts {
                    skip_filename,
                    ..Default::default()
                },
                |handler| {
                    let result = catch_unwind(AssertUnwindSafe(|| op(handler)));
                    match result {
                        Ok(v) => v,
                        Err(v) => {
                            if let Some(s) = v.downcast_ref::<String>() {
                                Err(anyhow!("failed to handle: {s}"))
                            } else if let Some(s) = v.downcast_ref::<&str>() {
                                Err(anyhow!("failed to handle: {s}"))
                            } else {
                                Err(anyhow!("failed to handle with unknown panic message"))
                            }
                        }
                    }
                },
            )
        })
        // `try_with_handler` returns Result<_, TWithDiagnosticArray<Error>>;
        // collapse the wrapper into a single anyhow error with the
        // pretty diagnostics rendered into the message.
        .map_err(|e| e.to_pretty_error())
}

/// Core transform — pure Rust entry, callable from `examples/perf.rs`
/// without going through napi. Returns the same `TransformOutput`
/// shape `swc::Compiler::process_js` returns.
///
/// `source` is always the raw source string; the
/// `transformSync(programJson, opts)` shape from `@swc/core` is not
/// supported here (the parity harness only ever calls the string
/// form). Adding it later is a `serde_json::from_str::<Program>`
/// branch + `Program: Deserialize` via `swc_core[ecma_ast_serde]`.
pub fn transform(source: String, options_json: &[u8]) -> Result<TransformOutput, Error> {
    let c = fresh_compiler();

    let mut options: Options =
        serde_json::from_slice(options_json).context("failed to deserialize Options")?;

    if !options.filename.is_empty() {
        options
            .config
            .adjust(std::path::Path::new(&options.filename));
    }

    // Pull the `jsc.experimental.plugins` config out before handing
    // `options` to the SWC compiler — it'd try to load each entry as
    // a wasm path otherwise. We translate them to native passes via
    // `native_plugins::build`.
    let plugin_configs = options.config.jsc.experimental.plugins.take();

    let skip_filename = !options.config.error.filename.into_bool();

    run_with_handler(c.cm.clone(), skip_filename, |handler| {
        c.run(|| {
            // **Fix 1 — mark threading.**
            //
            // SWC's `Options` exposes `unresolved_mark` / `top_level_mark`
            // that, if set, are used by `build_as_input`'s resolver pass
            // (see swc/src/config/mod.rs:318-319 + 343). We mint our own,
            // set them on `options`, AND hand the `unresolved_mark` to
            // the babel-plugin pass so its injected `import * as React`
            // gets the same `SyntaxContext` as the user-code free
            // references the resolver will mark immediately after.
            //
            // Without this: the new import lands in `SyntaxContext::empty()`,
            // SWC's hygiene pass treats it as a separate `React` from
            // user code's `React.createElement(...)`, and renames our
            // import to `React1` to disambiguate — leaving the body
            // referencing `React` (broken at runtime). 107 of 153
            // divergences in the prior triage came from this.
            //
            // WASI is unaffected: the WASI plugin gets its mark from
            // `meta.unresolved_mark` (provided by the SWC plugin runtime
            // after IT's already run resolver), and `apply_native`
            // accepts the mark as `Option<Mark>` either way.
            let unresolved_mark = Mark::new();
            let top_level_mark = Mark::new();
            options.unresolved_mark = Some(unresolved_mark);
            options.top_level_mark = Some(top_level_mark);

            // Construct the comments store once, hand a clone to the
            // pass so the line-index collector inside `apply_native`
            // sees the same data SWC's parser populates. SourceMap
            // is `Lrc`-shared the same way.
            let comments = SingleThreadedComments::default();
            // **Filename threading.** The babel-plugin's cross-file
            // resolver (`compat::resolve_binding`, `oxc_resolver`)
            // anchors imports relative to `state.filename` — set by
            // `apply_native` from its `raw_filename` parameter. WASI
            // gets this from `meta.get_context(Filename)`. Native
            // callers must thread the host-absolute path explicitly
            // or every cross-file import silently deopts (Compiled
            // emits a `var(--_xxx)` reference instead of the inlined
            // value, and class-name hashes diverge accordingly).
            // 18 of the prior 153 divergences were exactly this
            // — every `ct-css-imported-*` / `ct-styled-imported-*`
            // / `ct-css-shared-styles-*` / `ct-css-twice-imported`
            // / `ct-css-var-imported-*` fixture in the parity
            // harness.
            let pass_filename = options.filename.clone();
            let native_pass = native_plugins::build(
                plugin_configs,
                c.cm.clone(),
                comments.clone(),
                Some(unresolved_mark),
                pass_filename,
            )?;
            let fm = c
                .cm
                .new_source_file(filename_for_options(&options).into(), source);

            c.process_js_with_custom_pass(
                fm,
                None,
                handler,
                &options,
                comments,
                move |_| native_pass,
                |_| noop_pass(),
            )
        })
    })
}

/// NAPI entry — accepts the same arg shape as our `index.js` shim
/// (which mirrors swc-rust's binding.js). Returns the
/// JSON-stringified `TransformOutput` rather than an n-api object so
/// we don't have to wire the swc `TransformOutput` shape through
/// napi-2's `#[napi(object)]` derive (we don't own the type).
#[napi]
pub fn transform_sync(src: String, opts: Buffer) -> napi::Result<String> {
    transform(src, opts.as_ref())
        .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
        .and_then(|out| {
            serde_json::to_string(&out)
                .map_err(|e| napi::Error::from_reason(format!("serialize TransformOutput: {e}")))
        })
}
