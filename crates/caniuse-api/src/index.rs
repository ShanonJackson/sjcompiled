//! Port of `caniuse-api/dist/index.js`.

use std::sync::RwLock;
use once_cell::sync::Lazy;

use crate::utils::{clean_browsers_list, contains, parse_caniuse_data};

/// Module-level browser scope. Upstream JS lives on a single-threaded event
/// loop, so `setBrowserScope` from one async tick and `getSupport` from
/// another can never interleave **inside** a call. The Rust port can be
/// invoked from multiple NAPI worker threads, so we need an explicit
/// concurrency contract:
///
///   - All readers (`get_browser_scope`, the read inside `get_support`)
///     observe a single atomic snapshot via `RwLock`.
///   - `set_browser_scope` swaps the entire `Vec<String>` in one write —
///     callers never see a half-mutated scope.
///
/// `RwLock` over `Mutex` so concurrent reads (the common case — every
/// `is_supported`/`get_support` call reads, only an explicit
/// `set_browser_scope` writes) don't serialize.
static BROWSERS: Lazy<RwLock<Vec<String>>> = Lazy::new(|| RwLock::new(clean_browsers_list(None)));

pub fn set_browser_scope(query: Option<&str>) {
    // Resolve OUTSIDE the lock so the write critical section is a single
    // pointer/length swap. Holding the lock across `clean_browsers_list`
    // (which calls into `browserslist_shim::resolve` — file I/O on the
    // first call) would block all readers for the duration.
    let next = clean_browsers_list(query);
    let mut b = BROWSERS.write().unwrap();
    *b = next;
}

pub fn get_browser_scope() -> Vec<String> {
    BROWSERS.read().unwrap().clone()
}

pub fn features() -> Vec<&'static String> { caniuse_db::features::list() }

pub fn find(query: &str) -> Vec<String> {
    let list = caniuse_db::features::list();
    if list.iter().any(|s| s.as_str() == query) {
        return vec![query.to_string()];
    }
    list.into_iter().filter(|s| contains(s, query)).cloned().collect()
}

pub fn get_support(query: &str) -> Option<indexmap::IndexMap<String, indexmap::IndexMap<String, f64>>> {
    let feature = match caniuse_db::features::feature(query) {
        Some(f) => f,
        None => {
            let res = find(query);
            if res.len() == 1 { return get_support(&res[0]); }
            return None;
        }
    };
    let browsers = get_browser_scope();
    Some(parse_caniuse_data(feature, &browsers))
}

/// `isSupported(feature, browsers)` — index.js:49.
///
/// Replicates an upstream JS bug at index.js:56. When the initial
/// `feature(features[name])` call throws (i.e. the feature key is unknown),
/// the catch branch assigns the **packed** entry `_caniuseLite.features[res[0]]`
/// (a string) to `data` instead of unpacking it. The subsequent
/// `data.stats[browser[0]]` is therefore `undefined`, the `&&`
/// short-circuits, and `every()` returns `false` for any non-empty
/// browserslist resolution. We model this by setting `feature` to `None`
/// in the catch branch — the closure below treats `None` as "no usable
/// stats", returning false unless the browser list is empty (in which
/// case `every`/`all` are vacuously true).
///
/// In the `res.length !== 1` JS branch, upstream throws a `ReferenceError`.
/// The Rust port can't propagate an exception through a `bool` return, so
/// we treat it the same as the bugged-data path: `every`/`all` over the
/// resolved browsers, with `None` stats forcing each iteration to false.
/// In production callers (postcss plugins) only pass canonical feature
/// names, so this branch is never observed at the byte boundary.
pub fn is_supported(feature_name: &str, browsers_query: &str) -> bool {
    let feature: Option<&caniuse_db::features::Feature> =
        match caniuse_db::features::feature(feature_name) {
            Some(f) => Some(f),
            None => {
                let _res = find(feature_name);
                // Both `res.length === 1` and `res.length !== 1` branches in
                // JS lead to no usable stats reaching `every()`:
                //   - `res.length === 1`: `data` is the packed string;
                //     `data.stats` is undefined → callback returns false.
                //   - `res.length !== 1`: JS throws; we degrade to false.
                None
            }
        };

    let resolved = browserslist_shim::resolve(browsers_query, true);
    resolved.iter().all(|b| {
        // Mirror JS `browser.split(" ")` (literal-space, NOT splitn(2)).
        // Index 0 is the browser name, index 1 the version. Three-part
        // entries never arise from browserslist output, but the literal
        // split is the byte-faithful port.
        let parts: Vec<&str> = b.split(' ').collect();
        if parts.len() < 2 { return false; }
        match feature {
            Some(f) => f
                .stats
                .get(parts[0])
                .and_then(|m| m.get(parts[1]))
                .map(|v| v == "y")
                .unwrap_or(false),
            None => false,
        }
    })
}
