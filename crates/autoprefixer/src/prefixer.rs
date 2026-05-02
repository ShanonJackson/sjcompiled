//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/prefixer.js`.
//!
//! In JS `Prefixer` is a class that every hack subclasses. In Rust we
//! model the *protocol* as a trait, plus a base struct holding the shared
//! fields (`name`, `prefixes`, `all`). Hacks own a `PrefixerBase` and
//! delegate by composition. This is the cleanest way to keep the JS
//! `super.method(...)` pattern intact without runtime dynamic dispatch
//! on a deep class chain.
//!
//! The JS static methods (`hack`, `load`, `clone`) live on the trait as
//! free functions / associated items below — see `prefixer::registry`
//! for the runtime hack lookup that `prefixes.rs` populates.

use postcss_core::{Node, NodeKind};

use crate::utils;
use crate::vendor;

/// Shared state every Prefixer carries.
#[derive(Debug, Clone)]
pub struct PrefixerBase {
    pub name: String,
    pub prefixes: Vec<String>,
    /// In JS this is a back-pointer to the `Prefixes` registry. We hold an
    /// index into the `Processor` so the trait stays object-safe.
    pub all_id: usize,
}

impl PrefixerBase {
    pub fn new(name: impl Into<String>, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { name: name.into(), prefixes, all_id }
    }
}

/// The Prefixer protocol — `check` / `add` / `process` come from JS.
/// Subclasses (`Declaration`, `Value`, `Selector`, `AtRule`) implement
/// these in their own file; hacks override individual methods.
pub trait Prefixer {
    fn base(&self) -> &PrefixerBase;
    fn base_mut(&mut self) -> &mut PrefixerBase;

    /// JS default `check` is `true` (overridden by `Value` and several
    /// hacks). Subclasses override.
    fn check(&self, _node: &Node) -> bool {
        true
    }

    /// JS `add(node, prefix, prefixes, result)` — subclass-specific.
    /// Default unimplemented; each subclass file implements.
    fn add(
        &mut self,
        node: &mut Node,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()>;

    /// `process(node, result)` — JS default loops over `this.prefixes`,
    /// filters by `parentPrefix`, and calls `add` for each. Hacks
    /// (notably `Declaration`) override to add cascade logic.
    fn process(&mut self, node: &mut Node) -> Option<Vec<String>> {
        if !self.check(node) {
            return None;
        }
        let parent = parent_prefix(node);
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
            if self.add(node, prefix, &next).is_some() {
                added.push(prefix.clone());
            }
        }

        Some(added)
    }
}

/// JS `parentPrefix` walks up `node.parent` looking for a vendor-prefix
/// hint. Returns `None` for root, `Some("-webkit-")` for a prefixed
/// ancestor.
#[derive(Debug, Clone)]
pub enum ParentPrefix {
    /// `false` in JS (root reached without finding one).
    None,
    Some(String),
}

pub fn parent_prefix(node: &Node) -> ParentPrefix {
    // The `_autoprefixerPrefix` cache lives on the JS node. Our `Node`
    // doesn't yet have a freeform attribute bag; a real port adds one
    // (see TODO note below). For now, recompute on each call.

    match &node.kind {
        NodeKind::Declaration(decl) if decl.prop.starts_with('-') => {
            let p = vendor::prefix(&decl.prop);
            sanitize(p)
        }
        NodeKind::Root(_) => ParentPrefix::None,
        NodeKind::Rule(rule) if rule.selector.contains(":-") => {
            // Match `:(-\w+-)`.
            static PSEUDO: once_cell::sync::Lazy<regex::Regex> =
                once_cell::sync::Lazy::new(|| regex::Regex::new(r":(-\w+-)").unwrap());
            if let Some(caps) = PSEUDO.captures(&rule.selector) {
                sanitize(caps.get(1).unwrap().as_str().to_string())
            } else {
                walk_parent(node)
            }
        }
        NodeKind::AtRule(at) if at.name.starts_with('-') => {
            sanitize(vendor::prefix(&at.name))
        }
        _ => walk_parent(node),
    }
}

fn sanitize(p: String) -> ParentPrefix {
    if crate::browsers::Browsers::is_prefix(&p) {
        ParentPrefix::Some(p)
    } else {
        ParentPrefix::None
    }
}

fn walk_parent(_node: &Node) -> ParentPrefix {
    // TODO(phase-7): walk up parent pointers. `postcss-core`'s `Node`
    // does not yet carry a parent back-reference suitable for upward
    // walks during plugin execution; the processor injects it via a
    // visitor context. The processor port will add the appropriate
    // `parent_prefix(node, ctx)` overload.
    ParentPrefix::None
}

/// JS `clone` — deep-clone a node, dropping autoprefixer-internal
/// caches (`_autoprefixerPrefix`, `_autoprefixerValues`, `proxyCache`).
/// `postcss-core::Node` is `Clone` already; the cache scrub becomes a
/// method on whichever attribute bag we add.
pub fn clone_node(node: &Node) -> Node {
    node.clone()
}
