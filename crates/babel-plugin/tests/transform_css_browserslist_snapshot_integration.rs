//! Phase E7 — cross-pipeline gate for the browserslist snapshot path.
//!
//! Asserts that `transform_css` driven by
//! `TransformOpts::precomputed_browserslist` produces byte-identical
//! `{ sheets, class_names }` to the existing env-pinned baseline at
//! `crates/babel-plugin/tests/transform_css_integration.rs`.
//!
//! ## Why this test exists
//!
//! The env-pinned baseline (`rust_transform_css_matches_js_corpus`)
//! works because `BROWSERSLIST_CONFIG` is set at test-time and the
//! in-process `browserslist_shim::resolve("")` path picks it up
//! correctly. That path is broken inside WASI (env vars don't cross
//! the boundary, FS walks for `.browserslistrc` fail) — see
//! `DEFINITIVE_BROWSERSLIST_PLAN.md` for the bug shape.
//!
//! Phase A introduced the `PrecomputedBrowserslist` schema; Phase B
//! wired it into the 5 cssnano leaf plugins; Phase C threaded
//! `precomputed_browserslist` / `precomputed_browserslist_path`
//! through `TransformOpts`. **This test is the load-bearing proof
//! that the snapshot path agrees with the env-pinned path** —
//! without it we have no way to know that a snapshot-driven WASI
//! plugin invocation produces the same output as a Babel/NAPI
//! invocation against the same `.browserslistrc`.
//!
//! ## Methodology
//!
//! Side-by-side equivalence test: for each corpus entry, run
//! `transform_css` TWICE in this single test process —
//!
//!   1. **env-pinned arm**: `BROWSERSLIST_CONFIG=<AFM fixture>` set
//!      before the call, snapshot fields = `None`. This is the
//!      production NAPI path (Babel-equivalent).
//!   2. **snapshot arm**: env-vars cleared before the call,
//!      `precomputed_browserslist = Some(afm_canonical_bytes)`. This
//!      is the production WASI path.
//!
//! Assert **both arms produce byte-identical `(sheets, class_names)`**.
//! The JS-oracle `expected_sheets` are NOT compared — that's the
//! baseline test's job (`transform_css_integration.rs`). We only
//! prove the two Rust resolution paths agree, which is the
//! load-bearing claim Phase E7 needs to make.
//!
//! Why side-by-side instead of comparing each arm to the oracle:
//! pre-existing port bugs in `crates/css` (e.g. class-name
//! ordering on `22_comments_at_positions.css class-hash-prefix`)
//! cause the baseline itself to drift from the JS oracle on a few
//! fixtures. Those failures are unrelated to browserslist; folding
//! them into Phase E7 would conflate "Phase A-D introduced
//! browserslist drift" with "the comment-handling port already had
//! a bug". The env-vs-snapshot diff isolates the browserslist
//! contract cleanly: if env and snapshot ever diverge (even on a
//! buggy fixture), the bug is in the new wiring; if they always
//! agree, the new wiring is parity-clean regardless of any
//! orthogonal port bugs.
//!
//! ## Drift gate
//!
//! If the snapshot path drifts even by one byte from the env-pinned
//! path, this test fails — locking the contract that production
//! AFM (NAPI + env) and the WASI plugin (snapshot) emit identical
//! bytes for the same input + opts.
//!
//! ## Why a separate test file (not a sibling test in `transform_css_integration.rs`)
//!
//! `cargo test` builds each `tests/*.rs` as a separate binary, so
//! env-var pinning here cannot race the existing test's `EnvPin`
//! (different processes). See the long EnvPin doc in
//! `transform_css_integration.rs` for why intra-binary parallelism
//! makes mixing env-pinned and env-cleared tests in the same file
//! a hazard.

use std::fs;
use std::path::PathBuf;

