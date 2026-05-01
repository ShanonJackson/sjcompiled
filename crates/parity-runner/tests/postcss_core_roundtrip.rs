//! Integration test: postcss-core parser+stringifier round-trip parity
//! against `postcss.parse(css).toString()`.
//!
//! Reuses the same corpus dir layout as plugin stages — drop new
//! adversarial inputs into `corpus/postcss-core-roundtrip/` to widen
//! coverage.

use std::path::PathBuf;
use std::process::Command;

use parity_runner::{diff_summary, rust_run_stage, JsBridge, Stage};

#[test]
fn postcss_core_roundtrip_matches_js() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join("postcss-core-roundtrip");
    if !corpus.exists() {
        // Corpus is optional for this stage — if no files have been
        // dropped in yet, skip rather than fail.
        return;
    }
    let entries = collect_entries(&corpus);
    if entries.is_empty() { return; }

    let mut js = JsBridge::spawn().expect("spawn JS bridge");
    let mut failures: Vec<String> = Vec::new();
    for (label, css) in &entries {
        let js_resp = js.run(Stage::PostcssCoreRoundtrip, css).expect("JS bridge");
        if !js_resp.ok {
            failures.push(format!("[{label}] JS error: {}", js_resp.error));
            continue;
        }
        let rs_out = rust_run_stage(Stage::PostcssCoreRoundtrip, css).expect("rust stage");
        let d = diff_summary(label, &js_resp.css, &rs_out);
        if !d.equal { failures.push(d.summary); }
    }
    let _ = js.shutdown();
    assert!(failures.is_empty(), "{} corpus entries diverged:\n\n{}", failures.len(), failures.join("\n\n"));
}

fn js_runtime_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
        || Command::new("bun").arg("--version").output().is_ok()
}

fn collect_entries(dir: &PathBuf) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let read = match std::fs::read_dir(dir) { Ok(r) => r, Err(_) => return out };
    for entry in read {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("css") { continue; }
        let label = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let css = match std::fs::read_to_string(&path) { Ok(s) => s, Err(_) => continue };
        out.push((label, css));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
