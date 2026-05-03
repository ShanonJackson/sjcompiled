//! Phase 3 §3.3 — Rust hash parity gate.
//!
//! Reads the JS-locked corpus at `tests/hash_corpus.json` (regenerable
//! via `bun parity-harness/hash/oracle.mjs`) and asserts that
//! `compiled_utils::hash(&input)` is byte-equal to the upstream JS
//! `@compiled/utils.hash(input)` for every entry.
//!
//! The corpus contains:
//!   - 4 real-call-shape entries (one per `hash()` call site in
//!     `packages/babel-plugin/src/`).
//!   - ~30 categorical entries (ASCII boundaries, embedded NUL,
//!     UTF-8 multibyte, surrogate pairs, >4 KiB, length-tail
//!     coverage for every `(l mod 4)` branch in murmur2).
//!   - 10000 random entries (5000 ASCII + 5000 full-Unicode) from
//!     a deterministic mulberry32 stream seeded at 1.
//!
//! See `plugins/STATUS.md` Phase 3 row §3.3.

use std::fs;
use std::path::PathBuf;

use compiled_utils::hash;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    label: String,
    input: String,
    expected_hash: String,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("hash_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {} — regenerate with `bun parity-harness/hash/oracle.mjs`",
            path.display(),
            e
        )
    });
    let corpus: Corpus = serde_json::from_str(&raw).expect("hash_corpus.json malformed");
    assert_eq!(corpus.version, 1, "hash_corpus.json version drifted");
    corpus
}

#[test]
fn rust_hash_matches_js_corpus() {
    let corpus = load_corpus();
    assert!(
        corpus.entries.len() >= 30,
        "expected ≥30 entries (Phase 3 §3.2), got {}",
        corpus.entries.len()
    );

    let mut mismatches: Vec<String> = Vec::new();
    for entry in &corpus.entries {
        let actual = hash(&entry.input);
        if actual != entry.expected_hash {
            mismatches.push(format!(
                "label={:?} input={:?} expected={:?} actual={:?}",
                entry.label, entry.input, entry.expected_hash, actual
            ));
            // Cap the report so a regression doesn't dump 10K lines.
            if mismatches.len() >= 20 {
                break;
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "hash parity divergence ({} of {} entries; first {} shown):\n{}",
        mismatches.len(),
        corpus.entries.len(),
        mismatches.len().min(20),
        mismatches.join("\n")
    );
}

#[test]
fn corpus_includes_real_call_shapes() {
    // Lock that the four real-call-shape entries from the consuming
    // Babel plugin are in the corpus. If `oracle.mjs` is edited to
    // remove them, this test catches it before §3.4 closes.
    let corpus = load_corpus();
    let labels: Vec<&str> = corpus.entries.iter().map(|e| e.label.as_str()).collect();
    for required in [
        "real: keyframes generate().code",
        "real: variableName identifier",
        "real: atomicify composite key",
        "real: css value",
    ] {
        assert!(
            labels.iter().any(|l| *l == required),
            "missing real-call-shape entry {:?}",
            required
        );
    }
}

#[test]
fn corpus_covers_phase3_categories() {
    // Phase 3 §3.2 acceptance: ASCII, UTF-8 multibyte, empty, embedded
    // NUL, >4 KiB, leading/trailing whitespace.
    let corpus = load_corpus();
    let inputs: Vec<&str> = corpus.entries.iter().map(|e| e.input.as_str()).collect();

    assert!(inputs.iter().any(|s| s.is_empty()), "empty string missing");
    assert!(
        inputs.iter().any(|s| s.contains('\u{0}')),
        "embedded NUL missing"
    );
    assert!(
        inputs.iter().any(|s| s.len() > 4096),
        ">4 KiB string missing"
    );
    assert!(
        inputs.iter().any(|s| s.starts_with(' ')),
        "leading whitespace missing"
    );
    assert!(
        inputs.iter().any(|s| s.ends_with(' ')),
        "trailing whitespace missing"
    );
    // UTF-8 multibyte: any char above U+007F.
    assert!(
        inputs.iter().any(|s| s.chars().any(|c| c as u32 > 0x7f)),
        "UTF-8 multibyte missing"
    );
    // Astral plane (4-byte UTF-8 / surrogate pair in JS).
    assert!(
        inputs.iter().any(|s| s.chars().any(|c| c as u32 > 0xffff)),
        "astral-plane codepoint missing"
    );
}

#[test]
fn corpus_has_at_least_10k_random_entries() {
    // §3.3 acceptance: corpus + 10K random inputs.
    let corpus = load_corpus();
    let random_count = corpus
        .entries
        .iter()
        .filter(|e| e.label.starts_with("random-"))
        .count();
    assert!(
        random_count >= 10_000,
        "expected ≥10000 random entries, got {}",
        random_count
    );
}
