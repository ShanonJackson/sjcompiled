//! Per-phase profile harness for `transform_css`. Replicates the
//! pipeline stage-by-stage with `Instant` markers so we can attribute
//! cost to specific plugins. Matches `scripts/perf-test.ts` config
//! (optimizeCss=false, sortAtRules=true, sortShorthand=true) and uses
//! the precomputed prefix snapshot (production WASI mode).
//!
//! Run with:
//!   cargo run --profile bench-fast --example profile_phases -p css

use std::time::Instant;

use postcss_core::parse;

use css::plugins::compiled_css::plugins::atomicify_rules::{atomicify_rules, AtomicifyRulesOpts};
use css::plugins::compiled_css::plugins::discard_duplicates::discard_duplicates;
use css::plugins::compiled_css::plugins::extract_stylesheets::{extract_stylesheets, ExtractStyleSheetsOpts};
use css::plugins::compiled_css::plugins::parent_orphaned_pseudos::parent_orphaned_pseudos;
use css::plugins::compiled_css::plugins::sort_atomic_style_sheet::{
    sort_atomic_style_sheet, SortAtomicStyleSheetOpts,
};

use css::plugins::cssnano_preset_default::{default_preset, PresetOpts};

use css::plugins::postcss_nested::{postcss_nested, PostcssNestedOpts};
use css::plugins::postcss_normalize_whitespace::postcss_normalize_whitespace;

use autoprefixer::autoprefixer::AutoprefixerOptions;
use autoprefixer::precomputed::{
    build_prefixes_from_precomputed, encode_precomputed, precompute_prefixes,
};
use autoprefixer::processor::Processor as AutoprefixerProcessor;

use css::transform::interleaved_decl_walk;
use css::{transform_css, TransformOpts};

const SAMPLE_CSS: &str = r#"
  display: flex;
  flex-direction: column;
  align-items: center;
  user-select: none;
  color: hotpink;
  background: linear-gradient(to right, red, blue);
  transition: transform 0.2s ease-in-out;

  &:hover {
    color: rebeccapurple;
    transform: scale(1.05);
  }

  &:focus-visible {
    outline: 2px solid currentColor;
  }

  @media (max-width: 600px) {
    flex-direction: row;
    padding: 8px;
  }

  > .child {
    margin-bottom: 1rem;

    &:last-child {
      margin-bottom: 0;
    }
  }
"#;

const WARMUP: u32 = 500;
const ITERS: u32 = 10_000;

