//! Codegen for `src/data/prefixes.rs` — see `crates/PARITY_VERSIONS.md` and
//! `crates/autoprefixer/HANDOVER.md` §2.
//!
//! Evaluates `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`
//! via `bun -e`, dumps the exported object as JSON, codegens a series of
//! `m.insert(...)` statements at `$OUT_DIR/prefixes_table.rs`. The
//! generated file is `include!`-ed by `src/data/prefixes.rs` inside the
//! `Lazy::new(|| { ... })` body for `PREFIXES`.
//!
//! # Why bun, not a JS parser crate
//!
//! `data/prefixes.js` calls `require('caniuse-lite/dist/unpacker/feature')`
//! at module load and unpacks compressed caniuse data at runtime. Static
//! AST-walking it is harder than just letting bun evaluate it. The
//! caniuse-lite version is pinned to 1.0.30001690 via the root
//! `package.json` `overrides` block, so the resolution is stable.
//!
//! # Pre-condition
//!
//! `bun install` must have been run at the workspace root so that
//! `node_modules/caniuse-lite` is populated. Without it, bun's
//! `require('caniuse-lite')` fails and we panic with a directive. Cargo
//! re-runs this script whenever the vendored JS or `build.rs` itself
//! changes.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Workspace root = ../.. relative to crates/autoprefixer/.
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has no grandparent")
        .to_path_buf();
    let vendored_js = manifest_dir
        .join("..")
        .join("_vendor")
        .join("autoprefixer-10.4.14")
        .join("package")
        .join("data")
        .join("prefixes.js");

    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        vendored_js.display()
    );

    if !vendored_js.exists() {
        panic!(
            "autoprefixer build.rs: vendored prefixes.js not found at {}.\n\
             Expected upstream source under \
             crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js. \
             Re-vendor from node_modules/.bun/autoprefixer@10.4.14+*/node_modules/autoprefixer/.",
            vendored_js.display()
        );
    }

    // Sanity-check the workspace pin. `caniuse-lite` is a direct
    // devDependency on the root `package.json` (with override 1.0.30001690),
    // which causes bun to symlink it at the workspace's top-level
    // `node_modules/caniuse-lite/`. That, in turn, makes Node-style module
    // resolution from inside `crates/_vendor/...` find the workspace pin
    // before any parent-directory shadow project.
    //
    // The package.json line driving this is load-bearing — without it, bun's
    // isolated install layout leaves no top-level `caniuse-lite` and the
    // vendored JS resolves whatever `caniuse-lite` lives further up the
    // filesystem (observed during port: a parent dir at 1.0.30001754).
    let pinned_caniuse_dir = workspace_root
        .join("node_modules")
        .join("caniuse-lite");
    if !pinned_caniuse_dir.exists() {
        panic!(
            "autoprefixer build.rs: workspace caniuse-lite not found at {}.\n\
             Pre-condition: `bun install` must have run at the workspace root \
             with the package.json devDependency + override pinning \
             caniuse-lite to 1.0.30001690. Run `bun install` and retry.",
            pinned_caniuse_dir.display()
        );
    }

    // Write the dump-as-JSON script to OUT_DIR and invoke `bun <file>`. We
    // intentionally avoid `bun -e` because Windows command-line arg quoting
    // mangles JS strings containing parens/quotes (observed in the wild —
    // `process` was truncated to `ss` in the spawned subprocess).
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let js_path_str = vendored_js.to_string_lossy().replace('\\', "/");
    let dumper_js = format!(
        "process.stdout.write(JSON.stringify(require({:?})));\n",
        js_path_str,
    );
    let dumper_path = PathBuf::from(&out_dir).join("dump_prefixes.js");
    std::fs::write(&dumper_path, dumper_js).unwrap_or_else(|e| {
        panic!(
            "autoprefixer build.rs: failed to write {}: {e}",
            dumper_path.display()
        )
    });

    let output = run_bun(&dumper_path, &workspace_root);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "autoprefixer build.rs: `bun -e` exited non-zero ({:?}).\n\
             stderr: {stderr}\n\
             Pre-condition: `bun install` must have run at the workspace root \
             so that node_modules/caniuse-lite (pinned to 1.0.30001690 via \
             package.json overrides) is populated. Run `bun install` and retry.",
            output.status.code()
        );
    }

    let json = String::from_utf8(output.stdout)
        .expect("autoprefixer build.rs: bun stdout was not UTF-8");
    let table: serde_json::Value = serde_json::from_str(&json)
        .expect("autoprefixer build.rs: bun output was not valid JSON");
    let table = table
        .as_object()
        .expect("autoprefixer build.rs: top-level JSON value was not an object");

    let mut codegen = String::new();
    codegen.push_str("// AUTO-GENERATED by build.rs from\n");
    codegen.push_str("// crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js.\n");
    codegen.push_str("// Do not edit by hand. See build.rs for the codegen logic.\n\n");
    // Wrap in a block expression so `include!()` (which expects a single
    // expression) accepts the whole sequence of `m.insert(...)` statements.
    codegen.push_str("{\n");

    for (key, value) in table {
        let entry = value
            .as_object()
            .unwrap_or_else(|| panic!("entry {key:?} was not a JSON object"));

        let browsers = string_array(entry.get("browsers"), "browsers", key);
        let mistakes = string_array(entry.get("mistakes"), "mistakes", key);
        let props = string_array(entry.get("props"), "props", key);
        let feature = entry
            .get("feature")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let selector = entry
            .get("selector")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // `transition` field absent in this caniuse-lite snapshot. Kept on
        // PrefixEntry as `#[serde(default)]` for forward-compat.
        let transition = entry
            .get("transition")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        codegen.push_str(&format!(
            "    m.insert({}, PrefixEntry {{\n",
            rust_str_lit(key)
        ));
        codegen.push_str(&format!(
            "        browsers: vec![{}],\n",
            comma_separated_strs(&browsers)
        ));
        codegen.push_str(&format!(
            "        mistakes: vec![{}],\n",
            comma_separated_strs(&mistakes)
        ));
        codegen.push_str(&format!(
            "        feature: {},\n",
            match feature {
                Some(f) => format!("Some({}.to_string())", rust_str_lit(&f)),
                None => "None".into(),
            }
        ));
        codegen.push_str(&format!(
            "        props: vec![{}],\n",
            comma_separated_strs(&props)
        ));
        codegen.push_str(&format!("        transition: {transition},\n"));
        codegen.push_str(&format!("        selector: {selector},\n"));
        codegen.push_str("    });\n");
    }

    codegen.push_str("}\n");

    let out_path = PathBuf::from(&out_dir).join("prefixes_table.rs");
    std::fs::write(&out_path, codegen).unwrap_or_else(|e| {
        panic!(
            "autoprefixer build.rs: failed to write {}: {e}",
            out_path.display()
        )
    });
}

