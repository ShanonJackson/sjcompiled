//! `crates/cssnano-browserslist-snapshot`
//!
//! Host-resolved browserslist snapshot for the cssnano-preset-default
//! WASI fast path. Sibling abstraction to
//! [`crate::autoprefixer::precomputed`](../../autoprefixer/src/precomputed.rs) —
//! same architectural shape, smaller payload because the cssnano
//! plugins do less per-call browserslist-derived work than
//! autoprefixer.
//!
//! # Why this exists
//!
//! AFM's 1000-file production sample reports ~40 byte-divergent files
//! between the Babel pipeline (production today) and the SWC pipeline
//! (migration target). Empirical investigation
//! ([DEFINITIVE_BROWSERSLIST_PLAN.md §1.2](../../../DEFINITIVE_BROWSERSLIST_PLAN.md))
//! traced ~35 of them to one root cause:
//!
//!   - The Babel pipeline's leaf cssnano plugins (`postcss-reduce-initial`,
//!     `postcss-colormin`, etc.) call `browserslist(null, { path: __dirname })`
//!     internally. `__dirname` is the leaf plugin's installed location
//!     under `jira/node_modules/postcss-{reduce-initial|colormin}/src/`.
//!     `browserslist`'s `find_config_file` walks up from there → reaches
//!     `jira/.browserslistrc` (Yarn-classic-hoisted layout) → resolves
//!     to AFM's 14-entry modern browser list. CSS-`initial` is universally
//!     supported on that list, so reduce-initial preserves the keyword.
//!
//!   - The Rust-port leaf plugins do the same dance through
//!     [`browserslist_shim::resolve("", true)`] — which works in NAPI
//!     (cwd-walk lands on the same `.browserslistrc`) but fails in the
//!     SWC plugin's WASI sandbox (the host's `node_modules` and env
//!     vars don't cross the WASI boundary; only `process.cwd()` is
//!     preopened at `/cwd`, walking up from `/cwd` hits `/` and finds
//!     no config). Falls through to `browserslist@4.24.2` defaults
//!     (`> 0.5%, last 2 versions, Firefox ESR, not dead`) which
//!     includes ancient browsers → CSS-`initial` not universally
//!     supported → reduce-initial substitutes `initial` to its
//!     concrete fallback (`currentColor`, `transparent`, `content-box`,
//!     etc.) → divergence.
//!
//! The fix: **resolve browserslist on the host (where `process.cwd()`,
//! `process.env`, and the real FS exist), serialise the resolved list,
//! ship it through SWC's `experimental.plugins[i][1]` config to the
//! WASI plugin, and have the cssnano leaf plugins consume it instead
//! of the broken in-WASI resolution.** This crate is the snapshot
//! abstraction; the leaf-plugin consumers and the babel-plugin
//! plumbing live in their respective crates.
//!
//! # Schema
//!
//! Two fields:
//!
//!   - [`PrecomputedBrowserslist::selected`] — the resolved
//!     `"<name> <version>"` strings. Mirrors `Browsers.selected` in
//!     the autoprefixer snapshot. Used by leaf plugins for
//!     membership probes (`.iter().any(|b| LEGACY_SET.contains(b))`).
//!   - [`PrecomputedBrowserslist::joined_query`] — `selected` joined
//!     with `", "`, ready to pass to
//!     [`caniuse_api::is_supported`] without the leaf plugin having
//!     to re-join on every call.
//!
//! No pre-evaluated `feature_support: IndexMap<String, bool>`. See
//! `DEFINITIVE_BROWSERSLIST_PLAN.md §3.5` for the rejection rationale —
//! TL;DR: `is_supported` against literal `"name version"` query atoms
//! is cheap (sub-µs) on the AFM fast path; pre-evaluating adds drift
//! surface (every new plugin port has to register its features) for
//! negligible perf gain.
//!
//! # Byte-equivalence contract
//!
//! For any leaf plugin consuming this snapshot:
//!
//!   *(plugin output with `Some(snapshot)`) ≡ (plugin output with `None`)
//!   when invoked under conditions that make the in-plugin
//!   [`browserslist_shim::resolve("", true)`] resolve to the same list
//!   as `snapshot.selected`.*
//!
//! Pinned by:
//!
//!   - This crate's `tests::joined_query_resolves_back_to_selected_via_shim`:
//!     asserts the AFM fast path is a no-op for our serialised query
//!     (Vec → join → resolve → Vec is identity).
//!   - Each leaf plugin's `tests/snapshot_parity.rs`: with-vs-without
//!     snapshot under matching cwd produces identical bytes.
//!   - End-to-end `crates/babel-plugin/tests/transform_css_browserslist_snapshot_integration.rs`:
//!     `transform_css` with `Some(snapshot)` byte-equals
//!     `transform_css` with `BROWSERSLIST_CONFIG` env-pinned to the
//!     AFM fixture.
//!
//! # Versioning
//!
//! [`PrecomputedBrowserslist::format_version`] is the leading field
//! (so [`peek_format_version`] can read it without committing to the
//! struct layout). Bump on any field change. Old blobs are rejected
//! with [`PrecomputedError::UnsupportedVersion`] rather than silently
//! mis-decoded.

