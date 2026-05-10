//! `cargo run --release -p swc-native --example triage_dump -- [out-path]`
//!
//! Walk `/fixtures/*`, run each through the native pass, and dump
//! `{ "<fixture-name>": { ok: bool, code?: str, error?: str } }`
//! to `out-path` (default: `parity-harness/native-triage-dump.json`).
//!
//! Mirrors `parity-harness/fixtures-triage.mjs`'s entry-resolution
//! (`findEntry`) and `engines.ts::swcEngine`'s opts shape so the JS
//! comparison side can re-use the same `normalise()` + reconcilers
//! it applies to the WASI engine output.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

/// 64 MiB. The WASI build pins 8 MiB; Windows defaults to 1 MiB.
/// Some fixtures in the `ct-*` cluster (deeply-nested template
/// literals, conditional CSS expressions) push babel-plugin's
/// recursion past 16 MiB on native, so we go big — modern OSes
/// allocate stack pages on demand, the limit is just a max.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(real_main)?
        .join()
        .expect("worker thread panicked")
}

fn real_main() -> anyhow::Result<()> {
    let mut out_arg: Option<String> = None;
    let mut start_from: Option<String> = None;
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--start-from" => start_from = iter.next(),
            other if other.starts_with("--") => {
                anyhow::bail!("unknown flag {other}");
            }
            other => {
                if out_arg.is_some() {
                    anyhow::bail!("two positional args; expected one out path");
                }
                out_arg = Some(other.to_string());
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let fixtures_dir = repo_root.join("fixtures");
    let out_path = out_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("parity-harness/native-triage-dump.jsonl"));

    // ⚠️ DO NOT pass `precomputedBrowserslistPath` /
    // `precomputedPrefixesPath` here.
    //
    // The parity-harness `swcEngine` (in `engines.ts`) tries to write
    // these snapshots via `@compiled/css-native::precomputeBrowserslistDefault`,
    // but that package isn't on the parity-harness resolution path
    // in this repo, so the require silently fails and the snapshot
    // PATHS are never threaded into the WASI plugin. The WASI plugin
    // then falls back to its in-process resolution (which finds no
    // `.browserslistrc` reachable from `process.cwd()` and lands on
    // `browserslist@4.24.2` wide defaults — IE 11 included), and
    // Babel's cssnano in the same shell does the same. They match
    // because both sides hit the same fallback.
    //
    // If we anchor our native dumper on AFM here, our cssnano
    // normalises `white → #fff` while Babel's cssnano (still on
    // wide defaults) leaves it as `white`. Every CSS-value-hash
    // class name then shifts → ~150 fixture divergences. So we
    // mirror the in-environment `swcEngine` behaviour: no
    // precompute paths, in-process resolution falls back to the
    // same defaults Babel sees.
    //
    // The perf bench (`examples/perf.rs`) is a different story —
    // there we want the snapshot path so the per-call autoprefixer
    // setup cost doesn't dominate the timing. Triage prioritises
    // parity over throughput.

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&fixtures_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();

    let total = entries.len();
    let t0 = Instant::now();

    // JSONL output: one `{"name": ..., "ok": ..., ...}` line per
    // fixture, flushed after each. If a fixture overflows the host
    // thread (Windows SO terminates the process — `catch_unwind`
    // doesn't catch it), the launcher script restarts us with
    // `--start-from <next>` and the dump file already has every
    // prior fixture's result. The JS triage assembles all the
    // partial-run JSONL files into a single map.
    let mut out_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)?;
    let mut start_idx = 0;
    if let Some(target) = &start_from {
        for (i, p) in entries.iter().enumerate() {
            if p.file_name().and_then(|s| s.to_str()) == Some(target.as_str()) {
                start_idx = i;
                break;
            }
        }
    }
    eprintln!("starting at index {start_idx} of {total}");

    // Helper to flush a single fixture result as one JSON line.
    let mut emit = |name: &str, value: Value| -> anyhow::Result<()> {
        let line = json!({ "name": name, "value": value });
        writeln!(out_file, "{line}")?;
        out_file.flush()?;
        Ok(())
    };

    for (i, dir) in entries.iter().enumerate().skip(start_idx) {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let entry = match find_entry(dir) {
            Some(p) => p,
            None => {
                emit(&name, json!({ "ok": false, "error": "no input file" }))?;
                continue;
            }
        };
        let source = match std::fs::read_to_string(&entry) {
            Ok(s) => s,
            Err(e) => {
                emit(&name, json!({ "ok": false, "error": format!("read: {e}") }))?;
                continue;
            }
        };

        // Mirror swcEngine's option shape from
        // parity-harness/babel-plugin/engines.ts. /fixtures has no
        // per-fixture opts so importReact / optimizeCss stay default.
        let has_jsx_pragma = source.contains("@jsxImportSource");
        let react_runtime = if has_jsx_pragma { "automatic" } else { "classic" };

        let opts = json!({
            "filename": entry.to_string_lossy(),
            "jsc": {
                "target": "es2022",
                "parser": { "syntax": "typescript", "tsx": true },
                "transform": {
                    "verbatimModuleSyntax": true,
                    "react": { "runtime": react_runtime }
                },
                "preserveAllComments": false,
                "experimental": {
                    "runPluginFirst": true,
                    "plugins": [["babel_plugin.wasm", {
                        // No `root` here: native callers leave
                        // `opts.root = None` so the WASI path
                        // translation is a no-op (see
                        // `babel_plugin::lib::apply_native`).
                        // No `precomputed*Path` either — see the
                        // long comment above the snapshot block.
                        //
                        // **Mirrors `engines.ts::swcEngine`'s harness
                        // convention:** `optimizeCss` defaults to
                        // `false` in `packages/babel-plugin/src/test-utils.ts:23`
                        // and the parity harness destructures the same
                        // default. The WASI plugin sees `false` and runs
                        // cssnano in conservative mode (no PROD plugins
                        // — `white` stays as `white`, `0.8` stays as
                        // `0.8`). Without this, native's `apply_native`
                        // gets `optimize_css = None` → `.unwrap_or(true)`
                        // in `crates/css/src/transform.rs:421` → all PROD
                        // cssnano plugins fire → ~150 fixtures' class
                        // hashes diverge from Babel/WASI.
                        "optimizeCss": false,
                    }]]
                }
            }
        });
        let opts_bytes = serde_json::to_vec(&opts)?;

        // Print every fixture before transforming. The launcher
        // script reads the LAST `[N/M] name` line from stderr to
        // know which fixture was active when the process exited;
        // if stderr's last fixture is NOT in the JSONL output, that
        // fixture aborted the process (Windows stack overflow) and
        // gets restarted with `--start-from <next>`.
        eprintln!("[{}/{}] {}", i + 1, total, name);

        match swc_native::transform(source, &opts_bytes) {
            Ok(out) => emit(&name, json!({ "ok": true, "code": out.code }))?,
            Err(e) => emit(&name, json!({ "ok": false, "error": format!("{e}") }))?,
        }

        if (i + 1) % 25 == 0 {
            eprintln!("  -- {:.1}s elapsed", t0.elapsed().as_secs_f64());
        }
    }

    eprintln!(
        "\nfinished {} fixtures in {:.1}s → {}",
        total - start_idx,
        t0.elapsed().as_secs_f64(),
        out_path.display()
    );
    Ok(())
}

/// Same precedence as `parity-harness/fixtures-triage.mjs::findEntry`.
fn find_entry(dir: &Path) -> Option<PathBuf> {
    for ext in &["tsx", "jsx", "js"] {
        let p = dir.join(format!("input-preprocessed.{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    for ext in &["tsx", "jsx", "js"] {
        let p = dir.join(format!("input.{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}
