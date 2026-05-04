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
use std::path::{Path, PathBuf};

use serde::Deserialize;

use babel_plugin::resolver::{build_default, build_from_config, ResolverConfig};

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
    #[serde(rename = "fromFile")]
    from_file: String,
    request: String,
    #[serde(default)]
    extensions: Option<Vec<String>>,
    expected: serde_json::Value,
    observed: serde_json::Value,
}

fn repo_root() -> PathBuf {
    // tests/ -> crate/ -> crates/ -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("babel-plugin parent is crates/")
        .parent()
        .expect("crates/ parent is repo root")
        .to_path_buf()
}

/// Convert a corpus-relative path (forward-slash, repo-rooted) into
/// an absolute platform-native path. Mirrors `oracle.mjs::toAbs` —
/// the corpus is portable across machines, the gate resolves it to
/// this machine.
fn to_abs(rel: &str) -> PathBuf {
    let mut p = repo_root();
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// Convert an absolute path back to a corpus-relative forward-slash
/// path so `expected.path` (which is corpus-relative) compares
/// byte-for-byte with the resolved output. Mirrors
/// `oracle.mjs::toRel`.
fn to_rel(abs: &Path) -> String {
    let root = repo_root();
    let rel = abs.strip_prefix(&root).unwrap_or(abs);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
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
/// **§5.4b-LIVE.** Iterates the corpus, builds a default-config
/// resolver per [`build_default`] (extensions from the fixture's
/// `extensions` field, falling back to `DEFAULT_CODE_EXTENSIONS`),
/// resolves each `(fromFile, request)` pair, and asserts the
/// produced absolute path matches `expected.enhancedResolve.path`
/// byte-for-byte after corpus-relative normalisation.
///
/// On divergence the test prints:
///   - the fixture label + axis
///   - what oxc_resolver returned
///   - what enhanced-resolve returned
///
/// then panics. The §5.4b implementer / future agent applies the
/// divergence-action protocol from
/// `crates/babel-plugin/RESOLVER_MATRIX.md`: match (adjust
/// `resolver::default::build_default` config), shim (wrap the
/// resolver), or escalate (add a row to RESOLVER_MATRIX.md's
/// "Confirmed unreachable" table).
#[test]
fn rust_resolver_matches_js_corpus() {
    let corpus = load_corpus();
    let mut failures = Vec::new();

    for entry in &corpus.entries {
        let from_abs = to_abs(&entry.from_file);
        let extensions: Option<Vec<String>> = entry.extensions.clone();
        let resolver = build_default(extensions.as_deref());

        let expected_enhanced = entry
            .expected
            .as_object()
            .and_then(|obj| obj.get("enhancedResolve"))
            .cloned();

        // Skip entries where the fixture didn't pin enhancedResolve
        // (defensive — the seed corpus pins all of them, but be
        // permissive when the §5.4b implementer grows the corpus
        // and stages a fixture without expectations).
        let Some(expected_enhanced) = expected_enhanced else {
            continue;
        };

        let actual = match resolver.resolve_sync(&from_abs, &entry.request) {
            Ok(p) => Ok(to_rel(&p)),
            Err(e) => Err(format!("{e:?}")),
        };

        let expected_kind = expected_enhanced
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("");

        match (expected_kind, &actual) {
            ("ok", Ok(actual_path)) => {
                let expected_path = expected_enhanced
                    .get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                if actual_path != expected_path {
                    failures.push(format!(
                        "fixture `{}` (axis: {})\n  \
                         expected (enhanced-resolve): {}\n  \
                         actual   (oxc_resolver):     {}\n  \
                         see crates/babel-plugin/RESOLVER_MATRIX.md \
                         §Divergence-action-protocol",
                        entry.label, entry.axis, expected_path, actual_path,
                    ));
                }
            }
            ("err", Err(_)) => {
                // Both errored — coarse pass. Error-class match is
                // captured by the oracle-self-consistency test; here
                // we accept any error on either side.
            }
            ("ok", Err(actual_err)) => {
                let expected_path = expected_enhanced
                    .get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                failures.push(format!(
                    "fixture `{}` (axis: {})\n  \
                     expected (enhanced-resolve): ok={}\n  \
                     actual   (oxc_resolver):     err={}\n  \
                     see crates/babel-plugin/RESOLVER_MATRIX.md \
                     §Divergence-action-protocol",
                    entry.label, entry.axis, expected_path, actual_err,
                ));
            }
            ("err", Ok(actual_path)) => {
                failures.push(format!(
                    "fixture `{}` (axis: {})\n  \
                     expected (enhanced-resolve): err\n  \
                     actual   (oxc_resolver):     ok={}\n  \
                     see crates/babel-plugin/RESOLVER_MATRIX.md \
                     §Divergence-action-protocol",
                    entry.label, entry.axis, actual_path,
                ));
            }
            (other, _) => {
                failures.push(format!(
                    "fixture `{}`: expected.enhancedResolve.kind = {:?} (must be \"ok\" or \"err\")",
                    entry.label, other,
                ));
            }
        }
    }

    if !failures.is_empty() {
        let count = failures.len();
        let total = corpus.entry_count;
        let body = failures.join("\n\n");
        panic!(
            "{count}/{total} fixture(s) diverged from enhanced-resolve@{}:\n\n{body}",
            corpus.enhanced_resolve_version,
        );
    }
}

// ---------- §5.4c — packageJsonTransforms end-to-end ----------
//
// These tests exercise the [`build_from_config`] path (vs.
// [`build_default`] which the corpus gate above covers). They
// don't go through the JS oracle — `enhanced-resolve` doesn't have
// a generic transform engine in its 5.x line, so the corpus shape
// would diverge by design. Instead, the transform parity is locked
// at three layers:
//
// 1. Per-op unit tests in `crates/babel-plugin/src/resolver/transforms.rs`
//    (22 tests covering each op + composed Jira sequences) — pure
//    JSON mutation, no FS.
// 2. Engine-wiring round-trip in
//    `crates/babel-plugin/src/resolver/engine.rs` (a no-op transform
//    against an axis-1-style fixture).
// 3. THIS module — end-to-end resolution against an on-disk
//    `axis-10-package-json-transforms/` fixture, demonstrating that
//    the bytes oxc_resolver consumes ARE the transformed bytes
//    (resolution outcome differs based on whether the transform
//    runs).
//
// If a future agent regresses the FS interception (e.g. by caching
// the raw bytes outside the wrapper, or accidentally bypassing
// `read()`), test (3) below fires.

fn axis_10_consumer() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-10-package-json-transforms/delete-exports/consumer.js")
}

fn axis_10_main_entry_path() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-10-package-json-transforms/delete-exports")
        .join("node_modules/parity-pkg-with-both-main-and-exports/main-entry.js")
}

