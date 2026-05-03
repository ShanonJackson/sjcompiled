//! Reproduce the alleged AFM `user-select` drift reported in
//! `plugins/AUTOPREFIXER_DRIFT_BRIEF.md`. Direct call into the
//! source-built `transform_css` — bypasses NAPI entirely so the
//! result reflects the current working tree's autoprefixer logic.
//!
//! Expected (per JS + shipped NAPI): `-webkit-user-select:none;user-select:none`
//! Reported (per source build):       `-webkit-user-select:none;-moz-user-select:none;user-select:none`
//!
//! Run with:
//!   cargo run --profile bench-fast --example repro_user_select -p css

use css::{transform_css, TransformOpts};

fn main() {
    // Anchor browserslist resolution at the AFM fixture (same as
    // what packages/css-native consumers do under
    // BROWSERSLIST_CONFIG=.../afm/.browserslistrc).
    let afm_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm");
    std::env::set_current_dir(&afm_dir).expect("set cwd to AFM fixture dir");

    let opts = TransformOpts::default();

    println!("--- AFM cwd: {} ---", afm_dir.display());

    // Test 1: bare declaration (the brief's exact repro input).
    let css = "user-select: none;\n";
    let result = transform_css(css, &opts).expect("transform_css");
    println!("\nInput : {:?}", css);
    println!("Output: {:?}", result.sheets);

    // Test 2: pre-wrapped rule (mirrors the parity-runner autoprefixer
    // fixture 028-user-select.css shape).
    let css2 = ".search { user-select: none; }\n";
    let result2 = transform_css(css2, &opts).expect("transform_css");
    println!("\nInput : {:?}", css2);
    println!("Output: {:?}", result2.sheets);

    // Test 3: with -moz- explicitly added — shouldn't be added by autoprefixer
    // for AFM (firefox 146/147 ship native user-select).
    println!("\n--- check: does the source emit `-moz-user-select`? ---");
    let drifted_marker = "-moz-user-select";
    let any_moz = result.sheets.iter().any(|s| s.contains(drifted_marker))
        || result2.sheets.iter().any(|s| s.contains(drifted_marker));
    if any_moz {
        println!("DRIFT REPRODUCED: -moz-user-select present in output");
    } else {
        println!("NO DRIFT: source build produces JS-equivalent bytes");
    }

    // Test 4: brief claims under BROWSERSLIST=chrome 100, source emits
    // BOTH -webkit and -moz while JS emits neither. Reproduce that
    // claim directly. We set the env var; the autoprefixer code path
    // reads `BROWSERSLIST` via browserslist-shim.
    println!("\n--- chrome 100 test (brief claims source emits both prefixes) ---");
    std::env::set_var("BROWSERSLIST", "chrome 100");
    let result3 = transform_css(css, &opts).expect("transform_css chrome100");
    println!("Input : {:?}", css);
    println!("Output: {:?}", result3.sheets);
    let any_prefix_chrome = result3.sheets.iter().any(|s| {
        s.contains("-webkit-user-select") || s.contains("-moz-user-select")
    });
    if any_prefix_chrome {
        println!("UNEXPECTED: chrome 100 output has prefix(es) — JS would emit none");
    } else {
        println!("CHROME 100 OK: no prefixes (matches JS)");
    }
    std::env::remove_var("BROWSERSLIST");
}
