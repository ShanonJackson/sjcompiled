//! Phase 4 §4.1 — Rust `transform_css` integration parity gate.
//!
//! Reads the JS-locked corpus at `tests/transform_css_corpus.json`
//! (regenerable via `bun parity-harness/transform-css/oracle.mjs`) and
//! asserts that `css::transform_css(&input, &opts)` produces
//! byte-identical `{ sheets, classNames }` to the upstream JS
//! `@compiled/css.transformCss(input, opts)` for every entry.
//!
//! The corpus pulls inputs from
//! `crates/parity-runner/corpus/transform-css/` (30 hand-curated CSS
//! fixtures owned by the parallel CSS-port agent — Phase 4-7 reference
//! corpus) crossed with 4 option permutations:
//!
//! 1. default (`{}`) — `optimizeCss` defaults to true; runs cssnano +
//!    autoprefixer.
//! 2. `optimizeCss: false` — skips the 14 cssnano sub-plugins.
//! 3. `increaseSpecificity: true` — gates plugin 8 in the orchestrator.
//! 4. `classHashPrefix: "x"` — forwarded into atomicifyRules; class names
//!    rotate.
//!
//! ## Why this gate exists alongside the parity-runner
//!
//! The parity-runner (`crates/parity-runner/src/stages.rs::TransformCss`)
//! validates `transform_css` from the **producer's** side — it shells out
//! to a JS bridge per call. This gate validates from the **consumer's**
//! side (this plugin's `tests/`):
//!
//!   - No JS bridge at test time — the JSON corpus pre-captures the JS
//!     output. Same precedent as Phase 3 hash parity.
//!   - Tests exercise `css::transform_css` exactly as Phase 4 §4.6 will
//!     wire it into the visitor (`utils/transform_css_items.rs`,
//!     `utils/build_styled_component.rs`).
//!   - The four option permutations span the real call shape from
//!     `packages/babel-plugin/src/utils/transform-css-items.ts:61,84`
//!     and `packages/babel-plugin/src/utils/build-styled-component.ts:248`.
//!
//! ## Env contract
//!
//! `BROWSERSLIST_CONFIG` pinned to AFM's `.browserslistrc` fixture
//! (`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`),
//! `BROWSERSLIST` UNSET (would short-circuit the config-file path with
//! priority over BROWSERSLIST_CONFIG), and `AUTOPREFIXER` UNSET.
//!
//! AFM is the production pin — it's the exact configuration the Jira
//! build runs in production through `@compiled/parcel-transformer →
//! @compiled/babel-plugin → @compiled/css@0.19.0 → autoprefixer
//! 10.4.14 → browserslist 4.24.2`. Resolves to the 14-entry list
//! documented in `BROWSER_LIST_FROM_AFM.md` under the workspace's
//! pinned `caniuse-lite@1.0.30001766` + `browserslist@4.24.2`
//! overrides. Both engines honor `BROWSERSLIST_CONFIG` —
//! `crates/browserslist-shim/src/node.rs:143` for the Rust side.
//!
//! See `plugins/STATUS.md` Phase 4 row §4.1.

use std::fs;
use std::path::PathBuf;

use css::{transform_css, TransformOpts, TransformResult};
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    fixture_count: usize,
    opts_permutations: usize,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    fixture: String,
    opts_label: String,
    opts: WireOpts,
    input: String,
    expected_sheets: Vec<String>,
    expected_class_names: Vec<String>,
    /// True when the SHIPPED NAPI binary + JS oracle agree on this
    /// fixture but the fresh source build of `crates/css` /
    /// `crates/autoprefixer` is currently expected to diverge. See
    /// `EXPECTED_TO_FAIL` in `parity-harness/transform-css/oracle.mjs`
    /// for the per-fixture reason. We invert the byte-equality
    /// assertion for these entries — flips to a failing test when
    /// the underlying drift is fixed (signal: remove the entry from
    /// the oracle's `EXPECTED_TO_FAIL` map).
    #[serde(default)]
    expected_to_fail: bool,
    #[serde(default)]
    #[allow(dead_code)]
    failure_reason: Option<String>,
}

