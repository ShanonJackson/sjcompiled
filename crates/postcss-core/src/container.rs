//! Port of `postcss/lib/container.js`.
//!
//! Postcss's Container API is the load-bearing surface plugin authors use
//! to walk and mutate the AST. This module ports every mutation primitive
//! upstream exposes, with an ergonomic Rust shape:
//!
//! ## Iteration
//!
//! - [`each`] / [`each_mut`] — direct children, callback returns `bool` to continue.
//! - [`walk`] / [`walk_mut`] — every descendant, depth-first.
//! - [`walk_decls`] / [`walk_decls_mut`] — only declarations.
//! - [`walk_rules`] / [`walk_rules_mut`] — only rules.
//! - [`walk_at_rules`] / [`walk_at_rules_mut`] — only at-rules.
//! - [`walk_comments`] / [`walk_comments_mut`] — only comments.
//!
//! Walks are **mutation-safe**: they snapshot the visit positions at each
//! recursion level, so insert/remove during the walk does not skip or
//! revisit nodes (matches postcss's `proxyOf.nodes.length` checked-each-iter
//! semantics).
//!
//! ## Mutation
//!
//! - [`append`] — push children at the end of the container.
//! - [`prepend`] — insert children at the start.
//! - [`insert_before`] / [`insert_after`] — by index.
//! - [`remove_at`] — by index.
//! - [`replace_at`] — by index.
//! - [`remove_all`] — clear the container.
//!
//! All mutations target the *direct children* of a [`Node`] that is itself
//! a container (Root / Rule / AtRule-with-block). Calling them on a leaf
//! node (Declaration / Comment / bodyless AtRule) is a no-op — matches
//! upstream's `if (this.proxyOf.nodes)` guard.
//!
//! ## Why no `parent` field on `Node`?
//!
//! Cyclic references in Rust (each child holding `Weak<Node>` back to its
//! parent) make every plugin author fight the borrow checker. We instead
//! pass the parent's mutable child-vec into the walk callback so plugins
//! can `remove_at(i)` / `replace_at(i, new_node)` directly. That's enough
//! to express every upstream mutation pattern without parent links.
//!
//! For the rare "is this the only child of its parent?" question that
//! upstream answers via `node.parent.nodes.length`, the walker passes
//! `parent_len: usize` to the visitor. See [`WalkCtx`].

use crate::node::{Node, NodeKind};

/// Context passed to mutation-aware walk callbacks. Holds the index of the
/// current node within its parent's child list, plus the parent's child
/// count so plugins can answer "am I the only child?".
#[derive(Debug, Clone, Copy)]
pub struct WalkCtx {
    pub index: usize,
    pub parent_len: usize,
}

/// Iteration outcome — mirrors upstream `return false` to abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit {
    /// Continue walking.
    Continue,
    /// Stop walking. Mirrors upstream `return false`.
    Stop,
    /// Skip descending into this node's children.
    SkipChildren,
}

impl From<bool> for Visit {
    fn from(b: bool) -> Self { if b { Visit::Continue } else { Visit::Stop } }
}

/// Mutation directive returned by mutating walk callbacks. The walker
/// applies it to the container and adjusts its cursor so subsequent
/// iterations don't skip or re-visit.
#[derive(Debug, Clone)]
pub enum Mutation {
    /// Leave the node in place.
    Keep,
    /// Remove this node from its parent.
    Remove,
    /// Replace this node with the given node.
    Replace(Node),
    /// Replace this node with the given list of nodes (1-to-N).
    ReplaceMany(Vec<Node>),
    /// Insert nodes before this one (cursor still points at this node afterwards).
    InsertBefore(Vec<Node>),
    /// Insert nodes after this one (cursor advances past inserted nodes too).
    InsertAfter(Vec<Node>),
}

// --------------------------------------------------------------------------
// Read-only iteration
// --------------------------------------------------------------------------

