//! Cross-agent test helpers. **#[cfg(test)] only — never imported from
//! production paths.**
//!
//! Until this module landed, every agent (AGENT_1, AGENT_2, AGENT_3,
//! ...) was hand-rolling their own `afm_browsers()` / `afm_fixture_dir()`
//! / `dummy_prefixes()`. AGENT_4's review flagged that as a cross-agent
//! footgun: the moment one copy diverges, the AFM-fixture parity gate
//! reads the wrong fixture and the byte-test silently passes against
//! a drifted oracle.
//!
//! Hoist all such helpers here and import via
//! `use crate::test_support::*` in `#[cfg(test)] mod tests` blocks
//! across the crate. Integration tests (under `tests/`) cannot
//! `use crate::...` — they should mirror the helpers inline OR
//! import via the public `pub fn` shape below.
//!
//! Keep this module #[cfg(test)] (or compile-only-in-test) so the
//! release build never depends on it.

use std::path::PathBuf;

use crate::browsers::{BrowserslistOpts, Browsers, BrowsersOptions};

/// Path to AFM's fixture `.browserslistrc` (SHA256-pinned by
/// `crates/browserslist-shim/tests/afm_parity.rs`). The
/// browserslist-shim resolver walks up from this path to land on the
/// fixture file.
pub fn afm_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("browserslist-shim")
        .join("tests")
        .join("fixtures")
        .join("afm")
}

/// Build a `Browsers` resolved against AFM's pinned `.browserslistrc`.
/// Used by every test that needs the AFM-shaped 14-entry browser list
/// the production AFM build produces.
///
/// Pre-condition: caniuse-lite is at the version pinned by
/// `crates/PARITY_VERSIONS.md` Anomaly #3 (currently `1.0.30001766`).
/// The data parity gate enforces this; if it ever fails, the AFM
/// resolution drifts here too.
pub fn afm_browsers() -> Browsers {
    let opts = BrowsersOptions {
        from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
    };
    Browsers::new(Vec::new(), opts, BrowserslistOpts::default())
}

/// Build a `Browsers` with `selected = []`. Use for the
/// cleaner-early-return path and for `Prefixes::with_empty()`-style
/// scaffolding when peer agents need a hand-built `Prefixes`.
pub fn empty_browsers() -> Browsers {
    Browsers {
        selected: Vec::new(),
        options: BrowsersOptions::default(),
        browserslist_opts: BrowserslistOpts::default(),
    }
}
