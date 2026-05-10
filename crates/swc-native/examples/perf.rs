//! `cargo run --release -p swc-native --example perf -- <fixture> [iters]`
//!
//! Pure-Rust throughput benchmark. Loops one fixture through
//! `swc_native::transform` N times and reports
//! transforms-per-second + per-call latency. Compare against the
//! same fixture run through `bun parity-harness/babel-plugin/...`
//! to A/B native vs WASI.
//!
//! Defaults:
//!   * fixture = `fixtures/css-prop-basic/input.js`
//!   * iters   = 1000
//!
//! NOTE: build with `RUSTFLAGS="" cargo run --release` — the user's
//! global rustflags include `-C lto=thin` which breaks proc-macro
//! deps in the swc_core graph (same caveat as `compiled-css-napi`).

use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;

/// Match the WASI build's 8 MiB stack pin — Windows's default 1 MiB
/// blows up on babel-plugin's recursion depth on real fixtures.
/// One thread for the whole bench → spawn cost amortised away.
const WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(real_main)?
        .join()
        .expect("worker thread panicked")
}

fn real_main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let fixture = args
        .next()
        .unwrap_or_else(|| "fixtures/css-prop-basic/input.js".to_string());
    let iters: usize = args
        .next()
        .map(|s| s.parse().expect("iters must be int"))
        .unwrap_or(1000);

    // Find the repo root by walking up from CARGO_MANIFEST_DIR until
    // we hit the workspace toml (`/crates/Cargo.toml`).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // sjcompiled/
        .expect("repo root")
        .to_path_buf();
    let fixture_path = repo_root.join(&fixture);
    let source = std::fs::read_to_string(&fixture_path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", fixture_path.display(), e))?;

    // Same precompute snapshots the parity harness threads into the
    // WASI plugin via `@compiled/css-native::precomputeBrowserslistDefault`
    // / `precomputePrefixesDefault`. Skipping these costs ~6.6 ms per
    // call rebuilding autoprefixer prefix tables (and similarly for
    // browserslist) — making them the dominant cost on small fixtures.
    // Both benches must use identical snapshots for the A/B to mean
    // anything; we write them to `.parity-harness-cache/` (the same
    // location `engines.ts` uses) so the WASI bench's mjs side picks
    // them up too if it's run after this one.
    let cache_dir = repo_root.join(".parity-harness-cache");
    std::fs::create_dir_all(&cache_dir)?;
    let bs_path = cache_dir.join("browserslist-snapshot.bin");
    let pf_path = cache_dir.join("prefixes-snapshot.bin");
    let bs_snapshot = cssnano_browserslist_snapshot::precompute_browserslist_default();
    std::fs::write(&bs_path, cssnano_browserslist_snapshot::encode_precomputed(&bs_snapshot))?;
    let pf_snapshot = autoprefixer::precomputed::precompute_prefixes_default();
    std::fs::write(&pf_path, autoprefixer::precomputed::encode_precomputed(&pf_snapshot))?;

    // Mirror the harness's `swcEngine` options shape so the timing is
    // comparable with the WASI run. The plugin entry is named
    // `babel_plugin.wasm` so `native_plugins::build` dispatches to
    // `BabelPluginPass`.
    let opts = json!({
        "filename": fixture_path.to_string_lossy(),
        "jsc": {
            "target": "es2022",
            "parser": { "syntax": "typescript", "tsx": true },
            "transform": {
                "verbatimModuleSyntax": true,
                "react": { "runtime": "classic" }
            },
            "preserveAllComments": false,
            "experimental": {
                "runPluginFirst": true,
                "plugins": [["babel_plugin.wasm", {
                    "precomputedBrowserslistPath": bs_path.to_string_lossy(),
                    "precomputedPrefixesPath": pf_path.to_string_lossy(),
                }]]
            }
        }
    });
    let opts_bytes = serde_json::to_vec(&opts)?;

    // Warmup — first call dominates with one-time JIT/codegen setup.
    for _ in 0..10 {
        let _ = swc_native::transform(source.clone(), &opts_bytes)?;
    }

    let t0 = Instant::now();
    let mut last_len = 0usize;
    for _ in 0..iters {
        let out = swc_native::transform(source.clone(), &opts_bytes)?;
        last_len = out.code.len();
    }
    let elapsed = t0.elapsed();

    let per_call_us = elapsed.as_secs_f64() * 1e6 / iters as f64;
    let throughput = iters as f64 / elapsed.as_secs_f64();

    println!("fixture       : {}", fixture);
    println!("iterations    : {}", iters);
    println!("output bytes  : {}", last_len);
    println!("total elapsed : {:.3} s", elapsed.as_secs_f64());
    println!("per-call      : {:.1} µs", per_call_us);
    println!("throughput    : {:.1} transforms/s", throughput);
    Ok(())
}
