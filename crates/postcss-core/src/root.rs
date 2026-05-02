//! Port of `postcss/lib/root.js`.

use indexmap::IndexMap;

use crate::node::{Node, NodeKind, Raws, Source};

#[derive(Debug, Clone, Default)]
pub struct RootInner {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Default)]
pub struct Root {
    pub root: Node,
    /// `root.rawCache` upstream — the plugin-writable raws override map
    /// consumed by the stringifier at higher priority than the tree-scan
    /// fallback. See `postcss/lib/stringifier.js::raw()` line 158:
    ///
    /// ```js
    /// if (typeof root.rawCache[detect] !== 'undefined') {
    ///   return root.rawCache[detect]
    /// }
    /// ```
    ///
    /// A plugin writes the keys it cares about (e.g. `cssnano-util-raw-cache`
    /// writes `{ colon: ':', indent: '', beforeDecl: '', ... }` on
    /// `OnceExit` to make the subsequent stringification emit minified
    /// bytes without rewriting every node's raws individually).
    ///
    /// **Insertion order** is preserved (`IndexMap`) — even though the
    /// stringifier looks each key up by name, walking iteration order
    /// is observable for callers that scan `root.rawCache` themselves.
    ///
    /// Today's `transformCss` / `sort` pipelines filter out
    /// `cssnano-util-raw-cache` (see `normalize-css.ts:71-73`), so this
    /// map stays empty and the stringifier falls through to its
    /// tree-scan fallback. The field exists so any future plugin that
    /// writes `root.rawCache` works at the same priority JS does.
    pub raw_cache: IndexMap<String, String>,
}

impl Root {
    pub fn new() -> Self {
        Root {
            root: Node {
                kind: NodeKind::Root(RootInner::default()),
                ..Node::default()
            },
            raw_cache: IndexMap::new(),
        }
    }

    pub fn nodes(&self) -> &Vec<Node> {
        match &self.root.kind {
            NodeKind::Root(r) => &r.nodes,
            _ => unreachable!("Root::root.kind is always NodeKind::Root"),
        }
    }

    pub fn nodes_mut(&mut self) -> &mut Vec<Node> {
        match &mut self.root.kind {
            NodeKind::Root(r) => &mut r.nodes,
            _ => unreachable!("Root::root.kind is always NodeKind::Root"),
        }
    }

    pub fn raws(&self) -> &Raws { &self.root.raws }
    pub fn raws_mut(&mut self) -> &mut Raws { &mut self.root.raws }

    pub fn push(&mut self, n: Node) { self.nodes_mut().push(n); }

    /// Plugin-facing `root.rawCache[key] = value` setter. Mirrors
    /// upstream's pattern of plugins writing keys directly on the JS
    /// object. Use this from `OnceExit` hooks to influence
    /// stringification of every node that doesn't have its own
    /// `raws.<key>` set.
    pub fn set_raw_cache<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.raw_cache.insert(key.into(), value.into());
    }

    pub fn get_raw_cache(&self, key: &str) -> Option<&str> {
        self.raw_cache.get(key).map(|s| s.as_str())
    }
}
