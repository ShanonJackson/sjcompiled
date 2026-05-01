//! Port of `postcss/lib/rule.js`.

use crate::list;
use crate::node::Node;

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
    /// ```js
    /// let match = this.selector ? this.selector.match(/,\s*/) : null
    /// let sep = match ? match[0] : ',' + this.raw('between', 'beforeOpen')
    /// this.selector = values.join(sep)
    /// ```
    /// We mirror it byte-for-byte: detect `,\s*` in the existing
    /// selector, otherwise fall back to `, ` (default `beforeOpen`).
    pub fn set_selectors(&mut self, values: &[String]) {
        let sep = detect_selector_separator(&self.selector);
        self.selector = values.join(&sep);
    }
}

/// Find the first `,\s*` match in `selector` and return it. If none,
/// fall back to `", "` — matches upstream's `',' + raw('between','beforeOpen')`
/// where `beforeOpen` defaults to `' '`.
fn detect_selector_separator(selector: &str) -> String {
    let bytes = selector.as_bytes();
    if let Some(comma_idx) = bytes.iter().position(|&b| b == b',') {
        let mut end = comma_idx + 1;
        while end < bytes.len() {
            let c = bytes[end];
            // ECMAScript `\s` for `,\s*`: HT, LF, VT, FF, CR, SP, plus
            // Unicode whitespace. Limit byte-scan to ASCII whitespace
            // here since multi-byte Unicode whitespace inside a comma
            // selector list is exotic; extend if real corpora trip it.
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C) {
                end += 1;
            } else {
                break;
            }
        }
        return selector[comma_idx..end].to_string();
    }
    ", ".to_string()
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
}
