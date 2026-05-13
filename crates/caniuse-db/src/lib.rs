//! crates/caniuse-db
//! Vendored data tables for `caniuse-lite@1.0.30001766`.
//!
//! Per `crates/PARITY_VERSIONS.md` Anomaly #3, this version is frozen
//! forever for the parity port.
//!
//! The actual JSON snapshot is produced by `scripts/snapshot.js` (Node.js,
//! one-shot) which uses upstream's bundled unpacker against the vendored
//! `crates/_vendor/caniuse-lite-1.0.30001766` tarball. The result lives at
//! `data/features.snapshot.json` and is `include_str!`'d at compile time.
//!
//! # Prune policy (2026-05-14)
//!
//! The snapshot is NOT a verbatim copy of upstream. `scripts/snapshot.js`
//! applies a prune policy that drops version-level data far below AFM's
//! resolved browserslist matrix, shrinking the on-disk snapshot ~69%
//! (4.0 MB → 1.3 MB) without changing observable behavior for any query
//! AFM's `.browserslistrc` can resolve to. Floors:
//!
//! | Browser   | Floor | AFM resolves to (snapshot date) |
//! |-----------|-------|---------------------------------|
//! | chrome    | 120   | ~140-144 (last 5)              |
//! | edge      | 120   | ~143-144 (last 2)              |
//! | firefox   | 100   | ~141-142 + ESR 115, 128        |
//! | safari    |  16   | ~18.5-18.6 (last 2)            |
//! | ios_saf   |  16   | ~18.5-18.6 (last 2)            |
//! | and_chr   | 120   | ~143 (last 2)                  |
//! | samsung   |  20   | (not in AFM matrix)            |
//! | opera     | 100   | (not in AFM matrix)            |
//!
//! Dead browsers (`ie`, `ie_mob`, `op_mini`, `op_mob`, `bb`, `and_uc`,
//! `and_qq`, `baidu`, `kaios`, `android`, `and_ff`) keep their agent
//! stub — `agent.prefix` is preserved — but all version-level data is
//! emptied. **Do not drop dead-browser agents entirely**:
//! `autoprefixer/src/browsers.rs::build_prefixes` iterates `AGENTS` to
//! construct the prefix-recognition set (`-ms-`, `-webkit-`, `-moz-`,
//! `-o-`). Removing the `ie` agent would strip `-ms-` from that set
//! and silently break autoprefixer's recognition of `-ms-flex` etc. in
//! user input.
//!
//! The floor-invariant test below enforces the policy so future regens
//! cannot accidentally widen the snapshot back to upstream's verbatim
//! shape.

pub mod features;
pub mod agents;

pub const CANIUSE_LITE_VERSION: &str = "1.0.30001766";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_version_matches_pin() {
        assert_eq!(features::snapshot_version(), CANIUSE_LITE_VERSION);
    }

    #[test]
    fn flexbox_loaded() {
        let f = features::feature("flexbox").expect("flexbox feature exists");
        assert_eq!(f.title, "CSS Flexible Box Layout Module");
        assert!(!f.stats.is_empty());
    }

    #[test]
    fn css_grid_loaded() {
        let f = features::feature("css-grid").expect("css-grid feature exists");
        assert!(!f.stats.is_empty());
    }

    #[test]
    fn agent_chrome_loaded() {
        let a = agents::agent("chrome").expect("chrome agent exists");
        assert!(!a.versions.is_empty());
    }

    #[test]
    fn list_returns_582() {
        // Pinned snapshot (caniuse-lite@1.0.30001766) has exactly 582
        // features. Drift here means the vendored data has been
        // regenerated — investigate before merging.
        // (Was 579 at caniuse-lite@1.0.30001690; AFM repin bumped to 582.)
        assert_eq!(features::list().len(), 582);
    }

    /// Enforce the prune policy documented at the top of this file.
    /// If this test fails, `scripts/snapshot.js` has been re-run with a
    /// different policy (or no policy) — review `data/features.snapshot.json`
    /// against the floors table before merging.
    #[test]
    fn prune_policy_floors_hold() {
        let floors: &[(&str, f64)] = &[
            ("chrome",  120.0),
            ("edge",    120.0),
            ("firefox", 100.0),
            ("safari",   16.0),
            ("ios_saf",  16.0),
            ("and_chr", 120.0),
            ("samsung",  20.0),
            ("opera",   100.0),
        ];
        for (br, floor) in floors {
            let a = agents::agent(br).unwrap_or_else(|| panic!("agent `{br}` missing"));
            let min_kept: Option<f64> = a
                .versions
                .iter()
                .filter_map(|v| v.as_deref())
                .filter_map(|v| v.split('-').next().unwrap_or(v).parse::<f64>().ok())
                .reduce(f64::min);
            if let Some(m) = min_kept {
                assert!(
                    m >= *floor,
                    "agent `{br}`: pruned versions include {m} which is below floor {floor}"
                );
            }
        }
    }

    /// Every dead-browser agent must still be present with its `.prefix`
    /// field populated (autoprefixer's `build_prefixes` depends on this),
    /// but with all version-level data emptied.
    #[test]
    fn dead_browsers_keep_prefix_stub() {
        let dead = ["ie", "ie_mob", "op_mini", "op_mob", "bb",
                    "and_uc", "and_qq", "baidu", "kaios", "android", "and_ff"];
        for br in &dead {
            let a = agents::agent(br).unwrap_or_else(|| panic!("dead agent `{br}` missing"));
            assert!(!a.prefix.is_empty(), "dead agent `{br}` lost its prefix");
            assert!(
                a.versions.iter().filter_map(|v| v.as_deref()).next().is_none(),
                "dead agent `{br}` should have no concrete versions, found some"
            );
        }
        // The `-ms-`, `-webkit-`, `-moz-`, `-o-` set must still be derivable.
        let prefixes: std::collections::HashSet<_> =
            agents::AGENTS.values().map(|a| a.prefix.as_str()).collect();
        for p in &["ms", "webkit", "moz", "o"] {
            assert!(prefixes.contains(p), "vendor prefix family `{p}` missing from AGENTS");
        }
    }
}
