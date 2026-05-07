//! Spawns `node --experimental-loader … packages/css/scripts/
//! parity-bridge.mjs` as a subprocess and runs a batch of `{stage,
//! css}` requests over stdio. One spawn covers the whole corpus to
//! amortise the ~150ms node startup + babel-typescript transpile of
//! the bridge's TS imports.
//!
//! Wire format: NDJSON. Caller writes all requests, closes stdin
//! (EOF), then drains all responses. Order is preserved (request N
//! maps to response N).
//!
//! ## Why node, not bun?
//!
//! The AFM monorepo runs `transformCss` under node V8 in production.
//! Bun runs JavaScriptCore. V8 and JSC implement
//! `Array.prototype.sort` with different stable sort algorithms
//! (TimSort vs. merge-sort), and on `sort-shorthand-declarations`'s
//! deliberately non-transitive comparator the two engines disagree on
//! the final order for inputs that mix declarations with comments or
//! nested rules. Running the parity oracle under bun was masking real
//! V8-correct Rust output as "diverged"; switching to node makes the
//! oracle observably equal to AFM production.
//!
//! See `packages/css/scripts/parity-bridge-ts-loader.mjs` for the
//! on-the-fly TypeScript loader hook (node 20.15+ uses
//! `module.register`); it transpiles `.ts` plugin sources via
//! `@babel/preset-typescript` so the bridge can import them
//! directly without a build step.
//!
//! ## Batching policy
//!
//! Node's stdout to a pipe is non-blocking (libuv async writes), but
//! the kernel pipe buffer is finite (~64KB on macOS / Linux). The
//! Rust harness writes every request first, closes stdin, then
//! drains stdout — a "write-all-then-read" protocol. If the JS
//! pipeline emits more output than the pipe can hold before EOF on
//! stdin (which triggers the bridge's processing loop), we
//! deadlock. We chunk batches at `BATCH_MAX = 256` so the per-batch
//! response stream stays well below the pipe + node userspace
//! buffer ceiling for every stage, including `transform-css` which
//! emits ~5KB of `{sheets, classNames}` JSON per fixture.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::stages::Stage;

#[derive(Debug, Serialize)]
pub struct JsRequest<'a> {
    pub stage: &'a str,
    pub css: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct JsResponse {
    pub ok: bool,
    #[serde(default)]
    pub css: String,
    #[serde(default)]
    pub error: String,
}

/// Maximum number of inputs per JS-bridge subprocess invocation.
///
/// See module docs (§ "Batching policy") for the buffering rationale.
/// 256 was picked empirically against the bun runtime; node's
/// libuv-backed stdout has the same back-pressure characteristics on
/// the kernel-pipe side, so the same ceiling applies. Per-batch cost
/// is one node startup (~150ms) + one babel-TS transpile of the
/// bridge's plugin imports (~250ms cold, ~50ms with v8 code cache).
const BATCH_MAX: usize = 256;

/// Run a batch of requests through the JS bridge, transparently
/// chunking large batches across multiple bun subprocesses. Returns
/// one response per input, in input order.
pub fn run_batch(stage: Stage, inputs: &[&str]) -> Result<Vec<JsResponse>, String> {
    if inputs.len() <= BATCH_MAX {
        return run_batch_inner(stage, inputs);
    }
    let mut all = Vec::with_capacity(inputs.len());
    for chunk in inputs.chunks(BATCH_MAX) {
        let mut part = run_batch_inner(stage, chunk)?;
        all.append(&mut part);
    }
    Ok(all)
}