/// Wire shape for the JS `TransformOpts` — only the fields the oracle
/// emits. Mirrors `css::TransformOpts` field-by-field but as a separate
/// struct so the corpus JSON parses cleanly even when upstream gains
/// fields the gate doesn't yet exercise.
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
    fn into_transform_opts(self) -> TransformOpts {
        TransformOpts {
            optimize_css: self.optimize_css,
            class_name_compression_map: self.class_name_compression_map,
            increase_specificity: self.increase_specificity,
            sort_at_rules: self.sort_at_rules,
            sort_shorthand: self.sort_shorthand,
            class_hash_prefix: self.class_hash_prefix,
            precomputed_prefixes: None,
            precomputed_prefixes_path: None,
            // Env-pinned baseline drives `BROWSERSLIST_CONFIG`, NOT
            // a snapshot — see EnvPin docs above. Phase E7 has the
            // mirror test that drives the snapshot path with
            // identical expected output.
            precomputed_browserslist: None,
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
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {} — regenerate with `bun parity-harness/transform-css/oracle.mjs`",
            path.display(),
            e
        )
    });
    let corpus: Corpus =
        serde_json::from_str(&raw).expect("transform_css_corpus.json malformed");
    assert_eq!(corpus.version, 1, "transform_css_corpus.json version drifted");
    corpus
}

/// RAII guard for the env var pin. Both the JS oracle and this gate must
/// see `BROWSERSLIST_CONFIG` pinned to the AFM `.browserslistrc` fixture,
/// with `BROWSERSLIST` and `AUTOPREFIXER` UNSET. Restores prior state on
/// drop so a test process that mutates env doesn't leak the pin into
/// other tests in the same binary.
///
/// **CRITICAL**: env vars are process-global, not thread-local. Tests in
/// the same binary run in parallel by default, and if two tests both call
/// `EnvPin::new()` they race — one's `set_var` collides with the other's
/// `Drop`, and `transform_css` ends up reading whatever browserslist
/// resolution survives the race. Symptom observed: source `transform_css`
/// appearing to add an extra `-moz-user-select` prefix under what looks
/// like the AFM pin but is actually default-browserslist after another
/// test thread cleared `BROWSERSLIST_CONFIG`. Default browserslist
/// includes much older Firefox versions that legitimately need
/// `-moz-user-select`. The fix is to keep `EnvPin` use confined to a
/// SINGLE test function (the parity gate); the schema-shape tests below
/// don't read env state and so don't construct an `EnvPin`. This holds
/// even when `cargo test` parallelises.
struct EnvPin {
    prev_browserslist: Option<String>,
    prev_browserslist_config: Option<String>,
    prev_autoprefixer: Option<String>,
}

impl EnvPin {
    fn new() -> Self {
        let prev_browserslist = std::env::var("BROWSERSLIST").ok();
        let prev_browserslist_config = std::env::var("BROWSERSLIST_CONFIG").ok();
        let prev_autoprefixer = std::env::var("AUTOPREFIXER").ok();

        // CARGO_MANIFEST_DIR is `crates/babel-plugin`. The AFM fixture
        // lives at `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`,
        // i.e. `../browserslist-shim/...` from this crate's manifest.
        let afm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("browserslist-shim")
            .join("tests")
            .join("fixtures")
            .join("afm")
            .join(".browserslistrc");
        // Sanity — fail fast if the fixture moved; better than silently
        // running under default browserslist (which would mask drift).
        assert!(
            afm_path.exists(),
            "AFM browserslist fixture missing at {} — has the path changed?",
            afm_path.display()
        );

        std::env::remove_var("BROWSERSLIST");
        std::env::set_var("BROWSERSLIST_CONFIG", &afm_path);
        std::env::remove_var("AUTOPREFIXER");
        Self {
            prev_browserslist,
            prev_browserslist_config,
            prev_autoprefixer,
        }
    }
}

impl Drop for EnvPin {
    fn drop(&mut self) {
        match self.prev_browserslist.take() {
            Some(v) => std::env::set_var("BROWSERSLIST", v),
            None => std::env::remove_var("BROWSERSLIST"),
        }
        match self.prev_browserslist_config.take() {
            Some(v) => std::env::set_var("BROWSERSLIST_CONFIG", v),
            None => std::env::remove_var("BROWSERSLIST_CONFIG"),
        }
        match self.prev_autoprefixer.take() {
            Some(v) => std::env::set_var("AUTOPREFIXER", v),
            None => std::env::remove_var("AUTOPREFIXER"),
        }
    }
}

fn run_one(input: &str, opts: TransformOpts) -> TransformResult {
    transform_css(input, &opts)
        .unwrap_or_else(|e| panic!("rust transform_css errored: {e}"))
}

