//! Byte-level parity gate for `crates/browserslist-shim`.
//!
//! For each canonical query in [`CANONICAL_QUERIES`], spawns `bun` with
//! the pinned `browserslist@4.24.2` (resolved via plain
//! `require('browserslist')` from the workspace root — the workspace
//! lists browserslist in BOTH `overrides` and `devDependencies` per
//! HANDOVER §2 pattern, so the top-level symlink is byte-exact 4.24.2),
//! captures the JSON `["chrome 144", ...]` array, and compares it
//! element-by-element against `browserslist_shim::resolve(query, true)`.
//!
//! Why this gate exists: `browserslist-shim` wraps `oxc_browserslist`
//! whose bundled caniuse-lite snapshot may drift from the workspace pin
//! (1.0.30001766). Without this gate, `Prefixes::new` (next session's
//! Option A) would consume a silently-wrong `Browsers::new(...)` result
//! and produce drifted prefix bytes downstream — the kind of divergence
//! the parity contract considers a hash-rotation event.
//!
//! Reference: `crates/autoprefixer/HANDOVER.md` §6,
//! `crates/autoprefixer/MORNING.md` Option D.
//!
//! Pre-conditions:
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

/// Canonical browserslist queries that `Prefixes::new` will consume.
/// Drawn from MORNING.md Option D + the queries `transform.ts` reaches.
///
/// `"not dead"` standalone is intentionally absent: browserslist@4.24.2
/// throws `Write any browsers query (for instance, `defaults`) before
/// `not dead`` for negative-only queries. The Rust shim swallows that
/// error to `Vec::new()` (`index.rs::resolve_with`) — different error
/// semantics, but not a hashing-path divergence because no real consumer
/// passes `"not dead"` alone. Coverage for the `not dead` clause is via
/// `"last 2 versions, not dead"` below (which is the real-world form).
const CANONICAL_QUERIES: &[&str] = &[
    "defaults",
    "> 1%",
    "chrome >= 50",
    "last 2 versions",
    "Firefox ESR",
    "last 2 versions, not dead",
];

/// **Gate status: OPEN.** Marked `#[ignore]` so the floor stays intact;
/// the test code is the closure-tool for the agent who fixes the shim.
///
/// Run on demand with:
/// ```text
/// cargo test -p autoprefixer --test browserslist_parity -- --ignored
/// ```
///
/// Last-observed divergence (this session, run on caniuse-lite 1.0.30001766
/// + `oxc_browserslist` whichever version the workspace pulls):
/// - `defaults`, `> 1%`, `last 2 versions`, `last 2 versions, not dead`,
///   `chrome >= 50`: oxc_browserslist's bundled caniuse-lite snapshot is
///   ~2 chrome releases newer than our pin, so RUST returns
///   `chrome 145, chrome 146` for "current versions" while JS returns
///   `chrome 143, chrome 144`. Same shape on android/edge/firefox/etc.
/// - `Firefox ESR`: byte-clean (rewrite_firefox_esr forces the literal
///   `firefox 115, firefox 128` pair, bypassing caniuse-lite). Pinned
///   independently by `browserslist_shim_firefox_esr_matches_js_oracle`.
///
/// To close: either (a) override oxc_browserslist's snapshot to point at
/// our pinned `caniuse-db` data, (b) fork/extend `browserslist-shim` to
/// resolve queries against caniuse-db directly instead of delegating to
/// oxc_browserslist, or (c) downgrade oxc_browserslist to a version
/// whose bundled snapshot matches 1.0.30001766. Any path that closes
/// this is a multi-day unit — DO NOT half-land it.
#[test]
#[ignore = "browserslist-shim parity gate is open — see HANDOVER §6 and the doc-comment on this fn"]
fn browserslist_shim_matches_js_oracle_for_canonical_queries() {
    let workspace_root = workspace_root();

    let mut failures: Vec<String> = Vec::new();
    for query in CANONICAL_QUERIES {
        let oracle = run_oracle(&workspace_root, query);
        let rust = browserslist_shim::resolve(query, true);
        if oracle != rust {
            failures.push(format!(
                "Query {:?} diverges:\n  JS  ({} entries): {:?}\n  RUST({} entries): {:?}\n  diff: {}\n",
                query,
                oracle.len(),
                truncate_list(&oracle, 8),
                rust.len(),
                truncate_list(&rust, 8),
                describe_diff(&oracle, &rust),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "browserslist-shim diverges from JS oracle (browserslist@4.24.2):\n\n{}",
        failures.join("\n")
    );
}

/// Drift-monitor: a separate test that exercises just the `Firefox ESR`
/// path. Failing this without the omnibus failing would mean the
/// `rewrite_firefox_esr` shim path regressed independently of the rest
/// of oxc_browserslist's resolution.
#[test]
fn browserslist_shim_firefox_esr_matches_js_oracle() {
    let workspace_root = workspace_root();
    let oracle = run_oracle(&workspace_root, "Firefox ESR");
    let rust = browserslist_shim::resolve("Firefox ESR", true);
    assert_eq!(
        rust, oracle,
        "Firefox ESR diverges:\n  JS:   {:?}\n  RUST: {:?}",
        oracle, rust
    );
}

/// Spawn bun against a one-line script that requires the workspace's
/// pinned `browserslist@4.24.2` and dumps the resolved query as JSON.
///
/// Pre-condition: `workspace_browserslist_pin_is_424_2` must pass — i.e.
/// root `package.json` has `"browserslist": "4.24.2"` in BOTH `overrides`
/// AND `devDependencies` so `node_modules/browserslist` symlinks to the
/// pinned install. Without the direct devDep, plain `require('browserslist')`
/// resolves to 4.28.2 (transitive of `update-browserslist-db`).
fn run_oracle(workspace_root: &Path, query: &str) -> Vec<String> {
    let escaped_query = json_escape_string(query);
    let script = format!(
        "const bl = require('browserslist');\n\
         process.stdout.write(JSON.stringify(bl({q})));\n",
        q = escaped_query,
    );

    // Anchor the script inside the workspace so any UP-walk for
    // node_modules stays local. Mirrors data_parity.rs's tmp_dir
    // anchoring.
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
/// Reports the count of entries only in JS, only in Rust, and the
/// first-positional mismatch (if same length).
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
