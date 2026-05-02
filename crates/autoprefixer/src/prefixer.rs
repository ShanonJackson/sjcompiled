//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/prefixer.js`.
//!
//! In JS `Prefixer` is a class that every hack subclasses. In Rust we
//! model the *protocol* as a trait, plus a base struct holding the shared
//! fields (`name`, `prefixes`, `all`). Hacks own a `PrefixerBase` and
//! delegate by composition. This sidesteps the deep class-chain
//! `super.method(...)` pattern without runtime dynamic dispatch.

use postcss_core::{walk_up_with, AttrValue, Node, NodeKind};

use crate::browsers::Browsers;
use crate::utils;
use crate::vendor;

/// JS-side cache key set on every node `parentPrefix` visits.
pub const ATTR_PREFIX_CACHE: &str = "_autoprefixerPrefix";

/// Keys `prefixer.js::clone` strips when deep-cloning a node.
pub const CLONE_STRIP_KEYS: &[&str] = &[
    "_autoprefixerPrefix",
    "_autoprefixerValues",
    "_autoprefixerCascade",
    "_autoprefixerMax",
    "_autoprefixerPrefixeds",
    "proxyCache",
];

/// Shared state every Prefixer carries.
#[derive(Debug, Clone)]
pub struct PrefixerBase {
    pub name: String,
    pub prefixes: Vec<String>,
    /// Index into the `Processor`'s `Prefixes` registry (JS `this.all`).
    pub all_id: usize,
}

impl PrefixerBase {
    pub fn new(name: impl Into<String>, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { name: name.into(), prefixes, all_id }
    }
}

/// Result of `parentPrefix` — JS-side this is `string | false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParentPrefix {
    /// `false` — no prefixed ancestor found, OR found prefix isn't in
    /// `Browsers.prefixes()`.
    None,
    /// A vendor prefix like `-webkit-`.
    Some(String),
}

impl ParentPrefix {
    /// `true` if `Browsers.prefixes()` contains the candidate string.
    fn sanitize(p: String) -> Self {
        if Browsers::is_prefix(&p) {
            ParentPrefix::Some(p)
        } else {
            ParentPrefix::None
        }
    }

    /// As stored in `Node.attrs`. JS uses `false | string`; we encode
    /// `None` as `Bool(false)` and `Some(s)` as `String(s)`.
    fn from_cached(value: &AttrValue) -> Self {
        match value {
            AttrValue::Bool(false) => ParentPrefix::None,
            AttrValue::String(s) => ParentPrefix::Some(s.clone()),
            // Anything else is a programming error; treat as miss.
            _ => ParentPrefix::None,
        }
    }

    fn to_cached(&self) -> AttrValue {
        match self {
            ParentPrefix::None => AttrValue::Bool(false),
            ParentPrefix::Some(s) => AttrValue::String(s.clone()),
        }
    }
}

/// Inspect a single node's *self-prefix* without walking ancestors.
/// Returns `Some(prefix)` if this node's own prop/selector/at-name
/// already encodes a vendor prefix; `None` if the answer is "look at
/// the parent". Mirrors JS `parentPrefix`'s per-node `else if` ladder.
fn self_prefix(node: &Node) -> Option<ParentPrefix> {
    match &node.kind {
        NodeKind::Declaration(decl) if decl.prop.starts_with('-') => {
            Some(ParentPrefix::sanitize(vendor::prefix(&decl.prop)))
        }
        NodeKind::Root(_) => Some(ParentPrefix::None),
        NodeKind::Rule(rule) if rule.selector.contains(":-") => {
            static PSEUDO: once_cell::sync::Lazy<regex::Regex> =
                once_cell::sync::Lazy::new(|| regex::Regex::new(r":(-\w+-)").unwrap());
            PSEUDO
                .captures(&rule.selector)
                .map(|c| ParentPrefix::sanitize(c.get(1).unwrap().as_str().to_string()))
        }
        NodeKind::AtRule(at) if at.name.starts_with('-') => {
            Some(ParentPrefix::sanitize(vendor::prefix(&at.name)))
        }
        _ => None,
    }
}

/// Compute `parentPrefix(node_at_path(root, path))` — checks the cache
/// first, then walks up via `walk_up_with`. Read-only — does not write
/// the cache. Use [`parent_prefix_cached_mut`] when you also want to
/// memoise the answer back onto the node.
pub fn parent_prefix(root: &Node, path: &[usize]) -> ParentPrefix {
    let here = postcss_core::node_at_path(root, path);
    if let Some(node) = here {
        if let Some(cached) = node.attrs.get(ATTR_PREFIX_CACHE) {
            return ParentPrefix::from_cached(cached);
        }
        if let Some(self_) = self_prefix(node) {
            return self_;
        }
    }

    // Walk strict ancestors looking for the first self-prefix hit.
    let mut answer: ParentPrefix = ParentPrefix::None;
    walk_up_with(root, path, |anc| {
        if let Some(cached) = anc.attrs.get(ATTR_PREFIX_CACHE) {
            answer = ParentPrefix::from_cached(cached);
            return false;
        }
        if let Some(p) = self_prefix(anc) {
            answer = p;
            return false;
        }
        true
    });
    answer
}

