//! Precomputed `Prefixes` snapshot — postcard-serializable input bundle.
//!
//! ## Why this exists
//!
//! `build_prefixes_default()` performs three load-bearing pieces of
//! cold-path setup on every call:
//!
//!   1. `Browsers::new(...)` — filesystem walk for `.browserslistrc`,
//!      browserslist resolution against caniuse-db agents.
//!   2. `Prefixes::select(&PREFIXES)` — full iteration over the static
//!      prefix table (every CSS feature × every browser version),
//!      filtered against the resolved browsers.
//!   3. `Prefixes::preprocess()` — bucketing the per-name prefix lists
//!      into the dispatch tables (`AddTable`, `RemoveTable`) consumed
//!      by `processor.add` / `processor.remove`.
//!
//! Steps 1+2 are pure functions of `(browserslist query, options)` —
//! their output is stable across calls. Step 3 reads only their output.
//!
//! ## What we precompute
//!
//! Steps 1+2. The result of step 2 is the `Selected` shape — two
//! `IndexMap<String, Vec<String>>` tables — plus the resolved
//! `selected: Vec<String>` browsers and the `PrefixesOptions`. All four
//! are plain data, postcard-serializable.
//!
//! Step 3 (`preprocess`) still runs on each [`build_prefixes_from_precomputed`]
//! call because the output graph contains hack-trait instances
//! (`Box<dyn>`-shaped) that can't be losslessly serialized. Empirically
//! it's a small fraction of the cold cost — the dominant cost in
//! cargo-flamegraph traces is `select()`, which the snapshot eliminates.
//!
//! ## Byte-equality contract
//!
//! [`build_prefixes_from_precomputed`] is byte-identical to the slow
//! path in EVERY observable way that reaches the `processor.add` /
//! `processor.remove` walks. Specifically: the resulting `Prefixes`
//! struct's `add_table`, `remove_table`, `browsers.selected`, and
//! `options` fields are equal to those produced by `Prefixes::new`.
//! `preprocess()` then runs the same code on equal inputs, producing
//! equal `add` / `remove` dispatch tables.
//!
//! The verify-equality test `tests/precomputed_parity.rs` enforces this
//! field-by-field on the AFM browserslist; CI fails on drift.
//!
//! ## Versioning
//!
//! [`PrecomputedPrefixesV1`] carries a `format_version: u32` so the
//! consumer can reject blobs produced by a future incompatible layout.
//! Bump on any field change. Old blobs in the wild become a hard error
//! rather than silent drift.

use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

use crate::autoprefixer::{build_prefixes, AutoprefixerOptions};
use crate::browsers::{BrowserslistOpts, Browsers, BrowsersOptions};
use crate::prefixes::{AddTable, Prefixes, PrefixesOptions, RemoveTable};
use crate::supports::Supports;

/// Layout version. Bump on any backward-incompatible field change.
pub const PRECOMPUTED_FORMAT_VERSION: u32 = 1;

/// Serializable snapshot of the inputs required to skip
/// `Browsers::new` + `Prefixes::select`. Equivalent to the JS-side
/// `loadPrefixes()` cache value, but as plain data rather than a
/// runtime-bound object graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedPrefixesV1 {
    pub format_version: u32,
    /// Mirrors `Browsers.selected` — the resolved "name version"
    /// strings. Reused at runtime so `cleaner_cache` and any
    /// `browsers.prefix(...)` calls outside `select` (none on the
    /// hashing path today, but defensive against future agents wiring
    /// new lookups) keep working.
    pub selected: Vec<String>,
    /// Mirrors `Selected.add` — per-name prefix list to ADD.
    pub add_table: IndexMap<String, Vec<String>>,
    /// Mirrors `Selected.remove` — per-name prefix list to REMOVE.
    pub remove_table: IndexMap<String, Vec<String>>,
    /// Mirrors `PrefixesOptions`. Embedded directly (rather than via
    /// `#[serde(flatten)]`) so the postcard schema stays explicit.
    pub options: PrefixesOptionsSnapshot,
}

/// Serde-friendly mirror of [`PrefixesOptions`]. We mirror rather than
/// derive `Serialize`/`Deserialize` on `PrefixesOptions` itself so the
/// public type remains drift-free with upstream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrefixesOptionsSnapshot {
    pub flexbox: Option<String>,
    pub cascade: Option<bool>,
    pub add: Option<bool>,
    pub remove: Option<bool>,
    pub supports: Option<bool>,
    pub grid: Option<String>,
}

