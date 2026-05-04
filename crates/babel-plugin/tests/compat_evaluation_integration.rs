//! Phase 5 §5.0c — Rust `compat::evaluation` parity gate.
//!
//! Reads the JS-locked corpus at `tests/compat_evaluation_corpus.json`
//! (regenerable via `bun parity-harness/compat-evaluation/oracle.mjs`)
//! and asserts that the Rust 1:1 port of
//! `@babel/traverse@7.29.0/lib/path/evaluation.js` produces the same
//! `{confident, value}` shape Babel does for every reachable
//! expression form.
//!
//! ## Why this gate exists
//!
//! `packages/babel-plugin/src/utils/evaluate-expression.ts:93` calls
//! `path.evaluate()` as the FALLBACK constant-folder when the
//! Compiled-specific traversers (`traverseIdentifier`,
//! `traverseMemberExpression`, etc.) return without a confident
//! value. The fold result, when string-typed or number-typed, becomes
//! a `t.stringLiteral` / `t.numericLiteral` that flows into CSS
//! values → `transform_css` → atomic class hash. **A divergent fold
//! is a divergent class name in production.**
//!
//! The Q3 lock in `plugins/COMPAT_SCOPE_AUDIT.md` mandates a
//! line-for-line port of `@babel/traverse/lib/path/evaluation.js` —
//! NOT a partial port. The four evidenced-unreachable branches
//! enumerated in `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`
//! (Flow type-cast, JSX-as-evaluable, SequenceExpression,
//! TaggedTemplateExpression) MAY emit `unimplemented!("…")` with a
//! citation to that survey, but every reachable branch must fold.
//!
//! ## Status (Phase 5 §5.0c entry-gate, pre-port)
//!
//! At entry-gate time the corpus exists, the version pins are
//! guarded, and the shape contract is locked. The actual
//! shape-parity assertion (`rust_compat_evaluation_matches_js_corpus`)
//! is `#[ignore]`d because `compat::evaluation` does not exist yet
//! — it ships in §5.0c. The shape-lock test (`corpus_shape_lock`)
//! and the oracle self-consistency check run unconditionally so a
//! malformed corpus or pin drift fails fast before any port code
//! lands.
//!
//! When §5.0c lands: remove the `#[ignore]` attribute, wire
//! `evaluate(&expr)` against the real `compat::evaluation` API, and
//! the gate must be shape-clean across every reachable-branch entry
//! before §5.0c is signed off.
//!
//! See `plugins/STATUS.md` Phase 5 row §5.0,
//! `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` for the
//! reachable/unreachable branch survey, and
//! `parity-harness/compat-evaluation/{oracle.mjs,fixtures.json}` for
//! the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

const EXPECTED_TRAVERSE_VERSION: &str = "7.29.0";
const EXPECTED_PARSER_VERSION: &str = "7.29.2";

// Twelve category axes mirrored from
// `parity-harness/compat-evaluation/fixtures.json`. Adding a new
// category = changing this list AND the fixtures.json seeds AND
// the §5.0c port that handles it.
const EXPECTED_CATEGORIES: &[&str] = &[
    "binary",
    "binary-comparison",
    "conditional",
    "deopt",
    "identifier-global",
    "literal",
    "logical",
    "mixed",
    "parenthesized",
    "template",
    "ts",
    "unary",
];

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    babel_traverse_version: String,
    babel_parser_version: String,
    #[allow(dead_code)] // peer-dep slot; not asserted.
    babel_types_version: String,
    entry_count: usize,
    category_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[allow(dead_code)] // fields consumed by the byte-parity gate post-§5.0c.
struct Entry {
    label: String,
    category: String,
    input_source: String,
    expected: serde_json::Value,
    observed: serde_json::Value,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat_evaluation_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read compat-evaluation corpus at {}: {}\n\
             Regenerate with: bun parity-harness/compat-evaluation/oracle.mjs",
            path.display(),
            e
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("compat-evaluation corpus has invalid shape: {}", e)
    })
}

#[test]
fn corpus_shape_lock() {
    let corpus = load_corpus();

    assert_eq!(corpus.version, 1, "corpus schema version mismatch");
    assert_eq!(
        corpus.babel_traverse_version, EXPECTED_TRAVERSE_VERSION,
        "@babel/traverse pin drift in corpus — regenerate via \
         `bun parity-harness/compat-evaluation/oracle.mjs` after \
         confirming the pin in package.json#overrides AND \
         devDependencies matches crates/PARITY_VERSIONS.md"
    );
    assert_eq!(
        corpus.babel_parser_version, EXPECTED_PARSER_VERSION,
        "@babel/parser pin drift in corpus — same fix as above"
    );

    assert_eq!(
        corpus.entry_count,
        corpus.entries.len(),
        "corpus.entry_count != corpus.entries.len()"
    );
    assert!(
        corpus.entry_count >= 30,
        "corpus has fewer than 30 entries ({}) — every reachable \
         path.evaluate() branch should be covered by at least one \
         fixture; if you intentionally pruned, lower this floor in \
         lockstep with COMPAT_EVALUATION_COVERAGE.md",
        corpus.entry_count
    );

    for axis in EXPECTED_CATEGORIES {
        let count = corpus.category_counts.get(*axis).copied().unwrap_or(0);
        assert!(
            count > 0,
            "category axis `{axis}` has no entries — fixtures.json \
             must seed at least one fixture per axis."
        );
    }

    for entry in &corpus.entries {
        assert!(
            EXPECTED_CATEGORIES.contains(&entry.category.as_str()),
            "entry `{}` has unexpected category `{}` — update \
             EXPECTED_CATEGORIES + fixtures.json together",
            entry.label,
            entry.category
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
            let got = observed.get(k).unwrap_or_else(|| {
                panic!(
                    "entry `{}`: expected key `{}` missing from observed",
                    entry.label, k
                )
            });
            assert_eq!(
                got, v,
                "entry `{}`: oracle self-consistency violated on key `{}` (corpus is stale)",
                entry.label, k
            );
        }
    }
}

#[test]
#[ignore = "Phase 5 §5.0c not yet ported — compat::evaluation does not exist. \
            Remove this #[ignore] when §5.0c lands."]
fn rust_compat_evaluation_matches_js_corpus() {
    // Wired post-§5.0c: parse `entry.input_source` (wrapped in the
    // same `const __evalTarget = (…);` synthetic declarator the
    // oracle uses to dodge the directive-prologue trap), reach the
    // init expression, call `compat::evaluation::evaluate(&expr)`
    // returning `Option<EvaluatedValue>`, and assert against
    // `entry.expected.{confident, value_kind, value_string}`.
    //
    // Encoding contract (mirrors oracle.mjs):
    //   confident=true,  value=string  → value_kind="string",     value_string=JSON.stringify(s)
    //   confident=true,  value=number  → value_kind="number",     value_string=JSON.stringify(n)
    //                                    (NaN/Infinity → "null" per JSON quirk; Rust matches)
    //   confident=true,  value=bool    → value_kind="boolean",    value_string="true"|"false"
    //   confident=true,  value=null    → value_kind="object",     value_string="null"
    //   confident=true,  value=undef   → value_kind="undefined",  value_string="undefined"
    //   confident=false                → value_kind="undefined",  value_string="undefined"
    //
    // The Rust port's EvaluatedValue::Confident(v) / Deopt variants
    // map to those tuples deterministically.
    panic!("§5.0c not yet ported — see file-level docs for the unblock checkpoint.");
}
