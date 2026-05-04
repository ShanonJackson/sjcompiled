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
//! All three are pure functions of `(browserslist query, options)` —
//! their output is stable across calls.
//!
//! ## What we precompute
//!
//! All three. The snapshot carries:
//!   - `selected: Vec<String>` (browsers resolved by step 1)
//!   - `add_table` / `remove_table: IndexMap<String, Vec<String>>` (step 2)
//!   - `populated_add: AddTable` / `populated_remove: RemoveTable` (step 3)
//!   - `options: PrefixesOptionsSnapshot`
//!
//! All postcard-serializable. The `populated_*` tables are the
//! V2-specific addition that elides the ~345 µs/call `preprocess()`
//! cost on each cold start; the snapshot decoder moves them directly
//! into the `Prefixes.add` / `Prefixes.remove` `RefCell`s.
//!
//! ## Byte-equality contract
//!
//! [`build_prefixes_from_precomputed`] is byte-identical to the slow
//! `build_prefixes_default` path in EVERY observable way that reaches
//! the `processor.add` / `processor.remove` walks. Pinned by
//! `precomputed::tests::v2::*` (live-built vs decoded `Prefixes`
//! field-by-field, including IndexMap insertion order).
//!
//! ## Versioning
//!
//! [`PrecomputedPrefixes`] carries a `format_version: u32` so the
//! consumer can reject blobs produced by a future incompatible
//! layout. Bump on any field change. Old blobs in the wild become
//! a hard error rather than silent drift.

use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

use crate::autoprefixer::{build_prefixes, AutoprefixerOptions};
use crate::browsers::{BrowserslistOpts, Browsers, BrowsersOptions};
use crate::prefixes::{AddTable, Prefixes, PrefixesOptions, RemoveTable};
use crate::supports::Supports;

/// Layout version of the precomputed snapshot. Currently `2` — the
/// post-`preprocess()` populated-tables format. Version `1` (pre-
/// `preprocess()` only) was retired on 2026-05-04; bytes encoded under
/// V1 are rejected with [`PrecomputedError::UnsupportedVersion`].
pub const PRECOMPUTED_FORMAT_VERSION: u32 = 2;

