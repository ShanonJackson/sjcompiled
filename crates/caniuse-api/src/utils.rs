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
                // Upstream utils.js:32:
                //   letters = ...replace(/#\d+/, "").trim().split(" ");
                // JS `split(" ")` is literal-space split — preserves empty
                // strings between consecutive spaces. Do NOT use
                // `split_whitespace()` which collapses runs.
                let stripped = strip_first_hash_digits(raw);
                let trimmed = trim_js(&stripped);
                let letters: Vec<&str> = trimmed.split(' ').collect();
                // utils.js:33: `info = parseFloat(info.split("-")[0])`.
                // JS parseFloat is permissive — parses the longest numeric
                // prefix and ignores trailing garbage (e.g. "4.4.3" → 4.4,
                // "12abc" → 12). Rust `f64::parse` is strict — replicate
                // the JS semantics to keep the support table identical.
                let info_left = info.split('-').next().unwrap_or(info);
                let info_num = match js_parse_float(info_left) {
                    Some(n) => n,
                    None => continue, // matches `if (isNaN(info)) continue`
                };
                for letter in letters.iter() {
                    if *letter == "d" { continue; }
                    let key = (*letter).to_string();
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

/// Mirrors JS `String.prototype.replace(/#\d+/, "")` — replaces the FIRST
/// substring matching `#` followed by one or more ASCII digits. A bare `#`
/// with no trailing digits does NOT match (the regex requires `\d+`), so
/// `"# y"` is left unchanged and `"# #1 y"` becomes `"#  y"` (the second
/// `#` and its digits are removed).
fn strip_first_hash_digits(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut search_start = 0;
    while let Some(rel_idx) = s[search_start..].find('#') {
        let idx = search_start + rel_idx;
        let after = idx + 1;
        let mut end = after;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > after {
            // Found `#\d+`.
            let mut owned = String::with_capacity(s.len());
            owned.push_str(&s[..idx]);
            owned.push_str(&s[end..]);
            return owned;
        }
        // Lone `#` without digits — keep scanning for the next candidate.
        search_start = idx + 1;
    }
    s.to_string()
}

/// JS `String.prototype.trim()` strips ECMA-262 whitespace + line terminators.
/// Rust `str::trim()` strips Unicode whitespace, which is a superset for the
/// common cases we hit here. For caniuse-data inputs (ASCII-only stats
/// strings) the two agree; we use this thin wrapper as a documentation hook.
fn trim_js(s: &str) -> String { s.trim().to_string() }

/// Mirrors JavaScript `parseFloat` semantics:
///   - Skip leading whitespace.
///   - Optional `+`/`-` sign.
///   - Decimal digits, optional fractional digits, optional exponent.
///   - Stops at the first non-numeric character; returns the prefix value.
///   - Returns `None` for inputs that fail to parse anything (NaN in JS).
///
/// Required because `f64::from_str` is strict (rejects `"12abc"` and
/// `"4.4.3"`), whereas `parseFloat` returns `12` and `4.4` respectively.
/// caniuse-lite stats include keys like `"4.4.3-4.4.4"` (Android), which
/// after the `split("-")` produces `"4.4.3"`.
fn js_parse_float(s: &str) -> Option<f64> {
    let s = s.trim_start();
    if s.is_empty() { return None; }
    let bytes = s.as_bytes();
    let mut end = 0usize;

    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        end = 1;
    }

    let mut has_digits = false;

    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        has_digits = true;
    }

    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            has_digits = true;
        }
    }

    if !has_digits { return None; }

    // Try to extend with an exponent; if the exponent has no digits, drop it.
    let pre_exp_end = end;
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut probe = end + 1;
        if probe < bytes.len() && (bytes[probe] == b'+' || bytes[probe] == b'-') {
            probe += 1;
        }
        let exp_digits_start = probe;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > exp_digits_start {
            end = probe;
        } else {
            end = pre_exp_end;
        }
    }

    s[..end].parse::<f64>().ok()
}