/// `Container.prototype.each(cb)` upstream — direct children only.
pub fn each<F: FnMut(&Node, usize) -> Visit>(parent: &Node, mut f: F) {
    let nodes = match parent.nodes() { Some(n) => n, None => return };
    for (i, child) in nodes.iter().enumerate() {
        match f(child, i) {
            Visit::Stop => return,
            _ => {}
        }
    }
}

/// `Container.prototype.walk(cb)` — every descendant, depth-first.
pub fn walk<F: FnMut(&Node) -> Visit>(node: &Node, f: &mut F) -> Visit {
    if let Some(children) = node.nodes() {
        for child in children {
            match f(child) {
                Visit::Stop => return Visit::Stop,
                Visit::SkipChildren => continue,
                Visit::Continue => {}
            }
            if walk(child, f) == Visit::Stop { return Visit::Stop; }
        }
    }
    Visit::Continue
}

/// `walkDecls(cb)` — only declarations.
pub fn walk_decls<F: FnMut(&Node) -> Visit>(node: &Node, f: &mut F) {
    walk(node, &mut |n| {
        if matches!(n.kind, NodeKind::Declaration(_)) { f(n) } else { Visit::Continue }
    });
}

/// `walkRules(cb)` — only rules.
pub fn walk_rules<F: FnMut(&Node) -> Visit>(node: &Node, f: &mut F) {
    walk(node, &mut |n| {
        if matches!(n.kind, NodeKind::Rule(_)) { f(n) } else { Visit::Continue }
    });
}

/// `walkAtRules(cb)` — only at-rules.
pub fn walk_at_rules<F: FnMut(&Node) -> Visit>(node: &Node, f: &mut F) {
    walk(node, &mut |n| {
        if matches!(n.kind, NodeKind::AtRule(_)) { f(n) } else { Visit::Continue }
    });
}

/// `walkComments(cb)` — only comments.
pub fn walk_comments<F: FnMut(&Node) -> Visit>(node: &Node, f: &mut F) {
    walk(node, &mut |n| {
        if matches!(n.kind, NodeKind::Comment(_)) { f(n) } else { Visit::Continue }
    });
}

// --------------------------------------------------------------------------
// Mutation primitives — operate directly on a container node's child vec
// --------------------------------------------------------------------------

/// `Container.prototype.append(child)` — push to end.
pub fn append(parent: &mut Node, children: Vec<Node>) {
    if let Some(nodes) = parent.nodes_mut() {
        for c in children { nodes.push(c); }
    }
}

/// `Container.prototype.prepend(child)` — insert at start.
pub fn prepend(parent: &mut Node, children: Vec<Node>) {
    if let Some(nodes) = parent.nodes_mut() {
        for (i, c) in children.into_iter().enumerate() {
            nodes.insert(i, c);
        }
    }
}

/// `Container.prototype.insertBefore(index, child)`.
/// Inserts `children` immediately before `index`. No-op if the parent isn't
/// a container.
pub fn insert_before(parent: &mut Node, index: usize, children: Vec<Node>) {
    if let Some(nodes) = parent.nodes_mut() {
        let at = index.min(nodes.len());
        for (i, c) in children.into_iter().enumerate() {
            nodes.insert(at + i, c);
        }
    }
}

/// `Container.prototype.insertAfter(index, child)`.
pub fn insert_after(parent: &mut Node, index: usize, children: Vec<Node>) {
    if let Some(nodes) = parent.nodes_mut() {
        let at = (index + 1).min(nodes.len());
        for (i, c) in children.into_iter().enumerate() {
            nodes.insert(at + i, c);
        }
    }
}

