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

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub raws: Raws,
    pub source: Source,
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Node { kind, raws: Raws::default(), source: Source::default() }
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
}
