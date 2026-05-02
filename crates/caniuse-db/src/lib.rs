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
}