/// `removeChild(child)` upstream — by index.
///
/// Mirrors the Root-specific override in `postcss/lib/root.js::removeChild`:
/// when removing the **first** child of a `Root` and at least one sibling
/// remains, the removed node's `raws.before` is transferred onto the new
/// first child. This is *the* reason `postcss.parse(input).toString()`
/// after a first-child removal strips the original leading whitespace —
/// upstream calls it the "Hack for first rule in CSS" (see
/// `stringifier.js`'s `raw()` and the round-trip semantics depend on it).
///
/// The `ignore` flag in upstream Root.removeChild only fires from
/// internal `normalize()` shuffles during `prepend` — plugin-driven
/// removals always run through the transfer branch.
pub fn remove_at(parent: &mut Node, index: usize) -> Option<Node> {
    if matches!(parent.kind, NodeKind::Root(_)) && index == 0 {
        if let Some(nodes) = parent.nodes() {
            if nodes.len() > 1 {
                let donor = nodes[0].raws.before.clone();
                // Borrow again mutably to write — the read above already dropped.
                if let Some(nm) = parent.nodes_mut() {
                    nm[1].raws.before = donor;
                }
            }
        }
    }
    parent.nodes_mut().and_then(|nodes| {
        if index < nodes.len() { Some(nodes.remove(index)) } else { None }
    })
}

/// `child.replaceWith(newChild)` upstream — in-place swap by parent + index.
///
/// Use [`replace_with_at`] instead when faithfully porting a postcss
/// plugin's `node.replaceWith(...)` call: the plugin-facing semantics
/// of `replaceWith` are insertBefore-each-then-remove, which fires
/// Root's `normalize` and `removeChild` overrides (raws-transfer
/// between sample/new and removed/new-first-child). Plain `replace_at`
/// skips both overrides — fine for internal swaps where the new node
/// already carries the correct raws.
pub fn replace_at(parent: &mut Node, index: usize, replacement: Node) -> Option<Node> {
    parent.nodes_mut().and_then(|nodes| {
        if index < nodes.len() { Some(std::mem::replace(&mut nodes[index], replacement)) } else { None }
    })
}

/// `child.replaceWith(...newNodes)` upstream — full `Container.insertBefore`
/// dance + `remove`, including the Root-specific overrides in
/// `postcss/lib/root.js::normalize` (sample/new raws-transfer on
/// prepend / non-prepend).
///
/// Use this for plugin-driven replacements where the upstream
/// `node.replaceWith(...)` call should reproduce its byte-for-byte
/// raws-transfer behavior. `each_mut` / `walk_mut` route their
/// `Mutation::Replace` and `Mutation::ReplaceMany` cases through this.
pub fn replace_with_at(parent: &mut Node, index: usize, new_nodes: Vec<Node>) {
    if new_nodes.is_empty() {
        // No replacements — just remove the original. Equivalent to
        // upstream's `replaceWith()` with zero arguments which falls
        // through the loop without `foundSelf` and ends in `this.remove()`.
        remove_at(parent, index);
        return;
    }
    let n = new_nodes.len();
    for (i, new_node) in new_nodes.into_iter().enumerate() {
        // After each insertBefore the original shifts forward by 1, so
        // the existing-node index advances with the loop counter.
        let exist_index = index + i;
        insert_before_with_normalize(parent, exist_index, new_node);
    }
    // Original is now at index + n. Remove via remove_at so the
    // Root.removeChild override fires when the original was first.
    remove_at(parent, index + n);
}

