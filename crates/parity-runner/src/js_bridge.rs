//! Spawns `bun run packages/css/scripts/parity-bridge.mjs` as a
//! subprocess and runs a batch of `{stage, css}` requests over stdio.
//! One spawn covers the whole corpus to amortize the ~300ms bun
//! startup cost.
//!
//! Wire format: NDJSON. Caller writes all requests, closes stdin
//! (EOF), then drains all responses. Order is preserved (request N
//! maps to response N).
//!
//! ## Why batch and not streaming?
//!
//! Bun block-buffers BOTH `process.stdin` (when input is a pipe to a
//! non-TTY parent) AND `process.stdout` (when output is a pipe). The
//! buffer flushes only at ~64KB or on EOF. A streaming
//! request-per-line protocol deadlocks: the runner writes one
//! request, blocks on `read_line` for the response, but bun never
//! sees the request because its stdin buffer hasn't filled and never
//! emits the response because its stdout buffer hasn't filled
//! either.
//!
//! Closing stdin after sending all requests forces bun to drain its
//! stdin buffer (EOF triggers a flush), the bridge processes
//! everything, then exit-time stdout flush delivers all responses
//! to us at once. The whole corpus runs in a single spawn — same
//! startup cost as the original streaming design, no buffering
//! deadlock.

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

/// Run a batch of requests through one JS-bridge subprocess. Returns
/// one response per input, in input order. Returns `Err` if the
/// bridge fails to spawn or returns malformed output.
pub fn run_batch(stage: Stage, inputs: &[&str]) -> Result<Vec<JsResponse>, String> {
    let script = script_path();
    if !script.exists() {
        return Err(format!("JS pipeline script not found at {}", script.display()));
    }
    let candidates: &[&str] = if cfg!(windows) {
        &["bun.cmd", "bun.exe", "bun"]
    } else {
        &["bun"]
    };
    let runtime = candidates
        .iter()
        .copied()
        .find(|c| which(c))
        .ok_or("bun is required for the parity harness — install via https://bun.sh")?;

    let mut child = Command::new(runtime)
        .arg("run")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn {runtime}: {e}"))?;

    // Write every request, then drop stdin (EOF) to force bun to
    // drain its stdin buffer and process the batch.
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
/// written before the bun-buffering deadlock was diagnosed call
/// `JsBridge::spawn()` then `bridge.run(stage, css)` per fixture; that
/// streaming protocol deadlocks under bun. This shim accumulates each
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

fn which(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").output().is_ok()
}
