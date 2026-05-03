//! `MutationRecorder` — the SOLE channel for `State` mutations that
//! Phase 5's two-layer cache (`cache.bin`) needs to replay.
//!
//! Source of truth: `crates/babel-plugin/STATE_MUTATIONS.md` (Phase 0
//! enumeration) reconciled with PLAN.md §3.9.8. Five `StateDiff`
//! variants, five `ApiKind` variants. Adding a new variant requires
//! amending STATE_MUTATIONS.md AND bumping the cache schema version
//! (Phase 5 §5.3 `CACHE_VERSION` + `SCHEMA_HASH`).
//!
//! Why this lives in its own module: PLAN.md §3.9.8's compiler-
//! enforced encapsulation discipline says the only path that writes
//! into `State`'s private fields is `MutationRecorder::apply(diff,
//! &mut state)`. The TYPE definitions for `MutationRecorder` /
//! `StateDiff` / `ApiKind` live here; the `apply` IMPL block lives
//! in `state.rs` so it has same-module access to `State`'s
//! `pub(crate)` fields without exposing them more broadly.
//!
//! Phase 2 §2.4 status: this is the §2.4 deliverable (encapsulation +
//! diff capture). Today only `CompiledImportsAppend` has live call
//! sites — the §2.3(a) dispatcher routes its API-name push through
//! the recorder. The other four variants exist as the contract for
//! Phase 4–6 handler ports (`included_files` from the
//! module-traversal cache; `sheets` from `hoist-sheet`; `css_map`
//! from `cssMap`; `ignore_member_expressions` from `css-builders`).
//! Their tests exercise apply() so the wiring is locked before the
//! handlers land.

use serde::{Deserialize, Serialize};

/// Five known Compiled APIs. Matches the upstream tuple
/// `(['styled', 'ClassNames', 'css', 'keyframes', 'cssMap'] as const)`
/// in `babel-plugin.ts` line 275.
///
/// Why an enum (not `&'static str`): the Phase 5 `cache.bin` postcard
/// payload encodes this as a single byte tag. Stable wire shape →
/// stable cache schema → reproducible cross-host caches. If you add
/// a Compiled API upstream, add it here AND bump `CACHE_VERSION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiKind {
    ClassNames,
    Css,
    Keyframes,
    Styled,
    CssMap,
}

impl ApiKind {
    /// Resolve a JS-side API name (`"styled"`, `"ClassNames"`, …) to
    /// the matching `ApiKind`. Returns `None` for non-Compiled
    /// imports — the caller skips them.
    pub fn from_imported_name(name: &str) -> Option<Self> {
        match name {
            "styled" => Some(Self::Styled),
            "ClassNames" => Some(Self::ClassNames),
            "css" => Some(Self::Css),
            "keyframes" => Some(Self::Keyframes),
            "cssMap" => Some(Self::CssMap),
            _ => None,
        }
    }
}

/// The five evaluation-visible state mutations the cache must capture
/// to replay a Layer 2 hit.
///
/// Every variant maps 1:1 to a row in `STATE_MUTATIONS.md`. Every
/// variant has a `MutationRecorder::apply` arm in `state.rs`. Every
/// variant must round-trip through postcard serde (the §5.3 cache
/// schema).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateDiff {
    /// Site 6 (`utils/css-builders.ts:325`). Highest-frequency mutation
    /// — every static-eval file open appends to the HMR-invalidation set.
    IncludedFilesPush { path: String },

    /// Site 4 (`babel-plugin.ts:282-284`). Append a Compiled API's
    /// local binding name to its bucket in `state.compiled_imports`.
    /// First evaluation-visible mutation; cache replay at handler time
    /// must see it.
    CompiledImportsAppend {
        api: ApiKind,
        local_name: String,
    },

    /// Site 8 (`utils/hoist-sheet.ts:32`). Records that the literal
    /// stylesheet `sheet_text` has been hoisted under identifier
    /// `hoisted_name`. The full Babel `t.Identifier` node is
    /// reconstructed from the string at replay time (the AST node
    /// identity is regenerated per pass; only the name survives).
    SheetsInsert {
        sheet_text: String,
        hoisted_name: String,
    },

    /// Site 5 (`css-map/index.ts:115`). After evaluating a `cssMap({...})`
    /// call, store the resulting `Vec<String>` of sheets keyed by the
    /// local binding name. Per-binding, whole-array publish — there is
    /// no per-element append.
    CssMapInsert {
        binding: String,
        sheets: Vec<String>,
    },

    /// Site 7 (`utils/css-builders.ts:725`). Mark a binding name as
    /// "known-not-cssMap" so subsequent member-expression lookups
    /// short-circuit. Boolean presence-check; the value is always
    /// `true`.
    IgnoreMemberExprMark { name: String },
}