/// `Container.insertBefore(exist, add)` + `Root.normalize` override —
/// inserts `add` immediately before `parent.nodes[exist_index]`,
/// applying `super.normalize`'s raws-transfer (when `add.raws.before`
/// is undefined and the sample's is defined, copy with non-whitespace
/// stripped) and Root's prepend / non-prepend override.
///
/// Mirrors the byte-exact upstream behavior of:
///   - `node.replaceWith(new)` — calls insertBefore.
///   - `node.before(new)` — calls insertBefore via `Node.before`.
///   - direct `parent.insertBefore(exist, new)` calls.
pub fn insert_before_with_normalize(parent: &mut Node, exist_index: usize, mut add: Node) {
    let parent_is_root = matches!(parent.kind, NodeKind::Root(_));
    let is_prepend = exist_index == 0;

    // Step 1: super.normalize semantics — if add.raws.before is
    // undefined and the sample's is defined, copy with non-whitespace
    // stripped. Mirrors postcss/lib/container.js::normalize.
    if add.raws.before.is_none() {
        if let Some(nodes) = parent.nodes() {
            if let Some(sample) = nodes.get(exist_index) {
                if let Some(sb) = &sample.raws.before {
                    add.raws.before = Some(strip_non_whitespace(sb));
                }
            }
        }
    }

    // Step 2: Root.normalize override (postcss/lib/root.js::normalize).
    if parent_is_root {
        if is_prepend {
            // sample.raws.before = nodes[1].raws.before (or delete if
            // nodes.length <= 1).
            let nodes_len = parent.nodes().map(|n| n.len()).unwrap_or(0);
            if nodes_len > 1 {
                let donor = parent.nodes().unwrap()[1].raws.before.clone();
                parent.nodes_mut().unwrap()[exist_index].raws.before = donor;
            } else if nodes_len == 1 {
                parent.nodes_mut().unwrap()[exist_index].raws.before = None;
            }
            // No change to add.raws.before in the prepend branch.
        } else {
            // exist_index > 0, so this.first !== sample. Override
            // add.raws.before with the sample's, regardless of whether
            // it was previously defined. This is the path that drops
            // an explicit raws.before='' set by a plugin in favor of
            // the original node's raws.before.
            let sample_before = parent.nodes().unwrap()[exist_index].raws.before.clone();
            add.raws.before = sample_before;
        }
    }

    // Step 3: splice add at exist_index. Cursor for the original
    // existing node moves to exist_index + 1.
    if let Some(nodes) = parent.nodes_mut() {
        nodes.insert(exist_index, add);
    }
}

/// JS `value.replace(/\S/g, '')` — strips every non-whitespace character.
/// Used by `super.normalize` when copying raws.before from sample.
fn strip_non_whitespace(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_whitespace() || *c == '\u{FEFF}')
        .collect()
}

/// `removeAll()` upstream.
pub fn remove_all(parent: &mut Node) {
    if let Some(nodes) = parent.nodes_mut() { nodes.clear(); }
}

// --------------------------------------------------------------------------
// Mutating walks — visit + apply [`Mutation`] safely
// --------------------------------------------------------------------------

/// `each_mut(cb)` — visit each direct child, accept a [`Mutation`] return.
///
/// The walker applies the mutation and adjusts its cursor. For example,
/// returning `Mutation::Remove` causes the next iteration to advance to
/// what was previously `index + 1` (now sitting at `index`).
pub fn each_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(parent: &mut Node, mut f: F) {
    let parent_has_nodes = parent.nodes().is_some();
    if !parent_has_nodes { return; }

    let mut i = 0usize;
    loop {
        // Re-borrow each iter so we can apply the mutation without aliasing.
        let parent_len = match parent.nodes() {
            Some(n) => n.len(),
            None => return,
        };
        if i >= parent_len { break; }

        // Visit.
        let mutation = {
            let nodes = parent.nodes_mut().unwrap();
            let ctx = WalkCtx { index: i, parent_len };
            f(&mut nodes[i], ctx)
        };

        // Apply.
        match mutation {
            Mutation::Keep => { i += 1; }
            Mutation::Remove => { remove_at(parent, i); }
            Mutation::Replace(new_node) => {
                // Route through replace_with_at so plugin-driven
                // replacements inherit upstream's `replaceWith` raws
                // transfer (Root.normalize + Root.removeChild). Same
                // shape as upstream `node.replaceWith(newNode)`.
                replace_with_at(parent, i, vec![new_node]);
                i += 1;
            }
            Mutation::ReplaceMany(new_nodes) => {
                let len = new_nodes.len();
                replace_with_at(parent, i, new_nodes);
                i += len;
            }
            Mutation::InsertBefore(prefix) => {
                let len = prefix.len();
                insert_before(parent, i, prefix);
                i += len + 1;
            }
            Mutation::InsertAfter(suffix) => {
                let len = suffix.len();
                insert_after(parent, i, suffix);
                i += len + 1;
            }
        }
    }
}

