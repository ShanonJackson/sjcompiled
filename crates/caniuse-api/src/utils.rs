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
            // Upstream uses `for...in` over a plain object. Per ECMA-262
            // `OrdinaryOwnPropertyKeys`, JS visits **array-index keys
            // first, in ascending numeric order**, then string keys in
            // insertion order. Caniuse-lite stats objects typically mix
            // pure-integer keys ("4", "5", "49") with non-integer keys
            // ("4.1", "12.0-12.5", "TP", "all"); mismatching the visit
            // order would produce `entry`/`support` IndexMaps with a
            // different first-write key order — observable by any caller
            // that serializes or iterates the result.
            //
            // Final f64 values are order-invariant (min for "y", max for
            // others — both commutative/associative), but the **key
            // insertion order** of `entry` is not. Mirror JS exactly.
            let visit_order = js_for_in_order(stats);
            for key in &visit_order {
                let info = key.as_str();
                let raw = &stats[key.as_str()];
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

/// Mirrors ECMA-262 `OrdinaryOwnPropertyKeys` enumeration order for plain
/// objects: integer-index keys first in ascending numeric order, then
/// string keys in insertion order.
///
/// An "array index" per the spec (`IsArrayIndex`) is a string `P` such that
/// `ToString(ToUint32(P)) === P` and the value is in `[0, 2^32 - 1)`. So:
///   - `"0"`, `"1"`, `"42"`, `"4294967294"` → array indices.
///   - `"01"` (round-trips to `"1"`), `"-1"`, `"4.1"`, `"4294967295"` (the
///     max bound is exclusive), and anything non-numeric → NOT array
///     indices, fall into the insertion-order bucket.
fn js_for_in_order<V>(map: &IndexMap<String, V>) -> Vec<String> {
    let mut integer_keys: Vec<(u32, String)> = Vec::new();
    let mut string_keys: Vec<String> = Vec::new();
    for k in map.keys() {
        match parse_array_index(k) {
            Some(idx) => integer_keys.push((idx, k.clone())),
            None => string_keys.push(k.clone()),
        }
    }
    // Stable sort by numeric value. Multiple entries with the same numeric
    // value but different string forms cannot occur — by definition, only
    // the canonical string form qualifies as an array index.
    integer_keys.sort_by_key(|(idx, _)| *idx);
    let mut out = Vec::with_capacity(integer_keys.len() + string_keys.len());
    out.extend(integer_keys.into_iter().map(|(_, k)| k));
    out.extend(string_keys);
    out
}

/// Returns `Some(idx)` iff `s` is the canonical decimal string form of an
/// integer in `[0, 2^32 - 1)` — matching ECMA-262 `IsArrayIndex`. Returns
/// `None` otherwise.
fn parse_array_index(s: &str) -> Option<u32> {
    if s.is_empty() { return None; }
    // Reject leading zeros except for the single-digit `"0"` case (since
    // `"01"` round-trips to `"1"`, not itself).
    let bytes = s.as_bytes();
    if bytes.len() > 1 && bytes[0] == b'0' { return None; }
    // Must be all ASCII digits.
    if !bytes.iter().all(|b| b.is_ascii_digit()) { return None; }
    // Parse and bounds-check.
    let n: u64 = s.parse().ok()?;
    if n >= (u32::MAX as u64) { return None; } // 2^32 - 1 is excluded
    Some(n as u32)
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
    fn parse_array_index_matches_ecma_262() {
        // Spec-conformant array indices.
        assert_eq!(parse_array_index("0"), Some(0));
        assert_eq!(parse_array_index("1"), Some(1));
        assert_eq!(parse_array_index("42"), Some(42));
        assert_eq!(parse_array_index("4294967294"), Some(4294967294)); // 2^32 - 2

        // Spec rejections.
        assert_eq!(parse_array_index(""), None);
        assert_eq!(parse_array_index("01"), None);     // leading zero
        assert_eq!(parse_array_index("00"), None);     // leading zero
        assert_eq!(parse_array_index("-1"), None);     // sign
        assert_eq!(parse_array_index("+1"), None);     // sign
        assert_eq!(parse_array_index("1.0"), None);    // not integer string
        assert_eq!(parse_array_index("4.1"), None);    // version string
        assert_eq!(parse_array_index("4.4.3"), None);
        assert_eq!(parse_array_index("12.0-12.5"), None);
        assert_eq!(parse_array_index("TP"), None);
        assert_eq!(parse_array_index("4294967295"), None); // 2^32 - 1, excluded
        assert_eq!(parse_array_index("4294967296"), None); // 2^32, excluded
        assert_eq!(parse_array_index("99999999999"), None); // > u32
    }

    #[test]
    fn js_for_in_order_integers_first_then_insertion() {
        let mut m: IndexMap<String, ()> = IndexMap::new();
        // Insertion order: "TP", "12", "4.1", "5", "11", "all", "3"
        for k in ["TP", "12", "4.1", "5", "11", "all", "3"] {
            m.insert(k.to_string(), ());
        }
        // Expected JS for-in: integer ascending, then string-insertion.
        // Integer bucket: "3", "5", "11", "12"
        // String bucket: "TP", "4.1", "all" (insertion order preserved)
        let order = js_for_in_order(&m);
        assert_eq!(
            order,
            vec!["3", "5", "11", "12", "TP", "4.1", "all"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_caniuse_data_visit_order_matches_js_for_in() {
        // Construct a stats object whose insertion order would PRODUCE a
        // different `entry` first-write order than JS `for...in` would.
        // Inserting "12" first means Rust pure-insertion-order would
        // visit it first; JS visits "11" first (integer-ascending). The
        // "n" letter is first written by whichever version is visited
        // first — so the f64 value coincidentally matches (both 11 and
        // 12 yield the SAME final max via the comm./assoc. property),
        // but the **key insertion order** of `entry` would still not
        // diverge here since we only have one letter. To observe the
        // entry-order drift, mix two different new letters across
        // integer-vs-string buckets.
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        // Insertion order: "TP" (string bucket), "12" (int bucket),
        // "11" (int bucket).
        chrome.insert("TP".to_string(), "x".to_string()); // letter "x"
        chrome.insert("12".to_string(), "n".to_string()); // letter "n"
        chrome.insert("11".to_string(), "n".to_string()); // letter "n"
        feature.stats.insert("chrome".to_string(), chrome);

        // JS visit order: "11" → inserts "n", "12" → updates "n" max,
        // "TP" → continues (parseFloat("TP") is NaN).
        // Resulting `entry` keys in first-write order: ["n"].
        // Final values: n = 12 (max).
        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        let entry_keys: Vec<&String> = chrome_entry.keys().collect();
        assert_eq!(entry_keys, vec![&"n".to_string()]);
        assert_eq!(chrome_entry.get("n"), Some(&12.0));
    }

    #[test]
    fn parse_caniuse_data_visit_order_drives_entry_key_order() {
        // Direct test of entry key insertion order. Construct stats so
        // that Rust insertion order differs from JS for-in order, and
        // each version contributes a DIFFERENT new letter — making the
        // entry-key-order divergence observable.
        //
        //   "20" (string-bucket NO — "20" is integer!)   ...let's pick
        //   non-integer keys that contribute different letters first.
        //
        // Better construction:
        //   Inserted: "12" (int) yields letter "a"
        //             "5"  (int) yields letter "b"
        //   Pure-insertion would write "a" first, then "b" → entry: [a, b]
        //   JS for-in (int-ascending) writes "5" first ("b"), then "12"
        //   ("a") → entry: [b, a]
        let mut feature = Feature::default();
        let mut chrome = IndexMap::new();
        chrome.insert("12".to_string(), "a".to_string());
        chrome.insert("5".to_string(), "b".to_string());
        feature.stats.insert("chrome".to_string(), chrome);

        let support = parse_caniuse_data(&feature, &["chrome".to_string()]);
        let chrome_entry = support.get("chrome").unwrap();
        let entry_keys: Vec<String> = chrome_entry.keys().cloned().collect();
        // JS-faithful order: "5" visited first → "b" inserted first.
        assert_eq!(entry_keys, vec!["b".to_string(), "a".to_string()]);
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