use css::{transform_css, TransformOpts, TransformResult};
use cssnano_browserslist_snapshot::{
    encode_precomputed, precompute_browserslist, PrecomputeBrowserslistOpts,
};
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    fixture: String,
    opts_label: String,
    opts: WireOpts,
    input: String,
    // `expected_*` and `expected_to_fail` are present in the corpus
    // but unused here — Phase E7 compares the two Rust resolution
    // arms to each other, not to the JS oracle. Kept off the struct
    // so the JSON parses cleanly without dead-code warnings.
    #[serde(default)]
    #[allow(dead_code)]
    expected_sheets: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    expected_class_names: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct WireOpts {
    optimize_css: Option<bool>,
    class_name_compression_map: Option<IndexMap<String, String>>,
    increase_specificity: Option<bool>,
    sort_at_rules: Option<bool>,
    sort_shorthand: Option<bool>,
    class_hash_prefix: Option<String>,
}

impl WireOpts {
    fn to_transform_opts(&self, snapshot_bytes: Option<Vec<u8>>) -> TransformOpts {
        TransformOpts {
            optimize_css: self.optimize_css,
            class_name_compression_map: self.class_name_compression_map.clone(),
            increase_specificity: self.increase_specificity,
            sort_at_rules: self.sort_at_rules,
            sort_shorthand: self.sort_shorthand,
            class_hash_prefix: self.class_hash_prefix.clone(),
            precomputed_prefixes: None,
            precomputed_prefixes_path: None,
            precomputed_browserslist: snapshot_bytes,
            precomputed_browserslist_path: None,
        }
    }
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("transform_css_corpus.json")
}

fn load_corpus() -> Corpus {
    let raw = fs::read_to_string(corpus_path()).expect(
        "transform_css_corpus.json missing — regenerate via \
         `bun parity-harness/transform-css/oracle.mjs`",
    );
    let c: Corpus = serde_json::from_str(&raw).expect("corpus malformed");
    assert_eq!(c.version, 1, "corpus version drifted");
    c
}

fn afm_browserslistrc() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm")
        .join(".browserslistrc");
    assert!(
        p.exists(),
        "AFM .browserslistrc fixture missing at {} — has the path changed?",
        p.display(),
    );
    p
}

/// Build the snapshot bytes the harness ships in production. Anchors
/// the host-side `find_config` walk on the AFM fixture so the
/// resolved list matches the modern Jira list documented in
/// `DEFINITIVE_BROWSERSLIST_PLAN.md` §5.
fn afm_snapshot_bytes() -> Vec<u8> {
    let snap = precompute_browserslist(PrecomputeBrowserslistOpts {
        path: Some(afm_browserslistrc()),
        env: None,
    });
    // Cheap defensive sanity — if the snapshot didn't actually
    // pick up the fixture, every assertion below would fail with
    // an opaque content diff. Fail fast with a clear message.
    assert!(
        !snap.selected.is_empty(),
        "AFM snapshot resolved to empty list — fixture lookup likely failed",
    );
    assert!(
        snap.selected.iter().any(|b| b.starts_with("chrome ")),
        "AFM snapshot missing chrome entries — got {:?}",
        snap.selected,
    );
    encode_precomputed(&snap)
}

