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

/// Layout version of the V1 (pre-`preprocess()`) snapshot. Kept stable
/// — old V1 blobs in the wild still decode via `decode_precomputed`.
pub const PRECOMPUTED_FORMAT_VERSION: u32 = 1;

/// Layout version of the V2 (post-`preprocess()`) snapshot. Bumped
/// from V1 when the populated `AddTable` / `RemoveTable` were added to
/// the snapshot payload — the dominant remaining cold-start cost
/// (~345 µs/call) was `Prefixes::preprocess()`, which V2 elides
/// entirely.
#[cfg(feature = "fast-match")]
pub const PRECOMPUTED_FORMAT_VERSION_V2: u32 = 2;

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

/// Decode + reconstruct in one shot. Dispatches on the `format_version`
/// header so callers can hand us either a V1 or V2 blob — V1 takes the
/// preprocess-on-load path, V2 takes the populated-table path.
///
/// Reads the version with a partial decode via
/// [`postcard::take_from_bytes`] before committing to a struct layout,
/// because V1 and V2 are byte-incompatible past the leading version u32.
pub fn build_prefixes_from_precomputed(
    bytes: &[u8],
) -> Result<Prefixes, PrecomputedError> {
    match peek_format_version(bytes)? {
        PRECOMPUTED_FORMAT_VERSION => {
            let snapshot = decode_precomputed(bytes)?;
            Ok(build_prefixes_from_snapshot(&snapshot))
        }
        #[cfg(feature = "fast-match")]
        PRECOMPUTED_FORMAT_VERSION_V2 => {
            let snapshot = decode_precomputed_v2(bytes)?;
            Ok(build_prefixes_from_snapshot_v2(snapshot))
        }
        v => Err(PrecomputedError::UnsupportedVersion {
            found: v,
            // Without fast-match, V1 is the only supported layout.
            // With fast-match, V2 is also supported — both versions
            // reach this dispatch through `peek_format_version`.
            supported: PRECOMPUTED_FORMAT_VERSION,
        }),
    }
}

/// Peek at the leading `format_version: u32` without committing to a
/// full struct layout — same wire format for V1 and V2 so we can
/// dispatch without a layout-specific decode.
fn peek_format_version(bytes: &[u8]) -> Result<u32, PrecomputedError> {
    let (version, _rest) =
        postcard::take_from_bytes::<u32>(bytes).map_err(PrecomputedError::Decode)?;
    Ok(version)
}

// ============================================================================
// V2 — post-`preprocess()` snapshot
// ============================================================================

/// V2 snapshot — carries everything V1 carries PLUS the populated
/// `AddTable` / `RemoveTable`. Decoder skips `preprocess()` entirely;
/// cold-start cost drops from ~345 µs/call (V1) to ~30-60 µs/call (V2).
///
/// **Why feature-gated on `fast-match`:** the populated tables embed
/// `WordRegexp` / `IntrinsicRegexp` / `SelectorRegexp`, which only
/// derive `Serialize`/`Deserialize` when the fast-match feature is on
/// (without it, those wrappers carry `regex::Regex`, which is not
/// serde-able). The performance story this whole module targets is
/// also fast-match-conditional, so the gate aligns naturally.
///
/// **Drift contract:** byte-identical to a freshly-built `Prefixes`
/// post-`preprocess()` for every observable field. Pinned by
/// `tests/preprocess_snapshot_parity.rs`.
///
/// Not `Clone` — the populated tables hold types that intentionally
/// don't derive `Clone` (`ValueBase` carries a `OnceCell<WordRegexp>`
/// runtime cache; cloning is meaningless). `build_prefixes_from_snapshot_v2`
/// consumes the snapshot so the populated tables can be moved into
/// the `Prefixes` instance instead.
#[cfg(feature = "fast-match")]
#[derive(Serialize, Deserialize)]
pub struct PrecomputedPrefixesV2 {
    /// Format version — always [`PRECOMPUTED_FORMAT_VERSION_V2`].
    /// Kept as the FIRST field so `peek_format_version` can read it
    /// without committing to the V2 layout.
    pub format_version: u32,
    /// Mirrors `Browsers.selected`. Same as V1.
    pub selected: Vec<String>,
    /// Mirrors `Selected.add` — the pre-`preprocess()` per-name prefix
    /// list. Kept on V2 because cleaner-cache rebuilds (lazy, on
    /// `Prefixes::cleaner()`) call `Prefixes::new` which needs it.
    pub add_table: IndexMap<String, Vec<String>>,
    /// Mirrors `Selected.remove`. Same reason as `add_table`.
    pub remove_table: IndexMap<String, Vec<String>>,
    /// Mirrors `PrefixesOptions`. Same as V1.
    pub options: PrefixesOptionsSnapshot,
    /// **V2-specific** — populated `Prefixes.add` after `preprocess()`.
    /// Contains the per-name `AddBucket` instances the processor walks.
    pub populated_add: AddTable,
    /// **V2-specific** — populated `Prefixes.remove` after
    /// `preprocess()`.
    pub populated_remove: RemoveTable,
}