/// Per-evaluation diff log. Owns a `Vec<StateDiff>` capturing every
/// mutation `apply` performed in iteration order. The Phase 5 cache
/// drains this into a `Layer2Entry::state_diffs` field at evaluation
/// completion (subject to the `MAX_STATE_DIFFS = 64` cap; longer logs
/// decline caching).
///
/// `apply(diff, &mut state)` itself is implemented in `state.rs` so it
/// can write through `State`'s `pub(crate)` fields without exposing
/// them via `pub(crate)`-typed mutator methods. The split mirrors
/// PLAN.md §3.9.8's "lives in the same module as State" requirement.
///
/// **Construction:** `MutationRecorder::default()` (or `::new()` —
/// they're equivalent). The visitor allocates one per `process(...)`
/// call; SWC tears the WASI instance down between transforms, so
/// per-call recorders are the only safe shape.
#[derive(Debug, Default)]
pub struct MutationRecorder {
    /// Append-only diff log. Captured for cache replay; never read by
    /// the visitor itself.
    diff_log: Vec<StateDiff>,
}

impl MutationRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read-only view of the captured diffs. Phase 5 §5.3 will use
    /// this when serialising `Layer2Entry::state_diffs` at evaluation
    /// completion. Tests use it to assert the recorder captured what
    /// it applied.
    pub fn diff_log(&self) -> &[StateDiff] {
        &self.diff_log
    }

    /// Drain the diff log, returning the captured sequence. Used by
    /// the §5.3 cache writer at evaluation completion.
    pub fn drain_diff_log(&mut self) -> Vec<StateDiff> {
        std::mem::take(&mut self.diff_log)
    }

    /// `apply` is implemented in `state.rs` (it needs same-module
    /// access to `State`'s private fields). This method is the
    /// recorder-internal hook `apply` calls to push the captured diff
    /// after performing the write.
    ///
    /// `pub(crate)` because `apply` is in `state.rs` (a sibling
    /// module under the crate root). Not part of the public API —
    /// outside `state.rs` no one should call this directly; mutate
    /// state via `apply` instead.
    pub(crate) fn push_diff(&mut self, diff: StateDiff) {
        self.diff_log.push(diff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_kind_resolves_known_compiled_apis() {
        assert_eq!(ApiKind::from_imported_name("styled"), Some(ApiKind::Styled));
        assert_eq!(
            ApiKind::from_imported_name("ClassNames"),
            Some(ApiKind::ClassNames)
        );
        assert_eq!(ApiKind::from_imported_name("css"), Some(ApiKind::Css));
        assert_eq!(
            ApiKind::from_imported_name("keyframes"),
            Some(ApiKind::Keyframes)
        );
        assert_eq!(ApiKind::from_imported_name("cssMap"), Some(ApiKind::CssMap));
    }

    #[test]
    fn api_kind_returns_none_for_non_compiled_names() {
        // Drift watch point: if upstream adds a new API, this test
        // STAYS green (the new API is unrecognised here) AND the
        // ApiKind enum gains a variant. Keeps the cache schema as the
        // single source of truth.
        assert_eq!(ApiKind::from_imported_name("jsx"), None);
        assert_eq!(ApiKind::from_imported_name("xcss"), None);
        assert_eq!(ApiKind::from_imported_name(""), None);
        assert_eq!(ApiKind::from_imported_name("CSS"), None); // case-sensitive
    }

    #[test]
    fn recorder_starts_empty() {
        let r = MutationRecorder::new();
        assert!(r.diff_log().is_empty());
    }

    #[test]
    fn drain_returns_log_and_resets() {
        let mut r = MutationRecorder::new();
        r.push_diff(StateDiff::IncludedFilesPush {
            path: "a.ts".into(),
        });
        r.push_diff(StateDiff::IncludedFilesPush {
            path: "b.ts".into(),
        });
        let drained = r.drain_diff_log();
        assert_eq!(drained.len(), 2);
        assert!(r.diff_log().is_empty());
    }

    #[test]
    fn state_diff_round_trips_through_serde_json() {
        // Phase 5 §5.3 will lock the wire format as postcard for
        // `cache.bin`. Until then, serde JSON round-trip is the
        // available serde guarantor — every StateDiff variant must
        // serialize and deserialize cleanly. When the postcard dep
        // lands (§5.3) this test gets duplicated against postcard so
        // the wire format is locked from BOTH sides; the JSON one
        // stays as a debug-readable format for `cache_inspect.rs`'s
        // `--dump-as-json` flag (PLAN.md §3.9.10).
        let cases = vec![
            StateDiff::IncludedFilesPush {
                path: "src/theme.ts".into(),
            },
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name: "MyStyled".into(),
            },
            StateDiff::CompiledImportsAppend {
                api: ApiKind::CssMap,
                local_name: "cm".into(),
            },
            StateDiff::SheetsInsert {
                sheet_text: "._abc{color:red}".into(),
                hoisted_name: "_abc".into(),
            },
            StateDiff::CssMapInsert {
                binding: "vars".into(),
                sheets: vec!["._a{color:red}".into(), "._b{color:blue}".into()],
            },
            StateDiff::IgnoreMemberExprMark {
                name: "theme".into(),
            },
        ];
        for diff in cases {
            let json = serde_json::to_string(&diff).expect("serialize");
            let back: StateDiff = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(diff, back);
        }
    }
}