/// **Phase E7 cross-pipeline gate.**
///
/// For every corpus entry, run `transform_css` once with the env
/// pin and once with the snapshot, then assert the two
/// `TransformResult` values are byte-identical. Compares the two
/// resolution mechanisms directly rather than each against the JS
/// oracle, so pre-existing port bugs unrelated to browserslist
/// (e.g. comment-handling drift on
/// `22_comments_at_positions.css`) don't conflate the gate.
///
/// Failure mode: any single fixture where `env_result !=
/// snapshot_result` proves a divergence between the two
/// resolution mechanisms — either Phase A-C wired the snapshot
/// incorrectly, or the env path stopped honoring
/// `BROWSERSLIST_CONFIG`. Both are P0 regressions; this test
/// catches them.
#[test]
fn snapshot_path_byte_equal_to_env_pinned_baseline() {
    let snapshot_bytes = afm_snapshot_bytes();
    let corpus = load_corpus();

    // SAFETY: process-global env mutation. This test is the only
    // env-touching test in this binary (cargo compiles each
    // `tests/*.rs` to a separate binary, so no race vs the env
    // pin in `transform_css_integration.rs`). We sequence the two
    // arms strictly: SET env → run env arm → UNSET env → run
    // snapshot arm. No parallelism inside this test function, so
    // the env state at each `transform_css` call is determinate.
    let afm_path = afm_browserslistrc();

    let mut divergences: Vec<String> = Vec::new();

    for entry in &corpus.entries {
        // ---- env-pinned arm ----
        std::env::set_var("BROWSERSLIST_CONFIG", &afm_path);
        std::env::remove_var("BROWSERSLIST");
        std::env::remove_var("AUTOPREFIXER");
        let env_opts = entry.opts.to_transform_opts(None);
        let env_result: TransformResult = transform_css(&entry.input, &env_opts)
            .unwrap_or_else(|e| {
                panic!(
                    "env-pinned arm errored on fixture={} opts={}: {e}",
                    entry.fixture, entry.opts_label,
                )
            });

        // ---- snapshot arm ----
        std::env::remove_var("BROWSERSLIST_CONFIG");
        std::env::remove_var("BROWSERSLIST");
        std::env::remove_var("AUTOPREFIXER");
        let snap_opts = entry.opts.to_transform_opts(Some(snapshot_bytes.clone()));
        let snap_result: TransformResult = transform_css(&entry.input, &snap_opts)
            .unwrap_or_else(|e| {
                panic!(
                    "snapshot arm errored on fixture={} opts={}: {e}",
                    entry.fixture, entry.opts_label,
                )
            });

        if env_result.sheets != snap_result.sheets
            || env_result.class_names != snap_result.class_names
        {
            divergences.push(format!(
                "fixture={} opts={}\n  input={:?}\n  env_sheets ={:?}\n  snap_sheets={:?}\n  env_class  ={:?}\n  snap_class ={:?}",
                entry.fixture,
                entry.opts_label,
                entry.input,
                env_result.sheets,
                snap_result.sheets,
                env_result.class_names,
                snap_result.class_names,
            ));
            if divergences.len() >= 10 {
                break;
            }
        }
    }

    // Restore baseline state on exit so a subsequent run in the
    // same shell isn't perturbed by leaked env vars (test ran
    // last took the lock; other tests in this binary don't touch
    // env, but the developer's shell might).
    std::env::remove_var("BROWSERSLIST_CONFIG");
    std::env::remove_var("BROWSERSLIST");
    std::env::remove_var("AUTOPREFIXER");

    assert!(
        divergences.is_empty(),
        "snapshot-arm output drifted from env-pinned arm \
         ({} divergences; first {} shown):\n\n{}\n\n\
         The snapshot path MUST produce byte-identical output to \
         the env-pinned path; otherwise WASI consumers (driving \
         `precomputed_browserslist`) and NAPI consumers (driving \
         env-var-resolved browserslist) would emit different CSS \
         for the same input. This is the load-bearing contract \
         that Phase A-D plumbing has to uphold.",
        divergences.len(),
        divergences.len().min(10),
        divergences.join("\n\n"),
    );

    eprintln!(
        "Phase E7: all {} corpus entries match between env-pinned \
         and snapshot resolution paths",
        corpus.entries.len(),
    );
}

/// **Phase E7 negative control.** Demonstrates that with NO
/// snapshot AND NO env pin, the in-process `browserslist_shim::resolve("")`
/// falls back to the wide `browserslist@4.24.2` defaults. We only
/// assert that this case PRODUCES OUTPUT (no crash) — we don't
/// assert it differs from the AFM-pinned output, because some
/// fixtures don't exercise browserslist-gated decisions and would
/// match either way.
///
/// The point of this test is to lock in: "the absence of both
/// pinning mechanisms is not an error path; the output just
/// reflects the wide defaults." Pairs with the gate above —
/// together they prove the wiring is both load-bearing AND
/// optional.
#[test]
fn no_snapshot_no_env_pin_falls_back_to_defaults_without_error() {
    std::env::remove_var("BROWSERSLIST_CONFIG");
    std::env::remove_var("BROWSERSLIST");

    // A trivial input that doesn't exercise browserslist gating —
    // every cssnano leaf plugin no-ops on it. Output should match
    // either snapshot OR default-resolved browserslist resolution.
    let input = ".a { color: red; }";
    let opts = TransformOpts::default();

    let result = transform_css(input, &opts);
    assert!(
        result.is_ok(),
        "transform_css must succeed without a snapshot: {result:?}",
    );
}