/// Like [`parent_prefix`] but also writes the answer to
/// `Node.attrs[ATTR_PREFIX_CACHE]` so subsequent visits short-circuit.
/// Mirrors JS `parentPrefix`'s `node._autoprefixerPrefix = prefix`
/// memoisation.
pub fn parent_prefix_cached_mut(root: &mut Node, path: &[usize]) -> ParentPrefix {
    // Compute under an immutable borrow first to avoid borrowck pain
    // when the visit function holds `&mut Node` while needing `&Node`.
    let answer = parent_prefix(root, path);
    if let Some(node) = postcss_core::node_at_path_mut(root, path) {
        node.attrs.set(ATTR_PREFIX_CACHE, answer.to_cached());
    }
    answer
}

/// `prefixer.js::clone` — deep-clone a node, dropping the
/// autoprefixer-internal caches so the clone starts fresh.
pub fn clone_node(node: &Node) -> Node {
    node.clone_without(CLONE_STRIP_KEYS)
}

/// The Prefixer protocol — `check` / `add` / `process` come from JS.
/// Subclasses (`Declaration`, `Value`, `Selector`, `AtRule`) implement
/// these in their own file; hacks override individual methods.
///
/// **Subclasses receive `(root, path)` not `(node)`** because mutations
/// like `parent.insertBefore(node, cloned)` need the path.
pub trait Prefixer {
    fn base(&self) -> &PrefixerBase;
    fn base_mut(&mut self) -> &mut PrefixerBase;

    /// JS default `check` is `true` (overridden by `Value` and several
    /// hacks). Subclasses override.
    fn check(&mut self, _root: &Node, _path: &[usize]) -> bool {
        true
    }

    /// JS `add(node, prefix, prefixes, result)`. Subclass-specific.
    /// Returns `Some(())` if the prefix was applied (so callers can
    /// track which prefixes succeeded), `None` if skipped.
    fn add(
        &mut self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()>;

    /// JS `process(node, result)` — default loops over `this.prefixes`,
    /// filters by `parentPrefix`, and calls `add` for each. Hacks
    /// (notably `Declaration`) override to add cascade logic.
    fn process(&mut self, root: &mut Node, path: &[usize]) -> Vec<String> {
        if !self.check(root, path) {
            return Vec::new();
        }

        let parent = parent_prefix_cached_mut(root, path);

        let prefixes: Vec<String> = self
            .base()
            .prefixes
            .iter()
            .filter(|p| match &parent {
                ParentPrefix::None => true,
                ParentPrefix::Some(s) => s == utils::remove_note(p),
            })
            .cloned()
            .collect();

        let mut added: Vec<String> = Vec::new();
        for prefix in &prefixes {
            let mut next = added.clone();
            next.push(prefix.clone());
            if self.add(root, path, prefix, &next).is_some() {
                added.push(prefix.clone());
            }
        }

        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn parse_root(css: &str) -> Node {
        let r = parse(css).unwrap();
        r.root
    }

    #[test]
    fn parent_prefix_root_returns_none() {
        let root = parse_root("a { color: red; }");
        assert_eq!(parent_prefix(&root, &[]), ParentPrefix::None);
    }

    #[test]
    fn parent_prefix_self_decl_with_prefix() {
        // The decl `-webkit-foo: bar` itself encodes the prefix.
        let root = parse_root("a { -webkit-foo: bar; }");
        // path: root → rule(0) → decl(0)
        let p = parent_prefix(&root, &[0, 0]);
        assert_eq!(p, ParentPrefix::Some("-webkit-".into()));
    }

    #[test]
    fn parent_prefix_walks_up_to_prefixed_atrule() {
        // The decl is unprefixed but lives inside `@-webkit-keyframes`.
        let root = parse_root("@-webkit-keyframes a { from { color: red; } }");
        // path: root → atrule(0) → rule(0) → decl(0)
        let p = parent_prefix(&root, &[0, 0, 0]);
        assert_eq!(p, ParentPrefix::Some("-webkit-".into()));
    }

    #[test]
    fn parent_prefix_caches_answer_on_node() {
        let mut root = parse_root("a { color: red; }");
        let path = vec![0, 0];
        // First call writes the cache.
        let _ = parent_prefix_cached_mut(&mut root, &path);
        let node = postcss_core::node_at_path(&root, &path).unwrap();
        assert!(node.attrs.contains(ATTR_PREFIX_CACHE));
    }

    #[test]
    fn clone_node_strips_autoprefixer_keys() {
        let mut node = Node::new(NodeKind::Root(postcss_core::root::RootInner::default()));
        node.attrs.set(ATTR_PREFIX_CACHE, AttrValue::String("-webkit-".into()));
        node.attrs.set("keep_me", AttrValue::Bool(true));
        let cloned = clone_node(&node);
        assert!(!cloned.attrs.contains(ATTR_PREFIX_CACHE));
        assert!(cloned.attrs.contains("keep_me"));
    }
}