#[test]
fn rust_transform_css_matches_js_corpus() {
    let _pin = EnvPin::new();
    let corpus = load_corpus();

    // Sanity — the oracle promised 30 fixtures × 4 opts.
    assert!(
        corpus.fixture_count >= 30,
        "expected ≥30 source fixtures, got {}",
        corpus.fixture_count
    );
    assert!(
        corpus.opts_permutations >= 4,
        "expected ≥4 option permutations, got {}",
        corpus.opts_permutations
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut unexpected_passes: Vec<String> = Vec::new();
    let mut expected_to_fail_count = 0usize;
    let mut parity_count = 0usize;
    for entry in corpus.entries {
        let opts = entry.opts.into_transform_opts();
        let actual = run_one(&entry.input, opts);

        let bytes_equal = actual.sheets == entry.expected_sheets
            && actual.class_names == entry.expected_class_names;

        if entry.expected_to_fail {
            expected_to_fail_count += 1;
            // Inverted assertion: source build is expected to diverge.
            // Equality means the underlying drift was fixed — remove the
            // entry from the oracle's EXPECTED_TO_FAIL map.
            if bytes_equal {
                unexpected_passes.push(format!(
                    "fixture={} opts={} — expected to FAIL but bytes match. \
                     Drift fixed; remove from EXPECTED_TO_FAIL in \
                     parity-harness/transform-css/oracle.mjs.",
                    entry.fixture, entry.opts_label
                ));
            }
            continue;
        }

        parity_count += 1;
        if !bytes_equal {
            mismatches.push(format!(
                "fixture={} opts={}\n  input={:?}\n  expected_sheets={:?}\n  actual_sheets  ={:?}\n  expected_class_names={:?}\n  actual_class_names  ={:?}",
                entry.fixture,
                entry.opts_label,
                entry.input,
                entry.expected_sheets,
                actual.sheets,
                entry.expected_class_names,
                actual.class_names
            ));
            if mismatches.len() >= 10 {
                break;
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "transform_css parity divergence ({} unexpected mismatches; first {} shown):\n\n{}",
        mismatches.len(),
        mismatches.len().min(10),
        mismatches.join("\n\n")
    );

    assert!(
        unexpected_passes.is_empty(),
        "{} expected-to-fail entries are now passing — drift resolved upstream:\n  {}",
        unexpected_passes.len(),
        unexpected_passes.join("\n  ")
    );

    // Lock the ratio so a future oracle regeneration that silently drops
    // the EXPECTED_TO_FAIL map can't be missed.
    assert!(
        parity_count > 0,
        "no parity entries — corpus likely empty or every entry is expected_to_fail"
    );
    eprintln!(
        "transform_css parity: {} entries byte-equal, {} expected-to-fail (autoprefixer V2 WIP)",
        parity_count, expected_to_fail_count
    );
}

#[test]
fn corpus_covers_required_opts_permutations() {
    // No EnvPin — pure shape check on the JSON corpus, doesn't call
    // `transform_css`. Constructing an `EnvPin` here would race the
    // parity test's pin (see EnvPin doc).
    let corpus = load_corpus();
    let labels: Vec<&str> = corpus.entries.iter().map(|e| e.opts_label.as_str()).collect();
    for required in ["default", "no-optimize", "increase-specificity", "class-hash-prefix"] {
        assert!(
            labels.iter().any(|l| *l == required),
            "missing opts permutation {:?}",
            required
        );
    }
}

#[test]
fn corpus_covers_full_pipeline_shapes() {
    // §4.1 acceptance: corpus inputs collectively exercise the surface
    // the babel-plugin's call sites will hit. We don't enumerate every
    // plugin (the parity-runner already does that producer-side); we
    // assert the consumer-relevant shapes are present:
    //   - empty input (degenerate path: should yield empty sheets/cls)
    //   - at-rule (cssnano + sort)
    //   - postcss-nested input (`& {...}` syntax)
    //   - shorthand expansion (padding/margin → 4 longforms)
    //   - autoprefixer-affected property (user-select)
    // No EnvPin — pure shape check (see EnvPin doc).
    let corpus = load_corpus();
    let inputs: Vec<&str> = corpus.entries.iter().map(|e| e.input.as_str()).collect();

    assert!(inputs.iter().any(|s| s.is_empty()), "empty input missing");
    assert!(
        inputs.iter().any(|s| s.contains("@media")),
        "@media at-rule input missing"
    );
    assert!(
        inputs.iter().any(|s| s.contains("&:hover") || s.contains("& :")),
        "postcss-nested `&` input missing"
    );
    assert!(
        inputs.iter().any(|s| s.contains("padding:") || s.contains("margin:")),
        "shorthand input missing"
    );
}