/// Serializable snapshot of the inputs required to skip
/// `Browsers::new` + `Prefixes::select` + `Prefixes::preprocess`.
/// Equivalent to the JS-side `loadPrefixes()` cache value plus the
/// downstream populated tables, but as plain data rather than a
/// runtime-bound object graph.
///
/// **Drift contract:** byte-identical to a freshly-built `Prefixes`
/// post-`preprocess()` for every observable field. Pinned by
/// `precomputed::tests::v2::*` (this file).
///
/// Not `Clone` — the populated tables hold types that intentionally
/// don't derive `Clone` (`ValueBase` carries a `OnceCell<WordRegexp>`
/// runtime cache; cloning is meaningless). [`build_prefixes_from_snapshot`]
/// consumes the snapshot so the populated tables can be moved into
/// the `Prefixes` instance instead of cloned.
#[derive(Serialize, Deserialize)]
pub struct PrecomputedPrefixes {
    /// Format version — always [`PRECOMPUTED_FORMAT_VERSION`]. Kept as
    /// the FIRST field so [`peek_format_version`] can read it without
    /// committing to the full struct layout.
    pub format_version: u32,
    /// Mirrors `Browsers.selected` — the resolved "name version"
    /// strings.
    pub selected: Vec<String>,
    /// Mirrors `Selected.add` — the pre-`preprocess()` per-name prefix
    /// list. Kept in the snapshot because cleaner-cache rebuilds (lazy,
    /// on `Prefixes::cleaner()`) call `Prefixes::new` which needs it.
    pub add_table: IndexMap<String, Vec<String>>,
    /// Mirrors `Selected.remove`. Same reason as `add_table`.
    pub remove_table: IndexMap<String, Vec<String>>,
    /// Mirrors `PrefixesOptions`. Embedded directly (rather than via
    /// `#[serde(flatten)]`) so the postcard schema stays explicit.
    pub options: PrefixesOptionsSnapshot,
    /// Populated `Prefixes.add` after `preprocess()`. Contains the
    /// per-name `AddBucket` instances the processor walks.
    pub populated_add: AddTable,
    /// Populated `Prefixes.remove` after `preprocess()`.
    pub populated_remove: RemoveTable,
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

/// Build the precompute snapshot for a given query / options. Runs the
/// full slow path (select + preprocess) ONCE and snapshots all outputs.
/// Intended to run once at host startup; downstream WASI plugin calls
/// receive the resulting bytes via plugin_config or filesystem path.
pub fn precompute_prefixes(
    options: AutoprefixerOptions,
) -> PrecomputedPrefixes {
    let prefixes = build_prefixes(None, options).expect(
        "build_prefixes failed during precompute (browserslist resolution error). \
         Verify the resolved cwd has a valid .browserslistrc reachable.",
    );

    // RefCell::replace takes &self — no need for mut. Move populated
    // tables out of the Prefixes (about to be dropped).
    let populated_add = prefixes.add.replace(AddTable::default());
    let populated_remove = prefixes.remove.replace(RemoveTable::default());

    PrecomputedPrefixes {
        format_version: PRECOMPUTED_FORMAT_VERSION,
        selected: prefixes.browsers.selected.clone(),
        add_table: prefixes.add_table.clone(),
        remove_table: prefixes.remove_table.clone(),
        options: PrefixesOptionsSnapshot::from(&prefixes.options),
        populated_add,
        populated_remove,
    }
}

/// Convenience: snapshot the AFM call-site shape — empty query,
/// no overrides. Equivalent to the inputs `build_prefixes_default(None)`
/// resolves.
pub fn precompute_prefixes_default() -> PrecomputedPrefixes {
    precompute_prefixes(AutoprefixerOptions::default())
}

/// Serialize a snapshot to postcard bytes. Stable across runs given
/// stable inputs (postcard is deterministic).
pub fn encode_precomputed(snapshot: &PrecomputedPrefixes) -> Vec<u8> {
    postcard::to_allocvec(snapshot)
        .expect("postcard encode of PrecomputedPrefixes cannot fail (no IO, no fallible serializers)")
}

/// Decode postcard bytes into a snapshot. Returns `Err` on truncation
/// or version mismatch (e.g., legacy V1 blobs from before 2026-05-04).
pub fn decode_precomputed(
    bytes: &[u8],
) -> Result<PrecomputedPrefixes, PrecomputedError> {
    // Peek the version first so we can produce a clean error for
    // legacy-version bytes instead of a postcard truncation/format
    // error.
    let version = peek_format_version(bytes)?;
    if version != PRECOMPUTED_FORMAT_VERSION {
        return Err(PrecomputedError::UnsupportedVersion {
            found: version,
            supported: PRECOMPUTED_FORMAT_VERSION,
        });
    }
    postcard::from_bytes(bytes).map_err(PrecomputedError::Decode)
}

/// Reconstruct a [`Prefixes`] from a precomputed snapshot. Skips
/// `select()` AND `preprocess()` entirely — populated tables move
/// directly from the snapshot into the `Prefixes` `RefCell`s.
///
/// **Consumes** the snapshot so the populated tables can be moved
/// rather than cloned. If a caller needs the snapshot multiple times,
/// they should re-decode the bytes each time — that's the WASI hot
/// path anyway (one decode per transform).
///
/// `cleaner_cache` and `supports_inst` are freshly initialized: both
/// are runtime caches whose lazy first-access rebuild is byte-equivalent
/// to the live path:
///   - `cleaner()` is lazy; first access rebuilds against the same
///     snapshotted inputs, producing an equivalent `Prefixes`.
///   - `Supports::new()` starts with `prefixer_cache: None`; first
///     access populates against the same options/data the live path
///     would.
pub fn build_prefixes_from_snapshot(snapshot: PrecomputedPrefixes) -> Prefixes {
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

/// Decode + reconstruct in one shot. The WASI hot path.
pub fn build_prefixes_from_precomputed(
    bytes: &[u8],
) -> Result<Prefixes, PrecomputedError> {
    let snapshot = decode_precomputed(bytes)?;
    Ok(build_prefixes_from_snapshot(snapshot))
}

/// Peek at the leading `format_version: u32` without committing to a
/// full struct layout. Used by `decode_precomputed` to surface a clean
/// `UnsupportedVersion` error for legacy blobs instead of a postcard
/// truncation/format error.
fn peek_format_version(bytes: &[u8]) -> Result<u32, PrecomputedError> {
    let (version, _rest) =
        postcard::take_from_bytes::<u32>(bytes).map_err(PrecomputedError::Decode)?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoprefixer::build_prefixes_default;
    use crate::prefixes::{AddBucket, RemoveBucket};
    use crate::test_support::afm_fixture_dir;

    /// Build live + snapshot-decoded `Prefixes` from the same AFM query.
    fn live_and_decoded() -> (Prefixes, Prefixes) {
        std::env::set_current_dir(afm_fixture_dir())
            .expect("set cwd to AFM fixture dir");

        let live = build_prefixes_default(None).expect("live build");

        let snap = precompute_prefixes_default();
        let bytes = encode_precomputed(&snap);
        let decoded = decode_precomputed(&bytes).expect("decode");
        let from_snapshot = build_prefixes_from_snapshot(decoded);

        (live, from_snapshot)
    }

    /// Pre-`preprocess()` fields agree, including `IndexMap` insertion
    /// order. `IndexMap::eq` is order-INSENSITIVE — collect to Vec to
    /// catch order drift.
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

        let live_add: Vec<_> = live.add_table.iter().collect();
        let fast_add: Vec<_> = fast.add_table.iter().collect();
        assert_eq!(
            live_add, fast_add,
            "add_table insertion order drifted across round-trip"
        );

        let live_remove: Vec<_> = live.remove_table.iter().collect();
        let fast_remove: Vec<_> = fast.remove_table.iter().collect();
        assert_eq!(
            live_remove, fast_remove,
            "remove_table insertion order drifted across round-trip"
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
            assert_eq!(l.prefixed, f.prefixed, "selectors[{i}].prefixed drifted");
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

    #[test]
    fn legacy_version_byte_rejected() {
        std::env::set_current_dir(afm_fixture_dir())
            .expect("set cwd to AFM fixture dir");

        let mut snap = precompute_prefixes_default();
        snap.format_version = 1; // pretend this is an old V1 blob
        let bytes = encode_precomputed(&snap);

        match decode_precomputed(&bytes) {
            Err(PrecomputedError::UnsupportedVersion { found, supported }) => {
                assert_eq!(found, 1);
                assert_eq!(supported, PRECOMPUTED_FORMAT_VERSION);
            }
            Err(other) => panic!("expected UnsupportedVersion, got {other}"),
            Ok(_) => panic!("legacy version must NOT decode"),
        }
    }
}
