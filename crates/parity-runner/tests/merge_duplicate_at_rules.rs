//! Integration test: `merge-duplicate-at-rules` parity, JS vs Rust.
//!
//! Top-level merge cases only; nested-at-rule edge case is documented
//! out-of-scope (matches upstream's "Currently does not handle nested
//! at-rules" caveat).

use std::path::PathBuf;
use std::process::Command;

use parity_runner::{diff_summary, rust_run_stage, JsBridge, Stage};

#[test]
fn merge_duplicate_at_rules_parity() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }

    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join("merge-duplicate-at-rules");
    let entries = collect_entries(&corpus);
    assert!(!entries.is_empty(), "corpus is empty: {}", corpus.display());

    let mut js = JsBridge::spawn().expect("spawn JS bridge");
    let mut failures: Vec<String> = Vec::new();

    for (label, css) in &entries {
        let js_resp = js.run(Stage::MergeDuplicateAtRules, css).expect("JS bridge call");
        if !js_resp.ok {
            failures.push(format!("[{label}] JS error: {}", js_resp.error));
            continue;
        }
        let rs_out = match rust_run_stage(Stage::MergeDuplicateAtRules, css) {
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
