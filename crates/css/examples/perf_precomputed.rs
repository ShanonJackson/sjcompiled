//! A/B benchmark for `transform_css`: slow path vs precomputed-prefix
//! path. Mirrors the workload in `scripts/perf-test.ts` so the numbers
//! are comparable.
//!
//! Run with:
//!   cargo run --profile bench-fast --example perf_precomputed -p css
//!
//! Reports both paths' ops/sec on the same SAMPLE_CSS the JS perf-test
//! uses. The precomputed snapshot is built once outside the timing
//! loop, matching how a WASI consumer would receive it via plugin
//! config.

use std::time::{Duration, Instant};

use autoprefixer::autoprefixer::AutoprefixerOptions;
use autoprefixer::precomputed::{
    encode_precomputed, encode_precomputed_v2, precompute_prefixes,
    precompute_prefixes_v2,
};
use css::{transform_css, TransformOpts};

const SAMPLE_CSS: &str = r#"
  display: flex;
  flex-direction: column;
  align-items: center;
  user-select: none;
  color: hotpink;
  background: linear-gradient(to right, red, blue);
  transition: transform 0.2s ease-in-out;

  &:hover {
    color: rebeccapurple;
    transform: scale(1.05);
  }

  &:focus-visible {
    outline: 2px solid currentColor;
  }

  @media (max-width: 600px) {
    flex-direction: row;
    padding: 8px;
  }

  > .child {
    margin-bottom: 1rem;

    &:last-child {
      margin-bottom: 0;
    }
  }
"#;

const WARMUP_ITERS: u32 = 50;
const MEASURE_DURATION: Duration = Duration::from_secs(3);

fn make_opts(precomputed: Option<Vec<u8>>) -> TransformOpts {
    TransformOpts {
        optimize_css: Some(false),
        sort_at_rules: Some(true),
        sort_shorthand: Some(true),
        increase_specificity: Some(false),
        precomputed_prefixes: precomputed,
        ..Default::default()
    }
}

fn make_opts_with_path(path: std::path::PathBuf) -> TransformOpts {
    TransformOpts {
        optimize_css: Some(false),
        sort_at_rules: Some(true),
        sort_shorthand: Some(true),
        increase_specificity: Some(false),
        precomputed_prefixes_path: Some(path),
        ..Default::default()
    }
}

fn bench(label: &str, opts: &TransformOpts) {
    for _ in 0..WARMUP_ITERS {
        let _ = transform_css(SAMPLE_CSS, opts).expect("transform_css");
    }

    let start = Instant::now();
    let mut iters: u64 = 0;
    while start.elapsed() < MEASURE_DURATION {
        let _ = transform_css(SAMPLE_CSS, opts).expect("transform_css");
        iters += 1;
    }
    let elapsed = start.elapsed();
    let ops = iters as f64 / elapsed.as_secs_f64();
    let avg_us = elapsed.as_secs_f64() * 1e6 / iters as f64;
    println!(
        "{:<24} {:>10.2} ops/s  ({} iters in {:.2}s, avg {:.1} µs/call)",
        label,
        ops,
        iters,
        elapsed.as_secs_f64(),
        avg_us,
    );
}

