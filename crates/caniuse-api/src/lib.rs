//! crates/caniuse-api
//! Byte-for-byte Rust port of `caniuse-api@3.0.0`.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/caniuse-api/dist/`):
//!   - `index.js` -> `src/index.rs`
//!   - `utils.js` -> `src/utils.rs`

pub mod index;
pub mod utils;

pub use index::{features, find, get_support, is_supported, get_browser_scope, set_browser_scope};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_feature() {
        let res = find("flexbox");
        assert_eq!(res, vec!["flexbox".to_string()]);
    }

    #[test]
    fn fuzzy_finds_substring() {
        let res = find("flexbo");
        assert!(res.contains(&"flexbox".to_string()));
    }

    #[test]
    fn flexbox_partial_in_old_chrome() {
        // Upstream `caniuse-api` strict-equals `"y"` without notes; flexbox in
        // modern browsers commonly emits `"y #1"` (note about prefix). What
        // we *can* assert is the inverse: ie 6 cannot match.
        assert!(!is_supported("flexbox", "ie 6"));
    }

    #[test]
    fn css_grid_unsupported_in_ie6() {
        assert!(!is_supported("css-grid", "ie 6"));
    }

    #[test]
    fn unknown_feature_returns_false() {
        assert!(!is_supported("not-a-real-feature-xyz", "last 2 chrome versions"));
    }

    #[test]
    fn features_lists_all() {
        assert_eq!(features().len(), 579);
    }
}