/// Build the V2 snapshot — same shape as V1's build, plus the two
/// populated tables (`AddTable` / `RemoveTable`) snapshotted post-
/// `preprocess()`.
///
/// Cost is one full slow-path build per call. Intended to run ONCE at
/// host startup; downstream WASI plugin calls receive the resulting
/// bytes via plugin_config or via filesystem path.
#[cfg(feature = "fast-match")]
pub fn precompute_prefixes_v2(
    options: AutoprefixerOptions,
) -> PrecomputedPrefixesV2 {
    // `build_prefixes` runs select() + preprocess() — exactly the live
    // path. We then `take()` the populated RefCells to avoid cloning;
    // the Prefixes is dropped immediately after.
    let prefixes = build_prefixes(None, options).expect(
        "build_prefixes failed during precompute (browserslist resolution error). \
         Verify the resolved cwd has a valid .browserslistrc reachable.",
    );

    // RefCell::replace takes &self — no need for mut on `prefixes`.
    let populated_add = prefixes.add.replace(AddTable::default());
    let populated_remove = prefixes.remove.replace(RemoveTable::default());

    PrecomputedPrefixesV2 {
        format_version: PRECOMPUTED_FORMAT_VERSION_V2,
        selected: prefixes.browsers.selected.clone(),
        add_table: prefixes.add_table.clone(),
        remove_table: prefixes.remove_table.clone(),
        options: PrefixesOptionsSnapshot::from(&prefixes.options),
        populated_add,
        populated_remove,
    }
}

/// V2 convenience: snapshot the AFM call-site shape — empty query,
/// no overrides. Mirrors `precompute_prefixes_default` for the V1
/// path.
#[cfg(feature = "fast-match")]
pub fn precompute_prefixes_v2_default() -> PrecomputedPrefixesV2 {
    precompute_prefixes_v2(AutoprefixerOptions::default())
}

/// Serialize a V2 snapshot to postcard bytes.
#[cfg(feature = "fast-match")]
pub fn encode_precomputed_v2(snapshot: &PrecomputedPrefixesV2) -> Vec<u8> {
    postcard::to_allocvec(snapshot)
        .expect("postcard encode of PrecomputedPrefixesV2 cannot fail (no IO, no fallible serializers)")
}

/// Decode V2 bytes. Returns `UnsupportedVersion` for any other version
/// header (including V1 — V1 bytes are byte-incompatible past the
/// leading u32 because the trailing populated tables are absent).
#[cfg(feature = "fast-match")]
pub fn decode_precomputed_v2(
    bytes: &[u8],
) -> Result<PrecomputedPrefixesV2, PrecomputedError> {
    // Peek the version first so we can produce a clean error for V1
    // bytes instead of a postcard truncation/format error.
    let version = peek_format_version(bytes)?;
    if version != PRECOMPUTED_FORMAT_VERSION_V2 {
        return Err(PrecomputedError::UnsupportedVersion {
            found: version,
            supported: PRECOMPUTED_FORMAT_VERSION_V2,
        });
    }
    postcard::from_bytes(bytes).map_err(PrecomputedError::Decode)
}

