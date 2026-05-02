//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/at-rule.js`.
//!
//! `AtRule extends Prefixer`. Tiny — only `add` and `process`.
//!
//! # Status (Phase 7 foundation)
//!
//! Struct shape locked. Bodies unimplemented — depend on container API
//! support for `parent.insertBefore` from a plugin walk. Foundation
//! agent's TODO.

use postcss_core::Node;

use crate::prefixer::PrefixerBase;

pub struct AtRuleBase {
    pub prefixer: PrefixerBase,
}

impl AtRuleBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { prefixer: PrefixerBase::new(name, prefixes, all_id) }
    }

    /// JS: `add(rule, prefix)` — clone with `name = prefix + rule.name`,
    /// guarded by an existing-sibling check on `name + params`.
    pub fn add(&mut self, _rule: &mut Node, _prefix: &str) -> Option<()> {
        unimplemented!("Phase 7 — port at-rule.js::add")
    }

    /// JS: `process(node)` — for each prefix, call `add` if the parent
    /// prefix doesn't conflict.
    pub fn process(&mut self, _node: &mut Node) {
        unimplemented!("Phase 7 — port at-rule.js::process")
    }
}