use serde::{Deserialize, Serialize};

/// Layout version of the precomputed snapshot. Currently `1` — the
/// initial format with `selected` + `joined_query`. Bump on any field
/// addition / removal / type change. Bumping requires a coordinated
/// host-bootstrap update so AFM's bootstrap doesn't ship a
/// version-mismatched snapshot.
pub const PRECOMPUTED_FORMAT_VERSION: u32 = 1;

/// Serialisable snapshot of the inputs the cssnano leaf plugins need
/// to make their browserslist-derived decisions without running
/// [`browserslist_shim::resolve`] inside the WASI sandbox.
///
/// **Drift contract:** equivalent — for every observable plugin
/// behaviour — to running the leaf plugin's existing
/// `browserslist_shim::resolve("", true)` path under matching host
/// cwd. Pinned by [`tests::joined_query_resolves_back_to_selected_via_shim`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrecomputedBrowserslist {
    /// Format version — always [`PRECOMPUTED_FORMAT_VERSION`]. First
    /// field so [`peek_format_version`] can read it without committing
    /// to the full struct layout. Mirrors
    /// [`autoprefixer::precomputed::PrecomputedPrefixes::format_version`]
    /// for cross-snapshot consistency.
    pub format_version: u32,
    /// Resolved `"<name> <version>"` entries. Mirrors `Browsers.selected`
    /// in the autoprefixer snapshot. Consumed by leaf-plugin membership
    /// probes such as
    /// `selected.iter().any(|b| LEGACY_SET.contains(b.as_str()))`
    /// (see e.g. `cssnano-postcss-minify-params::ALL_BUG_BROWSERS`).
    pub selected: Vec<String>,
    /// `selected` joined with `", "`. The form leaf plugins pass
    /// directly to [`caniuse_api::is_supported(feature, &joined_query)`],
    /// avoiding a per-call re-join. For an empty `selected` the joined
    /// query is the empty string — which makes
    /// [`caniuse_api::is_supported`] fall through to its
    /// [`browserslist_shim::resolve`] path internally, identical to
    /// today's behaviour. (No empty-snapshot footgun: producing one
    /// requires explicit caller intent.)
    pub joined_query: String,
}

/// Errors surfaced by snapshot decoding.
#[derive(Debug)]
pub enum PrecomputedError {
    /// `postcard` decode failed — truncation, layout mismatch, or
    /// non-snapshot bytes.
    Decode(postcard::Error),
    /// Snapshot's `format_version` doesn't match
    /// [`PRECOMPUTED_FORMAT_VERSION`]. Surfaced as a hard error rather
    /// than silent mis-decode so a stale host-side snapshot blob in
    /// production fails loud.
    UnsupportedVersion { found: u32, supported: u32 },
}

impl std::fmt::Display for PrecomputedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "postcard decode error: {e}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "precomputed browserslist format version {found} is not supported (this build expects {supported})"
            ),
        }
    }
}

impl std::error::Error for PrecomputedError {}

