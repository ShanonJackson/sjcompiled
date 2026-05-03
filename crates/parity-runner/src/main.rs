//! parity-runner CLI.
//!
//! Usage:
//!   parity-runner --stage <stage-name> --corpus <dir>
//!   parity-runner --stage <stage-name> --corpus <dir> --determinism
//!
//! Default mode loads every `*.css` file under `<dir>` and diffs JS-vs-Rust.
//!
//! `--determinism` mode runs the JS bridge twice over the same inputs and
//! diffs JS-against-JS. This is the Phase 0 oracle-stability check: if the
//! JS pipeline produces different bytes across two runs on the same machine,
//! the JS oracle is non-deterministic and ALL downstream parity work is
//! suspect. Investigate browserslist resolution, env vars, file-system
//! traversal order, etc., until JS is stable byte-for-byte.
//!
//! Exit code 0 = all bytes equal; 1 = at least one divergence; 2 = setup
//! error (bad args, missing corpus, JS bridge failed to spawn).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use parity_runner::{diff_summary, rust_run_stage, JsBridge, Stage};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let stage_name = arg(&args, "--stage").unwrap_or_else(|| {
        usage();
        std::process::exit(2);
    });
    let corpus = arg(&args, "--corpus").unwrap_or_else(|| {
        usage();
        std::process::exit(2);
    });
    let determinism = args.iter().any(|a| a == "--determinism");

    let stage = match stage_name.as_str() {
        "postcss-core-roundtrip" => Stage::PostcssCoreRoundtrip,
        "discard-empty-rules" => Stage::DiscardEmptyRules,
        "discard-duplicates" => Stage::DiscardDuplicates,
        "extract-stylesheets" => Stage::ExtractStylesheets,
        "parent-orphaned-pseudos" => Stage::ParentOrphanedPseudos,
        "increase-specificity" => Stage::IncreaseSpecificity,
        "merge-duplicate-at-rules" => Stage::MergeDuplicateAtRules,
        "normalize-current-color" => Stage::NormalizeCurrentColor,
        "sort-atomic-style-sheet" => Stage::SortAtomicStyleSheet,
        "atomicify-rules" => Stage::AtomicifyRules,
        "expand-shorthands" => Stage::ExpandShorthands,
        "npm-postcss-discard-duplicates" => Stage::NpmPostcssDiscardDuplicates,
        "postcss-nested" => Stage::PostcssNested,
        "postcss-normalize-whitespace" => Stage::PostcssNormalizeWhitespace,
        "postcss-discard-comments" => Stage::PostcssDiscardComments,
        "postcss-normalize-string" => Stage::PostcssNormalizeString,
        "postcss-normalize-positions" => Stage::PostcssNormalizePositions,
        "postcss-normalize-timing-functions" => Stage::PostcssNormalizeTimingFunctions,
        "postcss-normalize-url" => Stage::PostcssNormalizeUrl,
        "postcss-normalize-unicode" => Stage::PostcssNormalizeUnicode,
        "postcss-minify-selectors" => Stage::PostcssMinifySelectors,
        "postcss-minify-params" => Stage::PostcssMinifyParams,
        "postcss-ordered-values" => Stage::PostcssOrderedValues,
        "postcss-reduce-initial" => Stage::PostcssReduceInitial,
        "postcss-colormin" => Stage::PostcssColormin,
        "postcss-minify-gradients" => Stage::PostcssMinifyGradients,
        "postcss-calc" => Stage::PostcssCalc,
        "postcss-convert-values" => Stage::PostcssConvertValues,
        "sort" => Stage::Sort,
        "cssnano-band" => Stage::CssnanoBand,
        "autoprefixer" => Stage::Autoprefixer,
        s => {
            eprintln!("unknown stage: {s}");
            return ExitCode::from(2);
        }
    };

    let corpus_dir = PathBuf::from(&corpus);
    let entries = match collect_entries(&corpus_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("corpus error: {e}");
            return ExitCode::from(2);
        }
    };
    if entries.is_empty() {
        eprintln!("no .css files in corpus dir {}", corpus_dir.display());
        return ExitCode::from(2);
    }

    if determinism {
        run_determinism(stage, &entries)
    } else {
        run_parity(stage, &entries)
    }
}

fn run_parity(stage: Stage, entries: &[(String, String)]) -> ExitCode {
    let mut js = match JsBridge::spawn() {
        Ok(b) => b,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };

    let mut failures = 0usize;
    for (label, css) in entries {
        let js_out = match js.run(stage, css) {
            Ok(r) if r.ok => r.css,
            Ok(r) => { eprintln!("[{label}] JS error: {}", r.error); failures += 1; continue; }
            Err(e) => { eprintln!("[{label}] bridge error: {e}"); failures += 1; continue; }
        };
        let rs_out = match rust_run_stage(stage, css) {
            Ok(s) => s,
            Err(e) => { eprintln!("[{label}] RUST error: {e}"); failures += 1; continue; }
        };
        let d = diff_summary(label, &js_out, &rs_out);
        if !d.equal {
            failures += 1;
            eprintln!("{}", d.summary);
        }
    }

    let _ = js.shutdown();

    if failures == 0 {
        println!("OK — {} inputs, all byte-clean (JS vs Rust)", entries.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("FAIL — {} of {} inputs diverged (JS vs Rust)", failures, entries.len());
        ExitCode::from(1)
    }
}

fn run_determinism(stage: Stage, entries: &[(String, String)]) -> ExitCode {
    // Two independent JS bridge spawns. Different processes — if their
    // outputs diverge, the JS oracle has hidden state (env, fs, cache)
    // bleeding into the answer.
    let mut js_a = match JsBridge::spawn() {
        Ok(b) => b,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };
    let mut js_b = match JsBridge::spawn() {
        Ok(b) => b,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };

    let mut failures = 0usize;
    for (label, css) in entries {
        let a = match js_a.run(stage, css) {
            Ok(r) if r.ok => r.css,
            Ok(r) => { eprintln!("[{label}] JS-A error: {}", r.error); failures += 1; continue; }
            Err(e) => { eprintln!("[{label}] bridge-A error: {e}"); failures += 1; continue; }
        };
        let b = match js_b.run(stage, css) {
            Ok(r) if r.ok => r.css,
            Ok(r) => { eprintln!("[{label}] JS-B error: {}", r.error); failures += 1; continue; }
            Err(e) => { eprintln!("[{label}] bridge-B error: {e}"); failures += 1; continue; }
        };
        let d = diff_summary(label, &a, &b);
        if !d.equal {
            failures += 1;
            eprintln!("JS-vs-JS divergence: {}", d.summary);
        }
    }

    let _ = js_a.shutdown();
    let _ = js_b.shutdown();

    if failures == 0 {
        println!(
            "OK — {} inputs, JS oracle is deterministic across two spawns",
            entries.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "FAIL — {} of {} inputs diverged (JS vs JS); the oracle is NOT stable",
            failures, entries.len()
        );
        ExitCode::from(1)
    }
}

fn usage() {
    eprintln!("Usage:");
    eprintln!("  parity-runner --stage <name> --corpus <dir>");
    eprintln!("  parity-runner --stage <name> --corpus <dir> --determinism");
}

fn arg(args: &[String], name: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == name { return iter.next().cloned(); }
    }
    None
}

fn collect_entries(dir: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("css") { continue; }
        let label = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let css = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        out.push((label, css));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