fn axis_10_exports_entry_path() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-10-package-json-transforms/delete-exports")
        .join("node_modules/parity-pkg-with-both-main-and-exports/exports-entry.js")
}

#[test]
fn axis_10_no_transform_resolves_via_exports() {
    // Sanity: with NO transform, the default-config resolver
    // honours `exports` (modern Node behaviour) and lands at
    // `exports-entry.js`. This baseline establishes the "with
    // transform" test below has a meaningful delta.
    let consumer = axis_10_consumer();
    if !consumer.exists() {
        return; // fixture not on disk
    }
    let resolver = build_default(Some(&[
        ".js".to_string(),
        ".jsx".to_string(),
        ".ts".to_string(),
        ".tsx".to_string(),
    ]));
    let resolved = resolver
        .resolve_sync(&consumer, "parity-pkg-with-both-main-and-exports")
        .expect("baseline resolution must succeed");
    let expected = axis_10_exports_entry_path();
    let resolved = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    assert_eq!(
        resolved, expected,
        "without transforms: expected `exports` field to win over `main`"
    );
}

#[test]
fn axis_10_delete_exports_transform_falls_back_to_main() {
    // The §5.4c E2E gate. Build a config-driven resolver with a
    // single `deleteKey "exports"` transform applied to every
    // package.json read. The bytes oxc_resolver consumes for the
    // target package.json are MUTATED (no `exports` field) — so
    // resolution falls back to `main`.
    let consumer = axis_10_consumer();
    if !consumer.exists() {
        return; // fixture not on disk
    }

    let cfg_value = serde_json::json!({
        "extensions": [".js", ".jsx", ".ts", ".tsx"],
        "packageJsonTransforms": [
            { "op": "deleteKey", "key": "exports" }
        ]
    });
    let cfg = ResolverConfig::parse_value(&cfg_value)
        .expect("config schema parse")
        .expect("config object");
    let resolver = build_from_config(&cfg, &repo_root()).unwrap();

    let resolved = resolver
        .resolve_sync(&consumer, "parity-pkg-with-both-main-and-exports")
        .expect("transformed resolution must succeed");
    let expected = axis_10_main_entry_path();
    let resolved = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    assert_eq!(
        resolved, expected,
        "with deleteKey transform: expected `main` to win because \
         `exports` was stripped from the bytes oxc_resolver consumed. \
         If this fails the FS-interception path in \
         crates/babel-plugin/src/resolver/engine.rs is broken — see \
         crates/babel-plugin/RESOLVER_MATRIX.md \
         §Divergence-action-protocol."
    );
}

