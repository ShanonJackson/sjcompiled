//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/selector.js`.
//!
//! `Selector extends Prefixer`. Hacks like `autofill`,
//! `file-selector-button`, `fullscreen`, `placeholder`,
//! `placeholder-shown` subclass this.
//!
//! # Status (Phase 7 foundation)
//!
//! Struct + signatures locked. Bodies unimplemented — depend on a
//! node-attribute bag (`_autoprefixerPrefixeds`) and parent-pointer
//! container API. Foundation agent's TODO.

use std::collections::HashMap;

use postcss_core::Node;
use regex::Regex;

use crate::old_selector::OldSelector;
use crate::prefixer::PrefixerBase;

pub struct SelectorBase {
    pub prefixer: PrefixerBase,
    /// `regexpCache` — lazy regex per prefix (or `""` for the unprefixed).
    pub regexp_cache: HashMap<String, Regex>,
}

impl SelectorBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            prefixer: PrefixerBase::new(name, prefixes, all_id),
            regexp_cache: HashMap::new(),
        }
    }

    /// JS: `check(rule)` — `rule.selector.includes(this.name) && match(regexp)`.
    pub fn check(&mut self, _rule: &Node) -> bool {
        unimplemented!("Phase 7 — port selector.js::check")
    }

    /// JS: `prefixed(prefix)` — `this.name.replace(/^(\W*)/, $1+prefix)`.
    pub fn prefixed(&self, _prefix: &str) -> String {
        unimplemented!("Phase 7 — port selector.js::prefixed")
    }

    /// JS: `regexp(prefix)` — lazy. `(^|[^:"'=])escape(name|prefixed)` /gi.
    pub fn regexp(&mut self, _prefix: Option<&str>) -> &Regex {
        unimplemented!("Phase 7 — port selector.js::regexp")
    }

    /// JS: `possible()` → Browsers.prefixes().
    pub fn possible(&self) -> &'static [String] {
        crate::browsers::Browsers::prefixes()
    }

    /// JS: `replace(selector, prefix)` — pin every `name` occurrence
    /// to its prefixed form.
    pub fn replace(&mut self, _selector: &str, _prefix: &str) -> String {
        unimplemented!("Phase 7 — port selector.js::replace")
    }

    /// JS: `add(rule, prefix)` — clone with prefixed selector inserted
    /// before the rule.
    pub fn add(&mut self, _rule: &mut Node, _prefix: &str) {
        unimplemented!("Phase 7 — port selector.js::add")
    }

    /// JS: `old(prefix)` → new OldSelector(this, prefix).
    pub fn old(&mut self, _prefix: &str) -> OldSelector {
        unimplemented!("Phase 7 — port selector.js::old (depends on `prefixed` + `regexp`)")
    }
}