/// Mutation-safe descent. Visits every descendant; mutations to direct
/// children of any container are applied with cursor adjustment matching
/// [`each_mut`]'s semantics.
pub fn walk_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(parent: &mut Node, f: &mut F) {
    if parent.nodes().is_none() { return; }
    let mut i = 0usize;
    loop {
        let parent_len = match parent.nodes() {
            Some(n) => n.len(),
            None => return,
        };
        if i >= parent_len { break; }

        // Visit this child first.
        let mutation = {
            let nodes = parent.nodes_mut().unwrap();
            let ctx = WalkCtx { index: i, parent_len };
            f(&mut nodes[i], ctx)
        };

        // If the visitor kept this node, descend into it before advancing.
        let descend = matches!(mutation, Mutation::Keep);

        match mutation {
            Mutation::Keep => { /* descend below, then i += 1 */ }
            Mutation::Remove => { remove_at(parent, i); continue; }
            Mutation::Replace(new_node) => {
                // Route through replace_with_at so the Root.normalize
                // raws-transfer matches upstream `node.replaceWith(new)`.
                replace_with_at(parent, i, vec![new_node]);
                // Re-descend into the replacement.
                let nodes = parent.nodes_mut().unwrap();
                walk_mut(&mut nodes[i], f);
                i += 1;
                continue;
            }
            Mutation::ReplaceMany(new_nodes) => {
                let len = new_nodes.len();
                replace_with_at(parent, i, new_nodes);
                let end = i + len;
                let mut j = i;
                while j < end {
                    let nodes = parent.nodes_mut().unwrap();
                    if j >= nodes.len() { break; }
                    walk_mut(&mut nodes[j], f);
                    j += 1;
                }
                i = end;
                continue;
            }
            Mutation::InsertBefore(prefix) => {
                let len = prefix.len();
                insert_before(parent, i, prefix);
                i += len + 1;
                // Don't descend into inserted nodes — matches postcss.
                continue;
            }
            Mutation::InsertAfter(suffix) => {
                let len = suffix.len();
                insert_after(parent, i, suffix);
                // Descend into the original (still at i), then advance past inserts.
                let nodes = parent.nodes_mut().unwrap();
                walk_mut(&mut nodes[i], f);
                i += len + 1;
                continue;
            }
        }

        if descend {
            let nodes = parent.nodes_mut().unwrap();
            walk_mut(&mut nodes[i], f);
        }
        i += 1;
    }
}

/// `walkDeclsMut(cb)` — mutating decl-only walk.
pub fn walk_decls_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(node: &mut Node, f: &mut F) {
    walk_mut(node, &mut |n, ctx| {
        if matches!(n.kind, NodeKind::Declaration(_)) { f(n, ctx) } else { Mutation::Keep }
    });
}

/// `walkRulesMut(cb)`.
pub fn walk_rules_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(node: &mut Node, f: &mut F) {
    walk_mut(node, &mut |n, ctx| {
        if matches!(n.kind, NodeKind::Rule(_)) { f(n, ctx) } else { Mutation::Keep }
    });
}

/// `walkAtRulesMut(cb)`.
pub fn walk_at_rules_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(node: &mut Node, f: &mut F) {
    walk_mut(node, &mut |n, ctx| {
        if matches!(n.kind, NodeKind::AtRule(_)) { f(n, ctx) } else { Mutation::Keep }
    });
}

