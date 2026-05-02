//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/old-selector.js`.
//!
//! Construction depends on the `Selector` base class (`prefixed`, `regexp`,
//! `possible`, `name`). To avoid a circular module dep, we accept a
//! pre-computed bundle (`SelectorView`) that `selector.rs` builds. This
//! keeps the 1:1 file mapping while sidestepping Rust's lack of class
//! inheritance.

use regex::Regex;

/// View of a `Selector` instance — only the bits OldSelector consumes.
#[derive(Debug, Clone)]
pub struct SelectorView {
    pub prefixed: String,
    pub regexp: Regex,
    pub prefixeds: Vec<(String, Regex)>,
    pub unprefixed: String,
    pub name_regexp: Regex,
}

#[derive(Debug, Clone)]
pub struct OldSelector {
    pub prefix: String,
    pub prefixed: String,
    pub regexp: Regex,
    pub prefixeds: Vec<(String, Regex)>,
    pub unprefixed: String,
    pub name_regexp: Regex,
}

impl OldSelector {
    /// JS:
    /// ```js
    /// constructor(selector, prefix) {
    ///   this.prefix = prefix
    ///   this.prefixed = selector.prefixed(this.prefix)
    ///   this.regexp = selector.regexp(this.prefix)
    ///   this.prefixeds = selector.possible().map(x => [selector.prefixed(x), selector.regexp(x)])
    ///   this.unprefixed = selector.name
    ///   this.nameRegexp = selector.regexp()
    /// }
    /// ```
    pub fn new(view: SelectorView, prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            prefixed: view.prefixed,
            regexp: view.regexp,
            prefixeds: view.prefixeds,
            unprefixed: view.unprefixed,
            name_regexp: view.name_regexp,
        }
    }

    /// Is rule a hack without unprefixed version below.
    ///
    /// Walks `rule.parent.nodes` from the rule's index forward; returns
    /// `true` (i.e. "this IS a hack") unless we find a sibling whose
    /// selector contains the unprefixed name AND matches `name_regexp`.
    /// Pseudo-rule signature kept abstract here — the `processor` glue
    /// supplies the iterator.
    pub fn is_hack<'a, I>(&self, mut following_selectors: I) -> bool
    where
        I: Iterator<Item = Option<&'a str>>,
    {
        loop {
            let Some(maybe) = following_selectors.next() else {
                return true;
            };
            let Some(before) = maybe else {
                return true;
            };

            if before.contains(&self.unprefixed) && self.name_regexp.is_match(before)
            {
                return false;
            }

            let mut some = false;
            for (string, regexp) in &self.prefixeds {
                if before.contains(string) && regexp.is_match(before) {
                    some = true;
                    break;
                }
            }

            if !some {
                return true;
            }
        }
    }

    /// Does rule contain an unnecessary prefixed selector.
    ///
    /// `following_selectors` should iterate the selectors of each sibling
    /// after the rule under examination; `Some(s)` for siblings that have
    /// a selector, `None` for siblings that don't (decls, etc.).
    pub fn check<'a, I>(&self, rule_selector: &str, following_selectors: I) -> bool
    where
        I: Iterator<Item = Option<&'a str>>,
    {
        if !rule_selector.contains(&self.prefixed) {
            return false;
        }
        if !self.regexp.is_match(rule_selector) {
            return false;
        }
        if self.is_hack(following_selectors) {
            return false;
        }
        true
    }
}
