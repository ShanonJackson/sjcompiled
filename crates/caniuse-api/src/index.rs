//! Port of `caniuse-api/dist/index.js`.

use std::sync::Mutex;
use once_cell::sync::Lazy;

use crate::utils::{clean_browsers_list, contains, parse_caniuse_data};

static BROWSERS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(clean_browsers_list(None)));

pub fn set_browser_scope(query: Option<&str>) {
    let mut b = BROWSERS.lock().unwrap();
    *b = clean_browsers_list(query);
}

pub fn get_browser_scope() -> Vec<String> {
    BROWSERS.lock().unwrap().clone()
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

pub fn is_supported(feature_name: &str, browsers_query: &str) -> bool {
    let feature = match caniuse_db::features::feature(feature_name) {
        Some(f) => f,
        None => {
            let res = find(feature_name);
            if res.len() == 1 {
                if let Some(f) = caniuse_db::features::feature(&res[0]) { f } else { return false; }
            } else { return false; }
        }
    };
    let resolved = browserslist_shim::resolve(browsers_query, true);
    resolved.iter().all(|b| {
        let parts: Vec<&str> = b.splitn(2, ' ').collect();
        if parts.len() != 2 { return false; }
        feature.stats.get(parts[0])
            .and_then(|m| m.get(parts[1]))
            .map(|v| v == "y").unwrap_or(false)
    })
}
