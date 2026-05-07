//! Build script — pin the WASM linear-memory stack size for any
//! `wasm32-wasi*` target build of this crate.
//!
//! ## Why a build.rs and not just `.cargo/config.toml`
//!
//! `.cargo/config.toml` only takes effect when Cargo discovers it
//! during config-file walk-up from the build's invocation cwd. That
//! means it's silently bypassed by:
//!
//!  - `cargo build --manifest-path crates/Cargo.toml` from outside
//!    the repo's `.cargo/` ancestor chain.
//!  - External consumers vendoring this crate.
//!  - CI scripts that `cd` to a parent directory and invoke cargo
//!    via `--manifest-path`.
//!
//! A 1 MiB lld-default stack is insufficient for the Compiled
//! babel-plugin's mutually-recursive evaluator
//! (`utils::evaluate_expression::dispatch_evaluate` ↔
//! `traverse_expression/*` leaves) under deep cross-file
//! member-access chains common in AFM theming. Symptom of overflow
//! is a SILENT WASM trap mid-`format!()`, which manifests as a
//! stuck plugin invocation and runaway host CPU growth as wasmer
//! attempts to grow linear memory. There is no error message —
//! making this exactly the kind of footgun production builds must
//! never hit.
//!
//! Emitting `cargo:rustc-link-arg` here pins the value at link-time
//! for every `wasm32-wasi*` build of this crate regardless of the
//! caller's environment. The repo-root `.cargo/config.toml` still
//! exists for parity with the @swc-project plugin examples and as
//! a primary-source documentation point; this build.rs is the
//! belt-and-braces guarantee.
//!
//! ## Sizing
//!
//! 8 MiB matches the value @swc-project's own plugin examples ship
//! with. The reservation is a single contiguous region inside the
//! plugin's linear memory — it does NOT add to the plugin's
//! on-disk size and does NOT change the host's resident memory
//! until the plugin actually pushes that deep.
//!
//! ## Scope
//!
//! Only emitted for `wasm32-wasi*` targets. Native `cargo test`
//! / `cargo build` runs (used by workspace integration tests and
//! `run_dispatcher`) get the host platform's default thread stack
//! and are unaffected.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("wasm32-wasi") {
        // 8 MiB. Keep in sync with `.cargo/config.toml`.
        println!("cargo:rustc-link-arg=-zstack-size=8388608");
    }
    // Re-run only when this script itself changes; nothing else
    // affects its output.
    println!("cargo:rerun-if-changed=build.rs");
}
