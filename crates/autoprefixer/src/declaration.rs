//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/declaration.js`.
//!
//! `Declaration extends Prefixer`. Hacks subclass this most often.
//!
//! # Status (Phase 7 foundation)
//!
//! Struct + trait shape locked. Method bodies are `unimplemented!()`
//! pending the postcss-core parent-pointer surface (see
//! `prefixer.rs::walk_parent` TODO) and a node-attribute bag for the
//! JS-side caches (`_autoprefixerCascade`, `_autoprefixerMax`). The
//! foundation agent owns extending postcss-core with both. **Hacks
//! agent: do not start subclassing this until method signatures are
//! filled in — the trait surface may shift.**

use postcss_core::Node;

use crate::prefixer::PrefixerBase;

/// `class Declaration extends Prefixer` — composition shim.
pub struct DeclarationBase {
    pub prefixer: PrefixerBase,
}

impl DeclarationBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { prefixer: PrefixerBase::new(name, prefixes, all_id) }
    }

    /// JS: `prefixed(prop, prefix) { return prefix + prop }`.
    pub fn prefixed(&self, prop: &str, prefix: &str) -> String {
        format!("{prefix}{prop}")
    }

    /// JS: `normalize(prop) { return prop }` (default; hacks override).
    pub fn normalize<'a>(&self, prop: &'a str) -> &'a str {
        prop
    }

    /// JS: `otherPrefixes(value, prefix)` — does `value` contain a vendor
    /// prefix that isn't `prefix`? Returns `false` when the only "other
    /// prefix" hits are inside `var(...)`.
    pub fn other_prefixes(&self, _value: &str, _prefix: &str) -> bool {
        unimplemented!("Phase 7 — port declaration.js::otherPrefixes")
    }

    /// JS: `set(decl, prefix) { decl.prop = this.prefixed(decl.prop, prefix) }`.
    pub fn set(&self, _decl: &mut Node, _prefix: &str) -> Option<()> {
        unimplemented!("Phase 7 — port declaration.js::set")
    }

    /// `add(decl, prefix, prefixes, result)` — JS lifecycle hook. Walks
    /// `isAlready` / `otherPrefixes` checks then `insert`s a clone.
    pub fn add(
        &mut self,
        _decl: &mut Node,
        _prefix: &str,
        _prefixes: &[String],
    ) -> Option<()> {
        unimplemented!("Phase 7 — port declaration.js::add")
    }

    // Cascade helpers — `needCascade`, `maxPrefixed`, `calcBefore`,
    // `restoreBefore`, `insert`, `isAlready`, `process`, `old` — all
    // unimplemented pending the postcss-core attribute bag.
}
