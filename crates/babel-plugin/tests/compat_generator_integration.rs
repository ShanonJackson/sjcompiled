//! Phase 4 §4.2 — Rust `compat::generator` parity gate.
//!
//! Reads the JS-locked corpus at `tests/compat_generator_corpus.json`
//! (regenerable via `bun parity-harness/compat-generator/oracle.mjs`)
//! and asserts that `babel_plugin::compat::generator::generate(&swc_node)`
//! produces byte-identical output to upstream `@babel/generator@7.23.0`
//! for every entry.
//!
//! ## Why this gate exists
//!
//! `packages/babel-plugin/src/utils/css-builders.ts:464` calls
//! `hash(generate(expression).code)` to compute keyframe class names.
//! Output bytes from `@babel/generator` feed `compiled-utils::hash`
//! with no prettier downstream — any whitespace, paren, quote, or
//! comment-attachment divergence between SWC and Babel silently
//! renames the class in production.
//!
//! Sites at `:280` (`generate(node).code → variableName`) and `:298`
//! (same shape from VariableDeclarator init) feed `hash(variableName)`
//! at `:639` / `:869` — same strict-byte requirement. Sites at
//! `build-compiled-component.ts:30` (JSX key attribute) and
//! `build-styled-component.ts:133` (conditional className item)
//! emit into source that prettier round-trips, but we lock byte
//! exactness identically — drift today = drift tomorrow.
//!
//! ## Status (Phase 4 §4.2 → §4.3 hand-off)
//!
//! At §4.2 the corpus exists, the version pins are guarded, and the
//! shape contract is locked. The actual byte-parity assertion
//! (`rust_compat_generator_matches_js_corpus`) is `#[ignore]`d
//! because `compat::generator::generate` panics with
//! `unimplemented!()` until §4.3 ports the line-for-line logic.
//!
//! When §4.3 lands the port: remove the `#[ignore]` attribute and
//! the wrapper that catches the panic. The test must be byte-clean
//! across every entry before §4.3 is signed off.
//!
//! ## Env contract
//!
//! No env vars affect this gate (unlike §4.1 which depends on
//! `BROWSERSLIST_CONFIG`). The corpus's `babel_generator_version` /
//! `babel_parser_version` fields are checked against the AFM pin
//! to fail-fast if someone bumped the override without rerunning
//! the oracle.
//!
//! See `plugins/STATUS.md` Phase 4 row §4.2,
//! `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` for the
//! per-call-site coverage rationale, and
//! `parity-harness/compat-generator/{oracle.mjs,fixtures.json}` for
//! the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use swc_core::common::comments::SingleThreadedComments;
use swc_core::common::sync::Lrc;
use swc_core::common::{FileName, SourceMap};
use swc_core::ecma::ast::{EsVersion, Expr, Module};
use swc_core::ecma::parser::{
    parse_file_as_expr, parse_file_as_module, EsSyntax, Syntax, TsSyntax,
};

use babel_plugin::compat::generator::{generate, generate_with_comments};

// AFM-pinned versions; mirror the constants in
// `parity-harness/compat-generator/oracle.mjs` and the row in
// `crates/PARITY_VERSIONS.md`.
const EXPECTED_GENERATOR_VERSION: &str = "7.23.0";
const EXPECTED_PARSER_VERSION: &str = "7.29.2";

// Five call-site axes mirrored from
// `parity-harness/compat-generator/oracle.mjs`'s `EXTRACTORS` table.
// Adding a new axis = changing this list AND adding an extractor
// branch in `extract_swc_node` below AND in oracle.mjs's table.
const EXPECTED_CALL_SITES: &[&str] = &[
    "conditional-classname-item",
    "generic-expression",
    "jsx-key-attribute",
    "keyframes-expression",
    "variable-init",
];

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    babel_generator_version: String,
    babel_parser_version: String,
    entry_count: usize,
    call_site_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    label: String,
    call_site: String,
    input_source: String,
    expected_code: String,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat_generator_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read compat-generator corpus at {}: {}\n\
             Regenerate with: bun parity-harness/compat-generator/oracle.mjs",
            path.display(),
            e
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("compat-generator corpus has invalid shape: {}", e)
    })
}