/// Options for [`precompute_browserslist`]. Mirrors the subset of
/// `browserslist(null, opts)` AFM exercises:
///
///   - `path` — directory or file used as the anchor for
///     `find_config_file` walk-up. AFM's bootstrap passes
///     `require.resolve('postcss-reduce-initial/package.json')` here
///     so the host walk-up is provably equivalent to the leaf
///     plugin's own `__dirname` walk (see
///     `DEFINITIVE_BROWSERSLIST_PLAN.md §4`).
///   - `env` — section selector for `[production]` / `[development]`
///     blocks. Defaults to `BROWSERSLIST_ENV` → `NODE_ENV` →
///     `"production"`, matching `browserslist@4.24.2`. AFM's
///     `.browserslistrc` has no sections so this is irrelevant for
///     them; included for parity completeness.
///
/// Intentionally NOT exposed:
///   - `BROWSERSLIST` query env var — short-circuits config-file
///     resolution; the bootstrap should pass the explicit list rather
///     than depend on env. If a future caller needs it we add a
///     `query: Option<String>` field; today's surface is config-file-only.
///   - `stats` — usage-data weighting; AFM doesn't use it.
#[derive(Debug, Clone, Default)]
pub struct PrecomputeBrowserslistOpts {
    pub path: Option<std::path::PathBuf>,
    pub env: Option<String>,
}

/// Run the full slow path on the host: FS walk for `.browserslistrc`,
/// browserslist resolution against the pinned `caniuse-db` snapshot
/// via [`browserslist-shim`]. Snapshot the result.
///
/// Intended to run **once per host process** (e.g. AFM's babel.js
/// bootstrap), with the resulting bytes shipped to every WASI plugin
/// invocation. The cost (~tens of µs to single-digit ms depending on
/// the FS-walk depth) is amortised across the entire build's
/// transforms.
///
/// # Behaviour
///
/// Equivalent to JS `browserslist(null, { path: opts.path, env: opts.env })`
/// piped through `Array.prototype.join(', ')`. Specifically:
///
///   1. `browserslist_shim::resolve_with("", &ResolveOpts { path, env, .. })`
///      — empty explicit query forces the config-file resolution path
///      (`load_config(path, env)`).
///   2. Join with `", "`.
///   3. Wrap in [`PrecomputedBrowserslist`].
///
/// **Returns the `browserslist@4.24.2` defaults if no config is
/// found.** AFM's bootstrap MUST pass a `path` that reaches the
/// production `.browserslistrc` — silent fallback to defaults would
/// re-introduce the bug we're fixing. The bootstrap snippet at
/// `DEFINITIVE_BROWSERSLIST_PLAN.md §4` uses
/// `require.resolve('postcss-reduce-initial/package.json')` precisely
/// to make this resolution provable rather than coincidental.
pub fn precompute_browserslist(opts: PrecomputeBrowserslistOpts) -> PrecomputedBrowserslist {
    let resolve_opts = browserslist_shim::index::ResolveOpts {
        path: opts.path.as_deref(),
        env: opts.env.as_deref(),
        // Mirror the leaf plugins' `is_supported` behaviour, which
        // calls `browserslist_shim::resolve(query, true)` with
        // `ignore_unknown_versions: true` (`crates/caniuse-api/src/index.rs:95`).
        // Same flag here means our snapshot's `selected` is the same
        // shape the leaf plugins would derive themselves under
        // matching host cwd.
        ignore_unknown_versions: true,
    };
    let selected = browserslist_shim::index::resolve_with("", &resolve_opts);
    let joined_query = selected.join(", ");
    PrecomputedBrowserslist {
        format_version: PRECOMPUTED_FORMAT_VERSION,
        selected,
        joined_query,
    }
}

/// Default invocation — no `path`, no `env`. Equivalent to
/// `precompute_browserslist(PrecomputeBrowserslistOpts::default())`.
///
/// Useful for tests and for hosts that pass `BROWSERSLIST_CONFIG` /
/// `BROWSERSLIST` env vars (the shim's [`browserslist_shim::node::load_config`]
/// reads them when no `path` is provided). Production AFM bootstrap
/// passes a `path` explicitly — see
/// `DEFINITIVE_BROWSERSLIST_PLAN.md §4`.
pub fn precompute_browserslist_default() -> PrecomputedBrowserslist {
    precompute_browserslist(PrecomputeBrowserslistOpts::default())
}

/// Serialise to postcard bytes. Stable across runs given stable
/// inputs (postcard is deterministic).
pub fn encode_precomputed(snapshot: &PrecomputedBrowserslist) -> Vec<u8> {
    postcard::to_allocvec(snapshot).expect(
        "postcard encode of PrecomputedBrowserslist cannot fail (no IO, no fallible serializers)",
    )
}

