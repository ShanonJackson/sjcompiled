//! Integration test: `discard-empty-rules` parity, JS vs Rust, byte-clean.
//!
//! Runs at `cargo test -p parity-runner` time. Spawns the JS bridge once,
//! streams every entry under `corpus/discard-empty-rules/` through both
//! pipelines, fails on the first divergence with the smallest divergent
//! byte range printed.
//!
//! When the Rust plugin body is `unimplemented!()` (the scaffold state),
//! the harness reports each panic as an "RS error" and the test fails
//! — that's the point: the test goes green only when the plugin lands.
//!
//! Skips itself if `node`/`bun` aren't on PATH (CI environments without
//! a JS runtime get a noop pass).

use std::path::PathBuf;
use std::process::Command;

use parity_runner::{diff_summary, rust_run_stage, JsBridge, Stage};

#[test]
fn discard_empty_rules_parity() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }

    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join("discard-empty-rules");
    let entries = collect_entries(&corpus);
    assert!(!entries.is_empty(), "corpus is empty: {}", corpus.display());

    let mut js = JsBridge::spawn().expect("spawn JS bridge");
    let mut failures: Vec<String> = Vec::new();
    let mut rust_unimplemented = 0usize;

    for (label, css) in &entries {
        let js_resp = js.run(Stage::DiscardEmptyRules, css).expect("JS bridge call");
        if !js_resp.ok {
            failures.push(format!("[{label}] JS error: {}", js_resp.error));
            continue;
        }
        let rs_out = match rust_run_stage(Stage::DiscardEmptyRules, css) {
            Ok(s) => s,
            Err(e) => {
                if e.contains("unimplemented") || e.contains("panicked") {
                    rust_unimplemented += 1;
                    continue;
                }
                failures.push(format!("[{label}] RUST error: {e}"));
                continue;
            }
        };
        let d = diff_summary(label, &js_resp.css, &rs_out);
        if !d.equal {
            failures.push(d.summary);
        }
    }
    let _ = js.shutdown();

    if rust_unimplemented == entries.len() {
        // The plugin hasn't been implemented yet. Print a clear message
        // so the plugin author knows the harness reached them but there's
        // nothing to diff against. Still fail the test so CI catches the
        // pending work.
        panic!(
            "discard_empty_rules is unimplemented — fill in \
             crates/compiled-css/src/plugins/discard_empty_rules.rs \
             then re-run `cargo test -p parity-runner`. \
             ({} corpus entries waiting)",
            entries.len()
        );
    }

    if !failures.is_empty() {
        let body = failures.join("\n\n----\n\n");
        panic!(
            "{} of {} corpus entries diverged ({} pending plugin impl):\n\n{}",
            failures.len(), entries.len(), rust_unimplemented, body
        );
    }
}

fn js_runtime_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
        || Command::new("bun").arg("--version").output().is_ok()
}

fn collect_entries(dir: &PathBuf) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let read = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read corpus dir: {e}"));
    for entry in read {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("css") { continue; }
        let label = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let css = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        out.push((label, css));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