fn run_batch_inner(stage: Stage, inputs: &[&str]) -> Result<Vec<JsResponse>, String> {
    let script = script_path();
    if !script.exists() {
        return Err(format!("JS pipeline script not found at {}", script.display()));
    }
    let loader = loader_path();
    if !loader.exists() {
        return Err(format!("TS loader not found at {}", loader.display()));
    }
    let cjs_hook = cjs_hook_path();
    if !cjs_hook.exists() {
        return Err(format!("CJS hook not found at {}", cjs_hook.display()));
    }
    let candidates: &[&str] = if cfg!(windows) {
        &["node.exe", "node"]
    } else {
        &["node"]
    };
    let runtime = candidates
        .iter()
        .copied()
        .find(|c| which(c))
        .ok_or("node is required for the parity harness — install Node.js 20.15+")?;

    // `--experimental-loader` (still the public name in node 20.x even
    // after `module.register` was promoted) hooks our babel-TS loader
    // before any user `import` runs. `--no-warnings` silences the
    // `ExperimentalWarning: Custom ESM Loaders is an experimental
    // feature…` line that would otherwise contaminate stderr and
    // make `stderr_buf` noisy on success.
    let loader_url = format!("file://{}", loader.display());
    // Set cwd to the workspace root so npm package resolution from
    // the bridge's `import 'postcss'` etc. lands on the workspace
    // `node_modules/`. Without this the inherited cwd from the Rust
    // process (often `crates/`) makes node walk up and miss the
    // hoisted dependencies. Also matters for `node`'s on-the-fly
    // require resolution inside the CJS-from-ESM translator path,
    // which uses cwd-relative resolution as a fallback for some
    // edge cases (`./peer` in a file loaded via ESM bridge).
    let workspace_root = script.parent().unwrap()  // scripts/
        .parent().unwrap()                          // css/
        .parent().unwrap()                          // packages/
        .parent().unwrap();                         // workspace
    let mut child = Command::new(runtime)
        .arg("--no-warnings")
        .arg("--no-deprecation")
        .arg("--require")
        .arg(&cjs_hook)
        .arg("--experimental-loader")
        .arg(&loader_url)
        .arg(&script)
        .current_dir(workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn {runtime}: {e}"))?;

    // Write every request, then drop stdin (EOF) so the bridge's
    // `rl.on('close')` fires and the process exits cleanly after
    // emitting its responses. Caller ensures `inputs.len() <=
    // BATCH_MAX` so the response stream fits in the kernel pipe +
    // node libuv userspace buffers without deadlocking.
    {
        let mut stdin = child.stdin.take().ok_or("missing stdin")?;
        for css in inputs {
            let req = JsRequest { stage: stage.name(), css };
            let payload = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            writeln!(stdin, "{payload}").map_err(|e| format!("stdin write: {e}"))?;
        }
        // dropping `stdin` closes the pipe -> bun sees EOF.
    }

    // Drain stdout to a single string, then split on newlines.
    let mut stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut out = String::new();
    stdout
        .read_to_string(&mut out)
        .map_err(|e| format!("stdout read: {e}"))?;

    // Drain stderr too so we can include it in the error message if
    // the bridge died unexpectedly.
    let mut stderr_buf = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_buf);
    }

    let status = child.wait().map_err(|e| format!("wait: {e}"))?;

    let mut responses = Vec::with_capacity(inputs.len());
    for line in out.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let resp: JsResponse = serde_json::from_str(trimmed).map_err(|e| {
            format!(
                "bad JSON from JS bridge: {e} — line was: {trimmed:?} — \
                 stderr: {stderr_buf}"
            )
        })?;
        responses.push(resp);
    }

    if responses.len() != inputs.len() {
        return Err(format!(
            "JS bridge returned {} responses for {} inputs (exit={status:?}, stderr: {stderr_buf})",
            responses.len(),
            inputs.len()
        ));
    }

    Ok(responses)
}

