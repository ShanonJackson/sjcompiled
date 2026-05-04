//! Phase 5 §5.0a/b — Rust `compat::scope` + `compat::path` parity gate.
//!
//! Reads the JS-locked corpus at `tests/compat_scope_corpus.json`
//! (regenerable via `bun parity-harness/compat-scope/oracle.mjs`)
//! and asserts that the Rust pre-indexed scope walker produces the
//! same binding/path-shape observables as upstream
//! `@babel/traverse@7.29.0` for every entry.
//!
//! ## Why this gate exists
//!
//! `packages/babel-plugin/src/utils/{evaluate-expression,resolve-binding}.ts`
//! and `utils/traverse-expression/*.ts` (the §5.4–§5.6 ports)
//! depend on `path.scope.getBinding(name)`,
//! `path.scope.getOwnBinding(name)`, `path.scope.push(...)`,
//! `binding.path.node`, `binding.constant`, `binding.referencePaths`,
//! `path.parentPath`, and `path.listKey`. None of those exist in
//! SWC's plugin runtime; `crates/babel-plugin/src/compat/{scope,path}.rs`
//! provides 1:1 analogues against the pre-indexed scope tree built
//! at `Program::enter`.
//!
//! Drift in any of those observables silently produces a wrong
//! evaluator output, which silently produces a wrong CSS class hash
//! (per §3 / §4.4 hash-call-shape sites), which silently renames a
//! production class. Same blast radius as `compat::generator`'s
//! byte-parity gate.
//!
//! ## Status (Phase 5 §5.0 entry-gate, pre-port)
//!
//! At entry-gate time the corpus exists, the version pins are
//! guarded, and the shape contract is locked. The actual
//! shape-parity assertion (`rust_compat_scope_matches_js_corpus`)
//! is `#[ignore]`d because `compat::scope`/`compat::path` do not
//! exist yet — they ship in §5.0a/b. The shape-lock test
//! (`corpus_shape_lock`) runs unconditionally so a malformed corpus
//! or pin drift fails fast before any port code lands.
//!
//! When §5.0a/b land: remove the `#[ignore]` attribute, wire
//! `extract_observed` against the real `compat::scope` API, and
//! the gate must be shape-clean across every entry before §5.0a/b
//! is signed off.
//!
//! See `plugins/STATUS.md` Phase 5 row §5.0,
//! `plugins/COMPAT_SCOPE_AUDIT.md` for the surface enumeration +
//! Q1/Q2/Q3 lock, and
//! `parity-harness/compat-scope/{oracle.mjs,fixtures.json}` for
//! the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

// AFM-pinned versions; mirror the constants in
// `parity-harness/compat-scope/oracle.mjs` and the row in
// `crates/PARITY_VERSIONS.md`.
const EXPECTED_TRAVERSE_VERSION: &str = "7.29.0";
const EXPECTED_PARSER_VERSION: &str = "7.29.2";

// Six query axes mirrored from
// `parity-harness/compat-scope/oracle.mjs`'s `QUERIES` table.
// Adding a new axis = changing this list AND adding a Rust
// query-runner branch in `run_observation` (post-§5.0a/b) AND in
// oracle.mjs's table.
const EXPECTED_CALL_SITES: &[&str] = &[
    "binding-lookup-from-reference",
    "generate-uid",
    "has-own-binding",
    "list-key-arguments",
    "path-predicate-via-binding",
    "scope-push-iife",
];

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    babel_traverse_version: String,
    babel_parser_version: String,
    #[allow(dead_code)] // read for documentation; not asserted (peer-dep slot).
    babel_types_version: String,
    entry_count: usize,
    call_site_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[allow(dead_code)] // fields consumed by the byte-parity gate post-§5.0a/b.
struct Entry {
    label: String,
    call_site: String,
    input_source: String,
    lookup_name: Option<String>,
    lookup_from: Option<String>,
    expected: serde_json::Value,
    observed: serde_json::Value,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat_scope_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read compat-scope corpus at {}: {}\n\
             Regenerate with: bun parity-harness/compat-scope/oracle.mjs",
            path.display(),
            e
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("compat-scope corpus has invalid shape: {}", e)
    })
}

#[test]
fn corpus_shape_lock() {
    let corpus = load_corpus();

    assert_eq!(corpus.version, 1, "corpus schema version mismatch");
    assert_eq!(
        corpus.babel_traverse_version, EXPECTED_TRAVERSE_VERSION,
        "@babel/traverse pin drift in corpus — regenerate via \
         `bun parity-harness/compat-scope/oracle.mjs` after \
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
        corpus.entry_count > 0,
        "corpus is empty — fixtures.json or oracle.mjs is broken"
    );

    for axis in EXPECTED_CALL_SITES {
        let count = corpus.call_site_counts.get(*axis).copied().unwrap_or(0);
        assert!(
            count > 0,
            "call_site axis `{axis}` has no entries — fixtures.json \
             must seed at least one fixture per axis. Either add a \
             fixture or remove the axis from EXPECTED_CALL_SITES."
        );
    }

    for entry in &corpus.entries {
        assert!(
            EXPECTED_CALL_SITES.contains(&entry.call_site.as_str()),
            "entry `{}` has unexpected call_site `{}` — update \
             EXPECTED_CALL_SITES + run_observation + oracle.mjs together",
            entry.label,
            entry.call_site
        );
    }
}

#[test]
fn corpus_observed_matches_expected_oracle_self_consistency() {
    // Sanity-checks the oracle-side self-consistency assertion that
    // `parity-harness/compat-scope/oracle.mjs` already enforces:
    // every key in `expected` must equal the corresponding key in
    // `observed`. If this fires, the oracle script silently regressed
    // — re-run it and check why.
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
                panic!("entry `{}`: expected key `{}` missing from observed", entry.label, k)
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
#[ignore = "Phase 5 §5.0a/b not yet ported — compat::scope and compat::path do not exist. \
            Remove this #[ignore] when §5.0a/b lands."]
fn rust_compat_scope_matches_js_corpus() {
    // Wired post-§5.0a/b: parse `entry.input_source` via swc_core,
    // run the pre-indexed scope walker, dispatch on `entry.call_site`
    // to the matching Rust query, and assert the output structurally
    // equals `entry.expected` for every entry.
    //
    // Pseudocode:
    //   for entry in corpus.entries {
    //       let module = parse_module(&entry.input_source);
    //       let scope = compat::scope::ScopeIndex::build(&module);
    //       let observed = match entry.call_site.as_str() {
    //           "binding-lookup-from-reference" => run_lookup(&module, &scope, &entry),
    //           "path-predicate-via-binding"    => run_predicate(&module, &scope, &entry),
    //           "has-own-binding"               => run_has_own_binding(&module, &scope, &entry),
    //           "scope-push-iife"               => run_scope_push_iife(&module, &scope, &entry),
    //           "generate-uid"                  => run_generate_uid(&module, &scope, &entry),
    //           "list-key-arguments"            => run_list_key_arguments(&module, &scope, &entry),
    //           other => panic!("unsupported call_site `{}`", other),
    //       };
    //       assert_observed_matches_expected(&entry, &observed);
    //   }
    panic!("§5.0a/b not yet ported — see file-level docs for the unblock checkpoint.");
}
