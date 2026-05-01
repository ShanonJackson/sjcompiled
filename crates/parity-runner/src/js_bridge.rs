//! Spawns `bun run packages/css/scripts/parity-bridge.mjs` as a
//! subprocess and streams `{stage, css}` requests over stdio. One spawn
//! covers the whole corpus to amortize the ~150ms Node/Bun startup cost.
//!
//! Wire format: NDJSON. Each line on stdin is a request, each line on
//! stdout is the matching response. Order is preserved (request N maps
//! to response N).

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

pub struct JsBridge {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    line_buf: String,
}

impl JsBridge {
    /// Spawn the JS pipeline subprocess. The script lives in
    /// `packages/css/scripts/parity-bridge.mjs` (sibling to the source it
    /// diffs against), so Node/Bun's module resolution naturally finds
    /// `postcss` and the local `src/plugins/*.ts` files.
    pub fn spawn() -> Result<Self, String> {
        let script = script_path();
        if !script.exists() {
            return Err(format!("JS pipeline script not found at {}", script.display()));
        }
        // bun handles `.ts` imports natively. On Windows the bare `bun`
        // alias is a POSIX shell wrapper (visible to `which` from bash
        // but not to `Command::new` which goes through CreateProcess);
        // the real entry there is `bun.cmd`. Try both.
        let candidates: &[&str] = if cfg!(windows) {
            &["bun.cmd", "bun.exe", "bun"]
        } else {
            &["bun"]
        };
        let runtime = candidates.iter().copied().find(|c| which(c)).ok_or(
            "bun is required for the parity harness — install via https://bun.sh"
        )?;
        let mut child = Command::new(runtime)
            .arg("run")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn {runtime}: {e}"))?;
        let stdin = child.stdin.take().ok_or("missing stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("missing stdout")?);
        Ok(JsBridge { child, stdin, stdout, line_buf: String::new() })
    }

    /// Send one request, await one response. Use after `spawn()` for the
    /// whole corpus.
    pub fn run(&mut self, stage: Stage, css: &str) -> Result<JsResponse, String> {
        let req = JsRequest { stage: stage.name(), css };
        let payload = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{payload}").map_err(|e| format!("stdin write: {e}"))?;
        self.stdin.flush().map_err(|e| format!("stdin flush: {e}"))?;

        self.line_buf.clear();
        self.stdout.read_line(&mut self.line_buf).map_err(|e| format!("stdout read: {e}"))?;
        if self.line_buf.is_empty() {
            return Err("JS bridge closed unexpectedly".to_string());
        }
        serde_json::from_str(self.line_buf.trim_end())
            .map_err(|e| format!("bad JSON from JS bridge: {e} — line was: {:?}", self.line_buf))
    }

    /// Tear down. Send EOF on stdin so the Node script can exit cleanly.
    pub fn shutdown(mut self) -> Result<(), String> {
        drop(self.stdin);
        let _ = self.child.wait();
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