impl From<&PrefixesOptions> for PrefixesOptionsSnapshot {
    fn from(o: &PrefixesOptions) -> Self {
        Self {
            flexbox: o.flexbox.clone(),
            cascade: o.cascade,
            add: o.add,
            remove: o.remove,
            supports: o.supports,
            grid: o.grid.clone(),
        }
    }
}

impl From<&PrefixesOptionsSnapshot> for PrefixesOptions {
    fn from(s: &PrefixesOptionsSnapshot) -> Self {
        Self {
            flexbox: s.flexbox.clone(),
            cascade: s.cascade,
            add: s.add,
            remove: s.remove,
            supports: s.supports,
            grid: s.grid.clone(),
        }
    }
}

/// Errors surfaced by precomputed-prefixes loading.
#[derive(Debug)]
pub enum PrecomputedError {
    Decode(postcard::Error),
    UnsupportedVersion { found: u32, supported: u32 },
}

impl std::fmt::Display for PrecomputedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "postcard decode error: {e}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "precomputed prefixes format version {found} is not supported (this build expects {supported})"
            ),
        }
    }
}

impl std::error::Error for PrecomputedError {}

/// Build the precompute snapshot for a given query / options. Mirrors
/// the JS `loadPrefixes(opts)` cache-fill side: `select(...)` is run
/// against the freshly-resolved browsers, and the result is captured
/// in plain data. `preprocess()` is NOT run here — it can't be
/// serialized losslessly — so it remains a per-load step in
/// [`build_prefixes_from_precomputed`].
///
/// In practice consumers call [`precompute_prefixes_default`] which
/// fixes the query to the same shape `build_prefixes_default(None)`
/// uses (the AFM call site).
pub fn precompute_prefixes(
    options: AutoprefixerOptions,
) -> PrecomputedPrefixesV1 {
    // Build a real `Prefixes` once, then snapshot its `select()` output.
    // We discard `preprocess()` output — the `add` / `remove` RefCells —
    // because they're rebuilt deterministically at load time.
    let prefixes = build_prefixes(None, options).expect(
        "build_prefixes failed during precompute (browserslist resolution error). \
         Verify the resolved cwd has a valid .browserslistrc reachable.",
    );
    PrecomputedPrefixesV1 {
        format_version: PRECOMPUTED_FORMAT_VERSION,
        selected: prefixes.browsers.selected.clone(),
        add_table: prefixes.add_table.clone(),
        remove_table: prefixes.remove_table.clone(),
        options: PrefixesOptionsSnapshot::from(&prefixes.options),
    }
}

/// Convenience: snapshot the AFM call-site shape — empty query, no
/// overrides. Equivalent to the inputs `build_prefixes_default(None)`
/// resolves.
pub fn precompute_prefixes_default() -> PrecomputedPrefixesV1 {
    precompute_prefixes(AutoprefixerOptions::default())
}

/// Serialize a snapshot to postcard bytes. Stable across runs given
/// stable inputs (postcard is deterministic).
pub fn encode_precomputed(snapshot: &PrecomputedPrefixesV1) -> Vec<u8> {
    postcard::to_allocvec(snapshot).expect("postcard encode of PrecomputedPrefixesV1 cannot fail (no IO, no fallible serializers)")
}

/// Decode postcard bytes into a snapshot. Returns `Err` on truncation
/// or version mismatch.
pub fn decode_precomputed(bytes: &[u8]) -> Result<PrecomputedPrefixesV1, PrecomputedError> {
    let snapshot: PrecomputedPrefixesV1 =
        postcard::from_bytes(bytes).map_err(PrecomputedError::Decode)?;
    if snapshot.format_version != PRECOMPUTED_FORMAT_VERSION {
        return Err(PrecomputedError::UnsupportedVersion {
            found: snapshot.format_version,
            supported: PRECOMPUTED_FORMAT_VERSION,
        });
    }
    Ok(snapshot)
}

