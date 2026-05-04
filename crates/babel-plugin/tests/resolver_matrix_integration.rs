//! Phase 5 §5.4a — Rust resolver-matrix parity gate.
//!
//! Reads the JS-locked corpus at `tests/resolver_matrix_corpus.json`
//! (regenerable via `bun parity-harness/resolver-matrix/oracle.mjs`)
//! and asserts that the Rust resolver under
//! `crates/babel-plugin/src/resolver/` (the in-plugin replacement
//! for the host's `createDefaultResolver` per `plugins/PLAN.md` §1
//! constraint 2 and `plugins/RESOLVER_SPEC_PART_TWO.md`) produces the
//! same resolved absolute path that `enhanced-resolve@5.18.3` does
//! for every Layer-1 (default-config) fixture.
//!
//! ## Why this gate exists
//!
//! `packages/babel-plugin/src/utils/resolve-binding.ts` calls into
//! a host-injected resolver (today: `createDefaultResolver` wrapping
//! `enhanced-resolve@5.x`) for every cross-file import the evaluator
//! needs to fold. The resolved path determines which file is read,
//! which AST is parsed, which expression is folded, and ultimately
//! which atomic CSS class hash is emitted. **A divergent resolved
//! path is a divergent class name in production** — same severity
//! tier as `compat/generator.rs` (§4.3) and `compat/evaluation.rs`
//! (§5.0c).
//!
//! The 9 corpus axes + the divergence-action protocol are recorded
//! in `crates/babel-plugin/RESOLVER_MATRIX.md`. The §5.4b implementer
//! un-ignores `rust_resolver_matches_js_corpus` once
//! `crates/babel-plugin/src/resolver/default.rs` exists; before that,
//! the byte-parity body would unconditionally fail and pollute the
//! workspace test signal.
//!
//! ## Status (Phase 5 §5.4a — ENTRY-GATE LANDED)
//!
//! - `corpus_shape_lock` runs unconditionally; pin drift fails fast.
//! - `corpus_observed_matches_expected_oracle_self_consistency`
//!   runs unconditionally; catches stale corpora.
//! - `rust_resolver_matches_js_corpus` is `#[ignore]`'d until §5.4b.
//!
//! See `plugins/STATUS.md` Phase 5 row §5.4 for the broader
//! checkpoint state, `crates/babel-plugin/RESOLVER_MATRIX.md` for
//! the axis-by-axis port plan, and
//! `parity-harness/resolver-matrix/{oracle.mjs,fixtures.json,fixtures-source/}`
//! for the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

const EXPECTED_ENHANCED_RESOLVE_VERSION: &str = "5.18.3";
const EXPECTED_RESOLVE_VERSION: &str = "1.22.12";

// Subset of the 9 axes enumerated in
// `crates/babel-plugin/RESOLVER_MATRIX.md`. The §5.4a entry-gate
// seeds 4 axes; §5.4b grows the corpus per the divergence-action
// protocol. Adding a new axis = changing this list AND fixtures.json
// AND (likely) the §5.4b port that handles it.
const EXPECTED_AXES_AT_ENTRY_GATE: &[&str] = &[
    "package.json-main",
    "package.json-exports-conditions",
    "extension-order",
    "directory-index",
];

// Lower bound on entry count at the §5.4a entry-gate. The §5.4b
// implementer raises this floor as they grow the corpus.
const MIN_ENTRY_COUNT_AT_ENTRY_GATE: usize = 4;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    enhanced_resolve_version: String,
    resolve_version: String,
    entry_count: usize,
    axis_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    label: String,
    axis: String,
    expected: serde_json::Value,
    observed: serde_json::Value,
    // `fromFile`, `request`, `extensions` are present on every JSON
    // entry but consumed only by `rust_resolver_matches_js_corpus`
    // post-§5.4b — added back when that test grows a real body.
    // serde ignores absent/extra fields by default.
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("resolver_matrix_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read resolver-matrix corpus at {}: {}\n\
             Regenerate with: bun parity-harness/resolver-matrix/oracle.mjs",
            path.display(),
            e
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("resolver-matrix corpus has invalid shape: {}", e)
    })
}

