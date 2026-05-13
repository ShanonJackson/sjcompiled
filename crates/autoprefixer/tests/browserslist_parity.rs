//! Byte-level parity gate for `crates/browserslist-shim`.
//!
//! ## Status (post-AFM fast-path landing)
//!
//! The previous canonical-queries omnibus gate (which was `#[ignore]`'d
//! because `oxc_browserslist`'s bundled caniuse-lite drifted ~2 chrome
//! releases ahead of our pin) is **closed for AFM's surface**. The
//! shim now has a hybrid AFM-fast-path / oxc-fallback architecture:
//! AFM's `.browserslistrc` atoms (`last N <browser> version[s]?`) and
//! the Firefox ESR rewrite (`firefox 115, firefox 128`) resolve via
//! `caniuse-db@1.0.30001766` directly, byte-correct. Anything else
//! (defaults, `> X%`, `<= 15`, `not all`) still routes through
//! `oxc_browserslist` — drift-tolerant, used by Phase 6 cssnano
//! consumers whose output reduces to a drift-stable boolean.
//!
//! See `crates/browserslist-shim/AFM_PORT_NOTES.md` and
//! `crates/autoprefixer/HANDOVER.md` §6 for the architecture rationale.
//!
//! This test now resolves bun's pinned `browserslist@4.24.2` against
//! the AFM `.browserslistrc` fixture (the SAME fixture the shim's own
//! integration test uses) and compares element-by-element against
//! `browserslist_shim::resolve_with("", { path: fixture_dir })`. Drift
//! between bun's JS oracle and our Rust shim on the AFM surface is a
//! hash-rotation event.
//!
//! ## Pre-conditions
//! - `bun` on PATH.
//! - `bun install` has populated `node_modules/.bun/browserslist@4.24.2+*`.
//!
//! Test lives here (not in `crates/browserslist-shim/tests/`) per
//! HANDOVER §8 "shared files" guidance — the autoprefixer agent owns
//! this file; modifying browserslist-shim's tests/ requires asking the
//! browserslist-shim agent first.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Pre-flight check: assert the workspace `require('browserslist')`
/// resolves to the pinned 4.24.2. Catches future regression of the
/// `package.json` devDependency entry (without which bun's isolated
/// layout floats `require('browserslist')` to 4.28.2).
#[test]
#[ignore = "requires bun on PATH + workspace node_modules; run with `cargo test -p autoprefixer -- --ignored`"]
fn workspace_browserslist_pin_is_424_2() {
    let workspace_root = workspace_root();
    let tmp_dir = workspace_root
        .join("crates")
        .join("target")
        .join("browserslist_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let script_path = tmp_dir.join("browserslist_version_probe.js");
    std::fs::write(
        &script_path,
        "process.stdout.write(require('browserslist/package.json').version);\n",
    )
    .expect("write probe script");
    let output = run_bun(&script_path, &workspace_root);
    assert!(
        output.status.success(),
        "bun version probe exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = String::from_utf8(output.stdout).unwrap().trim().to_string();
    assert_eq!(
        version, "4.24.2",
        "workspace `require('browserslist')` resolved to {version}, expected 4.24.2.\n\
         Root `package.json` MUST list `\"browserslist\": \"4.24.2\"` in BOTH \
         `overrides` AND `devDependencies` (mirrors the caniuse-lite pattern in \
         HANDOVER §2). Without the direct devDep, bun's isolated layout leaves no \
         top-level node_modules/browserslist symlink and resolution floats to \
         4.28.2 (a transitive of update-browserslist-db). Re-add the devDep and \
         run `bun install`."
    );
}

/// **Closure of the previously-OPEN browserslist-shim parity gate (AFM surface).**
///
/// Spawns bun against `browserslist@4.24.2` with the AFM `.browserslistrc`
/// fixture as the resolver's `path` opt, captures the JSON
/// `["chrome 144", ...]` array, and compares it element-by-element
/// against `browserslist_shim::resolve_with("", { path: fixture_dir })`.
///
/// Pre-conditions:
/// - The shim's `tests/fixtures/afm/.browserslistrc` SHA256 must match
///   AFM's pinned hash. The shim's `afm_parity.rs` integration test
///   asserts this independently — if THAT test fails, this test's
///   bun-side input has also drifted.
///
/// On failure, in priority order:
/// 1. Did `caniuse_db::CANIUSE_LITE_VERSION` change? Bun is reading the
///    workspace `node_modules/caniuse-lite`; the shim is reading
///    `crates/caniuse-db/data/features.snapshot.json`. They must
///    agree on the snapshot version.
/// 2. Did the AFM-fast-path resolver regress? Run
///    `cargo test -p browserslist-shim --test afm_parity` first.
#[test]
#[ignore = "requires bun on PATH + workspace node_modules; run with `cargo test -p autoprefixer -- --ignored`"]
fn browserslist_shim_matches_js_oracle_for_afm_browserslistrc() {
    let workspace_root = workspace_root();
    let fixture_dir = workspace_root
        .join("crates")
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm");
    assert!(
        fixture_dir.join(".browserslistrc").is_file(),
        "AFM fixture missing at {:?}",
        fixture_dir
    );

    let oracle = run_oracle_against_fixture(&workspace_root, &fixture_dir);

    let opts = browserslist_shim::index::ResolveOpts {
        path: Some(&fixture_dir),
        env: None,
        ignore_unknown_versions: true,
    };
    let rust = browserslist_shim::index::resolve_with("", &opts);

    assert_eq!(
        rust, oracle,
        "browserslist-shim diverges from JS oracle on AFM .browserslistrc.\n\
         JS  ({} entries): {:?}\n\
         RUST({} entries): {:?}\n\
         diff: {}\n\
         See AFM_PORT_NOTES.md for triage steps.",
        oracle.len(),
        truncate_list(&oracle, 16),
        rust.len(),
        truncate_list(&rust, 16),
        describe_diff(&oracle, &rust),
    );
}

/// Drift-monitor: a separate test that exercises just the `Firefox ESR`
/// path. Failing this without the omnibus failing would mean the
/// `rewrite_firefox_esr` shim path regressed independently of the rest
/// of the resolver.
#[test]
#[ignore = "requires bun on PATH + workspace node_modules; run with `cargo test -p autoprefixer -- --ignored`"]
fn browserslist_shim_firefox_esr_matches_js_oracle() {
    let workspace_root = workspace_root();
    let oracle = run_oracle_with_query(&workspace_root, "Firefox ESR");
    let rust = browserslist_shim::resolve("Firefox ESR", true);
    assert_eq!(
        rust, oracle,
        "Firefox ESR diverges:\n  JS:   {:?}\n  RUST: {:?}",
        oracle, rust
    );
}

/// Spawn bun against a one-line script that requires the workspace's
/// pinned `browserslist@4.24.2` and dumps the resolved query as JSON.
fn run_oracle_with_query(workspace_root: &Path, query: &str) -> Vec<String> {
    let escaped_query = json_escape_string(query);
    let script = format!(
        "const bl = require('browserslist');\n\
         process.stdout.write(JSON.stringify(bl({q})));\n",
        q = escaped_query,
    );

    let tmp_dir = workspace_root
        .join("crates")
        .join("target")
        .join("browserslist_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let script_path = tmp_dir.join(format!(
        "browserslist_oracle_{}.js",
        sanitize_for_filename(query)
    ));
    std::fs::write(&script_path, &script).expect("write oracle script");

    let output = run_bun(&script_path, workspace_root);
    if !output.status.success() {
        panic!(
            "bun oracle exited non-zero for query {:?} (code {:?}). stderr: {}",
            query,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8(output.stdout).expect("oracle stdout was not UTF-8");
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "oracle JSON parse failed for query {:?}: {e}\nraw stdout was: {}",
            query, stdout
        )
    })
}