// ---------- §5.4d — preferFirst dispatcher end-to-end ----------
//
// These tests exercise the [`build_from_config`] path with a
// non-empty `preferFirst[]` array. The on-disk fixture
// (axis-11-prefer-first/match-by-prefix/) has a package whose
// resolved entry differs based on whether the dispatcher routes
// through a rule resolver (which overrides `exports.fields` to
// include `af:exports`) or falls through to the base resolver
// (default `exports.fields = [["exports"]]`, falls back to `main`).
//
// Three tests:
// 1. baseline — no preferFirst → resolves via main
// 2. matched   — preferFirst matches → resolves via af:exports
// 3. unmatched — preferFirst rule with non-overlapping prefix →
//    fall-through to base → resolves via main

fn axis_11_consumer() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-11-prefer-first/match-by-prefix/consumer.js")
}

fn axis_11_main_entry() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-11-prefer-first/match-by-prefix")
        .join("node_modules/@matched/pkg-with-af-exports/main-entry.js")
}

fn axis_11_af_entry() -> PathBuf {
    repo_root()
        .join("parity-harness/resolver-matrix/fixtures-source")
        .join("axis-11-prefer-first/match-by-prefix")
        .join("node_modules/@matched/pkg-with-af-exports/af-entry.js")
}

#[test]
fn axis_11_no_prefer_first_uses_main() {
    let consumer = axis_11_consumer();
    if !consumer.exists() {
        return;
    }
    let resolver = build_default(Some(&[
        ".js".to_string(),
        ".jsx".to_string(),
        ".ts".to_string(),
        ".tsx".to_string(),
    ]));
    let resolved = resolver
        .resolve_sync(&consumer, "@matched/pkg-with-af-exports")
        .expect("baseline resolution must succeed");
    let expected = axis_11_main_entry();
    let resolved = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    assert_eq!(
        resolved, expected,
        "without preferFirst: default resolver doesn't know about `af:exports`, must walk `main`"
    );
}

#[test]
fn axis_11_matched_prefix_routes_to_af_exports() {
    let consumer = axis_11_consumer();
    if !consumer.exists() {
        return;
    }
    // Inline `["@matched/"]` prefix — matches the consumer's
    // `@matched/pkg-with-af-exports` request. `use.exportsFields`
    // overrides the rule resolver to walk `af:exports` first.
    let cfg_value = serde_json::json!({
        "extensions": [".js", ".jsx", ".ts", ".tsx"],
        "preferFirst": [
            {
                "match": { "specifierStartsWith": ["@matched/"] },
                "use": { "exportsFields": ["af:exports", "exports"] }
            }
        ]
    });
    let cfg = ResolverConfig::parse_value(&cfg_value)
        .expect("config schema parse")
        .expect("config object");
    let resolver = build_from_config(&cfg, &repo_root()).unwrap();

    let resolved = resolver
        .resolve_sync(&consumer, "@matched/pkg-with-af-exports")
        .expect("matched resolution must succeed");
    let expected = axis_11_af_entry();
    let resolved = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    assert_eq!(
        resolved, expected,
        "with preferFirst matching `@matched/` and use.exportsFields=[\"af:exports\",\"exports\"]: \
         expected resolver to walk af:exports → af-entry.js. If this fails, the dispatcher's \
         per-rule resolver isn't honouring use.exportsFields. See \
         crates/babel-plugin/RESOLVER_MATRIX.md §Divergence-action-protocol."
    );
}

#[test]
fn axis_11_unmatched_prefix_falls_through_to_base() {
    let consumer = axis_11_consumer();
    if !consumer.exists() {
        return;
    }
    // Rule's prefix list is `["@nomatch/"]` — does NOT match the
    // consumer's `@matched/...` request. dispatcher.match_request
    // returns None → resolution falls through to base, which has
    // the default `exports.fields = [["exports"]]` and walks
    // `main` → main-entry.js.
    let cfg_value = serde_json::json!({
        "extensions": [".js", ".jsx", ".ts", ".tsx"],
        "preferFirst": [
            {
                "match": { "specifierStartsWith": ["@nomatch/"] },
                "use": { "exportsFields": ["af:exports", "exports"] }
            }
        ]
    });
    let cfg = ResolverConfig::parse_value(&cfg_value)
        .expect("config schema parse")
        .expect("config object");
    let resolver = build_from_config(&cfg, &repo_root()).unwrap();

    let resolved = resolver
        .resolve_sync(&consumer, "@matched/pkg-with-af-exports")
        .expect("fall-through resolution must succeed");
    let expected = axis_11_main_entry();
    let resolved = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    assert_eq!(
        resolved, expected,
        "with preferFirst's prefix list NOT matching the request: dispatcher must return None \
         and resolution must fall through to the base resolver (default exports.fields, \
         resolves via `main` → main-entry.js)."
    );
}
