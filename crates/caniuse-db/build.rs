//! Build script: reads the pre-unpacked caniuse-lite snapshot and emits
//! `OUT_DIR/features_data.json` plus a small marker rust file. The runtime
//! `include_str!`s the JSON and parses lazily on first access via
//! `once_cell::Lazy`.
//!
//! Per `crates/PARITY_VERSIONS.md` Anomaly #3, the snapshot is frozen at
//! caniuse-lite@1.0.30001766 and must not be regenerated unless every
//! consumer is signed off on a hash rotation.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=data/features.snapshot.json");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let snapshot = fs::read("data/features.snapshot.json")
        .expect("data/features.snapshot.json missing — run `node scripts/snapshot.js`");
    fs::write(out_dir.join("features_data.json"), &snapshot)
        .expect("write features_data.json");
}