#[test]
fn corpus_shape_lock() {
    let corpus = load_corpus();

    assert_eq!(corpus.version, 1, "corpus schema version mismatch");

    assert_eq!(
        corpus.babel_generator_version, EXPECTED_GENERATOR_VERSION,
        "@babel/generator pin drift in corpus — regenerate via \
         `bun parity-harness/compat-generator/oracle.mjs` after \
         confirming the pin in package.json#overrides matches \
         crates/PARITY_VERSIONS.md"
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

    // Every declared call_site axis must contribute at least one
    // entry. If a future fixtures.json edit drops all entries for
    // an axis, this gate fires before the byte-parity gate runs.
    for axis in EXPECTED_CALL_SITES {
        let count = corpus.call_site_counts.get(*axis).copied().unwrap_or(0);
        assert!(
            count > 0,
            "call_site axis `{axis}` has no entries — fixtures.json \
             must seed at least one fixture per axis."
        );
    }

    // No entry's call_site falls outside the declared axes.
    for entry in &corpus.entries {
        assert!(
            EXPECTED_CALL_SITES.contains(&entry.call_site.as_str()),
            "entry `{}` has unexpected call_site `{}` — update \
             EXPECTED_CALL_SITES + extract_swc_node + oracle.mjs together",
            entry.label,
            entry.call_site
        );
    }

    // Labels are unique across the corpus (also locked in oracle.mjs;
    // belt-and-braces here in case someone hand-edits the corpus).
    let mut seen = std::collections::HashSet::new();
    for entry in &corpus.entries {
        assert!(
            seen.insert(entry.label.as_str()),
            "duplicate label in corpus: {}",
            entry.label
        );
    }
}

/// Parse `input_source` per `call_site` and return the
/// generator-input subnode as a printable representation. Mirrors
/// the JS oracle's `EXTRACTORS` table — adding a new call_site
/// axis means updating BOTH this dispatch AND the oracle's table.
///
/// Returns the parsed Expr (and, where applicable, the parsed Module
/// it lives inside, kept alive for span resolution).
enum SwcInput {
    /// Bare expression with its captured comment store. The store
    /// owns the leading/trailing comment lookups keyed by `BytePos`;
    /// `compat::generator::generate_with_comments` queries it at
    /// every node boundary.
    Expr(Box<Expr>, SingleThreadedComments),
    /// JSX-key attribute: we walk the parsed Module to find the
    /// `JSXAttribute` node — but `compat::generator::generate`
    /// signature takes `&Expr`, so the attribute case needs a
    /// distinct generate entry point. JSX is the next §4.3 sub-step
    /// (after comments + multi-line objects). For now the byte-parity
    /// test skips this branch.
    JsxAttributeFromModule(Box<Module>, SingleThreadedComments),
}

fn parser_syntax_for_expression() -> Syntax {
    // ES2022 + JSX + TS subset, per the §4.2 hand-off contract. Any
    // input outside this subset belongs to a separate Drift event.
    Syntax::Typescript(TsSyntax {
        tsx: true,
        decorators: false,
        dts: false,
        no_early_errors: false,
        disallow_ambiguous_jsx_like: false,
    })
}

fn parser_syntax_for_program() -> Syntax {
    // Module-level path needs JSX. We use the JS-with-JSX syntax
    // here rather than TS-with-tsx because none of the JSX-key
    // fixtures carry TS annotations, and the JS parser has fewer
    // subtle differences with @babel/parser's default.
    Syntax::Es(EsSyntax {
        jsx: true,
        ..Default::default()
    })
}

fn extract_swc_node(call_site: &str, input_source: &str) -> Result<SwcInput, String> {
    let cm: Lrc<SourceMap> = Default::default();
    // `swc_common@54.0.0`'s `new_source_file(file_name, src)` signature:
    //   - file_name: `Arc<FileName>`
    //   - src:       `impl Into<BytesStr>` — `&str` and `String` both satisfy.
    // We pass `input_source` (a `&str`) directly; the prior `.into()`
    // hop made the impl ambiguous between `From<&str> for BytesStr`
    // and `From<&str> for Bytes`. `FileName::Custom` takes a `String`,
    // so we hand it the `&str` literal converted explicitly.
    let fm = cm.new_source_file(
        Arc::new(FileName::Custom(String::from("compat-generator-fixture.js"))),
        String::from(input_source),
    );
    let comments = SingleThreadedComments::default();

    match call_site {
        "keyframes-expression"
        | "generic-expression"
        | "variable-init"
        | "conditional-classname-item" => {
            let mut errors = Vec::new();
            let expr = parse_file_as_expr(
                &fm,
                parser_syntax_for_expression(),
                EsVersion::Es2022,
                Some(&comments),
                &mut errors,
            )
            .map_err(|e| format!("SWC parse_file_as_expr failed: {:?}", e))?;
            if !errors.is_empty() {
                return Err(format!("SWC parse errors: {:?}", errors));
            }
            Ok(SwcInput::Expr(expr, comments))
        }
        "jsx-key-attribute" => {
            let mut errors = Vec::new();
            let module = parse_file_as_module(
                &fm,
                parser_syntax_for_program(),
                EsVersion::Es2022,
                Some(&comments),
                &mut errors,
            )
            .map_err(|e| format!("SWC parse_file_as_module failed: {:?}", e))?;
            if !errors.is_empty() {
                return Err(format!("SWC parse errors: {:?}", errors));
            }
            Ok(SwcInput::JsxAttributeFromModule(Box::new(module), comments))
        }
        other => Err(format!("unknown call_site: {}", other)),
    }
}

#[test]
fn corpus_input_sources_parse_under_swc() {
    // §4.2 sanity gate — independent of `compat::generator`. Ensures
    // every fixture's `input_source` is parseable by SWC at the same
    // syntax surface @babel/parser uses on the oracle side. If a
    // future fixture exercises a syntax SWC@54.0.0 doesn't accept
    // (e.g., a stage-3 proposal), this gate fires before the parity
    // gate would dereference an Err.
    let corpus = load_corpus();
    let mut failures = Vec::new();
    for entry in &corpus.entries {
        if let Err(e) = extract_swc_node(&entry.call_site, &entry.input_source) {
            failures.push(format!("{} ({}): {}", entry.label, entry.call_site, e));
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) failed to parse under SWC:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn rust_compat_generator_matches_js_corpus() {
    let corpus = load_corpus();
    let mut divergences = Vec::new();

    for entry in &corpus.entries {
        let parsed = match extract_swc_node(&entry.call_site, &entry.input_source) {
            Ok(p) => p,
            Err(e) => {
                divergences.push(format!(
                    "{} ({}): SWC parse failed: {}",
                    entry.label, entry.call_site, e
                ));
                continue;
            }
        };

        // §4.3-in-progress: JSX-key-attribute path needs a generate
        // entry point that takes a JSXAttribute (or extends `generate`
        // to dispatch on more node kinds). For now we only exercise
        // the Expr cases; JSX-key fixtures will be brought online
        // when JSX printers land (next sub-step).
        let actual = match parsed {
            SwcInput::Expr(expr, comments) => generate_with_comments(&expr, &comments),
            SwcInput::JsxAttributeFromModule(_, _) => continue,
        };

        if actual != entry.expected_code {
            divergences.push(format!(
                "{} ({}):\n  input:    {:?}\n  expected: {:?}\n  actual:   {:?}",
                entry.label, entry.call_site, entry.input_source, entry.expected_code, actual
            ));
        }
    }

    assert!(
        divergences.is_empty(),
        "{} compat-generator parity divergence(s):\n{}",
        divergences.len(),
        divergences.join("\n\n")
    );
}
