//! crates/parity-runner
//!
//! Differential harness for plugin parity. Each `Stage` represents one
//! pipeline shape we want to diff between JS and Rust. The harness takes
//! a CSS input, runs both pipelines, and reports byte differences.
//!
//! Architecture:
//!
//!   1. Spawn `node` once with `scripts/js-pipeline.mjs`. Pay the ~150ms
//!      Node startup cost a single time.
//!   2. Stream `{stage, css}` JSON-lines requests over stdin; read
//!      `{ok: true, css}` or `{ok: false, error}` JSON-lines responses
//!      from stdout.
//!   3. For each corpus entry: send to JS, run Rust, byte-compare.
//!      Print the smallest divergent byte range with surrounding context.
//!
//! Plugin authors invoke this via the integration test
//! `tests/<plugin>.rs`. They DO NOT run this in production — the JS
//! pipeline stays in `packages/css/src/transform.ts` as the oracle, and
//! Rust runs alongside it through NAPI in Phase 8.

pub mod diff;
pub mod js_bridge;
pub mod stages;

pub use diff::{diff_summary, DiffResult};
pub use js_bridge::{JsBridge, JsRequest, JsResponse};
pub use stages::{rust_run_stage, Stage};
