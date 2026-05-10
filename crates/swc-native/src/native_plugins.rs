//! Build the in-process Rust `Pass` for each plugin entry that the
//! caller threaded through `jsc.experimental.plugins`.
//!
//! The WASI build pairs each `[wasmPath, pluginOpts]` entry with a
//! per-plugin SWC plugin runtime. Native, we look at the wasm path's
//! basename, dispatch to the corresponding Rust crate's
//! `apply_native`, and ignore the wasm bytes themselves. That means
//! the JS surface is identical: callers pass the same plugins array
//! they'd pass to `@swc/core`, and they get the same output (modulo
//! the `unresolved_mark` caveat in `lib.rs`'s module doc).
//!
//! Today this only knows about `babel_plugin.wasm` →
//! `babel_plugin::apply_native`. Adding `babel_plugin_strip_runtime`
//! later is an `else if` here.

use anyhow::{anyhow, Result};
use serde_json::Value;
use swc_core::{
    base::config::PluginConfig,
    common::{comments::SingleThreadedComments, sync::Lrc, Mark, SourceMap},
    ecma::ast::{noop_pass, Pass, Program},
};

use babel_plugin::{apply_native, types::PluginOptions};

// **Fix 2 attempt (DISABLED, kept here for future investigation).**
//
// Initial theory: the cssnano divergence between WASI and native
// (white → #fff in native, `white` kept on WASI/Babel) was an
// environmental browserslist-resolution mismatch. The plan was to
// inject a process-local lazy snapshot via the existing
// `precomputedBrowserslistPath` plugin-opt path so both sides got
// the same bytes.
//
// Outcome: didn't move the parity number. Native still produces
// `#fff` even when given the same snapshot bytes WASI's fallback
// path computes. The divergence root cause is upstream of
// browserslist resolution (suspect: per-runtime difference in
// the cssnano-postcss-colormin transform itself, e.g.
// `caniuse_api::is_supported(..., "")` returning differently in
// WASI's no-FS context vs native's). Needs targeted instrumentation
// inside `crates/cssnano-postcss-colormin/src/lib.rs`'s
// `transform()` to confirm.
//
// The dispatch hook below is left in place so re-enabling is a
// one-liner once the root cause is identified.

/// `Pass` implementation that delegates to `babel_plugin::apply_native`.
///
/// Holds clones of the Compiler's `SourceMap` + the
/// `SingleThreadedComments` instance the Compiler is populating. Both
/// must come from the outer transform call — using fresh defaults
/// blows up at `source_map.lookup_char_pos(...)` because the program
/// spans point at the Compiler's source file, not ours.
pub struct BabelPluginPass {
    opts: PluginOptions,
    source_map: Lrc<SourceMap>,
    comments: SingleThreadedComments,
    /// **Fix 1 — mark threading.** The `unresolved_mark` minted by
    /// the dispatcher (in `lib.rs::transform`) and also set on
    /// `Options::unresolved_mark` so SWC's `build_as_input` resolver
    /// uses the same mark for user code's free identifiers. The
    /// babel-plugin colors its injected `import * as React` with
    /// this same mark so the post-resolver hygiene pass sees no
    /// conflict and doesn't rename to `React1`.
    ///
    /// `None` is still a valid value (matches the `apply_native`
    /// fallback used by the in-process workspace tests), but the
    /// dispatcher always passes `Some(_)` now.
    unresolved_mark: Option<Mark>,
    /// Host-absolute path of the file under transform. Threaded
    /// through to `apply_native` so the babel-plugin's cross-file
    /// resolver can anchor `import './tokens'` lookups against the
    /// fixture directory. WASI gets this via
    /// `meta.get_context(Filename)`; native callers have to pass it
    /// explicitly or `state.set_filename(...)` is skipped, breaking
    /// resolve_binding's deopt gate.
    filename: String,
}

impl BabelPluginPass {
    pub fn new(
        opts: PluginOptions,
        source_map: Lrc<SourceMap>,
        comments: SingleThreadedComments,
        unresolved_mark: Option<Mark>,
        filename: String,
    ) -> Self {
        Self {
            opts,
            source_map,
            comments,
            unresolved_mark,
            filename,
        }
    }
}

impl Pass for BabelPluginPass {
    fn process(&mut self, program: &mut Program) {
        // `apply_native` consumes `comments` by value — clone the
        // shared store so subsequent passes (and our own line-index
        // collector inside apply_native) see the same data. Cheap:
        // SingleThreadedComments wraps Rc<RefCell> internally.
        apply_native(
            program,
            self.opts.clone(),
            self.comments.clone(),
            &*self.source_map,
            self.filename.clone(),
            self.unresolved_mark,
        );
    }
}

/// Build the combined Pass for every plugin entry in `plugin_configs`.
/// Currently only `babel_plugin.wasm` is recognised; unknown wasm
/// paths return an error so a typo in the harness doesn't silently
/// no-op a transform.
///
/// `source_map` and `comments` come from the caller (the
/// `transform` entry in `lib.rs`), so the pass operates against the
/// same store SWC's parser populates.
pub fn build(
    plugin_configs: Option<Vec<PluginConfig>>,
    source_map: Lrc<SourceMap>,
    comments: SingleThreadedComments,
    unresolved_mark: Option<Mark>,
    filename: String,
) -> Result<Box<dyn Pass>> {
    let configs = match plugin_configs {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(Box::new(noop_pass())),
    };

    // Today there's only one supported plugin so the runtime cost of
    // the dispatch chain is negligible — kept as a Vec→reduce pattern
    // so `babel-plugin-strip-runtime` slots in by adding a branch
    // below.
    let mut passes: Vec<Box<dyn Pass>> = Vec::with_capacity(configs.len());
    for cfg in configs {
        let (path, opts_value) = (cfg.0, cfg.1);
        let basename = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        match basename {
            "babel_plugin.wasm" => {
                let opts: PluginOptions = parse_plugin_options(opts_value)?;
                passes.push(Box::new(BabelPluginPass::new(
                    opts,
                    source_map.clone(),
                    comments.clone(),
                    unresolved_mark,
                    filename.clone(),
                )));
            }
            other => {
                return Err(anyhow!(
                    "swc-native: no native binding registered for plugin '{}' (path: {})",
                    other,
                    path
                ));
            }
        }
    }

    Ok(Box::new(SequencePass { passes }))
}

struct SequencePass {
    passes: Vec<Box<dyn Pass>>,
}

impl Pass for SequencePass {
    fn process(&mut self, program: &mut Program) {
        for p in &mut self.passes {
            p.process(program);
        }
    }
}

fn parse_plugin_options(value: Value) -> Result<PluginOptions> {
    serde_json::from_value(value).map_err(|e| anyhow!("plugin options decode failed: {e}"))
}
