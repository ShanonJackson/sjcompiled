//! Phase 0 exit gate (item #1): the JS pipeline must be byte-deterministic
//! across two independent spawns on the same machine.
//!
//! Why: the parity contract for the entire Rust port is "Rust output ==
//! JS output, byte-for-byte." If the JS side itself is unstable (a hidden
//! cache, an env-var-dependent code path, a non-deterministic iteration
//! order), then any Rust port will fail to match the JS oracle randomly,
//! and we lose the ability to bisect divergences. Catch that here, BEFORE
//! it contaminates a plugin port's parity test.
//!
//! Skips itself if a JS runtime isn't on PATH so the workspace test runs
//! cleanly on machines without bun/node.

use std::path::PathBuf;
use std::process::Command;

use parity_runner::{diff_summary, JsBridge, Stage};

#[test]
fn postcss_core_roundtrip_oracle_is_deterministic() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }
    run_determinism(Stage::PostcssCoreRoundtrip, "postcss-core-roundtrip");
}

#[test]
fn discard_empty_rules_oracle_is_deterministic() {
    if !js_runtime_available() {
        eprintln!("skipping: node/bun not on PATH");
        return;
    }
    run_determinism(Stage::DiscardEmptyRules, "discard-empty-rules");
}

fn run_determinism(stage: Stage, corpus_dir: &str) {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(corpus_dir);
    if !corpus.exists() {
        eprintln!("corpus dir missing: {}", corpus.display());
        return;
    }
    let entries = collect_entries(&corpus);
    if entries.is_empty() {
        eprintln!("corpus dir empty: {}", corpus.display());
        return;
    }

    let mut js_a = JsBridge::spawn().expect("spawn JS bridge A");
    let mut js_b = JsBridge::spawn().expect("spawn JS bridge B");

    let mut failures: Vec<String> = Vec::new();
    for (label, css) in &entries {
        let a = js_a.run(stage, css).expect("bridge A call");
        let b = js_b.run(stage, css).expect("bridge B call");
        if !a.ok {
            failures.push(format!("[{label}] JS-A error: {}", a.error));
            continue;
        }
        if !b.ok {
            failures.push(format!("[{label}] JS-B error: {}", b.error));
            continue;
        }
        let d = diff_summary(label, &a.css, &b.css);
        if !d.equal {
            failures.push(format!("JS-vs-JS divergence: {}", d.summary));
        }
    }
    let _ = js_a.shutdown();
    let _ = js_b.shutdown();

    assert!(
        failures.is_empty(),
        "JS oracle is non-deterministic for stage {:?} ({} of {} corpus entries):\n\n{}",
        stage,
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
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
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