/// Reconstruct a [`Prefixes`] from a precomputed snapshot. Equivalent
/// to `Prefixes::new(...)` minus the `select()` step — `preprocess()`
/// still runs.
///
/// This is the WASI-friendly hot path: no filesystem I/O, no
/// browserslist resolution, no full-table iteration. Cold-start cost
/// drops to `decode + preprocess`.
pub fn build_prefixes_from_snapshot(snapshot: &PrecomputedPrefixesV1) -> Prefixes {
    // We construct the struct directly because all the runtime fields
    // (`add`, `remove`, `supports_inst`, `cleaner_cache`) need to be
    // freshly initialized empty — `preprocess()` populates them.
    //
    // `browsers.options` and `browsers.browserslist_opts` are
    // intentionally `Default` here: AFM consumers don't read them on
    // the hashing path (only `selected` and `prefix()` lookups matter,
    // both of which derive from `selected`), and stuffing them into
    // the snapshot would bloat the blob without behavioral change.
    let browsers = Browsers {
        selected: snapshot.selected.clone(),
        options: BrowsersOptions::default(),
        browserslist_opts: BrowserslistOpts::default(),
    };
    let options = PrefixesOptions::from(&snapshot.options);

    let mut prefixes = Prefixes {
        browsers,
        options,
        add_table: snapshot.add_table.clone(),
        remove_table: snapshot.remove_table.clone(),
        cleaner_cache: OnceCell::new(),
        add: RefCell::new(AddTable::default()),
        remove: RefCell::new(RemoveTable::default()),
        supports_inst: RefCell::new(Box::new(Supports::new())),
    };
    prefixes.preprocess_for_precomputed();
    prefixes
}

/// Decode + reconstruct in one shot.
pub fn build_prefixes_from_precomputed(
    bytes: &[u8],
) -> Result<Prefixes, PrecomputedError> {
    let snapshot = decode_precomputed(bytes)?;
    Ok(build_prefixes_from_snapshot(&snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoprefixer::build_prefixes_default;
    use crate::test_support::afm_fixture_dir;

    /// Snapshot+rebuild produces a `Prefixes` whose post-`select()`
    /// state is field-equal to the slow-path equivalent. The
    /// `preprocess()` outputs aren't structurally compared (they
    /// contain non-`Eq` `Box<dyn>` shapes), but they're deterministic
    /// functions of the snapshotted inputs — equal inputs → equal
    /// outputs.
    #[test]
    fn snapshot_rebuild_matches_slow_path_inputs() {
        let _guard = std::env::set_current_dir(afm_fixture_dir())
            .ok()
            .map(|_| ());

        let slow = build_prefixes_default(None).expect("slow path");
        let snapshot = precompute_prefixes_default();
        let fast = build_prefixes_from_snapshot(&snapshot);

        assert_eq!(slow.browsers.selected, fast.browsers.selected);
        assert_eq!(slow.add_table, fast.add_table);
        assert_eq!(slow.remove_table, fast.remove_table);
        assert_eq!(slow.options.flexbox, fast.options.flexbox);
        assert_eq!(slow.options.cascade, fast.options.cascade);
        assert_eq!(slow.options.add, fast.options.add);
        assert_eq!(slow.options.remove, fast.options.remove);
        assert_eq!(slow.options.supports, fast.options.supports);
        assert_eq!(slow.options.grid, fast.options.grid);
    }

    #[test]
    fn encode_decode_round_trips() {
        let _guard = std::env::set_current_dir(afm_fixture_dir())
            .ok()
            .map(|_| ());
        let snap = precompute_prefixes_default();
        let bytes = encode_precomputed(&snap);
        let decoded = decode_precomputed(&bytes).expect("decode");
        assert_eq!(decoded.format_version, snap.format_version);
        assert_eq!(decoded.selected, snap.selected);
        assert_eq!(decoded.add_table, snap.add_table);
        assert_eq!(decoded.remove_table, snap.remove_table);
    }

    #[test]
    fn version_mismatch_is_an_error() {
        let _guard = std::env::set_current_dir(afm_fixture_dir())
            .ok()
            .map(|_| ());
        let mut snap = precompute_prefixes_default();
        snap.format_version = 999;
        let bytes = encode_precomputed(&snap);
        let err = decode_precomputed(&bytes).expect_err("must reject");
        assert!(matches!(err, PrecomputedError::UnsupportedVersion { .. }));
    }
}