/// Backwards-compatible streaming-style facade over `run_batch`. Tests
/// written before the buffering deadlock was diagnosed call
/// `JsBridge::spawn()` then `bridge.run(stage, css)` per fixture; that
/// streaming protocol deadlocks under both bun and node. This shim accumulates each
/// `run()` request, replays the whole batch on first `run()` if the
/// queue is non-empty, and returns the cached response. Behaves
/// identically to the old streaming API for the cases the tests
/// exercise (one stage per `JsBridge`, all calls in order, no
/// interleaving with mutated state on the JS side).
pub struct JsBridge {
    /// Pending requests, accumulated across `run()` calls before the
    /// first batch executes. After the first execution this stays
    /// drained — subsequent `run()` calls each spawn a fresh
    /// single-request batch (correct, just slow; tests with this
    /// pattern are uncommon).
    queued: Vec<(Stage, String)>,
    cached: std::collections::VecDeque<JsResponse>,
    last_stage: Option<Stage>,
}

impl JsBridge {
    /// Open a deferred batch. The actual subprocess does not spawn
    /// until the first `run()` call (or `prepare()` if the caller
    /// wants to preflight the bridge eagerly).
    pub fn spawn() -> Result<Self, String> {
        Ok(Self {
            queued: Vec::new(),
            cached: std::collections::VecDeque::new(),
            last_stage: None,
        })
    }

    /// Send one request, return the matching response. The first call
    /// flushes any previously-queued requests through a single
    /// subprocess invocation; later calls each spawn their own
    /// (cheap for one-off tests, the common case).
    pub fn run(&mut self, stage: Stage, css: &str) -> Result<JsResponse, String> {
        if let Some(prev) = self.last_stage {
            if prev != stage && !self.queued.is_empty() {
                // Stage switched mid-queue; flush what we have first.
                self.flush()?;
            }
        }
        self.last_stage = Some(stage);

        if let Some(resp) = self.cached.pop_front() {
            return Ok(resp);
        }

        // Nothing cached: run a fresh single-request batch.
        let inputs: Vec<&str> = std::iter::once(css).collect();
        let mut responses = run_batch(stage, &inputs)?;
        Ok(responses.remove(0))
    }

    /// Pre-queue a request without blocking. Use to build a batch the
    /// runner will execute on the next `flush()` or `run()`.
    pub fn enqueue(&mut self, stage: Stage, css: String) {
        self.last_stage = Some(stage);
        self.queued.push((stage, css));
    }

    /// Execute the queued batch and stash responses for subsequent
    /// `run()` calls.
    pub fn flush(&mut self) -> Result<(), String> {
        if self.queued.is_empty() {
            return Ok(());
        }
        let stage = self.queued[0].0;
        if !self.queued.iter().all(|(s, _)| *s == stage) {
            return Err("JsBridge: queued requests span multiple stages".to_string());
        }
        let inputs: Vec<&str> = self.queued.iter().map(|(_, c)| c.as_str()).collect();
        let responses = run_batch(stage, &inputs)?;
        self.cached.extend(responses);
        self.queued.clear();
        Ok(())
    }

    /// Tear down. Discards any unanswered queued/cached requests.
    pub fn shutdown(self) -> Result<(), String> {
        Ok(())
    }
}

fn script_path() -> PathBuf {
    // Crate sits at `<workspace>/crates/parity-runner/`. Walk to the
    // workspace root and into `packages/css/scripts/`.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent().unwrap()                  // crates/
        .parent().unwrap()                  // workspace root
        .join("packages")
        .join("css")
        .join("scripts")
        .join("parity-bridge.mjs")
}

fn loader_path() -> PathBuf {
    // Sibling of `parity-bridge.mjs` — the on-the-fly TS loader hook.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent().unwrap()
        .parent().unwrap()
        .join("packages")
        .join("css")
        .join("scripts")
        .join("parity-bridge-ts-loader.mjs")
}

fn cjs_hook_path() -> PathBuf {
    // Sibling — registers `.ts`/`.tsx` in `require.extensions` so
    // CJS-graph nested requires (transpiled .ts → require('./peer'))
    // resolve to the adjacent `.ts` file. Preloaded with `--require`.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent().unwrap()
        .parent().unwrap()
        .join("packages")
        .join("css")
        .join("scripts")
        .join("parity-bridge-cjs-hook.cjs")
}

fn which(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").output().is_ok()
}
