//! Port of `postcss/lib/rule.js`.

use crate::list;
use crate::node::Node;
use crate::stringifier::is_js_regex_whitespace;

#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub selector: String,
    pub nodes: Vec<Node>,
}

impl Rule {
    /// `rule.selectors` getter upstream — `list.comma(this.selector)`.
    /// Each element is trimmed (matches `list.split`'s `current.trim()`).
    pub fn get_selectors(&self) -> Vec<String> {
        list::comma(&self.selector)
    }

    /// `rule.selectors = values` setter upstream:
    ///
    /// ```js
    /// let match = this.selector ? this.selector.match(/,\s*/) : null
    /// let sep = match ? match[0] : ',' + this.raw('between', 'beforeOpen')
    /// this.selector = values.join(sep)
    /// ```
    ///
    /// **Plugin authors who care about byte parity should use
    /// [`set_selectors_with_between`].** This convenience wrapper assumes
    /// the rule's `raws.between` is the default `" "`. For freshly-built
    /// rules whose raws.between is something else (e.g. `"\n"`,
    /// `" /* comment */ "`), the hardcoded fallback diverges from
    /// upstream's `raw('between', 'beforeOpen')` lookup.
    ///
    /// In the common case (parser-built tree, default raws), this matches
    /// upstream byte-for-byte.
    pub fn set_selectors(&mut self, values: &[String]) {
        self.set_selectors_with_between(values, " ");
    }

    /// `rule.selectors = values` setter — full upstream port.
    ///
    /// `between_fallback` is the value `this.raw('between','beforeOpen')`
    /// would return upstream — typically `node.raws.between` if defined,
    /// otherwise the rawCache `beforeOpen` sample, otherwise `" "`.
    /// Callers that hold a `&Node` should pass
    /// `node.raws.between.as_deref().unwrap_or(" ")`.
    pub fn set_selectors_with_between(&mut self, values: &[String], between_fallback: &str) {
        let sep = detect_selector_separator(&self.selector, between_fallback);
        self.selector = values.join(&sep);
    }
}

/// Find the first `,\s*` match in `selector` and return it. If none,
/// fall back to `',' + between_fallback` — matches upstream's
/// `',' + raw('between','beforeOpen')`. The `\s*` scan uses the full
/// JS regex whitespace set (Unicode `Space_Separator` + line
/// terminators + ZWNBSP), not Rust's `char::is_whitespace`.
fn detect_selector_separator(selector: &str, between_fallback: &str) -> String {
    if let Some(comma_pos) = selector.find(',') {
        let after = &selector[comma_pos + 1..];
        let mut end_offset = 0usize;
        for c in after.chars() {
            if is_js_regex_whitespace(c) {
                end_offset += c.len_utf8();
            } else {
                break;
            }
        }
        return selector[comma_pos..comma_pos + 1 + end_offset].to_string();
    }
    format!(",{}", between_fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(sel: &str) -> Rule {
        Rule { selector: sel.to_string(), nodes: Vec::new() }
    }

    #[test]
    fn get_selectors_single() {
        assert_eq!(rule(".a").get_selectors(), vec![".a"]);
    }

    #[test]
    fn get_selectors_comma_list() {
        assert_eq!(
            rule(".a, .b, .c").get_selectors(),
            vec![".a", ".b", ".c"]
        );
    }

    #[test]
    fn set_selectors_single_falls_back_to_comma_space() {
        let mut r = rule(".a");
        r.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r.selector, ".x, .y");
    }

    #[test]
    fn set_selectors_preserves_comma_space_separator() {
        let mut r = rule(".a, .b, .c");
        r.set_selectors(&[".x".to_string(), ".y".to_string(), ".z".to_string()]);
        assert_eq!(r.selector, ".x, .y, .z");
    }

    #[test]
    fn set_selectors_preserves_bare_comma_separator() {
        let mut r = rule(".a,.b");
        r.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r.selector, ".x,.y");
    }

    #[test]
    fn set_selectors_preserves_comma_newline_separator() {
        let mut r = rule(".a,\n.b");
        r.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r.selector, ".x,\n.y");
    }

    /// Regression: the `,\s*` matcher must respect the full JS `\s` set,
    /// not just ASCII whitespace. U+00A0 (NBSP), U+2028 (LS), U+FEFF
    /// (ZWNBSP) are all valid post-comma whitespace per ES `\s`. The
    /// previous byte-scan only matched ASCII and would truncate the
    /// separator at the first non-ASCII whitespace character.
    #[test]
    fn set_selectors_preserves_unicode_whitespace_separator() {
        // NBSP after comma — JS regex `,\s*` matches `,\u{A0}`.
        let mut r = rule(".a,\u{00A0}.b");
        r.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r.selector, ".x,\u{00A0}.y");

        // Line Separator (U+2028).
        let mut r2 = rule(".a,\u{2028}.b");
        r2.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r2.selector, ".x,\u{2028}.y");

        // U+FEFF (ZWNBSP) — JS treats it as whitespace.
        let mut r3 = rule(".a,\u{FEFF}.b");
        r3.set_selectors(&[".x".to_string(), ".y".to_string()]);
        assert_eq!(r3.selector, ".x,\u{FEFF}.y");
    }

    /// Regression: U+0085 (NEL) is in Unicode `White_Space` but NOT in
    /// JS `\s`. The matcher must NOT consume it as whitespace after a
    /// comma. JS would stop the match at the comma; we should too.
    #[test]
    fn set_selectors_stops_at_nel_u0085() {
        let mut r = rule(".a,\u{0085}.b");
        r.set_selectors(&[".x".to_string(), ".y".to_string()]);
        // JS sees `,` followed by NEL (non-whitespace), so separator is
        // just `,`. Output is `".x,.y"` (NEL is dropped along with the
        // rest of the original selector).
        assert_eq!(r.selector, ".x,.y");
    }

    /// Regression: with no comma in the existing selector, the fallback
    /// is `',' + between`. Caller passes the rule's `raws.between` (or
    /// the rawCache `beforeOpen` sample) — we must use it verbatim.
    #[test]
    fn set_selectors_with_between_uses_caller_value() {
        let mut r = rule(".a");
        r.set_selectors_with_between(&[".x".to_string(), ".y".to_string()], "\n");
        assert_eq!(r.selector, ".x,\n.y");

        let mut r2 = rule(".a");
        r2.set_selectors_with_between(&[".x".to_string(), ".y".to_string()], " /* hi */ ");
        assert_eq!(r2.selector, ".x, /* hi */ .y");
    }
}
