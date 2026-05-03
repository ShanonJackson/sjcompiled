//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/value.js`.
//!
//! `class Value extends Prefixer`. Hacks like `gradient`, `cross-fade`,
//! `display-flex`, `image-set`, `pixelated`, `intrinsic`, `filter-value`
//! subclass this.

use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use postcss_core::{AttrValue, Node, NodeKind};

use crate::fast_match::WordRegexp;
use crate::old_value::OldValue;
use crate::prefixer::PrefixerBase;

/// `decl.attrs[_autoprefixerValues]: { prefix → value-with-prefixed-tokens }`.
pub const ATTR_VALUES: &str = "_autoprefixerValues";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ValueBase {
    pub prefixer: PrefixerBase,
    /// `regexpCache` — JS uses a per-instance lazy field. We cache once.
    /// Skipped on serde because it's a runtime cache, not load-bearing
    /// state. Decoded `ValueBase` starts with an empty cache; first
    /// `regexp()` call rebuilds it via `WordRegexp::new(name)` —
    /// byte-identical to a freshly-constructed instance.
    #[serde(skip)]
    regexp_cache: OnceCell<WordRegexp>,
}

impl ValueBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            prefixer: PrefixerBase::new(name, prefixes, all_id),
            regexp_cache: OnceCell::new(),
        }
    }

    /// JS: `check(decl)` — `decl.value` includes `this.name` AND matches
    /// `this.regexp()`.
    pub fn check(&self, decl: &Node) -> bool {
        let value = match &decl.kind {
            NodeKind::Declaration(d) => &d.value,
            _ => return false,
        };
        if !value.contains(&self.prefixer.name) {
            return false;
        }
        self.regexp().is_match(value)
    }

    /// JS: `regexp()` — lazy. Built from `utils.regexp(this.name)`.
    pub fn regexp(&self) -> &WordRegexp {
        self.regexp_cache
            .get_or_init(|| WordRegexp::new(&self.prefixer.name))
    }

    /// JS: `replace(string, prefix)` — `string.replace(regexp, '$1' + prefix + '$2')`.
    pub fn replace(&self, string: &str, prefix: &str) -> String {
        // utils::regexp captures group 1 = prefix-context (`^` or `[\s,(]`),
        // group 2 = `name` followed by `$|[\s(,]`. JS `.replace()` puts
        // the prefix between groups 1 and 2. `WordRegexp::replace_all_with_prefix`
        // implements that exact semantic, byte-equal to the regex.
        self.regexp().replace_all_with_prefix(string, prefix)
    }

    /// JS: `value(decl)` — return `decl.raws.value.raw` if it represents
    /// the unmodified original `decl.value`, else `decl.value`.
    /// ```js
    /// if (decl.raws.value && decl.raws.value.value === decl.value) return decl.raws.value.raw
    /// else return decl.value
    /// ```
    pub fn value(&self, decl: &Node) -> String {
        let (current_value, raws_value) = match &decl.kind {
            NodeKind::Declaration(d) => (&d.value, decl.raws.value.as_ref()),
            _ => return String::new(),
        };
        match raws_value {
            Some(rv) if rv.value == *current_value => rv.raw.clone(),
            _ => current_value.clone(),
        }
    }

    /// JS: `add(decl, prefix)` — populate `decl._autoprefixerValues[prefix]`
    /// by repeatedly calling `replace` until the string stabilises.
    /// ```js
    /// add(decl, prefix) {
    ///   if (!decl._autoprefixerValues) decl._autoprefixerValues = {}
    ///   let value = decl._autoprefixerValues[prefix] || this.value(decl)
    ///   let before
    ///   do {
    ///     before = value
    ///     value = this.replace(value, prefix)
    ///     if (value === false) return
    ///   } while (value !== before)
    ///   decl._autoprefixerValues[prefix] = value
    /// }
    /// ```
    pub fn add(&mut self, decl: &mut Node, prefix: &str) {
        // Pull or initialise the cache map.
        let initial = self
            .stored_value(decl, prefix)
            .unwrap_or_else(|| self.value(decl));

        let mut value = initial;
        loop {
            let before = value.clone();
            value = self.replace(&before, prefix);
            if value == before {
                break;
            }
        }

        let map = decl
            .attrs
            .get_string_map_mut(ATTR_VALUES)
            .map(|m| m as *mut IndexMap<String, String>);
        match map {
            Some(ptr) => unsafe { (*ptr).insert(prefix.to_string(), value) },
            None => {
                let mut m = IndexMap::new();
                m.insert(prefix.to_string(), value);
                decl.attrs.set(ATTR_VALUES, AttrValue::StringMap(m));
                None
            }
        };
    }

    fn stored_value(&self, decl: &Node, prefix: &str) -> Option<String> {
        decl.attrs
            .get_string_map(ATTR_VALUES)
            .and_then(|m| m.get(prefix).cloned())
    }

    /// JS: `old(prefix)` → `new OldValue(this.name, prefix + this.name)`.
    pub fn old(&self, prefix: &str) -> OldValue {
        OldValue::new(
            self.prefixer.name.clone(),
            format!("{prefix}{}", self.prefixer.name),
            None,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn first_decl(root: &mut Node) -> &mut Node {
        // root → rule(0) → decl(0)
        let rule = root.nodes_mut().unwrap().get_mut(0).unwrap();
        rule.nodes_mut().unwrap().get_mut(0).unwrap()
    }

    #[test]
    fn check_returns_true_when_value_contains_name() {
        let mut r = parse("a { display: flex; }").unwrap();
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        assert!(v.check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_returns_false_when_value_lacks_name() {
        let mut r = parse("a { display: block; }").unwrap();
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        assert!(!v.check(first_decl(&mut r.root)));
    }

    #[test]
    fn replace_inserts_prefix_on_bare_value() {
        // `value()` returns the bare decl value (no `display: ` prefix,
        // no trailing `;`). So `replace` is always called with strings
        // matching `^name($|[\s(,])`.
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        assert_eq!(v.replace("flex", "-webkit-"), "-webkit-flex");
    }

    #[test]
    fn replace_inserts_prefix_with_leading_boundary() {
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        // Group 1 = `(^|[\s,(])` — leading space matches.
        assert_eq!(v.replace("inline flex", "-webkit-"), "inline -webkit-flex");
    }

    #[test]
    fn replace_does_not_match_inside_word() {
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        // `inflex` — name is preceded by `n`, not a boundary char.
        assert_eq!(v.replace("inflex", "-webkit-"), "inflex");
    }

    #[test]
    fn value_returns_decl_value_by_default() {
        let mut r = parse("a { display: flex; }").unwrap();
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        assert_eq!(v.value(first_decl(&mut r.root)), "flex");
    }

    #[test]
    fn add_caches_prefixed_value_on_node() {
        let mut r = parse("a { display: flex; }").unwrap();
        let mut v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        v.add(first_decl(&mut r.root), "-webkit-");
        let cached = first_decl(&mut r.root)
            .attrs
            .get_string_map(ATTR_VALUES)
            .unwrap();
        assert_eq!(cached.get("-webkit-").unwrap(), "-webkit-flex");
    }

    #[test]
    fn old_constructs_old_value_with_combined_string() {
        let v = ValueBase::new("flex".into(), vec!["-webkit-".into()], 0);
        let ov = v.old("-webkit-");
        assert_eq!(ov.unprefixed, "flex");
        assert_eq!(ov.prefixed, "-webkit-flex");
    }
}
