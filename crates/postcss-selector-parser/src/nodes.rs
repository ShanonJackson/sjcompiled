//! Port of `postcss-selector-parser/dist/selectors/*.js` (Node hierarchy).
//!
//! Folder/file mapping (1:1 with `dist/selectors/`):
//!   - `attribute.js`   -> [`NodeKind::Attribute`] + [`AttributePayload`]
//!   - `className.js`   -> [`NodeKind::ClassName`]
//!   - `combinator.js`  -> [`NodeKind::Combinator`]
//!   - `comment.js`     -> [`NodeKind::Comment`]
//!   - `id.js`          -> [`NodeKind::Identifier`]
//!   - `nesting.js`     -> [`NodeKind::Nesting`]
//!   - `pseudo.js`      -> [`NodeKind::Pseudo`]
//!   - `root.js`        -> [`NodeKind::Root`]
//!   - `selector.js`    -> [`NodeKind::Selector`]
//!   - `string.js`      -> [`NodeKind::String`]
//!   - `tag.js`         -> [`NodeKind::Tag`]
//!   - `universal.js`   -> [`NodeKind::Universal`]

#[derive(Debug, Clone, Default)]
pub struct Spaces { pub before: String, pub after: String }

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub kind: NodeKind,
    /// Bare value: class name without `.`, id without `#`, tag name,
    /// attribute bracket text, pseudo selector with `:` or `::` prefix,
    /// combinator char (`>`/`+`/`~`/` `), etc.
    pub value: String,
    pub spaces: Spaces,
    /// Selector / Root containers hold child nodes. Pseudo nodes hold
    /// argument selectors here too.
    pub nodes: Vec<Node>,
    /// Original source bytes for byte-identical round-trip when no
    /// mutation has occurred. Mutators MUST clear this when changing
    /// `value` / `nodes` / `spaces`.
    pub raw_value: Option<String>,
    /// Attribute payload (operator, value, quote, etc.) when [`kind`] is Attribute.
    pub attribute: Option<AttributePayload>,
}

#[derive(Debug, Clone, Default)]
pub struct AttributePayload {
    pub namespace: Option<String>,
    pub attribute: String,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub quote_mark: Option<char>,
    pub case_insensitive: bool,
    pub raws_unquoted: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NodeKind {
    #[default]
    Root,
    Selector,
    ClassName,
    Combinator,
    Identifier,
    Pseudo,
    Universal,
    String,
    Tag,
    Attribute,
    Comment,
    Nesting,
}

impl Node {
    pub fn root() -> Self { Node { kind: NodeKind::Root, ..Default::default() } }
    pub fn selector() -> Self { Node { kind: NodeKind::Selector, ..Default::default() } }
    pub fn set_raw_value(&mut self, v: String) { self.raw_value = Some(v); }

    /// Mutate `value` and clear `raw_value` so the stringifier re-renders.
    /// Plugin authors: call this whenever you change a Node's value.
    pub fn set_value(&mut self, v: String) {
        self.value = v;
        self.raw_value = None;
    }

    pub fn to_string_repr(&self) -> String {
        if let Some(raw) = &self.raw_value { return raw.clone(); }
        self.value.clone()
    }
}
