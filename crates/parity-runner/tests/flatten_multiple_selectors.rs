//! Integration test: `flatten-multiple-selectors` parity, JS vs Rust.
//!
//! Skips itself if no JS runtime is on PATH.

use std::path::PathBuf;
use std::process::Command;

use parity_runner::{diff_summary, rust_run_stage, JsBridge, Stage};

#[test]
fn flatten_multiple_selectors_parity() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }

    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join("flatten-multiple-selectors");
    let entries = collect_entries(&corpus);
    assert!(!entries.is_empty(), "corpus is empty: {}", corpus.display());

    let mut js = JsBridge::spawn().expect("spawn JS bridge");
    let mut failures: Vec<String> = Vec::new();

    for (label, css) in &entries {
        let js_resp = js.run(Stage::FlattenMultipleSelectors, css).expect("JS bridge call");
        if !js_resp.ok {
            failures.push(format!("[{label}] JS error: {}", js_resp.error));
            continue;
        }
        let rs_out = match rust_run_stage(Stage::FlattenMultipleSelectors, css) {
            Ok(s) => s,
            Err(e) => {
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

    assert!(
        failures.is_empty(),
        "{} of {} corpus entries diverged:\n\n{}",
        failures.len(),
        entries.len(),
        failures.join("\n\n----\n\n")
    );
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
