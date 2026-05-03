//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/selector.js`.
//!
//! `class Selector extends Prefixer`. Hacks like `autofill`,
//! `file-selector-button`, `fullscreen`, `placeholder`,
//! `placeholder-shown` subclass this.

use std::cell::RefCell;
use std::collections::HashMap;

use indexmap::IndexMap;
use postcss_core::{
    insert_before_at_path, parent_nodes, AttrValue, Node, NodeKind,
};
use regex::Regex;

use crate::browsers::Browsers;
use crate::fast_match::SelectorRegexp;
use crate::old_selector::{OldSelector, SelectorView};
use crate::prefixer::{clone_node, PrefixerBase};

/// `rule.attrs[_autoprefixerPrefixeds]: { name → { prefix → selector } }`.
pub const ATTR_PREFIXEDS: &str = "_autoprefixerPrefixeds";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SelectorBase {
    pub prefixer: PrefixerBase,
    /// `regexpCache: Map<prefix, RegExp>` — `prefix` here is the JS
    /// argument (`undefined` keyed under the empty string).
    /// Skipped on serde — same reasoning as `ValueBase::regexp_cache`:
    /// runtime cache rebuilt deterministically on demand.
    #[serde(skip)]
    regexp_cache: RefCell<HashMap<String, SelectorRegexp>>,
}