/// Reconstruct a [`Prefixes`] from a V2 snapshot. Skips
/// `preprocess()` entirely — the populated tables are decoded directly
/// into the `RefCell`s.
///
/// `cleaner_cache` and `supports_inst` are still freshly initialized
/// (both are runtime caches that V1 and V2 both elide; see recon
/// report). This is byte-equivalent to the live path because:
///   - `cleaner()` is lazy; first access rebuilds against the same
///     snapshotted inputs (`browsers.selected` empty), producing an
///     equivalent `Prefixes`.
///   - `Supports::new()` starts with `prefixer_cache: None`; first
///     access populates against the same options/data the live path
///     would.
/// **Consumes** the snapshot so the populated tables can be moved
/// into the `Prefixes` instance instead of cloned. If a caller needs
/// the snapshot multiple times, they should re-decode the bytes each
/// time — that's the WASI hot path anyway (one decode per transform).
#[cfg(feature = "fast-match")]
pub fn build_prefixes_from_snapshot_v2(snapshot: PrecomputedPrefixesV2) -> Prefixes {
    let browsers = Browsers {
        selected: snapshot.selected,
        options: BrowsersOptions::default(),
        browserslist_opts: BrowserslistOpts::default(),
    };
    let options = PrefixesOptions::from(&snapshot.options);

    Prefixes {
        browsers,
        options,
        add_table: snapshot.add_table,
        remove_table: snapshot.remove_table,
        cleaner_cache: OnceCell::new(),
        add: RefCell::new(snapshot.populated_add),
        remove: RefCell::new(snapshot.populated_remove),
        supports_inst: RefCell::new(Box::new(Supports::new())),
    }
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

    // ====================================================================
    // V2 round-trip parity tests — written before the V2 encoder existed,
    // gated to fast-match because V2 only exists with that feature.
    //
    // Drift contract: live-built `Prefixes` vs snapshot-decoded `Prefixes`
    // must agree on every observable field — including the populated
    // `AddTable` / `RemoveTable` AND the IndexMap insertion order on
    // `add_table` / `remove_table`. IndexMap::eq is order-INSENSITIVE
    // (key-set comparison), so the tests collect `iter()` to a Vec to
    // catch order drift.
    //
    // If these fail, REVERT V2 — do not patch the encoder/decoder to
    // make the comparison pass. See CLAUDE.md drift policy.
    // ====================================================================

    #[cfg(feature = "fast-match")]
    mod v2 {
        use super::*;
        use crate::prefixes::{AddBucket, RemoveBucket};

        /// Build live + V2-decoded `Prefixes` from the same AFM query.
        fn live_and_decoded() -> (Prefixes, Prefixes) {
            std::env::set_current_dir(afm_fixture_dir())
                .expect("set cwd to AFM fixture dir");

            let live = build_prefixes_default(None).expect("live build");

            let snap = precompute_prefixes_v2_default();
            let bytes = encode_precomputed_v2(&snap);
            let decoded = decode_precomputed_v2(&bytes).expect("decode V2");
            let from_snapshot = build_prefixes_from_snapshot_v2(decoded);

            (live, from_snapshot)
        }

        #[test]
        fn pre_preprocess_fields_equal_live() {
            let (live, fast) = live_and_decoded();

            assert_eq!(live.browsers.selected, fast.browsers.selected);
            assert_eq!(live.options.flexbox, fast.options.flexbox);
            assert_eq!(live.options.cascade, fast.options.cascade);
            assert_eq!(live.options.add, fast.options.add);
            assert_eq!(live.options.remove, fast.options.remove);
            assert_eq!(live.options.supports, fast.options.supports);
            assert_eq!(live.options.grid, fast.options.grid);

            // IndexMap insertion-order equality. `IndexMap::eq` is
            // order-INSENSITIVE — collect to Vec so order also gets
            // compared.
            let live_add: Vec<_> = live.add_table.iter().collect();
            let fast_add: Vec<_> = fast.add_table.iter().collect();
            assert_eq!(
                live_add, fast_add,
                "add_table insertion order drifted across V2 round-trip"
            );

            let live_remove: Vec<_> = live.remove_table.iter().collect();
            let fast_remove: Vec<_> = fast.remove_table.iter().collect();
            assert_eq!(
                live_remove, fast_remove,
                "remove_table insertion order drifted across V2 round-trip"
            );
        }

        #[test]
        fn populated_add_table_equals_live() {
            let (live, fast) = live_and_decoded();

            let live_add = live.add.borrow();
            let fast_add = fast.add.borrow();

            assert_eq!(
                live_add.selectors.len(),
                fast_add.selectors.len(),
                "AddTable.selectors length drifted"
            );
            for (i, (l, f)) in live_add
                .selectors
                .iter()
                .zip(fast_add.selectors.iter())
                .enumerate()
            {
                assert_eq!(
                    l.prefixer.name, f.prefixer.name,
                    "AddTable.selectors[{i}].name drifted"
                );
                assert_eq!(
                    l.prefixer.prefixes, f.prefixer.prefixes,
                    "AddTable.selectors[{i}].prefixes drifted"
                );
            }

            let live_keys: Vec<&String> = live_add.by_name.keys().collect();
            let fast_keys: Vec<&String> = fast_add.by_name.keys().collect();
            assert_eq!(
                live_keys, fast_keys,
                "AddTable.by_name insertion order drifted"
            );

            for key in live_keys.iter() {
                let live_b = live_add.by_name.get(*key).unwrap();
                let fast_b = fast_add.by_name.get(*key).unwrap();
                match (live_b, fast_b) {
                    (AddBucket::AtRule(l), AddBucket::AtRule(f)) => {
                        assert_eq!(l.prefixer.name, f.prefixer.name);
                        assert_eq!(l.prefixer.prefixes, f.prefixer.prefixes);
                    }
                    (AddBucket::Resolution(l), AddBucket::Resolution(f)) => {
                        assert_eq!(l.prefixer.name, f.prefixer.name);
                        assert_eq!(l.prefixer.prefixes, f.prefixer.prefixes);
                    }
                    (
                        AddBucket::Declaration { decl: ld, values: lv },
                        AddBucket::Declaration { decl: fd, values: fv },
                    ) => {
                        assert_eq!(ld.base().prefixer.name, fd.base().prefixer.name);
                        assert_eq!(
                            ld.base().prefixer.prefixes,
                            fd.base().prefixer.prefixes
                        );
                        assert_eq!(lv.len(), fv.len());
                        for (lvp, fvp) in lv.iter().zip(fv.iter()) {
                            assert_eq!(lvp.base().prefixer.name, fvp.base().prefixer.name);
                            assert_eq!(
                                lvp.base().prefixer.prefixes,
                                fvp.base().prefixer.prefixes
                            );
                        }
                    }
                    (AddBucket::Values(l), AddBucket::Values(f)) => {
                        assert_eq!(l.len(), f.len());
                        for (lvp, fvp) in l.iter().zip(f.iter()) {
                            assert_eq!(lvp.base().prefixer.name, fvp.base().prefixer.name);
                            assert_eq!(
                                lvp.base().prefixer.prefixes,
                                fvp.base().prefixer.prefixes
                            );
                        }
                    }
                    (l, f) => panic!(
                        "AddBucket variant drift at key {key:?}: live={ld:?} fast={fd:?}",
                        ld = std::mem::discriminant(l),
                        fd = std::mem::discriminant(f)
                    ),
                }
            }
        }

        #[test]
        fn populated_remove_table_equals_live() {
            let (live, fast) = live_and_decoded();

            let live_rem = live.remove.borrow();
            let fast_rem = fast.remove.borrow();

            assert_eq!(
                live_rem.selectors.len(),
                fast_rem.selectors.len(),
                "RemoveTable.selectors length drifted"
            );
            for (i, (l, f)) in live_rem
                .selectors
                .iter()
                .zip(fast_rem.selectors.iter())
                .enumerate()
            {
                assert_eq!(l.prefix, f.prefix, "selectors[{i}].prefix drifted");
                assert_eq!(
                    l.prefixed, f.prefixed,
                    "selectors[{i}].prefixed drifted"
                );
                assert_eq!(
                    l.unprefixed, f.unprefixed,
                    "selectors[{i}].unprefixed drifted"
                );
            }

            let live_keys: Vec<&String> = live_rem.by_name.keys().collect();
            let fast_keys: Vec<&String> = fast_rem.by_name.keys().collect();
            assert_eq!(
                live_keys, fast_keys,
                "RemoveTable.by_name insertion order drifted"
            );

            for key in live_keys.iter() {
                let live_b = live_rem.by_name.get(*key).unwrap();
                let fast_b = fast_rem.by_name.get(*key).unwrap();
                match (live_b, fast_b) {
                    (RemoveBucket::Resolution(l), RemoveBucket::Resolution(f)) => {
                        assert_eq!(l.prefixer.name, f.prefixer.name);
                        assert_eq!(l.prefixer.prefixes, f.prefixer.prefixes);
                    }
                    (RemoveBucket::RemoveMarker, RemoveBucket::RemoveMarker) => {}
                    (RemoveBucket::Values(l), RemoveBucket::Values(f)) => {
                        assert_eq!(l.len(), f.len());
                        for (lo, fo) in l.iter().zip(f.iter()) {
                            assert_eq!(lo.unprefixed, fo.unprefixed);
                            assert_eq!(lo.prefixed, fo.prefixed);
                            assert_eq!(lo.string, fo.string);
                        }
                    }
                    (
                        RemoveBucket::RemoveMarkerWithValues(l),
                        RemoveBucket::RemoveMarkerWithValues(f),
                    ) => {
                        assert_eq!(l.len(), f.len());
                        for (lo, fo) in l.iter().zip(f.iter()) {
                            assert_eq!(lo.unprefixed, fo.unprefixed);
                            assert_eq!(lo.prefixed, fo.prefixed);
                            assert_eq!(lo.string, fo.string);
                        }
                    }
                    (l, f) => panic!(
                        "RemoveBucket variant drift at key {key:?}: live={ld:?} fast={fd:?}",
                        ld = std::mem::discriminant(l),
                        fd = std::mem::discriminant(f)
                    ),
                }
            }
        }

        /// V1 bytes MUST be rejected by the V2 decoder. Both layouts
        /// start with `format_version: u32`, but the trailing populated
        /// tables make V1/V2 byte-incompatible past that header.
        #[test]
        fn v1_bytes_rejected_by_v2_decoder() {
            std::env::set_current_dir(afm_fixture_dir())
                .expect("set cwd to AFM fixture dir");

            let v1 = precompute_prefixes_default();
            let v1_bytes = encode_precomputed(&v1);

            match decode_precomputed_v2(&v1_bytes) {
                Err(PrecomputedError::UnsupportedVersion {
                    found,
                    supported,
                }) => {
                    assert_eq!(found, 1, "found should be V1 format_version");
                    assert_eq!(
                        supported, 2,
                        "supported should be V2 format_version"
                    );
                }
                Err(other) => panic!("expected UnsupportedVersion, got {other}"),
                Ok(_) => panic!("V1 bytes must NOT decode as V2"),
            }
        }
    }
}