/// Spawn `bun <file>` resilient to Windows shim resolution.
///
/// On Windows, bun is typically installed as `bun.cmd` (a shim). Rust's
/// `Command::new("bun")` does NOT walk `PATHEXT`, so it fails to find
/// `bun.cmd`. We try the bare name first (works on POSIX) and fall back
/// to Windows-specific candidates.
fn run_bun(script_path: &std::path::Path, cwd: &std::path::Path) -> std::process::Output {
    let candidates: &[&str] = if cfg!(windows) {
        &["bun", "bun.cmd", "bun.exe"]
    } else {
        &["bun"]
    };

    let mut last_err = None;
    for cand in candidates {
        match Command::new(cand)
            .arg(script_path)
            .current_dir(cwd)
            .output()
        {
            Ok(o) => return o,
            Err(e) => {
                last_err = Some((cand.to_string(), e));
            }
        }
    }

    let (cand, err) = last_err.expect("at least one bun candidate is tried");
    panic!(
        "autoprefixer build.rs: failed to spawn `bun <file>` (last attempt: {cand}): {err}\n\
         Pre-condition: `bun` must be on PATH. This repo uses bun (see bun.lock + \
         STATUS.md). Install bun from https://bun.sh and re-run `bun install` at \
         the workspace root."
    )
}

fn string_array(value: Option<&serde_json::Value>, field: &str, key: &str) -> Vec<String> {
    match value {
        None => Vec::new(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap_or_else(|| {
                        panic!("entry {key:?} field {field}: non-string element {v:?}")
                    })
                    .to_string()
            })
            .collect(),
        Some(other) => panic!(
            "entry {key:?} field {field}: expected array, got {other:?}"
        ),
    }
}

fn comma_separated_strs(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{}.to_string()", rust_str_lit(s)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Emit a Rust string literal for an arbitrary str. Uses Rust's `Debug`
/// impl which escapes non-printables, quotes, and backslashes correctly.
fn rust_str_lit(s: &str) -> String {
    format!("{s:?}")
}
