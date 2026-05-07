//! One-shot probe: with `BROWSERSLIST=chrome 100`, does the assembled
//! `transform_css` pipeline emit `-webkit-background-clip:` for the
//! AFM divergence fixture 00061? If so, what does autoprefixer alone
//! emit for the same input? Used to localise the AFM Group A drift
//! (spurious `-webkit-background-clip` prefix).
//!
//! Run from `crates/`:
//!   cargo run --example probe_bg_clip -p parity-runner

use std::fs;

fn main() {
    std::env::set_var("BROWSERSLIST", "chrome 100");
    std::env::remove_var("AUTOPREFIXER");

    let css_path = "parity-runner/corpus/afm-transform-css/00061_732b78ee.css";
    let css = fs::read_to_string(css_path).expect("fixture read");

    println!("=== resolved browserslist ===");
    let resolved = browserslist_shim::index::resolve_with(
        "",
        &browserslist_shim::index::ResolveOpts {
            path: std::env::current_dir().ok().as_deref(),
            env: None,
            ignore_unknown_versions: true,
        },
    );
    println!("{resolved:?}");

    println!("\n=== transform_css output sheets that mention background-clip ===");
    let opts = css::transform::TransformOpts::default();
    match css::transform::transform_css(&css, &opts) {
        Ok(r) => {
            for s in &r.sheets {
                if s.contains("background-clip") {
                    println!("{s}");
                }
            }
        }
        Err(e) => eprintln!("transform_css error: {e}"),
    }
}
