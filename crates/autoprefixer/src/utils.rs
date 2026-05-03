//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/utils.js`.

use once_cell::sync::Lazy;
use regex::Regex;

/// Throw special error, to tell binary that this error is from Autoprefixer.
///
/// JS: `module.exports.error = function (text) { let err = new Error(text); err.autoprefixer = true; throw err; }`
#[derive(Debug, Clone)]
pub struct AutoprefixerError {
    pub message: String,
}

impl std::fmt::Display for AutoprefixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AutoprefixerError {}

pub fn error(text: impl Into<String>) -> AutoprefixerError {
    AutoprefixerError {
        message: text.into(),
    }
}

/// Return array, that doesn't contain duplicates. Preserves first-seen order
/// (matching JS `[...new Set(array)]` semantics).
pub fn uniq<T: Clone + Eq + std::hash::Hash>(array: &[T]) -> Vec<T> {
    let mut seen = indexmap::IndexSet::new();
    for item in array {
        seen.insert(item.clone());
    }
    seen.into_iter().collect()
}

/// Return "-webkit-" on "-webkit- old".
///
/// JS: split on space, return part[0] (or whole string if no space).
pub fn remove_note(s: &str) -> &str {
    if !s.contains(' ') {
        return s;
    }
    s.split(' ').next().unwrap_or("")
}

/// Escape RegExp symbols.
///
/// JS regex: `/[$()*+-.?[\\\]^{|}]/g` → `\$&`.
pub fn escape_regexp(s: &str) -> String {
    static ESC: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"[$()*+\-.?\[\\\]^{|}]").unwrap());
    ESC.replace_all(s, |caps: &regex::Captures| {
        format!("\\{}", &caps[0])
    })
    .into_owned()
}

/// Return regexp source to check that CSS string contains `word`.
///
/// JS: `new RegExp("(^|[\\s,(])(" + word + "($|[\\s(,]))", 'gi')`.
/// Returns the source string — callers compile per-use because regex flags
/// (`gi`) and the pcre2-vs-re2 dialect differ.
pub fn regexp_source(word: &str, escape: bool) -> String {
    let w = if escape {
        escape_regexp(word)
    } else {
        word.to_string()
    };
    format!("(^|[\\s,(])({}($|[\\s(,]))", w)
}

/// Compile the case-insensitive (`gi`) regex from `regexp_source`.
pub fn regexp(word: &str, escape: bool) -> Regex {
    let src = regexp_source(word, escape);
    crate::profile::time_regex_compile(|| {
        Regex::new(&format!("(?i){}", src)).expect("valid regexp")
    })
}

/// Change comma list. Splits via postcss `list.comma`, calls callback,
/// re-joins with the original separator (preserving comma+spacing).
///
/// JS:
/// ```js
/// editList(value, callback) {
///   let origin = list.comma(value)
///   let changed = callback(origin, [])
///   if (origin === changed) return value
///   let join = value.match(/,\s*/)
///   join = join ? join[0] : ', '
///   return changed.join(join)
/// }
/// ```
pub fn edit_list<F>(value: &str, callback: F) -> String
where
    F: FnOnce(Vec<String>) -> Vec<String>,
{
    let origin = postcss_core::list::comma(value);
    let changed = callback(origin.clone());

    if origin == changed {
        return value.to_string();
    }

    static SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*").unwrap());
    let join = SEP.find(value).map(|m| m.as_str()).unwrap_or(", ");
    changed.join(join)
}

/// Split the selector into parts. 3-deep array: comma-separated (1),
/// space-separated (2), and combined per `.`/`#` boundaries (3).
///
/// JS:
/// ```js
/// splitSelector(selector) {
///   return list.comma(selector).map(i =>
///     list.space(i).map(k => k.split(/(?=\.|#)/g))
///   )
/// }
/// ```
pub fn split_selector(selector: &str) -> Vec<Vec<Vec<String>>> {
    postcss_core::list::comma(selector)
        .into_iter()
        .map(|i| {
            postcss_core::list::space(&i)
                .into_iter()
                .map(|k| split_on_class_id(&k))
                .collect()
        })
        .collect()
}

/// `s.split(/(?=\.|#)/g)` — split *before* every `.` or `#`, leaving the
/// delimiter attached to the next chunk. Empty leading chunk is preserved
/// only when the first char is `.` or `#` (matches JS lookahead split).
fn split_on_class_id(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut first = true;
    for ch in s.chars() {
        if (ch == '.' || ch == '#') && !first {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
        first = false;
    }
    out.push(cur);
    out
}

/// Return true if a given value only contains numbers.
///
/// JS accepts both `number` and `string` (matching `/^[0-9]+$/`).
pub fn is_pure_number(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniq_preserves_first_seen_order() {
        let v = vec!["a", "b", "a", "c", "b"];
        assert_eq!(uniq(&v), vec!["a", "b", "c"]);
    }

    #[test]
    fn remove_note_strips_after_space() {
        assert_eq!(remove_note("-webkit- old"), "-webkit-");
        assert_eq!(remove_note("-webkit-"), "-webkit-");
    }

    #[test]
    fn escape_regexp_handles_metachars() {
        assert_eq!(escape_regexp("a.b"), "a\\.b");
        assert_eq!(escape_regexp("a+b*c"), "a\\+b\\*c");
        assert_eq!(escape_regexp("(x)"), "\\(x\\)");
    }

    #[test]
    fn split_on_class_id_lookahead() {
        // JS: `"a.b#c".split(/(?=\.|#)/g)` → `["a", ".b", "#c"]`
        assert_eq!(
            split_on_class_id("a.b#c"),
            vec!["a".to_string(), ".b".to_string(), "#c".to_string()]
        );
        // JS: `".x".split(/(?=\.|#)/g)` → `[".x"]` (V8 suppresses the
        // leading empty when the lookahead matches at index 0).
        assert_eq!(split_on_class_id(".x"), vec![".x".to_string()]);
        assert_eq!(split_on_class_id("abc"), vec!["abc".to_string()]);
    }

    #[test]
    fn is_pure_number_matches_js() {
        assert!(is_pure_number("123"));
        assert!(!is_pure_number(""));
        assert!(!is_pure_number("12.3"));
        assert!(!is_pure_number("12px"));
    }
}
