//! Port of `caniuse-api/dist/utils.js`.

use indexmap::IndexMap;
use caniuse_db::features::Feature;

/// `contains(str, substr)` — line 20 upstream. JS `~indexOf(...)` truthy.
pub fn contains(s: &str, substr: &str) -> bool { s.contains(substr) }

/// `parseCaniuseData(feature, browsers)` — line 24 upstream.
pub fn parse_caniuse_data(feature: &Feature, browsers: &[String]) -> IndexMap<String, IndexMap<String, f64>> {
    let mut support: IndexMap<String, IndexMap<String, f64>> = IndexMap::new();
    for browser in browsers {
        let mut entry: IndexMap<String, f64> = IndexMap::new();
        if let Some(stats) = feature.stats.get(browser) {
            for (info, raw) in stats.iter() {
                // Strip `#\d+` and trim, then split on space.
                let stripped = strip_hash_digits(raw);
                let letters: Vec<&str> = stripped.split_whitespace().collect();
                let info_left = info.split('-').next().unwrap_or(info);
                let info_num: f64 = match info_left.parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                for letter in letters.iter() {
                    if *letter == "d" { continue; }
                    let key = letter.to_string();
                    if *letter == "y" {
                        match entry.get(&key) {
                            None => { entry.insert(key, info_num); }
                            Some(prev) if info_num < *prev => { entry.insert(key, info_num); }
                            _ => {}
                        }
                    } else {
                        match entry.get(&key) {
                            None => { entry.insert(key, info_num); }
                            Some(prev) if info_num > *prev => { entry.insert(key, info_num); }
                            _ => {}
                        }
                    }
                }
            }
        }
        support.insert(browser.clone(), entry);
    }
    support
}

fn strip_hash_digits(s: &str) -> String {
    // Mirrors JS `replace(/#\d+/, "")` — first occurrence only.
    if let Some(idx) = s.find('#') {
        let tail = &s[idx + 1..];
        let mut end = idx + 1;
        for c in tail.chars() {
            if c.is_ascii_digit() { end += c.len_utf8(); } else { break; }
        }
        let mut owned = String::with_capacity(s.len());
        owned.push_str(&s[..idx]);
        owned.push_str(&s[end..]);
        owned.trim().to_string()
    } else {
        s.trim().to_string()
    }
}

/// `cleanBrowsersList(browserList)` — line 58 upstream.
pub fn clean_browsers_list(query: Option<&str>) -> Vec<String> {
    let resolved = browserslist_shim::resolve(query.unwrap_or(""), false);
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for b in resolved {
        let name = b.split(' ').next().unwrap_or(&b).to_string();
        if seen.insert(name.clone()) { out.push(name); }
    }
    out
}
