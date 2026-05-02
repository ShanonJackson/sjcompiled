//! Port of `postcss/lib/node.js`.
//!
//! postcss uses prototype-based subclassing (Root, AtRule, Rule, Declaration,
//! Comment, Container). We model the AST as a tagged enum (`NodeKind`) with a
//! shared metadata struct (`Node`) so every variant carries `raws` and
//! `source`.

use indexmap::IndexMap;

#[derive(Debug, Clone, Default)]
pub struct Raws {
    /// Raw whitespace + comments before this node.
    pub before: Option<String>,
    /// Raw whitespace + comments after this node (containers only).
    pub after: Option<String>,
    /// Raw text between selector and `{` (rules) / between name and params (atrules) / between prop and `:` (decls).
    pub between: Option<String>,
    /// Whether the declaration list ended with a semicolon (containers).
    pub semicolon: Option<bool>,
    /// Atrule-specific: between `@name` and params.
    pub after_name: Option<String>,
    /// Decl `!important` raws.
    pub important: Option<String>,
    /// Comment-specific raws.
    pub left: Option<String>,
    pub right: Option<String>,
    /// Raw declaration value (preserves original byte sequence, including comments).
    pub value: Option<RawValue>,
    /// Raw selector (rule) — preserves exact bytes.
    pub selector: Option<RawValue>,
    /// Raw atrule params.
    pub params: Option<RawValue>,
    /// Bare `;` token after a rule body — `rule.raws.ownSemicolon`.
    pub own_semicolon: Option<String>,
    /// Other raws postcss attaches dynamically.
    pub other: IndexMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RawValue { pub value: String, pub raw: String }

#[derive(Debug, Clone, Default)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Source {
    pub start: Option<SourcePosition>,
    pub end: Option<SourcePosition>,
    pub input_id: u64,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Root(crate::root::RootInner),
    AtRule(crate::at_rule::AtRule),
    Rule(crate::rule::Rule),
    Declaration(crate::declaration::Declaration),
    Comment(crate::comment::Comment),
}

