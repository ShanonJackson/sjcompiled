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
//! ## Status (Phase 5 §5.0c — POST-PORT)
//!
//! `compat::evaluation` is ported. The parity gate
//! `rust_compat_evaluation_matches_js_corpus` runs the 45-entry
//! corpus through the Rust evaluator and asserts byte-equal
//! `(value_kind, value_string, confident)` triples against the JS
//! oracle. The `corpus_shape_lock` and oracle-self-consistency
//! tests still run unconditionally so pin drift fails fast.
//!
//! See `plugins/STATUS.md` Phase 5 row §5.0,
//! `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` for the
//! reachable/unreachable branch survey, and
//! `parity-harness/compat-evaluation/{oracle.mjs,fixtures.json}` for
//! the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

use babel_plugin::compat::evaluation::{evaluate, EvaluatedValue, Value};
use babel_plugin::compat::scope::ScopeIndex;
use swc_core::common::sync::Lrc;
use swc_core::common::{FileName, SourceMap};
use swc_core::ecma::ast::{Decl, EsVersion, Module, ModuleItem, Pat, Stmt};
use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

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

/// Parse a `const __evalTarget = (EXPR);` snippet, build a
/// `ScopeIndex`, return `(Module, init_expr_index)` so callers can
/// evaluate the init expr against the program scope.
///
/// Mirrors the JS oracle's wrapper at
/// `parity-harness/compat-evaluation/oracle.mjs::evaluateExpression`.
fn parse_eval_target(source: &str) -> Module {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Arc::new(FileName::Custom("compat-evaluation-corpus.ts".into())),
        format!("const __evalTarget = ({source});"),
    );
    let mut errors = Vec::new();
    let module = parse_file_as_module(
        &fm,
        // TS syntax — the corpus has TSAsExpression fixtures.
        Syntax::Typescript(TsSyntax {
            tsx: false,
            decorators: false,
            no_early_errors: true,
            disallow_ambiguous_jsx_like: false,
            dts: false,
        }),
        EsVersion::Es2022,
        None,
        &mut errors,
    )
    .expect("parse");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    module
}

/// Encode an [`EvaluatedValue`] as the same `{confident, value_kind,
/// value_string}` triple the oracle emits. Mirrors
/// `oracle.mjs::valueKind` / `valueString`.
fn encode(v: &EvaluatedValue) -> (bool, &'static str, String) {
    match v {
        EvaluatedValue::Deopt => (false, "undefined", "undefined".to_string()),
        EvaluatedValue::Confident(value) => match value {
            Value::Undefined => (true, "undefined", "undefined".to_string()),
            Value::Null => (true, "object", "null".to_string()),
            Value::Bool(b) => (
                true,
                "boolean",
                if *b { "true" } else { "false" }.to_string(),
            ),
            Value::Number(n) => {
                // JSON.stringify quirk: NaN/Infinity stringify to
                // "null". Mirror.
                let s = if n.is_nan() || n.is_infinite() {
                    "null".to_string()
                } else if n.fract() == 0.0 && n.abs() < 1e21 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                };
                (true, "number", s)
            }
            Value::String(s) => (true, "string", json_stringify_string(s)),
            // Object/Array fold paths — corpus has no fixtures for
            // these today; encode defensively as "object" with a
            // best-effort JSON form. If a future fixture surfaces,
            // tighten this to match `JSON.stringify`'s exact output.
            Value::Array(_) | Value::Object(_) => (true, "object", "null".to_string()),
        },
    }
}

/// Minimal JSON-string encoder matching `JSON.stringify(s)` for the
/// shapes the corpus exercises (ASCII printable + standard escapes).
fn json_stringify_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[test]
fn rust_compat_evaluation_matches_js_corpus() {
    let corpus = load_corpus();
    let mut failures: Vec<String> = Vec::new();

    for entry in &corpus.entries {
        let module = parse_eval_target(&entry.input_source);
        let idx = ScopeIndex::build(&module);

        // Reach the `__evalTarget` declarator's init expression.
        let init_expr = module
            .body
            .iter()
            .find_map(|item| {
                let ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) = item else {
                    return None;
                };
                v.decls.iter().find_map(|d| {
                    let Pat::Ident(b) = &d.name else { return None };
                    if b.id.sym.as_str() == "__evalTarget" {
                        d.init.as_deref()
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| panic!("__evalTarget init not found for `{}`", entry.label));

        let observed = evaluate(init_expr, &idx, idx.program_scope());
        let (confident, value_kind, value_string) = encode(&observed);

        let expected = entry.expected.as_object().unwrap_or_else(|| {
            panic!("entry `{}` expected is not an object", entry.label)
        });
        let exp_confident = expected
            .get("confident")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| panic!("entry `{}` missing confident", entry.label));
        let exp_value_kind = expected
            .get("value_kind")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("entry `{}` missing value_kind", entry.label));
        let exp_value_string = expected
            .get("value_string")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("entry `{}` missing value_string", entry.label));

        if confident != exp_confident
            || value_kind != exp_value_kind
            || value_string != exp_value_string
        {
            failures.push(format!(
                "  {} ({}): expected ({}, {}, {}), got ({}, {}, {})",
                entry.label,
                entry.category,
                exp_confident,
                exp_value_kind,
                exp_value_string,
                confident,
                value_kind,
                value_string
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "§5.0c parity gate: {} of {} fixtures diverged from \
             @babel/traverse@7.29.0:\n{}",
            failures.len(),
            corpus.entries.len(),
            failures.join("\n")
        );
    }
}