/// Spawn bun against `browserslist(null, { path: <fixture_dir> })` —
/// mirrors autoprefixer's `browserslist(null, { path })` call exactly.
/// JS will walk up from `path` and discover the AFM `.browserslistrc`
/// at that location.
fn run_oracle_against_fixture(workspace_root: &Path, fixture_dir: &Path) -> Vec<String> {
    let escaped_path = json_escape_string(&fixture_dir.to_string_lossy());
    let script = format!(
        "const bl = require('browserslist');\n\
         process.stdout.write(JSON.stringify(bl(null, {{ path: {p} }})));\n",
        p = escaped_path,
    );

    let tmp_dir = workspace_root
        .join("crates")
        .join("target")
        .join("browserslist_parity_tmp");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp_dir");
    let script_path = tmp_dir.join("browserslist_oracle_afm_fixture.js");
    std::fs::write(&script_path, &script).expect("write oracle script");

    let output = run_bun(&script_path, workspace_root);
    if !output.status.success() {
        panic!(
            "bun oracle exited non-zero against AFM fixture (code {:?}). stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8(output.stdout).expect("oracle stdout was not UTF-8");
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "oracle JSON parse failed against AFM fixture: {e}\nraw stdout was: {}",
            stdout
        )
    })
}

/// Same Windows-shim-aware bun spawn as data_parity.rs / build.rs.
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
        "browserslist_parity: failed to spawn `bun <file>` (last attempt: {cand}): {err}\n\
         Pre-condition: `bun` must be on PATH and `bun install` must have populated \
         node_modules/.bun/browserslist@4.24.2+*."
    )
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has no grandparent")
        .to_path_buf()
}

/// Minimal JSON string escape — enough for the canonical queries
/// (which contain spaces, `>`, `=`, digits, ASCII letters). Avoids
/// pulling in `serde_json::to_string` for a 1-line embed.
fn json_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn truncate_list(v: &[String], max: usize) -> Vec<String> {
    if v.len() <= max {
        v.to_vec()
    } else {
        let mut out: Vec<String> = v.iter().take(max).cloned().collect();
        out.push(format!("...({} more)", v.len() - max));
        out
    }
}

/// Summarize what diverges between two browserslist outputs.
fn describe_diff(oracle: &[String], rust: &[String]) -> String {
    use std::collections::BTreeSet;
    let oset: BTreeSet<&String> = oracle.iter().collect();
    let rset: BTreeSet<&String> = rust.iter().collect();
    let only_in_js: Vec<&&String> = oset.difference(&rset).collect();
    let only_in_rust: Vec<&&String> = rset.difference(&oset).collect();

    let mut parts: Vec<String> = Vec::new();
    if !only_in_js.is_empty() {
        parts.push(format!(
            "missing from RUST ({}): {:?}",
            only_in_js.len(),
            only_in_js.iter().take(8).collect::<Vec<_>>()
        ));
    }
    if !only_in_rust.is_empty() {
        parts.push(format!(
            "extra in RUST ({}): {:?}",
            only_in_rust.len(),
            only_in_rust.iter().take(8).collect::<Vec<_>>()
        ));
    }
    if oracle.len() == rust.len() {
        for (i, (o, r)) in oracle.iter().zip(rust.iter()).enumerate() {
            if o != r {
                parts.push(format!("first index mismatch at [{i}]: js={o:?} rust={r:?}"));
                break;
            }
        }
    }
    if parts.is_empty() {
        "(unknown — sets equal but vectors differ)".into()
    } else {
        parts.join("; ")
    }
}
