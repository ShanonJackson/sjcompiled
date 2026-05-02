//! Byte-level parity gate for `crates/autoprefixer/src/data/prefixes.rs`.
//!
//! Spawns `bun` to dump the upstream JS table as JSON, serializes the
//! Rust `PREFIXES` static to JSON, canonicalizes both via recursive
//! object-key sort, and asserts byte equality. Any divergence — extra
//! field, dropped browser version, transcribed wrong, etc. — fails this
//! gate.
//!
//! Pre-condition: `bun install` must have been run with the workspace
//! `caniuse-lite: 1.0.30001766` devDependency + override active. That
//! causes bun to symlink `node_modules/caniuse-lite` to the pinned copy
//! so resolution from the vendored `_vendor/...` path lands on the pin.
//!
//! See `crates/autoprefixer/HANDOVER.md` §2 and `crates/PARITY_VERSIONS.md`
//! Anomaly #3 for the rationale.

use std::path::{Path, PathBuf};
use std::process::Command;

use autoprefixer::data::prefixes::{PrefixEntry, PREFIXES};
use indexmap::IndexMap;
use serde_json::Value;

#[test]
fn data_table_matches_js_oracle() {
    // 1. Serialize the Rust PREFIXES table to JSON.
    let rust_table: IndexMap<&'static str, &PrefixEntry> =
        PREFIXES.iter().map(|(k, v)| (*k, v)).collect();
    let rust_json = serde_json::to_string(&rust_table)
        .expect("failed to serialize Rust PREFIXES to JSON");
    let rust_value: Value = serde_json::from_str(&rust_json)
        .expect("failed to round-trip Rust JSON for canonicalization");

    // 2. Dump the JS oracle table to JSON via bun.
    let workspace_root = workspace_root();
    let vendored_js = vendored_prefixes_js();
    let js_path_str = vendored_js.to_string_lossy().replace('\\', "/");
    let dumper_js = format!(
        "process.stdout.write(JSON.stringify(require({:?})));\n",
        js_path_str,
    );

    // Must be inside the workspace, not std::env::temp_dir() — bun's
    // CommonJS resolver walks UP from the script file's directory looking
    // for node_modules, NOT from cwd. A tmpdir script would resolve to
    // some unrelated parent project's caniuse-lite. Anchor inside the
    // workspace's target dir to keep walk-up local.
    let tmp_dir = workspace_root.join("target").join("data_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let dumper_path = tmp_dir.join("autoprefixer_data_parity_dump.js");
    std::fs::write(&dumper_path, dumper_js)
        .expect("failed to write dump script to temp dir");

    let output = run_bun(&dumper_path, &workspace_root);
    assert!(
        output.status.success(),
        "bun dump exited non-zero ({:?}). stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle_json = String::from_utf8(output.stdout)
        .expect("bun stdout was not UTF-8");
    let oracle_value: Value = serde_json::from_str(&oracle_json)
        .expect("oracle JSON parse failed");

    // 3. Canonicalize both sides — recursive sorted-key tree walk —
    //    so iteration-order divergence on nested objects is not a
    //    false positive.
    let canonical_rust = canonicalize(&rust_value);
    let canonical_oracle = canonicalize(&oracle_value);

    assert_eq!(
        canonical_rust, canonical_oracle,
        "Rust PREFIXES table diverges from JS oracle. \
         Run `cargo build -p autoprefixer && cargo test -p autoprefixer data_table_matches_js_oracle` \
         and inspect the diff. The first ~200 chars of the divergence:\n\
         RUST: {}...\n\
         JS:   {}...",
        truncate(&canonical_rust, 200),
        truncate(&canonical_oracle, 200),
    );
}

#[test]
fn entry_count_matches_js_oracle() {
    let workspace_root = workspace_root();
    let vendored_js = vendored_prefixes_js();
    let js_path_str = vendored_js.to_string_lossy().replace('\\', "/");
    let dumper_js = format!(
        "process.stdout.write(String(Object.keys(require({:?})).length));\n",
        js_path_str,
    );

    // Must be inside the workspace, not std::env::temp_dir() — bun's
    // CommonJS resolver walks UP from the script file's directory looking
    // for node_modules, NOT from cwd. A tmpdir script would resolve to
    // some unrelated parent project's caniuse-lite. Anchor inside the
    // workspace's target dir to keep walk-up local.
    let tmp_dir = workspace_root.join("target").join("data_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let dumper_path = tmp_dir.join("autoprefixer_data_parity_count.js");
    std::fs::write(&dumper_path, dumper_js)
        .expect("failed to write count script to temp dir");

    let output = run_bun(&dumper_path, &workspace_root);
    assert!(output.status.success());
    let count: usize = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .expect("count was not a usize");
    assert_eq!(
        PREFIXES.len(),
        count,
        "PREFIXES.len() == {} but JS oracle has {count} entries",
        PREFIXES.len()
    );
}

#[test]
fn key_order_matches_js_oracle() {
    // PREFIXES is an IndexMap; insertion order equals JS Object.keys order.
    // Catches the regression hit during port: serde_json without
    // preserve_order alphabetized the codegen, drifting the table.
    let workspace_root = workspace_root();
    let vendored_js = vendored_prefixes_js();
    let js_path_str = vendored_js.to_string_lossy().replace('\\', "/");
    let dumper_js = format!(
        "process.stdout.write(JSON.stringify(Object.keys(require({:?}))));\n",
        js_path_str,
    );

    // Must be inside the workspace, not std::env::temp_dir() — bun's
    // CommonJS resolver walks UP from the script file's directory looking
    // for node_modules, NOT from cwd. A tmpdir script would resolve to
    // some unrelated parent project's caniuse-lite. Anchor inside the
    // workspace's target dir to keep walk-up local.
    let tmp_dir = workspace_root.join("target").join("data_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let dumper_path = tmp_dir.join("autoprefixer_data_parity_keys.js");
    std::fs::write(&dumper_path, dumper_js)
        .expect("failed to write keys script to temp dir");

    let output = run_bun(&dumper_path, &workspace_root);
    assert!(output.status.success());
    let oracle_keys: Vec<String> = serde_json::from_slice(&output.stdout)
        .expect("keys parse failed");
    let rust_keys_owned: Vec<String> = PREFIXES.keys().map(|s| (*s).to_string()).collect();

    assert_eq!(
        rust_keys_owned, oracle_keys,
        "PREFIXES key order diverges from JS Object.keys order"
    );
}

#[test]
fn caniuse_lite_pin_matches_parity_versions() {
    // Belt-and-braces: assert the workspace caniuse-lite resolves to the
    // exact version pinned in REFERENCE_LOCK_FILE/yarn.lock + Anomaly #3.
    // Catches any future bun.lock drift even if data_table_matches_js_oracle
    // happens to still be byte-clean (would require the next caniuse-lite
    // release to add no autoprefixer-relevant data, which is rare but
    // possible).
    let workspace_root = workspace_root();
    let dumper_js =
        "process.stdout.write(require('caniuse-lite/package.json').version);\n";
    // Must be inside the workspace, not std::env::temp_dir() — bun's
    // CommonJS resolver walks UP from the script file's directory looking
    // for node_modules, NOT from cwd. A tmpdir script would resolve to
    // some unrelated parent project's caniuse-lite. Anchor inside the
    // workspace's target dir to keep walk-up local.
    let tmp_dir = workspace_root.join("target").join("data_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let dumper_path = tmp_dir.join("autoprefixer_caniuse_lite_version.js");
    std::fs::write(&dumper_path, dumper_js)
        .expect("failed to write version-probe script to temp dir");

    let output = run_bun(&dumper_path, &workspace_root);
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap().trim().to_string();
    assert_eq!(
        version, "1.0.30001766",
        "workspace caniuse-lite resolved to {version}, expected 1.0.30001766 \
         (PARITY_VERSIONS.md Anomaly #3, AFM_MONOREPO_DEPENDENCIES_MORE.md). \
         Update root package.json devDependencies + overrides and re-run \
         `bun install`."
    );
}

/// Canonicalize a serde_json `Value` to a stable string form: recursively
/// sort all object keys alphabetically. Arrays preserve order (JS arrays
/// are ordered).
fn canonicalize(v: &Value) -> String {
    let canon = canonical_value(v);
    serde_json::to_string(&canon).expect("canonical re-serialize failed")
}

fn canonical_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonical_value(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonical_value).collect()),
        other => other.clone(),
    }
}

fn truncate(s: &str, n: usize) -> &str {
    if s.len() <= n {
        s
    } else {
        let mut idx = n;
        while !s.is_char_boundary(idx) {
            idx -= 1;
        }
        &s[..idx]
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has no grandparent")
        .to_path_buf()
}

fn vendored_prefixes_js() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("_vendor")
        .join("autoprefixer-10.4.14")
        .join("package")
        .join("data")
        .join("prefixes.js")
}

/// Same Windows-shim-aware bun spawn as build.rs.
fn run_bun(script_path: &Path, cwd: &Path) -> std::process::Output {
    let candidates: &[&str] = if cfg!(windows) {
        &["bun", "bun.cmd", "bun.exe"]
    } else {
        &["bun"]
    };
    let mut last_err = None;
    for cand in candidates {
        match Command::new(cand)
            .arg(script_path)
            .current_dir(cwd)
            .output()
        {
            Ok(o) => return o,
            Err(e) => last_err = Some((cand.to_string(), e)),
        }
    }
    let (cand, err) = last_err.unwrap();
    panic!(
        "data_parity test: failed to spawn `bun <file>` (last attempt: {cand}): {err}\n\
         Pre-condition: `bun` must be on PATH and `bun install` must have populated \
         node_modules/caniuse-lite. See bun.lock + STATUS.md."
    )
}