/// `cleanBrowsersList(browserList)` — line 58 upstream.
/// `lodash.uniq` preserves first-occurrence order; we replicate via a
/// linear-scan Vec with a HashSet only used for membership checks (no Vec
/// element ever comes from the HashSet, so iteration order is unaffected).
pub fn clean_browsers_list(query: Option<&str>) -> Vec<String> {
    // `lodash.uniq` is a linear first-occurrence dedup. We mirror with an
    // `IndexSet` (insertion-ordered) rather than `HashSet`. `HashSet` works
    // here today (Vec is the source of truth and we only call `.insert()`),
    // but the structural invariant matters: any future iteration over the
    // dedup set must stay deterministic. `RandomState` would silently
    // introduce process-randomized order if a refactor ever reads back from
    // the set. `IndexSet` makes "deterministic" load-bearing in the type.
    let resolved = browserslist_shim::resolve(query.unwrap_or(""), false);
    let mut out: Vec<String> = Vec::new();
    let mut seen: indexmap::IndexSet<String> = indexmap::IndexSet::new();
    for b in resolved {
        let name = b.split(' ').next().unwrap_or(&b).to_string();
        if seen.insert(name.clone()) { out.push(name); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_parse_float_accepts_trailing_garbage() {
        // JS: parseFloat("4.4.3") === 4.4
        assert_eq!(js_parse_float("4.4.3"), Some(4.4));
        // JS: parseFloat("12abc") === 12
        assert_eq!(js_parse_float("12abc"), Some(12.0));
        // JS: parseFloat("12") === 12
        assert_eq!(js_parse_float("12"), Some(12.0));
        // JS: parseFloat("12.5") === 12.5
        assert_eq!(js_parse_float("12.5"), Some(12.5));
        // JS: parseFloat(".5") === 0.5
        assert_eq!(js_parse_float(".5"), Some(0.5));
        // JS: parseFloat("12e2") === 1200
        assert_eq!(js_parse_float("12e2"), Some(1200.0));
        // JS: parseFloat("12e") === 12 (incomplete exponent dropped)
        assert_eq!(js_parse_float("12e"), Some(12.0));
        // JS: parseFloat("TP") -> NaN
        assert_eq!(js_parse_float("TP"), None);
        // JS: parseFloat("") -> NaN
        assert_eq!(js_parse_float(""), None);
        // JS: parseFloat("all") -> NaN
        assert_eq!(js_parse_float("all"), None);
        // JS: parseFloat("-12") === -12
        assert_eq!(js_parse_float("-12"), Some(-12.0));
        // JS: parseFloat("  4.4 ") === 4.4 (leading whitespace OK)
        assert_eq!(js_parse_float("  4.4 "), Some(4.4));
    }

    #[test]
    fn strip_first_hash_digits_only_matches_with_digits() {
        // JS: "y #1".replace(/#\d+/, "") === "y "
        assert_eq!(strip_first_hash_digits("y #1"), "y ");
        // JS: "y #".replace(/#\d+/, "") === "y #" (no digits, no match)
        assert_eq!(strip_first_hash_digits("y #"), "y #");
        // JS: "# #5 y".replace(/#\d+/, "") === "#  y" (first match w/ digits)
        assert_eq!(strip_first_hash_digits("# #5 y"), "#  y");
        // No `#` at all.
        assert_eq!(strip_first_hash_digits("y n"), "y n");
        // Multi-digit footnote.
        assert_eq!(strip_first_hash_digits("a #123 b"), "a  b");
        // Replaces only first match (no /g flag upstream).
        assert_eq!(strip_first_hash_digits("a #1 b #2 c"), "a  b #2 c");
    }

    #[test]
    fn parse_caniuse_data_preserves_double_space_letters() {
        // JS `split(" ")` on "y  n" yields ["y", "", "n"]. Our pipeline
        // must produce the same letter array (literal-space split, NOT
        // whitespace collapse) so downstream `letter === "y"` etc. match.
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        chrome.insert("49".to_string(), "y  n".to_string());
        feature.stats.insert("chrome".to_string(), chrome);

        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        // "y" recorded as min, "n" recorded as max, and the empty letter
        // from the double-space lands under key "" (matches JS bug-for-bug).
        assert_eq!(chrome_entry.get("y"), Some(&49.0));
        assert_eq!(chrome_entry.get("n"), Some(&49.0));
        assert_eq!(chrome_entry.get(""), Some(&49.0));
    }

    #[test]
    fn parse_caniuse_data_handles_three_part_version() {
        // Android 4.4.3 — must parse via JS parseFloat as 4.4 (not error).
        let mut feature = Feature::default();
        let mut android = IndexMap::new();
        android.insert("4.4.3".to_string(), "y".to_string());
        feature.stats.insert("android".to_string(), android);

        let support = parse_caniuse_data(&feature, &["android".to_string()]);
        let android_entry = support.get("android").unwrap();
        assert_eq!(android_entry.get("y"), Some(&4.4));
    }

    #[test]
    fn parse_caniuse_data_skips_d_letter() {
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        chrome.insert("49".to_string(), "y d".to_string());
        feature.stats.insert("chrome".to_string(), chrome);

        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        assert_eq!(chrome_entry.get("y"), Some(&49.0));
        assert!(!chrome_entry.contains_key("d"));
    }

    #[test]
    fn parse_caniuse_data_y_takes_min_others_take_max() {
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        chrome.insert("49".to_string(), "y".to_string());
        chrome.insert("60".to_string(), "y".to_string());
        chrome.insert("30".to_string(), "n".to_string());
        chrome.insert("40".to_string(), "n".to_string());
        feature.stats.insert("chrome".to_string(), chrome);

        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        assert_eq!(chrome_entry.get("y"), Some(&49.0)); // min
        assert_eq!(chrome_entry.get("n"), Some(&40.0)); // max
    }

    #[test]
    fn parse_caniuse_data_strips_footnote_first_match() {
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        chrome.insert("49".to_string(), "y #1".to_string());
        feature.stats.insert("chrome".to_string(), chrome);

        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        assert_eq!(chrome_entry.get("y"), Some(&49.0));
    }
}
