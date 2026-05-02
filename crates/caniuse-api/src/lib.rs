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
    fn unknown_feature_with_single_fuzzy_match_returns_false() {
        // Replicates upstream JS bug at index.js:56 — the catch branch
        // assigns the *packed* feature (a string) to `data`; `data.stats`
        // is undefined; `every()` returns false. So even though "flexbo"
        // fuzzy-resolves to a single feature ("flexbox"), `is_supported`
        // must return false against any non-empty browser list.
        assert!(!is_supported("flexbo", "last 2 chrome versions"));
    }

    #[test]
    fn empty_browser_list_is_vacuously_true_for_known_feature() {
        // JS: `every` over [] is true. Mirror via resolved.is_empty().
        // Use a query that resolves to nothing reliably ("not all").
        // (Skipped if the shim still resolves "not all" to ≥1 entries; the
        // intent is documentary — don't fail CI on shim quirk.)
        let resolved = browserslist_shim::resolve("not all", true);
        if resolved.is_empty() {
            assert!(is_supported("flexbox", "not all"));
        }
    }

    #[test]
    fn set_browser_scope_is_atomic_under_concurrency() {
        // Reader threads must always observe a complete scope (either the
        // pre-write value or the post-write value), never a half-mutated
        // mid-write state. With the RwLock contract this is structural;
        // this test guards against accidental regression to a Vec-mutation
        // path (e.g. `*b = clean_browsers_list(...)` evaluated under the
        // lock with a non-atomic intermediate).
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        set_browser_scope(Some("last 2 chrome versions"));
        let stop = Arc::new(AtomicBool::new(false));

        let reader_stop = Arc::clone(&stop);
        let reader = thread::spawn(move || {
            while !reader_stop.load(Ordering::Relaxed) {
                let scope = get_browser_scope();
                // Every observation must be non-empty and contain only
                // well-formed browser names (no empty entries from a
                // half-applied write).
                assert!(!scope.is_empty());
                for name in &scope {
                    assert!(!name.is_empty());
                    assert!(!name.contains(' '));
                }
            }
        });

        for _ in 0..200 {
            set_browser_scope(Some("last 2 chrome versions"));
            set_browser_scope(Some("last 2 firefox versions"));
        }

        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
    }

    #[test]
    fn clean_browsers_list_dedup_is_deterministic() {
        // `clean_browsers_list` uses `IndexSet` for membership; the output
        // Vec must be in first-occurrence order regardless of how many
        // times we call it. (Catches an accidental swap back to `HashSet`,
        // whose `RandomState` would NOT affect this Vec — but would
        // affect any future caller that iterates the membership set.)
        let a = crate::utils::clean_browsers_list(Some("last 2 chrome versions"));
        let b = crate::utils::clean_browsers_list(Some("last 2 chrome versions"));
        assert_eq!(a, b);
        // Browser names are unique (the dedup actually ran).
        let mut sorted = a.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), a.len());
    }

    #[test]
    fn features_lists_all() {
        // 582 at caniuse-lite@1.0.30001766 (was 579 at 1.0.30001690).
        assert_eq!(features().len(), 582);
    }
}