/// `walkCommentsMut(cb)`.
pub fn walk_comments_mut<F: FnMut(&mut Node, WalkCtx) -> Mutation>(node: &mut Node, f: &mut F) {
    walk_mut(node, &mut |n, ctx| {
        if matches!(n.kind, NodeKind::Comment(_)) { f(n, ctx) } else { Mutation::Keep }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, stringify};

    fn root(css: &str) -> Node {
        parse(css).unwrap().root.clone()
    }

    #[test]
    fn each_visits_top_level_only() {
        let r = root("a {} b {} c {}");
        let mut count = 0;
        each(&r, |_, _| { count += 1; Visit::Continue });
        assert_eq!(count, 3);
    }

    #[test]
    fn walk_visits_descendants() {
        let r = root("a { color: red } b { color: blue }");
        let mut decls = 0;
        walk_decls(&r, &mut |_| { decls += 1; Visit::Continue });
        assert_eq!(decls, 2);
    }

    #[test]
    fn each_mut_remove_then_keep() {
        let mut r = root("a {} b {} c {}");
        // Remove the second rule we visit (count visits, not indices).
        let mut visited = 0;
        each_mut(&mut r, |_n, _ctx| {
            visited += 1;
            if visited == 2 { Mutation::Remove } else { Mutation::Keep }
        });
        assert_eq!(r.nodes().unwrap().len(), 2);
    }

    #[test]
    fn walk_mut_remove_decl() {
        // Remove all declarations whose prop is `color`.
        let mut root_node = root("a { color: red; font-size: 12px; } b { color: blue; }");
        walk_decls_mut(&mut root_node, &mut |n, _| {
            if let NodeKind::Declaration(d) = &n.kind {
                if d.prop == "color" { return Mutation::Remove; }
            }
            Mutation::Keep
        });
        // Stringify and confirm `color:` is gone.
        let mut tmp_root = crate::root::Root::default();
        *tmp_root.root.nodes_mut().unwrap() = root_node.nodes().unwrap().clone();
        tmp_root.root.raws = root_node.raws.clone();
        let out = stringify(&tmp_root);
        assert!(!out.contains("color:"), "color decl should be gone, got: {}", out);
        assert!(out.contains("font-size:"));
    }

    #[test]
    fn append_pushes_to_end() {
        let mut r = root("a {}");
        let extra = root("b {}").nodes().unwrap()[0].clone();
        append(&mut r, vec![extra]);
        assert_eq!(r.nodes().unwrap().len(), 2);
    }

    #[test]
    fn prepend_pushes_to_front() {
        let mut r = root("b {}");
        let first = root("a {}").nodes().unwrap()[0].clone();
        prepend(&mut r, vec![first]);
        // The parser preserves selector raws verbatim — `a {}` parses as
        // selector value `"a"` with raws.between=" "; the rule's stringified
        // selector is just `"a"`.
        match &r.nodes().unwrap()[0].kind {
            NodeKind::Rule(rule) => assert_eq!(rule.selector.trim(), "a"),
            _ => panic!(),
        }
    }

    #[test]
    fn insert_before_at_index() {
        let mut r = root("a {} c {}");
        let mid = root("b {}").nodes().unwrap()[0].clone();
        insert_before(&mut r, 1, vec![mid]);
        assert_eq!(r.nodes().unwrap().len(), 3);
    }

    #[test]
    fn replace_at_swaps() {
        let mut r = root("a {}");
        let new_rule = root("z {}").nodes().unwrap()[0].clone();
        replace_at(&mut r, 0, new_rule);
        assert!(matches!(r.nodes().unwrap()[0].kind, NodeKind::Rule(_)));
    }

    #[test]
    fn walk_with_skip_children() {
        let mut visit_order = Vec::new();
        let r = root("a { color: red } b { color: blue }");
        walk(&r, &mut |n| {
            visit_order.push(n.type_name());
            // Skip descending into rules — should miss the decls.
            if matches!(n.kind, NodeKind::Rule(_)) { Visit::SkipChildren } else { Visit::Continue }
        });
        assert!(!visit_order.contains(&"decl"), "decls should have been skipped, got: {:?}", visit_order);
    }

    #[test]
    fn walk_mut_replace_many() {
        // Replace the first rule with two rules.
        let mut r = root("a {} b {}");
        let replacements = {
            let r2 = root("x {} y {}");
            r2.nodes().unwrap().clone()
        };
        each_mut(&mut r, |_n, ctx| {
            if ctx.index == 0 { Mutation::ReplaceMany(replacements.clone()) } else { Mutation::Keep }
        });
        // Original `a {}` replaced by `x {}`, `y {}`; original `b {}` still at end.
        assert_eq!(r.nodes().unwrap().len(), 3);
    }
}
