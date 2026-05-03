//! Lightweight runtime profiling counters — `cfg`-gated probes so the
//! release build pays nothing.
//!
//! Used by the perf example to confirm where `preprocess()` time goes.
//! NOT part of the byte-equality contract; counters never affect output.

use std::sync::atomic::{AtomicU64, Ordering};

static REGEX_COMPILE_COUNT: AtomicU64 = AtomicU64::new(0);
static REGEX_COMPILE_NANOS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct Counters {
    pub regex_compile_count: u64,
    pub regex_compile_ns: u64,
}

pub fn reset_counters() {
    REGEX_COMPILE_COUNT.store(0, Ordering::Relaxed);
    REGEX_COMPILE_NANOS.store(0, Ordering::Relaxed);
}

pub fn snapshot_counters() -> Counters {
    Counters {
        regex_compile_count: REGEX_COMPILE_COUNT.load(Ordering::Relaxed),
        regex_compile_ns: REGEX_COMPILE_NANOS.load(Ordering::Relaxed),
    }
}

/// Time a `Regex::new`-equivalent closure and accumulate into the
/// global counters. Inline so the closure body inlines through.
#[inline(always)]
pub fn time_regex_compile<T>(f: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let out = f();
    let ns = start.elapsed().as_nanos() as u64;
    REGEX_COMPILE_NANOS.fetch_add(ns, Ordering::Relaxed);
    REGEX_COMPILE_COUNT.fetch_add(1, Ordering::Relaxed);
    out
}
