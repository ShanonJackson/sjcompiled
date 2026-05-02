//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/value.js`.
//!
//! `Value extends Prefixer`. Hacks like `gradient`, `cross-fade`,
//! `display-flex`, `image-set`, `pixelated`, `intrinsic`, `filter-value`
//! subclass this.
//!
//! # Status (Phase 7 foundation)
//!
//! Struct + signatures locked. Bodies unimplemented — depend on a
//! node-attribute bag (`_autoprefixerValues` cache) plus parent-pointer
//! API. Foundation agent's TODO. **Hacks agent: do not start.**

use postcss_core::Node;

use crate::old_value::OldValue;
use crate::prefixer::PrefixerBase;

pub struct ValueBase {
    pub prefixer: PrefixerBase,
}

impl ValueBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { prefixer: PrefixerBase::new(name, prefixes, all_id) }
    }

    /// JS: `check(decl)` — `decl.value` includes name AND matches regexp.
    pub fn check(&self, _decl: &Node) -> bool {
        unimplemented!("Phase 7 — port value.js::check")
    }

    /// JS: `regexp()` — lazy. `(^|[\\s,(])(name($|[\\s(,]))` case-insensitive.
    pub fn regexp(&self) -> regex::Regex {
        unimplemented!("Phase 7 — port value.js::regexp (lazy cache)")
    }

    /// JS: `replace(string, prefix)` — `$1${prefix}$2`.
    pub fn replace(&self, _string: &str, _prefix: &str) -> String {
        unimplemented!("Phase 7 — port value.js::replace")
    }

    /// JS: `value(decl)` — return `decl.raws.value.raw` if it represents
    /// the unmodified original `decl.value`, else `decl.value`.
    pub fn value(&self, _decl: &Node) -> String {
        unimplemented!("Phase 7 — port value.js::value")
    }

    /// JS: `add(decl, prefix)` — apply `replace` repeatedly until stable.
    pub fn add(&mut self, _decl: &mut Node, _prefix: &str) {
        unimplemented!("Phase 7 — port value.js::add")
    }

    /// JS: `old(prefix)` → new OldValue(this.name, prefix + this.name).
    pub fn old(&self, prefix: &str) -> OldValue {
        OldValue::new(
            self.prefixer.name.clone(),
            format!("{prefix}{}", self.prefixer.name),
            None,
            None,
        )
    }
}