#[test]
fn corpus_shape_lock() {
    let corpus = load_corpus();

    assert_eq!(corpus.version, 1, "corpus schema version mismatch");
    assert_eq!(
        corpus.enhanced_resolve_version, EXPECTED_ENHANCED_RESOLVE_VERSION,
        "enhanced-resolve pin drift in corpus — regenerate via \
         `bun parity-harness/resolver-matrix/oracle.mjs` after \
         confirming the pin in package.json#overrides AND \
         devDependencies matches crates/PARITY_VERSIONS.md"
    );
    assert_eq!(
        corpus.resolve_version, EXPECTED_RESOLVE_VERSION,
        "resolve pin drift in corpus — same fix as above"
    );

    assert_eq!(
        corpus.entry_count,
        corpus.entries.len(),
        "corpus.entry_count != corpus.entries.len()"
    );
    assert!(
        corpus.entry_count >= MIN_ENTRY_COUNT_AT_ENTRY_GATE,
        "corpus has fewer than {MIN_ENTRY_COUNT_AT_ENTRY_GATE} entries ({}) — every \
         §5.4a entry-gate axis should be covered by at least one fixture; \
         if you intentionally pruned, lower this floor in lockstep with \
         crates/babel-plugin/RESOLVER_MATRIX.md",
        corpus.entry_count
    );

    for axis in EXPECTED_AXES_AT_ENTRY_GATE {
        let count = corpus.axis_counts.get(*axis).copied().unwrap_or(0);
        assert!(
            count > 0,
            "axis `{axis}` has no entries — fixtures.json must seed at \
             least one fixture per entry-gate axis. The §5.4b implementer \
             may add more axes (the 9 in RESOLVER_MATRIX.md) but cannot \
             drop any of these."
        );
    }

    for entry in &corpus.entries {
        // Entry-gate axes must round-trip the EXPECTED_AXES list.
        // §5.4b may legitimately add new axes (axes 3/4/5/8/9 from
        // RESOLVER_MATRIX.md not in the entry-gate seed); accept those
        // without failure but lock the entry-gate ones.
        if EXPECTED_AXES_AT_ENTRY_GATE.iter().any(|a| *a == entry.axis) {
            continue;
        }
        // Permit unknown new axes; emit a console line so reviewers see
        // the corpus growth surface in CI logs.
        eprintln!(
            "note: entry `{}` has new axis `{}` — confirm it's \
             enumerated in crates/babel-plugin/RESOLVER_MATRIX.md",
            entry.label, entry.axis
        );
    }
}

#[test]
fn corpus_observed_matches_expected_oracle_self_consistency() {
    let corpus = load_corpus();
    for entry in &corpus.entries {
        let expected = entry.expected.as_object().unwrap_or_else(|| {
            panic!("entry `{}`: expected is not an object", entry.label)
        });
        let observed = entry.observed.as_object().unwrap_or_else(|| {
            panic!("entry `{}`: observed is not an object", entry.label)
        });
        for (k, v) in expected {
            // Skip the JSON-comment-style "//" key (used in fixtures.json
            // for inline narrative). It's not a real expected output.
            if k == "//" {
                continue;
            }
            let got = observed.get(k).unwrap_or_else(|| {
                panic!(
                    "entry `{}`: expected key `{}` missing from observed",
                    entry.label, k
                )
            });
            assert_eq!(
                got, v,
                "entry `{}`: oracle self-consistency violated on key `{}` \
                 (corpus is stale — re-run \
                 `bun parity-harness/resolver-matrix/oracle.mjs`)",
                entry.label, k
            );
        }
    }
}

/// Byte-parity gate. Compares `oxc_resolver` output against the
/// `enhancedResolve` column of every corpus entry.
///
/// **`#[ignore]`'d at §5.4a entry-gate** — the Rust resolver under
/// `crates/babel-plugin/src/resolver/` doesn't exist yet. The §5.4b
/// implementer:
///
/// 1. Lands `crates/babel-plugin/src/resolver/{mod,config,default,engine}.rs`
///    per `plugins/RESOLVER_SPEC_PART_TWO.md` and
///    `crates/babel-plugin/RESOLVER_MATRIX.md`.
/// 2. Wires this gate body to call into `resolver::build_default(...)`
///    and `Resolver::resolve_sync(...)`.
/// 3. Removes `#[ignore]` and runs.
/// 4. Applies the divergence-action protocol from RESOLVER_MATRIX.md
///    for every fixture that fails: match | shim | escalate.
#[test]
#[ignore]
fn rust_resolver_matches_js_corpus() {
    let corpus = load_corpus();

    // Placeholder until §5.4b. The body below intentionally fails
    // with a clear pointer to the next step so a future agent who
    // accidentally un-ignores this test sees the right diagnostic.
    panic!(
        "Phase 5 §5.4a entry-gate placeholder — the Rust resolver \
         under crates/babel-plugin/src/resolver/ has not been ported \
         yet. Land §5.4b (the engine + default config) and replace \
         this body. Corpus has {} entries across {} axes.",
        corpus.entry_count,
        corpus.axis_counts.len()
    );
}