fn main() {
    println!("transform_css per-phase profile");
    println!("================================");
    println!("input bytes : {}", SAMPLE_CSS.len());
    println!("iters       : {}", ITERS);
    println!("config      : optimizeCss=false (matches scripts/perf-test.ts)");
    println!();

    let snapshot = precompute_prefixes(AutoprefixerOptions::default());
    let prefix_bytes = encode_precomputed(&snapshot);
    println!("precomputed prefix bytes: {}\n", prefix_bytes.len());

    let warm_opts = TransformOpts {
        optimize_css: Some(false),
        sort_at_rules: Some(true),
        sort_shorthand: Some(true),
        increase_specificity: Some(false),
        precomputed_prefixes: Some(prefix_bytes.clone()),
        ..Default::default()
    };
    for _ in 0..WARMUP {
        let _ = transform_css(SAMPLE_CSS, &warm_opts).expect("warmup transform_css");
    }

    let preset_opts = PresetOpts {
        browserslist_snapshot: None,
    };
    let preset = default_preset(&preset_opts);

    let plugins_to_include: std::collections::HashSet<&str> =
        ["postcss-minify-selectors", "postcss-minify-params"]
            .into_iter()
            .collect();

    let nested_opts = PostcssNestedOpts {
        bubble: vec![
            "container".to_string(),
            "-moz-document".to_string(),
            "layer".to_string(),
            "else".to_string(),
            "when".to_string(),
            "starting-style".to_string(),
        ],
        unwrap: vec![
            "color-profile".to_string(),
            "counter-style".to_string(),
            "font-palette-values".to_string(),
            "page".to_string(),
            "property".to_string(),
        ],
        preserve_empty: false,
    };
    let sort_opts = SortAtomicStyleSheetOpts {
        sort_at_rules_enabled: Some(true),
        sort_shorthand_enabled: Some(true),
    };

    let mut p_parse = 0u128;
    let mut p_discard_dups = 0u128;
    let mut p_parent_orphan = 0u128;
    let mut p_sort_atomic = 0u128;
    let mut p_postcss_nested = 0u128;
    let mut p_decl_walk = 0u128;
    let mut p_cssnano = 0u128;
    let mut p_atomicify = 0u128;
    let mut p_ap_build = 0u128;
    let mut p_ap_remove = 0u128;
    let mut p_ap_add = 0u128;
    let mut p_whitespace = 0u128;
    let mut p_extract = 0u128;

    let mut cssnano_by_plugin: std::collections::BTreeMap<&'static str, u128> =
        std::collections::BTreeMap::new();

    let t_total = Instant::now();
    for _ in 0..ITERS {
        let t = Instant::now();
        let mut root = parse(SAMPLE_CSS).expect("parse");
        p_parse += t.elapsed().as_nanos();

        let t = Instant::now();
        discard_duplicates(&mut root).expect("discard-duplicates");
        p_discard_dups += t.elapsed().as_nanos();

        let t = Instant::now();
        parent_orphaned_pseudos(&mut root).expect("parent-orphaned-pseudos");
        p_parent_orphan += t.elapsed().as_nanos();

        let t = Instant::now();
        sort_atomic_style_sheet(&mut root, &sort_opts).expect("sort-atomic-style-sheet");
        p_sort_atomic += t.elapsed().as_nanos();

        let t = Instant::now();
        postcss_nested(&mut root, &nested_opts).expect("postcss-nested");
        p_postcss_nested += t.elapsed().as_nanos();

        let t = Instant::now();
        let _ = interleaved_decl_walk(&mut root.root, false);
        p_decl_walk += t.elapsed().as_nanos();

        let t_cssnano = Instant::now();
        for entry in &preset.plugins {
            if plugins_to_include.contains(entry.name) {
                let t_plugin = Instant::now();
                (entry.apply)(&mut root, &preset_opts).expect("cssnano plugin");
                *cssnano_by_plugin.entry(entry.name).or_insert(0) +=
                    t_plugin.elapsed().as_nanos();
            }
        }
        p_cssnano += t_cssnano.elapsed().as_nanos();

        let t = Instant::now();
        let mut atomicify_opts = AtomicifyRulesOpts {
            class_name_compression_map: None,
            class_hash_prefix: None,
            class_names: Vec::new(),
        };
        atomicify_rules(&mut root, &mut atomicify_opts).expect("atomicify-rules");
        p_atomicify += t.elapsed().as_nanos();
        std::hint::black_box(&atomicify_opts.class_names);

        let t = Instant::now();
        let prefixes = build_prefixes_from_precomputed(&prefix_bytes).expect("ap build");
        p_ap_build += t.elapsed().as_nanos();

        let proc = AutoprefixerProcessor::new(&prefixes);
        let mut warnings: Vec<String> = Vec::new();

        let t = Instant::now();
        proc.remove(&mut root.root, &mut warnings);
        p_ap_remove += t.elapsed().as_nanos();

        let t = Instant::now();
        proc.add(&mut root.root, &mut warnings);
        p_ap_add += t.elapsed().as_nanos();

        let t = Instant::now();
        postcss_normalize_whitespace(&mut root).expect("normalize-whitespace");
        p_whitespace += t.elapsed().as_nanos();

        let t = Instant::now();
        let mut extract_opts = ExtractStyleSheetsOpts {
            sheets: Vec::new(),
        };
        extract_stylesheets(&root, &mut extract_opts).expect("extract-stylesheets");
        p_extract += t.elapsed().as_nanos();
        std::hint::black_box(&extract_opts.sheets);
    }
    let total = t_total.elapsed();

    let n = ITERS as f64;
    let to_us = |ns: u128| ns as f64 / n / 1000.0;
    let total_phase_ns = p_parse
        + p_discard_dups
        + p_parent_orphan
        + p_sort_atomic
        + p_postcss_nested
        + p_decl_walk
        + p_cssnano
        + p_atomicify
        + p_ap_build
        + p_ap_remove
        + p_ap_add
        + p_whitespace
        + p_extract;
    let pct = |ns: u128| 100.0 * ns as f64 / total_phase_ns as f64;

    println!(
        "wallclock total : {:.2} s",
        total.as_secs_f64()
    );
    println!(
        "avg per iter    : {:.2} µs",
        total.as_secs_f64() * 1e6 / n
    );
    println!(
        "sum of phases   : {:.2} µs/iter  (residual = scheduling/Instant overhead)",
        to_us(total_phase_ns)
    );
    println!();
    println!("Phase                                 µs/call    % of phases");
    println!("------------------------------------ ---------  ------------");
    println!(
        "parse                                {:>9.3}  {:>10.2}%",
        to_us(p_parse),
        pct(p_parse)
    );
    println!(
        "discard-duplicates                   {:>9.3}  {:>10.2}%",
        to_us(p_discard_dups),
        pct(p_discard_dups)
    );
    println!(
        "parent-orphaned-pseudos              {:>9.3}  {:>10.2}%",
        to_us(p_parent_orphan),
        pct(p_parent_orphan)
    );
    println!(
        "sort-atomic-style-sheet              {:>9.3}  {:>10.2}%",
        to_us(p_sort_atomic),
        pct(p_sort_atomic)
    );
    println!(
        "postcss-nested                       {:>9.3}  {:>10.2}%",
        to_us(p_postcss_nested),
        pct(p_postcss_nested)
    );
    println!(
        "interleaved-decl-walk                {:>9.3}  {:>10.2}%",
        to_us(p_decl_walk),
        pct(p_decl_walk)
    );
    println!(
        "cssnano (BASE only, total)           {:>9.3}  {:>10.2}%",
        to_us(p_cssnano),
        pct(p_cssnano)
    );
    for (name, ns) in &cssnano_by_plugin {
        println!("  └─ {:<33} {:>9.3}", name, to_us(*ns));
    }
    println!(
        "atomicify-rules                      {:>9.3}  {:>10.2}%",
        to_us(p_atomicify),
        pct(p_atomicify)
    );
    println!(
        "autoprefixer: build (from snapshot)  {:>9.3}  {:>10.2}%",
        to_us(p_ap_build),
        pct(p_ap_build)
    );
    println!(
        "autoprefixer: remove (tree walk)     {:>9.3}  {:>10.2}%",
        to_us(p_ap_remove),
        pct(p_ap_remove)
    );
    println!(
        "autoprefixer: add    (tree walk)     {:>9.3}  {:>10.2}%",
        to_us(p_ap_add),
        pct(p_ap_add)
    );
    println!(
        "postcss-normalize-whitespace         {:>9.3}  {:>10.2}%",
        to_us(p_whitespace),
        pct(p_whitespace)
    );
    println!(
        "extract-stylesheets                  {:>9.3}  {:>10.2}%",
        to_us(p_extract),
        pct(p_extract)
    );

    println!();
    println!("Top 5 phases by share:");
    let mut ranked: Vec<(&str, u128)> = vec![
        ("parse", p_parse),
        ("discard-duplicates", p_discard_dups),
        ("parent-orphaned-pseudos", p_parent_orphan),
        ("sort-atomic-style-sheet", p_sort_atomic),
        ("postcss-nested", p_postcss_nested),
        ("interleaved-decl-walk", p_decl_walk),
        ("cssnano (BASE)", p_cssnano),
        ("atomicify-rules", p_atomicify),
        ("ap:build", p_ap_build),
        ("ap:remove", p_ap_remove),
        ("ap:add", p_ap_add),
        ("normalize-whitespace", p_whitespace),
        ("extract-stylesheets", p_extract),
    ];
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    for (i, (name, ns)) in ranked.iter().take(5).enumerate() {
        println!(
            "  {}. {:<28} {:>7.3} µs  ({:>5.2}%)",
            i + 1,
            name,
            to_us(*ns),
            pct(*ns)
        );
    }
}
