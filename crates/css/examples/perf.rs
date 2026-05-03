//! Native ops/sec benchmark for `transform_css`.
//!
//! Run with:
//!   cargo run --release --example perf -p css
//!
//! Loads every fixture from `crates/parity-runner/corpus/transform-css/`,
//! runs each through `transform_css` in a tight loop, and reports per-
//! fixture and aggregate throughput. NAPI is bypassed entirely — this is
//! the cost of the Rust pipeline on its own.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use css::{transform_css, TransformOpts};

const WARMUP_ITERS: u32 = 50;
const MEASURE_MIN_ITERS: u32 = 200;
const MEASURE_MIN_DURATION: Duration = Duration::from_millis(500);

fn corpus_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/css.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("parity-runner")
        .join("corpus")
        .join("transform-css")
}

fn load_fixtures() -> Vec<(String, String)> {
    let dir = corpus_dir();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read corpus dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("css"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    entries
        .into_iter()
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let css = fs::read_to_string(e.path()).expect("read fixture");
            (name, css)
        })
        .collect()
}

fn bench_one(css: &str, opts: &TransformOpts) -> (u32, Duration) {
    // Warmup.
    for _ in 0..WARMUP_ITERS {
        let _ = transform_css(css, opts).expect("transform_css");
    }

    let start = Instant::now();
    let mut iters: u32 = 0;
    loop {
        let _ = transform_css(css, opts).expect("transform_css");
        iters += 1;
        if iters >= MEASURE_MIN_ITERS && start.elapsed() >= MEASURE_MIN_DURATION {
            break;
        }
    }
    (iters, start.elapsed())
}

fn fmt_ops(ops_per_sec: f64) -> String {
    if ops_per_sec >= 1_000_000.0 {
        format!("{:>9.2} Mops/s", ops_per_sec / 1_000_000.0)
    } else if ops_per_sec >= 1_000.0 {
        format!("{:>9.2} kops/s", ops_per_sec / 1_000.0)
    } else {
        format!("{:>9.2}  ops/s", ops_per_sec)
    }
}

fn main() {
    let fixtures = load_fixtures();
    let opts = TransformOpts::default();

    println!("transform_css native ops/sec");
    println!("============================");
    println!(
        "warmup={WARMUP_ITERS} iters; measure>={} iters and >={}ms\n",
        MEASURE_MIN_ITERS,
        MEASURE_MIN_DURATION.as_millis()
    );
    println!(
        "{:<42} {:>6}  {:>10}  {:>16}  {:>14}",
        "fixture", "bytes", "iters", "time", "ops/sec"
    );
    println!("{}", "-".repeat(96));

    let mut total_iters: u64 = 0;
    let mut total_time = Duration::ZERO;
    let mut total_bytes: u64 = 0;

    for (name, css) in &fixtures {
        let (iters, elapsed) = bench_one(css, &opts);
        let ops = iters as f64 / elapsed.as_secs_f64();
        let avg_us = elapsed.as_secs_f64() * 1e6 / iters as f64;
        println!(
            "{:<42} {:>6}  {:>10}  {:>10.3}ms ({:>5.1}µs)  {}",
            name,
            css.len(),
            iters,
            elapsed.as_secs_f64() * 1000.0,
            avg_us,
            fmt_ops(ops),
        );
        total_iters += iters as u64;
        total_time += elapsed;
        total_bytes += (css.len() as u64) * (iters as u64);
    }

    println!("{}", "-".repeat(96));
    let agg_ops = total_iters as f64 / total_time.as_secs_f64();
    let mb_per_sec = (total_bytes as f64) / total_time.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "TOTAL: {} fixtures, {} iters in {:.3}s — {} ({:.2} MiB/s of input)",
        fixtures.len(),
        total_iters,
        total_time.as_secs_f64(),
        fmt_ops(agg_ops),
        mb_per_sec,
    );
}
