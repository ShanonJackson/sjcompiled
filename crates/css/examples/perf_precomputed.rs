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
use autoprefixer::precomputed::{encode_precomputed, precompute_prefixes};
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

    println!("Building precomputed snapshot (one-time, outside timing)...");
    let snapshot_build_start = Instant::now();
    let snapshot = precompute_prefixes(AutoprefixerOptions {
        from: Some(from.clone()),
        ..Default::default()
    });
    let bytes = encode_precomputed(&snapshot);
    let snapshot_build = snapshot_build_start.elapsed();
    println!(
        "  snapshot built in {:.2} ms, encoded to {} bytes\n",
        snapshot_build.as_secs_f64() * 1000.0,
        bytes.len()
    );

    // Make the slow path resolve against AFM's `.browserslistrc` too,
    // so both paths are doing identical pipeline work — the only
    // measured difference is the autoprefixer prefix construction.
    std::env::set_current_dir(&afm_dir).expect("set cwd to AFM fixture dir");

    let slow_opts = make_opts(None);
    let fast_opts = make_opts(Some(bytes));

    bench("slow path (default)", &slow_opts);
    bench("precomputed (fast)", &fast_opts);

    // Phase breakdown — answers "where does the autoprefixer cost go?"
    // by measuring just the autoprefixer construction cost in isolation.
    println!("\nAutoprefixer phase breakdown (per-call)");
    println!("-------------------------------------");
    {
        use autoprefixer::autoprefixer::build_prefixes_default;
        use autoprefixer::precomputed::build_prefixes_from_precomputed;

        let snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions {
                from: Some(afm_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let bytes = autoprefixer::precomputed::encode_precomputed(&snap);

        // Warmup
        for _ in 0..50 {
            let _ = build_prefixes_default(None).unwrap();
            let _ = build_prefixes_from_precomputed(&bytes).unwrap();
        }

        let n = 200;
        let t = Instant::now();
        for _ in 0..n {
            let _ = build_prefixes_default(None).unwrap();
        }
        let slow_build = t.elapsed();

        let t = Instant::now();
        for _ in 0..n {
            let _ = build_prefixes_from_precomputed(&bytes).unwrap();
        }
        let fast_build = t.elapsed();

        println!(
            "  build_prefixes_default          {:>8.2} µs/call",
            slow_build.as_secs_f64() * 1e6 / n as f64
        );
        println!(
            "  build_prefixes_from_precomputed {:>8.2} µs/call (delta = preprocess only)",
            fast_build.as_secs_f64() * 1e6 / n as f64
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
