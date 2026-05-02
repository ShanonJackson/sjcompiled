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

/// Mirrors upstream `Attribute.spaces` / `raws.spaces` named-sub-space
/// shape (`attribute.js::_spacesFor`, lines 213-220 in 6.1.2). Each
/// sub-field is the per-name `{ before, after }` pair — `cssnano-postcss-
/// minify-selectors@5.2.1` writes all four to clear whitespace around
/// `[name op "value" i]` parts. Carried as `Option` on `Node` so
/// non-Attribute kinds pay zero memory cost.
#[derive(Debug, Clone, Default)]
pub struct AttributeSpaces {
    pub attribute: Spaces,
    pub operator: Spaces,
    pub value: Spaces,
    pub insensitive: Spaces,
}

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
    /// Per-name sub-space pairs for Attribute nodes (mirrors upstream
    /// `Attribute.spaces`). `None` on every other kind. Plugins that
    /// mutate any sub-space MUST also set
    /// `attribute.as_mut().unwrap().dirty = true` and clear
    /// `raw_value` so the payload-aware stringifier branch fires.
    pub attribute_spaces: Option<AttributeSpaces>,
    /// 6.1.0: zero-based byte offset into the original source where this
    /// Selector node begins. Mirrors upstream `Selector.sourceIndex`
    /// (parser.js lines 120, 582, 653 in 6.1.2). Currently only set on
    /// Selector nodes; diagnostic-only — not consulted by `stringify`,
    /// but downstream plugin ports may read it.
    pub source_index: Option<usize>,
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
    /// When `true`, the stringifier rebuilds the `[ns|name op "value" i]`
    /// form from this payload + `node.attribute_spaces` instead of
    /// emitting the raw bracket text on `node.value`. Default `false`
    /// preserves byte-perfect round-trip for un-mutated Attribute nodes.
    /// Set this whenever you mutate `quote_mark`, `operator`,
    /// `attribute`, `value`, or `attribute_spaces`.
    pub dirty: bool,
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

    /// `selectorParser.nesting()` factory — produces a fresh `&` Nesting
    /// node ready for `parent.insertBefore`/`insertAfter`. Mirrors
    /// `dist/selectors/nesting.js` (default value `"&"`).
    pub fn nesting() -> Self {
        Node { kind: NodeKind::Nesting, value: "&".to_string(), ..Default::default() }
    }

    /// `selectorParser.pseudo({ value })` factory — produces a fresh
    /// Pseudo node with the given value (must include the leading `:`
    /// or `::`). The plugin author is responsible for any inner
    /// selectors via `nodes.push(Node::selector())`.
    pub fn pseudo(value: impl Into<String>) -> Self {
        Node { kind: NodeKind::Pseudo, value: value.into(), ..Default::default() }
    }

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

// --------------------------------------------------------------------------
// Mutating walks — visit every descendant of a parent and operate on the
// parent's child Vec. Used by plugins that need to insert siblings around
// a matched node (e.g. `parent-orphaned-pseudos` inserts a Nesting
// before each Pseudo; `increase-specificity` inserts a `:not(#\#)` Pseudo
// after each ClassName).
// --------------------------------------------------------------------------

/// Visit every descendant of `parent` in pre-order. For each child whose
/// kind is in `match_kinds`, invoke `f(parent, idx)`. The visitor may
/// mutate `parent.nodes` at-will; the walker re-reads the length each
/// iteration and only advances the cursor past inserted siblings if the
/// visitor returns `Skip { advance }`. Default returns advance the
/// cursor by 1 past the matched node.
///
/// This mirrors postcss-selector-parser's `walk(callback)` semantics
/// where the callback is called pre-order on every descendant; insertions
/// before the visited node DO NOT cause it to be re-visited because
/// upstream passes the unmodified node ref into the closure and the
/// caller handles indices.
pub fn walk_each<F>(parent: &mut Node, match_kinds: &[NodeKind], f: &mut F)
where
    F: FnMut(&mut Node, usize),
{
    let mut i = 0usize;
    loop {
        let len = parent.nodes.len();
        if i >= len { break; }
        // Recurse into this child first (post-order on descend, pre-order
        // on the match itself — matches upstream which calls `each` then
        // recurses into the node). We do pre-order on match here: visit
        // self, then recurse, so an `f` that inserts BEFORE the matched
        // node has a chance to operate before we descend.
        let kind_matches = match_kinds.contains(&parent.nodes[i].kind);
        let pre_len = parent.nodes.len();
        if kind_matches {
            f(parent, i);
        }
        let len_after_f = parent.nodes.len();
        // Compute how many siblings the visitor inserted BEFORE the matched
        // node. If `f` only inserts before, the matched node has shifted
        // forward by `len_after_f - pre_len` positions; we advance past
        // the inserts AND the matched node.
        let inserted_before = len_after_f.saturating_sub(pre_len);
        let new_i = i + inserted_before;
        // Recurse into the matched node's children (or any container's
        // children — pseudos hold inner Selectors).
        if new_i < parent.nodes.len() {
            walk_each(&mut parent.nodes[new_i], match_kinds, f);
        }
        i = new_i + 1;
    }
}

/// `walk(callback)` upstream — visits every descendant of `parent`
/// (including container kinds: Selector, Pseudo) in pre-order. Mirrors
/// `container.js::walk` semantics: callback fires on EVERY descendant,
/// not filtered by kind. Used by `cssnano-postcss-minify-selectors`'s
/// `pseudo()` reducer to dedup sibling Selector containers
/// (`selector.walk((child) => ...)`).
///
/// Mutation-during-walk follows `walk_each`'s rules: visitor returns
/// nothing, walker re-reads `parent.nodes.len()` each iteration, and
/// inserts BEFORE the visited node shift the cursor forward.
pub fn walk_all<F>(parent: &mut Node, f: &mut F)
where
    F: FnMut(&mut Node, usize),
{
    let mut i = 0usize;
    loop {
        let len = parent.nodes.len();
        if i >= len { break; }
        let pre_len = parent.nodes.len();
        f(parent, i);
        let len_after_f = parent.nodes.len();
        let inserted_before = len_after_f.saturating_sub(pre_len);
        let new_i = i + inserted_before;
        if new_i < parent.nodes.len() {
            walk_all(&mut parent.nodes[new_i], f);
        }
        i = new_i + 1;
    }
}

/// `walkPseudos` upstream — depth-first walk of every Pseudo descendant,
/// callback receives `(parent, index)` so the visitor can mutate the
/// parent's child Vec around the matched Pseudo.
pub fn walk_pseudos<F>(parent: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, usize),
{
    walk_each(parent, &[NodeKind::Pseudo], &mut f);
}

/// `walkClasses` upstream — depth-first walk of every ClassName.
pub fn walk_classes<F>(parent: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, usize),
{
    walk_each(parent, &[NodeKind::ClassName], &mut f);
}

/// `walkAttributes` — completes the pseudo/class/attr trio for plugins
/// that need it.
pub fn walk_attributes<F>(parent: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, usize),
{
    walk_each(parent, &[NodeKind::Attribute], &mut f);
}