impl NodeKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Root(_) => "root",
            NodeKind::AtRule(_) => "atrule",
            NodeKind::Rule(_) => "rule",
            NodeKind::Declaration(_) => "decl",
            NodeKind::Comment(_) => "comment",
        }
    }

    /// Whether this kind carries a child list (Root/AtRule with body/Rule).
    pub fn is_container(&self) -> bool {
        match self {
            NodeKind::Root(_) | NodeKind::Rule(_) => true,
            NodeKind::AtRule(a) => a.has_block,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub kind: NodeKind,
    pub raws: Raws,
    pub source: Source,
    /// Free-form per-node attribute bag.
    ///
    /// Mirrors the JS pattern of stashing plugin-private state directly on
    /// the AST node — e.g. `node._autoprefixerPrefix`,
    /// `node._autoprefixerValues`, `decl._autoprefixerCascade`. Plugin
    /// authors namespace their keys (`_<plugin>_<field>`) to avoid
    /// collisions; the cardinal-rule check on `IndexMap` (no `HashMap`)
    /// stands because some consumers iterate this map and that order can
    /// reach output bytes (e.g. autoprefixer's `_autoprefixerValues`).
    ///
    /// See `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` § "Per-node attribute
    /// bag" for the contract.
    pub attrs: NodeAttrs,
}

/// Free-form per-node attribute bag. Insertion-ordered (`IndexMap`) —
/// downstream plugins (notably autoprefixer) iterate the map and that
/// order reaches output bytes.
#[derive(Debug, Clone, Default)]
pub struct NodeAttrs {
    map: IndexMap<String, AttrValue>,
}

/// Tagged value stashed on a [`Node`]'s [`NodeAttrs`]. Variants cover
/// every JS pattern observed in upstream — extend if a new plugin needs
/// something this enum can't express.
#[derive(Debug, Clone)]
pub enum AttrValue {
    Bool(bool),
    String(String),
    Int(i64),
    /// `_autoprefixerValues: { 'foo' → 'bar' }` shape.
    StringMap(IndexMap<String, String>),
    /// `_autoprefixerPrefixeds: { 'name' → { 'prefix' → 'value' } }` shape.
    NestedStringMap(IndexMap<String, IndexMap<String, String>>),
}

impl NodeAttrs {
    pub fn new() -> Self { NodeAttrs::default() }

    pub fn get(&self, key: &str) -> Option<&AttrValue> { self.map.get(key) }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut AttrValue> { self.map.get_mut(key) }
    pub fn set<K: Into<String>>(&mut self, key: K, value: AttrValue) {
        self.map.insert(key.into(), value);
    }
    pub fn contains(&self, key: &str) -> bool { self.map.contains_key(key) }
    pub fn remove(&mut self, key: &str) -> Option<AttrValue> {
        self.map.shift_remove(key)
    }
    pub fn iter(&self) -> indexmap::map::Iter<String, AttrValue> { self.map.iter() }
    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, String, AttrValue> { self.map.iter_mut() }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }

    // Typed-accessor sugar. Returns None if the key is missing OR the
    // variant doesn't match — matches JS's loose-typing default of
    // `undefined` on shape mismatch.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.map.get(key) { Some(AttrValue::Bool(b)) => Some(*b), _ => None }
    }
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.map.get(key) { Some(AttrValue::String(s)) => Some(s.as_str()), _ => None }
    }
    pub fn get_int(&self, key: &str) -> Option<i64> {
        match self.map.get(key) { Some(AttrValue::Int(i)) => Some(*i), _ => None }
    }
    pub fn get_string_map(&self, key: &str) -> Option<&IndexMap<String, String>> {
        match self.map.get(key) { Some(AttrValue::StringMap(m)) => Some(m), _ => None }
    }
    pub fn get_string_map_mut(&mut self, key: &str) -> Option<&mut IndexMap<String, String>> {
        match self.map.get_mut(key) { Some(AttrValue::StringMap(m)) => Some(m), _ => None }
    }
    pub fn get_nested_string_map(&self, key: &str) -> Option<&IndexMap<String, IndexMap<String, String>>> {
        match self.map.get(key) { Some(AttrValue::NestedStringMap(m)) => Some(m), _ => None }
    }
    pub fn get_nested_string_map_mut(&mut self, key: &str) -> Option<&mut IndexMap<String, IndexMap<String, String>>> {
        match self.map.get_mut(key) { Some(AttrValue::NestedStringMap(m)) => Some(m), _ => None }
    }
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Root(crate::root::RootInner::default())
    }
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Node { kind, raws: Raws::default(), source: Source::default(), attrs: NodeAttrs::default() }
    }

    pub fn type_name(&self) -> &'static str { self.kind.type_name() }

    /// Mutable child list, when this node is a container.
    pub fn nodes_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.kind {
            NodeKind::Root(r) => Some(&mut r.nodes),
            NodeKind::Rule(r) => Some(&mut r.nodes),
            NodeKind::AtRule(a) if a.has_block => Some(&mut a.nodes),
            _ => None,
        }
    }

    pub fn nodes(&self) -> Option<&Vec<Node>> {
        match &self.kind {
            NodeKind::Root(r) => Some(&r.nodes),
            NodeKind::Rule(r) => Some(&r.nodes),
            NodeKind::AtRule(a) if a.has_block => Some(&a.nodes),
            _ => None,
        }
    }

    /// Deep clone, then drop the named attr keys from the cloned root
    /// AND every descendant. Mirrors upstream `node.clone()` callers
    /// that strip `_autoprefixerPrefix` / `_autoprefixerValues` / `proxyCache`
    /// in `prefixer.js::clone`. Generalized so any plugin can declare
    /// the keys to strip; pass `&[]` for a plain deep clone.
    pub fn clone_without(&self, attrs_to_drop: &[&str]) -> Self {
        let mut cloned = self.clone();
        cloned.strip_attrs_recursive(attrs_to_drop);
        cloned
    }

    fn strip_attrs_recursive(&mut self, attrs_to_drop: &[&str]) {
        for k in attrs_to_drop { self.attrs.remove(k); }
        if let Some(children) = self.nodes_mut() {
            for child in children { child.strip_attrs_recursive(attrs_to_drop); }
        }
    }
}