fn main() {
    println!("transform_css A/B: slow path vs precomputed prefixes");
    println!("====================================================");
    println!(
        "input: {} bytes ({} chars)\n",
        SAMPLE_CSS.len(),
        SAMPLE_CSS.chars().count()
    );

    // Resolve AFM's pinned .browserslistrc. Mirrors the path
    // `test_support::afm_fixture_dir()` uses; we can't `use` that since
    // it's `#[cfg(test)]`. The path lives under
    // `crates/browserslist-shim/tests/fixtures/afm/`.
    let afm_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm");
    let from = afm_dir.to_string_lossy().into_owned();

    println!("Building V1 precomputed snapshot (one-time, outside timing)...");
    let v1_build_start = Instant::now();
    let v1_snapshot = precompute_prefixes(AutoprefixerOptions {
        from: Some(from.clone()),
        ..Default::default()
    });
    let v1_bytes = encode_precomputed(&v1_snapshot);
    let v1_build = v1_build_start.elapsed();
    println!(
        "  V1 snapshot built in {:.2} ms, encoded to {} bytes",
        v1_build.as_secs_f64() * 1000.0,
        v1_bytes.len()
    );

    println!("Building V2 precomputed snapshot (one-time, outside timing)...");
    let v2_build_start = Instant::now();
    let v2_snapshot = precompute_prefixes_v2(AutoprefixerOptions {
        from: Some(from.clone()),
        ..Default::default()
    });
    let v2_bytes = encode_precomputed_v2(&v2_snapshot);
    let v2_build = v2_build_start.elapsed();
    println!(
        "  V2 snapshot built in {:.2} ms, encoded to {} bytes\n",
        v2_build.as_secs_f64() * 1000.0,
        v2_bytes.len()
    );

    // Write V2 bytes to a temp file for path-delivery bench.
    let v2_path = std::env::temp_dir().join("compiled-css-v2-snapshot.bin");
    std::fs::write(&v2_path, &v2_bytes).expect("write V2 snapshot to temp");
    println!("  V2 snapshot also written to {} for path-delivery bench\n", v2_path.display());

    // Make the slow path resolve against AFM's `.browserslistrc` too,
    // so all paths are doing identical pipeline work — the only
    // measured difference is the autoprefixer prefix construction.
    std::env::set_current_dir(&afm_dir).expect("set cwd to AFM fixture dir");

    let slow_opts = make_opts(None);
    let v1_opts = make_opts(Some(v1_bytes));
    let v2_opts = make_opts(Some(v2_bytes));
    let v2_path_opts = make_opts_with_path(v2_path.clone());

    bench("slow path (default)",      &slow_opts);
    bench("V1 (preprocess on load)",  &v1_opts);
    bench("V2 (no preprocess, inline)", &v2_opts);
    bench("V2 (no preprocess, path)",   &v2_path_opts);

    // Phase breakdown — answers "where does the autoprefixer cost go?"
    // by measuring just the autoprefixer construction cost in isolation.
    println!("\nAutoprefixer phase breakdown (per-call)");
    println!("-------------------------------------");
    {
        use autoprefixer::autoprefixer::build_prefixes_default;
        use autoprefixer::precomputed::{
            build_prefixes_from_precomputed, build_prefixes_from_snapshot_v2,
            decode_precomputed_v2,
        };

        let v1_snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions {
                from: Some(afm_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let v1_bytes_local = autoprefixer::precomputed::encode_precomputed(&v1_snap);

        let v2_snap = autoprefixer::precomputed::precompute_prefixes_v2(
            autoprefixer::autoprefixer::AutoprefixerOptions {
                from: Some(afm_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let v2_bytes_local = autoprefixer::precomputed::encode_precomputed_v2(&v2_snap);

        // Warmup
        for _ in 0..50 {
            let _ = build_prefixes_default(None).unwrap();
            let _ = build_prefixes_from_precomputed(&v1_bytes_local).unwrap();
            let decoded = decode_precomputed_v2(&v2_bytes_local).unwrap();
            let _ = build_prefixes_from_snapshot_v2(decoded);
        }

        let n = 200;
        let t = Instant::now();
        for _ in 0..n {
            let _ = build_prefixes_default(None).unwrap();
        }
        let slow_build = t.elapsed();

        let t = Instant::now();
        for _ in 0..n {
            let _ = build_prefixes_from_precomputed(&v1_bytes_local).unwrap();
        }
        let v1_build = t.elapsed();

        // V2 — split decode and reconstruct so we can attribute the cost.
        let t = Instant::now();
        for _ in 0..n {
            let _ = decode_precomputed_v2(&v2_bytes_local).unwrap();
        }
        let v2_decode = t.elapsed();

        let t = Instant::now();
        for _ in 0..n {
            let decoded = decode_precomputed_v2(&v2_bytes_local).unwrap();
            let _ = build_prefixes_from_snapshot_v2(decoded);
        }
        let v2_total = t.elapsed();

        println!(
            "  build_prefixes_default          {:>8.2} µs/call",
            slow_build.as_secs_f64() * 1e6 / n as f64
        );
        println!(
            "  V1 build_from_precomputed       {:>8.2} µs/call (decode + preprocess)",
            v1_build.as_secs_f64() * 1e6 / n as f64
        );
        println!(
            "  V2 decode only                  {:>8.2} µs/call",
            v2_decode.as_secs_f64() * 1e6 / n as f64
        );
        println!(
            "  V2 decode + reconstruct         {:>8.2} µs/call (no preprocess)",
            v2_total.as_secs_f64() * 1e6 / n as f64
        );
    }

    // Profile WHERE preprocess time goes — does regex compilation
    // dominate, or are non-regex code paths hot too?
    println!("\nPreprocess regex-counter probe");
    println!("------------------------------");
    {
        use autoprefixer::profile;
        let snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions {
                from: Some(afm_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let bytes = autoprefixer::precomputed::encode_precomputed(&snap);

        profile::reset_counters();
        let n = 100;
        let t = Instant::now();
        for _ in 0..n {
            let _ =
                autoprefixer::precomputed::build_prefixes_from_precomputed(&bytes).unwrap();
        }
        let elapsed = t.elapsed();
        let counters = profile::snapshot_counters();

        let avg_us = elapsed.as_secs_f64() * 1e6 / n as f64;
        let regex_ns_per_call = counters.regex_compile_ns as f64 / n as f64;
        let regex_us_per_call = regex_ns_per_call / 1000.0;
        let regex_count_per_call = counters.regex_compile_count as f64 / n as f64;

        println!(
            "  preprocess avg            : {:>8.2} µs/call",
            avg_us
        );
        println!(
            "  regex compiles            : {:>8.0} per call ({:.2} µs each)",
            regex_count_per_call,
            if regex_count_per_call > 0.0 {
                regex_us_per_call / regex_count_per_call
            } else {
                0.0
            }
        );
        println!(
            "  regex compile time        : {:>8.2} µs/call ({:.1}% of preprocess)",
            regex_us_per_call,
            100.0 * regex_us_per_call / avg_us
        );
        println!(
            "  non-regex preprocess time : {:>8.2} µs/call",
            avg_us - regex_us_per_call
        );
    }
}