impl SelectorBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            prefixer: PrefixerBase::new(name, prefixes, all_id),
            regexp_cache: RefCell::new(HashMap::new()),
        }
    }

    /// JS: `check(rule)` — `rule.selector.includes(name) && match(regexp)`.
    pub fn check(&self, rule: &Node) -> bool {
        let selector = match &rule.kind {
            NodeKind::Rule(r) => &r.selector,
            _ => return false,
        };
        if !selector.contains(&self.prefixer.name) {
            return false;
        }
        let re = self.regexp(None);
        re.is_match(selector)
    }

    /// JS: `prefixed(prefix)` — `this.name.replace(/^(\W*)/, '$1' + prefix)`.
    pub fn prefixed(&self, prefix: &str) -> String {
        // `\W*` matches the leading non-word characters — for `:fullscreen`
        // that's `:` (or `::` for pseudo-elements). Insert `prefix`
        // between the leading `\W*` block and the rest.
        static LEADING: once_cell::sync::Lazy<Regex> =
            once_cell::sync::Lazy::new(|| Regex::new(r"^(\W*)").unwrap());
        let name = &self.prefixer.name;
        let m = LEADING.find(name).expect("\\W* always matches");
        let leading = m.as_str();
        let rest = &name[m.end()..];
        format!("{leading}{prefix}{rest}")
    }

    /// JS: `regexp(prefix)` — lazy.
    /// `new RegExp("(^|[^:\"'=])" + escape(name|prefixed), "gi")`.
    /// Returns by value (cheap clone — wrapper either holds a small
    /// `SelectorMatcher` or a refcounted regex internal).
    pub fn regexp(&self, prefix: Option<&str>) -> SelectorRegexp {
        let key = prefix.unwrap_or("").to_string();
        if let Some(re) = self.regexp_cache.borrow().get(&key) {
            return re.clone();
        }
        let target = match prefix {
            Some(p) => self.prefixed(p),
            None => self.prefixer.name.clone(),
        };
        let re = SelectorRegexp::new(&target);
        self.regexp_cache.borrow_mut().insert(key, re.clone());
        re
    }

    /// JS: `possible()` → Browsers.prefixes().
    pub fn possible(&self) -> &'static [String] {
        Browsers::prefixes()
    }

    /// JS: `replace(selector, prefix)` —
    /// `selector.replace(regexp, '$1' + prefixed(prefix))`.
    pub fn replace(&self, selector: &str, prefix: &str) -> String {
        let re = self.regexp(None);
        let prefixed = self.prefixed(prefix);
        re.replace_all_with(selector, &prefixed)
    }

    /// JS: `prefixeds(rule)` — populate per-rule cache of all possible
    /// prefixed selectors.
    /// ```js
    /// prefixeds(rule) {
    ///   if (rule._autoprefixerPrefixeds && rule._autoprefixerPrefixeds[this.name]) return rule._autoprefixerPrefixeds
    ///   else rule._autoprefixerPrefixeds = {}
    ///   let prefixeds = {}
    ///   if (rule.selector.includes(',')) {
    ///     let toProcess = list.comma(rule.selector).filter(el => el.includes(this.name))
    ///     for (let prefix of this.possible()) prefixeds[prefix] = toProcess.map(el => this.replace(el, prefix)).join(', ')
    ///   } else {
    ///     for (let prefix of this.possible()) prefixeds[prefix] = this.replace(rule.selector, prefix)
    ///   }
    ///   rule._autoprefixerPrefixeds[this.name] = prefixeds
    ///   return rule._autoprefixerPrefixeds
    /// }
    /// ```
    pub fn prefixeds(
        &self,
        rule: &mut Node,
    ) -> IndexMap<String, IndexMap<String, String>> {
        if let Some(cache) = rule.attrs.get_nested_string_map(ATTR_PREFIXEDS) {
            if cache.contains_key(&self.prefixer.name) {
                return cache.clone();
            }
        }

        let selector = match &rule.kind {
            NodeKind::Rule(r) => r.selector.clone(),
            _ => String::new(),
        };

        let mut prefixeds: IndexMap<String, String> = IndexMap::new();
        if selector.contains(',') {
            let parts: Vec<String> = postcss_core::list::comma(&selector)
                .into_iter()
                .filter(|el| el.contains(&self.prefixer.name))
                .collect();
            for prefix in self.possible() {
                let joined: Vec<String> =
                    parts.iter().map(|el| self.replace(el, prefix)).collect();
                prefixeds.insert(prefix.clone(), joined.join(", "));
            }
        } else {
            for prefix in self.possible() {
                prefixeds.insert(prefix.clone(), self.replace(&selector, prefix));
            }
        }

        // Merge into existing cache (if any).
        let mut existing = rule
            .attrs
            .get_nested_string_map(ATTR_PREFIXEDS)
            .cloned()
            .unwrap_or_default();
        existing.insert(self.prefixer.name.clone(), prefixeds);
        rule.attrs
            .set(ATTR_PREFIXEDS, AttrValue::NestedStringMap(existing.clone()));
        existing
    }

    /// JS: `already(rule, prefixeds, prefix)` — walks BACKWARDS through
    /// previous siblings of `rule`. If a previous rule has a selector
    /// matching `prefixeds[name][prefix]`, return true. If we hit a
    /// non-rule sibling or a rule whose selector doesn't match ANY
    /// known prefixed form, return false.
    pub fn already(
        &self,
        root: &Node,
        path: &[usize],
        prefixeds: &IndexMap<String, IndexMap<String, String>>,
        prefix: &str,
    ) -> bool {
        let parent_kids = match parent_nodes(root, path) {
            Some(p) => p,
            None => return false,
        };
        let here_index = match path.last().copied() {
            Some(i) => i,
            None => return false,
        };
        if here_index == 0 {
            return false;
        }
        let mut idx = here_index as isize - 1;

        while idx >= 0 {
            let before = match parent_kids.get(idx as usize) {
                Some(n) => n,
                None => return false,
            };
            let before_selector = match &before.kind {
                NodeKind::Rule(r) => &r.selector,
                _ => return false,
            };

            let mut some = false;
            if let Some(map_for_name) = prefixeds.get(&self.prefixer.name) {
                for (key, prefixed_sel) in map_for_name {
                    if before_selector == prefixed_sel {
                        if key == prefix {
                            return true;
                        } else {
                            some = true;
                            break;
                        }
                    }
                }
            }
            if !some {
                return false;
            }

            idx -= 1;
        }

        false
    }

    /// JS: `add(rule, prefix)` — clone with prefixed selector, insert
    /// before. Skips if `already(...)` returns true.
    pub fn add(&self, root: &mut Node, path: &[usize], prefix: &str) {
        // Build prefixeds cache on the rule.
        let prefixeds = {
            let here = match postcss_core::node_at_path_mut(root, path) {
                Some(n) => n,
                None => return,
            };
            self.prefixeds(here)
        };

        if self.already(root, path, &prefixeds, prefix) {
            return;
        }

        let new_selector = match prefixeds
            .get(&self.prefixer.name)
            .and_then(|m| m.get(prefix))
        {
            Some(s) => s.clone(),
            None => return,
        };

        let original = match postcss_core::node_at_path(root, path) {
            Some(n) => n,
            None => return,
        };
        let mut cloned = clone_node(original);
        if let NodeKind::Rule(ref mut r) = cloned.kind {
            r.selector = new_selector;
        }

        insert_before_at_path(root, path, cloned);
    }

    /// JS: `old(prefix)` → `new OldSelector(this, prefix)`.
    pub fn old(&self, prefix: &str) -> OldSelector {
        let view = SelectorView {
            prefixed: self.prefixed(prefix),
            regexp: self.regexp(Some(prefix)),
            prefixeds: self
                .possible()
                .iter()
                .map(|p| (self.prefixed(p), self.regexp(Some(p))))
                .collect(),
            unprefixed: self.prefixer.name.clone(),
            name_regexp: self.regexp(None),
        };
        OldSelector::new(view, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    #[test]
    fn check_matches_when_selector_contains_name() {
        let r = parse(":fullscreen { color: red; }").unwrap();
        let s = SelectorBase::new(":fullscreen".into(), vec![], 0);
        let rule = &r.root.nodes().unwrap()[0];
        assert!(s.check(rule));
    }

    #[test]
    fn check_does_not_match_unrelated_selector() {
        let r = parse(".other { color: red; }").unwrap();
        let s = SelectorBase::new(":fullscreen".into(), vec![], 0);
        let rule = &r.root.nodes().unwrap()[0];
        assert!(!s.check(rule));
    }

    #[test]
    fn prefixed_inserts_prefix_after_leading_non_word() {
        let s = SelectorBase::new(":fullscreen".into(), vec![], 0);
        assert_eq!(s.prefixed("-webkit-"), ":-webkit-fullscreen");
    }

    #[test]
    fn replace_swaps_selector_in_place() {
        let s = SelectorBase::new(":fullscreen".into(), vec![], 0);
        assert_eq!(
            s.replace(":fullscreen", "-webkit-"),
            ":-webkit-fullscreen"
        );
    }

    #[test]
    fn add_inserts_prefixed_clone_before_rule() {
        let mut r = parse(":fullscreen { color: red; }").unwrap();
        let s = SelectorBase::new(
            ":fullscreen".into(),
            vec!["-webkit-".into()],
            0,
        );
        s.add(&mut r.root, &[0], "-webkit-");
        let out = stringify(&r);
        assert!(out.contains(":-webkit-fullscreen"));
        assert!(out.contains(":fullscreen"));
    }

    #[test]
    fn add_skips_when_already_prefixed_sibling_exists() {
        let mut r = parse(
            ":-webkit-fullscreen { color: red; }\n:fullscreen { color: red; }",
        )
        .unwrap();
        let len_before = r.root.nodes().unwrap().len();
        let s = SelectorBase::new(
            ":fullscreen".into(),
            vec!["-webkit-".into()],
            0,
        );
        // path [1] points at the unprefixed rule.
        s.add(&mut r.root, &[1], "-webkit-");
        assert_eq!(r.root.nodes().unwrap().len(), len_before);
    }
}