/// Decode postcard bytes into a snapshot. Returns `Err` on truncation,
/// layout mismatch, or version drift.
pub fn decode_precomputed(bytes: &[u8]) -> Result<PrecomputedBrowserslist, PrecomputedError> {
    let version = peek_format_version(bytes)?;
    if version != PRECOMPUTED_FORMAT_VERSION {
        return Err(PrecomputedError::UnsupportedVersion {
            found: version,
            supported: PRECOMPUTED_FORMAT_VERSION,
        });
    }
    postcard::from_bytes(bytes).map_err(PrecomputedError::Decode)
}

/// Read the leading `format_version: u32` without committing to the
/// full struct layout. Used by [`decode_precomputed`] to surface a
/// clean [`PrecomputedError::UnsupportedVersion`] for legacy blobs
/// instead of a postcard truncation/format error.
fn peek_format_version(bytes: &[u8]) -> Result<u32, PrecomputedError> {
    let (version, _rest) =
        postcard::take_from_bytes::<u32>(bytes).map_err(PrecomputedError::Decode)?;
    Ok(version)
}

// ---------------------------------------------------------------------------
// Tests — Phase E gates E1, E2, E3.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// E1 — round-trip identity. encode → decode preserves all fields.
    #[test]
    fn precompute_then_decode_roundtrip_byte_identical() {
        let original = PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: vec![
                "chrome 144".to_string(),
                "firefox 147".to_string(),
                "safari 26.2".to_string(),
            ],
            joined_query: "chrome 144, firefox 147, safari 26.2".to_string(),
        };
        let bytes = encode_precomputed(&original);
        let decoded = decode_precomputed(&bytes).expect("decode must succeed");
        assert_eq!(decoded, original);
    }

    /// E2 — THE schema-choice gate. The `joined_query` field must
    /// resolve back to `selected` via [`browserslist_shim::resolve`]
    /// for the AFM canonical 14-entry list. If this fails, the
    /// schema-design assumption "leaf plugins can pass `joined_query`
    /// to `caniuse_api::is_supported` and get identical results to
    /// running against the resolved list directly" is wrong, and we
    /// have to escalate (pre-evaluate `is_supported` results inside
    /// the snapshot, OR pull resolved `Vec<String>` deeper into
    /// `caniuse_api`).
    #[test]
    fn joined_query_resolves_back_to_selected_via_shim() {
        // The AFM canonical list — frozen against AFM-PROBE empirical
        // output (see DEFINITIVE_BROWSERSLIST_PLAN.md §10) and
        // already pinned by `browserslist_shim::index::tests::afm_fast_path_full_query_byte_clean`.
        let afm_canonical: Vec<String> = vec![
            "and_chr 144",
            "chrome 144",
            "chrome 143",
            "chrome 142",
            "chrome 141",
            "chrome 140",
            "edge 144",
            "edge 143",
            "firefox 147",
            "firefox 146",
            "ios_saf 26.2",
            "ios_saf 26.1",
            "safari 26.2",
            "safari 26.1",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let snapshot = PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: afm_canonical.clone(),
            joined_query: afm_canonical.join(", "),
        };

        // Round-trip through the shim: joined_query → resolve → list.
        // Must equal selected (modulo the shim's canonical sort,
        // which the AFM canonical list above is ALREADY in — see
        // browserslist-shim's `sort_distribs` order rules).
        let resolved = browserslist_shim::resolve(&snapshot.joined_query, true);
        assert_eq!(
            resolved, snapshot.selected,
            "joined_query → resolve drifted from selected. \
             Schema assumption `caniuse_api::is_supported(feature, &joined_query)` \
             gives identical results to `is_supported(feature, &resolved)` is broken — \
             escalate per DEFINITIVE_BROWSERSLIST_PLAN.md §3.5."
        );
    }

    /// E3 — version mismatch surfaces a clean error.
    #[test]
    fn legacy_version_byte_rejected() {
        let mut snap = PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: vec!["chrome 144".to_string()],
            joined_query: "chrome 144".to_string(),
        };
        // Pretend this is a future v2 blob — should be rejected by
        // this v1 build with a clean UnsupportedVersion error.
        snap.format_version = 2;
        let bytes = encode_precomputed(&snap);

        match decode_precomputed(&bytes) {
            Err(PrecomputedError::UnsupportedVersion { found, supported }) => {
                assert_eq!(found, 2);
                assert_eq!(supported, PRECOMPUTED_FORMAT_VERSION);
            }
            Err(other) => panic!("expected UnsupportedVersion, got {other}"),
            Ok(_) => panic!("version-mismatched blob must NOT decode"),
        }
    }

    /// E4 — sanity: `precompute_browserslist_default()` produces a
    /// non-empty list when invoked with no config (falls through to
    /// `browserslist@4.24.2` defaults, which is non-empty by
    /// construction). This is a smoke test, NOT a pin against the
    /// canonical AFM list — that comparison only makes sense with a
    /// `path` argument that reaches the AFM fixture, exercised by
    /// `crates/babel-plugin/tests/transform_css_browserslist_snapshot_integration.rs`.
    #[test]
    fn precompute_default_non_empty_list() {
        let snap = precompute_browserslist_default();
        assert_eq!(snap.format_version, PRECOMPUTED_FORMAT_VERSION);
        assert!(
            !snap.selected.is_empty(),
            "default precompute should fall back to browserslist defaults (non-empty)"
        );
        assert_eq!(snap.joined_query, snap.selected.join(", "));
    }

    /// Additional gate: round-trip the canonical AFM list through
    /// `precompute → encode → decode` (i.e. the actual production
    /// path the bootstrap exercises). Verifies our wire format
    /// preserves the canonical list bit-for-bit, not just a synthetic
    /// 3-entry list.
    #[test]
    fn afm_canonical_roundtrip_via_postcard() {
        let afm_canonical: Vec<String> = vec![
            "and_chr 144", "chrome 144", "chrome 143", "chrome 142",
            "chrome 141", "chrome 140", "edge 144", "edge 143",
            "firefox 147", "firefox 146", "ios_saf 26.2", "ios_saf 26.1",
            "safari 26.2", "safari 26.1",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let original = PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: afm_canonical.clone(),
            joined_query: afm_canonical.join(", "),
        };
        let bytes = encode_precomputed(&original);
        let decoded = decode_precomputed(&bytes).expect("AFM canonical decode must succeed");
        assert_eq!(decoded, original);
    }

    /// **E4-strict** — Phase E gate that exercises the FULL host
    /// bootstrap path: feed `precompute_browserslist` the vendored
    /// AFM `.browserslistrc` via `path:`, run the actual upward
    /// `find_config` walk + `browserslist` resolution, and
    /// cross-check the result against the AFM-team-confirmed 14
    /// modern entries (their parity-oracle probe in
    /// `DEFINITIVE_BROWSERSLIST_PLAN.md` §5).
    ///
    /// Drift gate: if the vendored caniuse-db, the shim's AFM fast
    /// path, or oxc-browserslist's resolution shifts the result
    /// even by one entry, this test fails LOUDLY — instead of
    /// silently shipping a snapshot that drifts from the modern
    /// Jira list. Locks the contract that the Phase D harness path
    /// (`engines.ts`) relies on.
    #[test]
    fn precompute_against_afm_fixture_yields_canonical_14_entries() {
        // Vendored at `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`.
        // The path here is relative to this crate's manifest dir;
        // `CARGO_MANIFEST_DIR` ends in
        // `crates/cssnano-browserslist-snapshot`, so we walk one
        // level up to reach `crates/`.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let fixture = std::path::PathBuf::from(manifest_dir)
            .parent()
            .expect("manifest dir has parent")
            .join("browserslist-shim/tests/fixtures/afm/.browserslistrc");
        assert!(
            fixture.exists(),
            "AFM fixture missing at {} — has it moved?",
            fixture.display(),
        );

        let snap = precompute_browserslist(PrecomputeBrowserslistOpts {
            path: Some(fixture),
            env: None,
        });

        let expected: Vec<String> = vec![
            "and_chr 144", "chrome 144", "chrome 143", "chrome 142",
            "chrome 141", "chrome 140", "edge 144", "edge 143",
            "firefox 147", "firefox 146", "ios_saf 26.2", "ios_saf 26.1",
            "safari 26.2", "safari 26.1",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        assert_eq!(
            snap.selected, expected,
            "AFM canonical 14 list drift — snap.selected diverged \
             from the AFM-team-confirmed list. If the upstream \
             caniuse-db or shim resolution legitimately changed, \
             update both this test and `engines.ts` together.",
        );
        assert_eq!(snap.joined_query, expected.join(", "));
        assert_eq!(snap.format_version, PRECOMPUTED_FORMAT_VERSION);
    }
}
