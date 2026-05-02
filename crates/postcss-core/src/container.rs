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
///
/// Mirrors upstream `Container.append → this.normalize(child, this.last)
/// → push`. For a `Root` parent, this triggers the Root.normalize override
/// (`postcss/lib/root.js::normalize`):
///
/// > if (sample) { ... else if (this.first !== sample) {
/// >   for (let node of nodes) { node.raws.before = sample.raws.before }
/// > } }
///
/// In plain English: when root already has at least 2 children, the
/// to-be-appended child's `raws.before` is overwritten with the *current
/// last* child's `raws.before`. (`sample` is `this.last`; the inner branch
/// only fires when `this.first !== sample`, i.e. root has ≥2 children.)
///
/// For non-Root parents, base `Container.normalize` does no raws transfer,
/// so we just push.
pub fn append(parent: &mut Node, children: Vec<Node>) {
    let is_root = matches!(parent.kind, NodeKind::Root(_));
    if let Some(nodes) = parent.nodes_mut() {
        for mut c in children {
            // For Root parents: upstream does `this.normalize(child, this.last)`
            // PER child, re-reading `this.last` each iteration. The
            // raws-transfer fires only when `this.first !== this.last` —
            // i.e., when root already has ≥2 children. After our first
            // push, root has more children, so subsequent pushes may also
            // trigger the transfer based on the new last.
            if is_root && nodes.len() >= 2 {
                if let Some(last) = nodes.last() {
                    c.raws.before = last.raws.before.clone();
                }
            }
            nodes.push(c);
        }
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

    // Step 1: `Container.normalize(nodes, sample)` — line 173 upstream.
    // The strip-non-whitespace pass at line 211 only fires when a sample
    // is supplied. `Container.insertBefore` (line 156) passes a sample,
    // but `Root.normalize` (root.js line 15) calls `super.normalize(child)`
    // WITHOUT one — the JS `if (sample && ...)` short-circuits, so the
    // strip step is a no-op on Root parents. We mirror by skipping
    // Step 1 entirely when `parent_is_root`. Otherwise use the next-
    // sibling at `exist_index` as the sample, matching the call from
    // `Container.insertBefore(exist, add)` (line 156).
    if !parent_is_root && add.raws.before.is_none() {
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

// ============================================================================
// Parent-aware visitor surface
// ============================================================================
//
// Autoprefixer (and plugins like it) need access to a node's *ancestors*
// during a visit — `node.parent.parent` etc. JS expresses that with back-
// pointers; in Rust those back-pointers turn ownership into a fight.
//
// **Architectural drift from upstream — by design:**
// We model the parent chain as a [`NodePath`] (a `Vec<usize>` of child
// indices, root → ... → node). This is an *index path*, not a back-pointer.
// Plugin authors get the equivalent functionality via:
//
//   - [`node_at_path`] / [`node_at_path_mut`] — resolve the path back to
//     a node reference.
//   - [`parent_path`] — drop the last index to get the parent's path.
//   - [`parent_index_of`] — `node.parent.index(node)` — the last element
//     of the path.
//   - [`parent_some`] / [`parent_every`] — `node.parent.some(f)` /
//     `node.parent.every(f)`.
//   - [`walk_up_with`] — `parentPrefix` walk from a node up through every
//     ancestor.
//   - [`insert_before_at_path`] — `node.parent.insertBefore(node, cloned)`
//     equivalent. Defers to [`insert_before_with_normalize`] so Root's
//     raws-transfer fires correctly.
//
// The visitor family `walk_*_mut_with_parent` runs alongside the existing
// `walk_*_mut` family. Plugins that don't need parent access stay on the
// simpler API. Authors are expected to read this section before reaching
// for the parent-aware variants — see PLUGIN_IMPLEMENTATION_GUIDE.md.

/// A path of child indices from a [`Root`](crate::Root)'s top container
/// to a target node. Empty path == the Root itself.
///
/// We expose this as a plain `Vec<usize>` alias for ergonomic slicing;
/// callers who hold one across visitor invocations should clone it (the
/// walker reuses an internal scratch buffer).
pub type NodePath = Vec<usize>;

/// Resolve `path` against `root` and return the node at that path, or
/// the root node when the path is empty. `None` if any index is out of
/// bounds.
pub fn node_at_path<'a>(root: &'a Node, path: &[usize]) -> Option<&'a Node> {
    let mut cur = root;
    for &i in path {
        cur = cur.nodes()?.get(i)?;
    }
    Some(cur)
}

/// Mutable variant of [`node_at_path`].
pub fn node_at_path_mut<'a>(root: &'a mut Node, path: &[usize]) -> Option<&'a mut Node> {
    let mut cur = root;
    for &i in path {
        cur = cur.nodes_mut()?.get_mut(i)?;
    }
    Some(cur)
}

/// Drop the last index — gives the path to the node's parent. Panics on
/// empty paths (you don't have a parent if you're root).
#[inline]
pub fn parent_path(path: &[usize]) -> &[usize] {
    debug_assert!(!path.is_empty(), "parent_path called on root");
    &path[..path.len() - 1]
}

/// `node.parent.index(node)` — last element of the path.
#[inline]
pub fn parent_index_of(path: &[usize]) -> usize {
    debug_assert!(!path.is_empty(), "parent_index_of called on root");
    path[path.len() - 1]
}

/// `node.parent.some(f)` upstream — true if any sibling (including the
/// node itself) satisfies the predicate.
pub fn parent_some<F: FnMut(&Node) -> bool>(root: &Node, path: &[usize], mut f: F) -> bool {
    let parent = node_at_path(root, parent_path(path));
    let Some(parent) = parent else { return false; };
    let Some(nodes) = parent.nodes() else { return false; };
    nodes.iter().any(|c| f(c))
}

/// `node.parent.every(f)` upstream.
pub fn parent_every<F: FnMut(&Node) -> bool>(root: &Node, path: &[usize], mut f: F) -> bool {
    let parent = node_at_path(root, parent_path(path));
    let Some(parent) = parent else { return false; };
    let Some(nodes) = parent.nodes() else { return false; };
    nodes.iter().all(|c| f(c))
}

/// `parentPrefix(node)` style ancestor walk — invoke `f(ancestor)` for
/// each ancestor of the node at `path`, from immediate parent up to
/// root. Stops when `f` returns `false`.
///
/// Mirrors upstream `prefixer.js::parentPrefix`'s recursive
/// `this.parentPrefix(node.parent)` chain.
pub fn walk_up_with<F: FnMut(&Node) -> bool>(root: &Node, path: &[usize], mut f: F) {
    if path.is_empty() { return; }
    let mut p = path.len();
    while p > 0 {
        p -= 1;
        let ancestor_path = &path[..p];
        if let Some(anc) = node_at_path(root, ancestor_path) {
            if !f(anc) { return; }
        } else {
            return;
        }
    }
}

/// `node.parent.insertBefore(node, newNode)` upstream — splices `add`
/// in front of the node at `path`. Adjusts subsequent operations'
/// reasoning about the path: the old node now lives at
/// `parent_path[..-1] + [parent_idx + 1]`. **The walk cursor is buffered
/// by the parent-aware visitor family** so calling this from inside a
/// `walk_*_mut_with_parent` callback is safe — the visitor applies the
/// insert after the callback returns and shifts its cursor accordingly.
pub fn insert_before_at_path(root: &mut Node, path: &[usize], add: Node) {
    let idx = parent_index_of(path);
    let pp = parent_path(path).to_vec();
    if let Some(parent_node) = node_at_path_mut(root, &pp) {
        insert_before_with_normalize(parent_node, idx, add);
    }
}

/// `node.parent.nodes` upstream — returns the parent's full child list.
/// Returns `None` if `path` is empty (the node is root, no parent) or if
/// the parent isn't a container.
pub fn parent_nodes<'a>(root: &'a Node, path: &[usize]) -> Option<&'a Vec<Node>> {
    if path.is_empty() { return None; }
    let parent = node_at_path(root, parent_path(path))?;
    parent.nodes()
}

/// `node.parent.nodes[i]` upstream — sibling at an absolute index in
/// the parent's child list. `None` if out of bounds. Useful for
/// `selector.js::already`-style backward scans where the index math is
/// the obvious source of off-by-one bugs.
pub fn sibling_at<'a>(root: &'a Node, path: &[usize], abs_index: usize) -> Option<&'a Node> {
    parent_nodes(root, path)?.get(abs_index)
}

/// Sibling at a relative offset from the node at `path`. `offset = -1`
/// is the previous sibling, `+1` is the next. Returns `None` if the
/// resulting index is out of bounds (or underflows when `offset` is
/// negative on a node already at index 0).
pub fn sibling_relative<'a>(root: &'a Node, path: &[usize], offset: isize) -> Option<&'a Node> {
    if path.is_empty() { return None; }
    let cur = parent_index_of(path) as isize;
    let target = cur.checked_add(offset)?;
    if target < 0 { return None; }
    sibling_at(root, path, target as usize)
}

// ----------------------------------------------------------------------------
// Parent-aware visitor family
// ----------------------------------------------------------------------------
//
// `walk_*_mut_with_parent` callbacks receive `(root, path, ctx)`:
//   - `root`: `&mut Node` rooted at the original Root, so callbacks can
//     reach any ancestor or sibling via the helpers above.
//   - `path`: `&[usize]` index path to the current node (last element is
//     the cursor index in the immediate parent).
//   - `ctx`: `WalkCtx { index, parent_len }` for backwards-compat with
//     existing visitors.
//
// Mutations are buffered in a `Vec<DeferredMutation>` and applied after
// each callback returns. This is required because rust's borrow checker
// can't statically prove that an `&mut Node` returned by the visitor
// doesn't alias `&mut Root` passed in alongside.

/// A buffered mutation — emitted by parent-aware callbacks.
#[derive(Debug, Clone)]
pub enum DeferredMutation {
    Keep,
    Remove,
    Replace(Node),
    ReplaceMany(Vec<Node>),
    /// `node.parent.insertBefore(node, ...)` — splice these nodes in
    /// front of the path the callback was invoked with.
    InsertBefore(Vec<Node>),
    /// `node.parent.insertAfter(node, ...)`.
    InsertAfter(Vec<Node>),
}

impl From<Mutation> for DeferredMutation {
    fn from(m: Mutation) -> Self {
        match m {
            Mutation::Keep => DeferredMutation::Keep,
            Mutation::Remove => DeferredMutation::Remove,
            Mutation::Replace(n) => DeferredMutation::Replace(n),
            Mutation::ReplaceMany(v) => DeferredMutation::ReplaceMany(v),
            Mutation::InsertBefore(v) => DeferredMutation::InsertBefore(v),
            Mutation::InsertAfter(v) => DeferredMutation::InsertAfter(v),
        }
    }
}

/// Walk every descendant of `root_node` depth-first. The callback
/// receives `(root, path, ctx)`:
///
///   - `root: &mut Node` — the entire tree, so the callback can call
///     [`parent_some`] / [`parent_every`] / [`walk_up_with`] /
///     [`node_at_path_mut`] to inspect or mutate any ancestor or
///     sibling.
///   - `path: &[usize]` — index path from `root` to the current visited
///     node.
///   - `ctx: WalkCtx` — `{ index, parent_len }` for the current visit.
///
/// **Why pass `root` instead of the current node directly?** Rust can't
/// statically prove that `&mut Node` (current) and `&Node` (root) don't
/// alias, even though current is a descendant. By giving the callback
/// only `root`, we let it choose when to take a mutable borrow (via
/// [`node_at_path_mut`]) and when to re-borrow immutably (via
/// [`parent_some`] etc.). This is the architectural drift from upstream
/// — see PLUGIN_IMPLEMENTATION_GUIDE.md § "Parent-aware visitors".
///
/// **Cursor adjustment:** the walker maintains the invariant that
/// `path[..-1]` always points at the parent of the current visit
/// position. Insert/remove operations are applied AFTER the callback
/// and the cursor at `path[-1]` is shifted accordingly:
///
///   - `Remove`: cursor stays — the next sibling slid down.
///   - `InsertBefore(N)`: cursor advances past the N inserts.
///   - `InsertAfter(N)`: cursor advances past the original AND the N inserts.
///   - `Replace`: cursor advances past the replacement (and we descend into it).
///   - `ReplaceMany(N)`: cursor advances past the N replacements (no descent).
///
/// Any sibling-list mutation outside `path[..-1]`'s subtree is
/// unsupported in this version — the callback should restrict its
/// `insert_before_at_path` calls to the immediate parent.
pub fn walk_mut_with_parent<F>(root: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    let mut path: Vec<usize> = Vec::new();
    walk_mut_with_parent_inner(root, &mut path, &mut f);
}

fn walk_mut_with_parent_inner<F>(root: &mut Node, path: &mut Vec<usize>, f: &mut F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    // Snapshot the parent's child count for cursor bookkeeping.
    let cur_len = node_at_path(root, path).and_then(|n| n.nodes()).map(|c| c.len()).unwrap_or(0);
    if cur_len == 0 { return; }

    let mut i = 0usize;
    loop {
        // Re-read len each iter — it can change as we mutate.
        let parent_len = node_at_path(root, path).and_then(|n| n.nodes()).map(|c| c.len()).unwrap_or(0);
        if i >= parent_len { break; }

        path.push(i);
        let ctx = WalkCtx { index: i, parent_len };

        // Visit. The callback gets `&mut Node` to root + the path —
        // it picks when to take mutable vs immutable borrows.
        let mutation = f(root, path.as_slice(), ctx);

        // Decide whether to descend.
        let descend = matches!(mutation, DeferredMutation::Keep);

        // Apply mutation.
        let path_to_parent = path[..path.len() - 1].to_vec();
        match mutation {
            DeferredMutation::Keep => {
                if descend {
                    walk_mut_with_parent_inner(root, path, f);
                }
                path.pop();
                i += 1;
            }
            DeferredMutation::Remove => {
                path.pop();
                if let Some(parent) = node_at_path_mut(root, &path_to_parent) {
                    remove_at(parent, i);
                }
                // cursor stays
            }
            DeferredMutation::Replace(new_node) => {
                path.pop();
                if let Some(parent) = node_at_path_mut(root, &path_to_parent) {
                    replace_at(parent, i, new_node);
                }
                // descend into the replacement.
                path.push(i);
                walk_mut_with_parent_inner(root, path, f);
                path.pop();
                i += 1;
            }
            DeferredMutation::ReplaceMany(new_nodes) => {
                let len = new_nodes.len();
                path.pop();
                if let Some(parent) = node_at_path_mut(root, &path_to_parent) {
                    remove_at(parent, i);
                    insert_before(parent, i, new_nodes);
                }
                // skip past the replacements (no descent — matches Mutation::ReplaceMany).
                i += len;
            }
            DeferredMutation::InsertBefore(prefix) => {
                let len = prefix.len();
                path.pop();
                if let Some(parent) = node_at_path_mut(root, &path_to_parent) {
                    insert_before(parent, i, prefix);
                }
                i += len + 1;
            }
            DeferredMutation::InsertAfter(suffix) => {
                let len = suffix.len();
                path.pop();
                if let Some(parent) = node_at_path_mut(root, &path_to_parent) {
                    insert_after(parent, i, suffix);
                }
                i += len + 1;
            }
        }
    }
}

/// Filtered variants — same shape as `walk_mut_with_parent` but only
/// fires the callback when the *current node* (at `path`) matches the
/// named kind. The kind check resolves through `node_at_path(root, path)`
/// since the closure receives root, not the current node.
pub fn walk_decls_mut_with_parent<F>(root: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    walk_mut_with_parent(root, |r, path, ctx| {
        let is_match = matches!(node_at_path(r, path).map(|n| &n.kind), Some(NodeKind::Declaration(_)));
        if is_match { f(r, path, ctx) } else { DeferredMutation::Keep }
    });
}

pub fn walk_rules_mut_with_parent<F>(root: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    walk_mut_with_parent(root, |r, path, ctx| {
        let is_match = matches!(node_at_path(r, path).map(|n| &n.kind), Some(NodeKind::Rule(_)));
        if is_match { f(r, path, ctx) } else { DeferredMutation::Keep }
    });
}

pub fn walk_at_rules_mut_with_parent<F>(root: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    walk_mut_with_parent(root, |r, path, ctx| {
        let is_match = matches!(node_at_path(r, path).map(|n| &n.kind), Some(NodeKind::AtRule(_)));
        if is_match { f(r, path, ctx) } else { DeferredMutation::Keep }
    });
}

pub fn walk_comments_mut_with_parent<F>(root: &mut Node, mut f: F)
where
    F: FnMut(&mut Node, &[usize], WalkCtx) -> DeferredMutation,
{
    walk_mut_with_parent(root, |r, path, ctx| {
        let is_match = matches!(node_at_path(r, path).map(|n| &n.kind), Some(NodeKind::Comment(_)));
        if is_match { f(r, path, ctx) } else { DeferredMutation::Keep }
    });
}

#[cfg(test)]
mod parent_aware_tests {
    use super::*;
    use crate::{parse, stringify};

    #[test]
    fn parent_index_and_some_every() {
        let r = parse("a { color: red; font-size: 12px; }").unwrap();
        // Walk to the second decl.
        let path = vec![0, 1];
        let n = node_at_path(&r.root, &path).expect("decl exists");
        assert!(matches!(n.kind, NodeKind::Declaration(_)));
        assert_eq!(parent_index_of(&path), 1);
        // Parent has at least one decl with prop="color"?
        assert!(parent_some(&r.root, &path, |n| matches!(&n.kind, NodeKind::Declaration(d) if d.prop == "color")));
        // Every child of parent is a decl.
        assert!(parent_every(&r.root, &path, |n| matches!(n.kind, NodeKind::Declaration(_))));
    }

    #[test]
    fn walk_up_visits_each_ancestor() {
        let r = parse("@media (min-width: 100px) { a { color: red; } }").unwrap();
        // Path to the decl: root → atrule(0) → rule(0) → decl(0).
        let path = vec![0, 0, 0];
        let mut visited: Vec<&'static str> = Vec::new();
        walk_up_with(&r.root, &path, |anc| {
            visited.push(match &anc.kind {
                NodeKind::Root(_) => "root",
                NodeKind::AtRule(_) => "atrule",
                NodeKind::Rule(_) => "rule",
                _ => "other",
            });
            true
        });
        assert_eq!(visited, vec!["rule", "atrule", "root"]);
    }

    #[test]
    fn insert_before_at_path_splices_sibling() {
        let mut r = parse("a { color: red; }").unwrap();
        // Path to the decl.
        let path = vec![0, 0];
        // Build a new decl to splice in.
        let new_decl = Node::new(NodeKind::Declaration(crate::declaration::Declaration {
            prop: "background".to_string(),
            value: "blue".to_string(),
            important: false,
            variable: false,
        }));
        insert_before_at_path(&mut r.root, &path, new_decl);
        let out = stringify(&r);
        // The new decl arrived first.
        let bg_idx = out.find("background").expect("bg present");
        let color_idx = out.find("color").expect("color present");
        assert!(bg_idx < color_idx, "bg should come first: {out:?}");
    }

    #[test]
    fn walk_decls_mut_with_parent_can_check_sibling() {
        // Plugin pattern: only count `color: red` decls when their parent
        // also has a `background` decl. The closure takes only `root`,
        // re-borrows it immutably to read the current node and check
        // siblings — borrow-checker friendly.
        let mut r = parse("a { color: red; } b { color: red; background: blue; }").unwrap();
        let mut hits = 0usize;
        walk_decls_mut_with_parent(&mut r.root, |root, path, _ctx| {
            // Read the current decl.
            let is_color_red = match node_at_path(root, path).map(|n| &n.kind) {
                Some(NodeKind::Declaration(d)) => d.prop == "color" && d.value == "red",
                _ => false,
            };
            if is_color_red {
                let has_bg = parent_some(root, path, |s| {
                    matches!(&s.kind, NodeKind::Declaration(sd) if sd.prop == "background")
                });
                if has_bg { hits += 1; }
            }
            DeferredMutation::Keep
        });
        // Only the `b { ... }` rule has a `background` sibling.
        assert_eq!(hits, 1);
    }

    #[test]
    fn deferred_insert_before_advances_cursor() {
        // Insert a new sibling before each decl; verify no double-visit.
        let mut r = parse("a { color: red; font-size: 12px; }").unwrap();
        let mut visited = 0usize;
        walk_decls_mut_with_parent(&mut r.root, |_root, _path, _ctx| {
            visited += 1;
            // Insert one new decl before the current one.
            let new_decl = Node::new(NodeKind::Declaration(crate::declaration::Declaration {
                prop: "border".to_string(),
                value: "0".to_string(),
                important: false,
                variable: false,
            }));
            DeferredMutation::InsertBefore(vec![new_decl])
        });
        // Visited count == original decl count (2). If cursor adjustment
        // were broken we'd loop forever or visit the inserts.
        assert_eq!(visited, 2, "visited count: {visited}");
        let out = stringify(&r);
        assert!(out.matches("border: 0").count() == 2, "got: {out:?}");
    }

    #[test]
    fn parent_nodes_returns_full_child_list() {
        let r = parse("a { color: red; font-size: 12px; background: blue; }").unwrap();
        // Path to second decl.
        let path = vec![0, 1];
        let siblings = parent_nodes(&r.root, &path).expect("parent has children");
        assert_eq!(siblings.len(), 3);
        // Cardinal: order matches doc order.
        if let NodeKind::Declaration(d) = &siblings[0].kind { assert_eq!(d.prop, "color"); }
        if let NodeKind::Declaration(d) = &siblings[2].kind { assert_eq!(d.prop, "background"); }
    }

    #[test]
    fn parent_nodes_returns_none_for_root() {
        let r = parse("a {}").unwrap();
        // Empty path → node is root → no parent.
        assert!(parent_nodes(&r.root, &[]).is_none());
    }

    #[test]
    fn sibling_at_absolute_index() {
        let r = parse("a { color: red; font-size: 12px; }").unwrap();
        let path = vec![0, 0];
        let sib = sibling_at(&r.root, &path, 1).expect("sibling exists");
        if let NodeKind::Declaration(d) = &sib.kind { assert_eq!(d.prop, "font-size"); }
        // OOB returns None.
        assert!(sibling_at(&r.root, &path, 99).is_none());
    }

    #[test]
    fn sibling_relative_walks_backward() {
        // `selector.js::already` use case — walk backward looking for a
        // non-rule sibling.
        let r = parse("a {} b {} c {}").unwrap();
        let path = vec![2]; // the `c` rule
        let prev = sibling_relative(&r.root, &path, -1).expect("prev exists");
        if let NodeKind::Rule(rule) = &prev.kind { assert_eq!(rule.selector.trim(), "b"); }
        let prev_prev = sibling_relative(&r.root, &path, -2).expect("prev-prev exists");
        if let NodeKind::Rule(rule) = &prev_prev.kind { assert_eq!(rule.selector.trim(), "a"); }
        // Underflow returns None — the most common off-by-one trap.
        assert!(sibling_relative(&r.root, &path, -3).is_none());
    }

    #[test]
    fn sibling_relative_underflow_does_not_panic() {
        let r = parse("a {}").unwrap();
        // node at index 0, offset -1 underflows.
        let path = vec![0];
        assert!(sibling_relative(&r.root, &path, -1).is_none());
        // Even with isize::MIN, no panic.
        assert!(sibling_relative(&r.root, &path, isize::MIN).is_none());
    }

    #[test]
    fn parent_every_returns_true_when_all_match() {
        let r = parse("a { color: red; font-size: 12px; }").unwrap();
        let path = vec![0, 0];
        // All siblings are decls.
        assert!(parent_every(&r.root, &path, |n| matches!(n.kind, NodeKind::Declaration(_))));
        // Not all siblings are atrules.
        assert!(!parent_every(&r.root, &path, |n| matches!(n.kind, NodeKind::AtRule(_))));
    }

    /// Direct exercise of the autoprefixer pattern from `value.js:37` —
    /// `rule.every(i => i.prop !== prefixed)`. Returns true iff *no*
    /// sibling has the named prop.
    #[test]
    fn parent_every_inverse_predicate_pattern() {
        let r = parse("a { color: red; font-size: 12px; }").unwrap();
        let path = vec![0, 0];
        // No sibling has prop=="display" → every() inverse is true.
        let no_display = parent_every(&r.root, &path, |n| match &n.kind {
            NodeKind::Declaration(d) => d.prop != "display",
            _ => true,
        });
        assert!(no_display);
        // A sibling has prop=="color" → every() inverse is false.
        let no_color = parent_every(&r.root, &path, |n| match &n.kind {
            NodeKind::Declaration(d) => d.prop != "color",
            _ => true,
        });
        assert!(!no_color);
    }

    /// Sanity check the autoprefixer agent flagged: `clone_without` must
    /// recurse into nested rules so `_autoprefixerPrefix` set on a decl
    /// inside an at-rule body is also stripped on the clone.
    #[test]
    fn clone_without_recurses_into_descendants() {
        use crate::node::AttrValue;
        // Parse a 3-level tree: root → atrule → rule → decl.
        let mut r = parse("@media print { a { color: red; } }").unwrap();
        // Stash a key on every level.
        let path_atrule: Vec<usize> = vec![0];
        let path_rule: Vec<usize> = vec![0, 0];
        let path_decl: Vec<usize> = vec![0, 0, 0];
        for p in [&path_atrule[..], &path_rule[..], &path_decl[..]] {
            let n = node_at_path_mut(&mut r.root, p).expect("path exists");
            n.attrs.set("_autoprefixerPrefix", AttrValue::Bool(true));
            n.attrs.set("kept", AttrValue::Bool(true));
        }

        // Clone the at-rule node, strip the autoprefixer key.
        let original_atrule = node_at_path(&r.root, &path_atrule).unwrap();
        let cloned = original_atrule.clone_without(&["_autoprefixerPrefix"]);

        // Top-level clone is stripped.
        assert!(cloned.attrs.get("_autoprefixerPrefix").is_none());
        assert!(cloned.attrs.get("kept").is_some(), "non-listed keys are kept");

        // Descendant rule and decl are also stripped (the autoprefixer
        // agent's actual bug case).
        let cloned_rule = cloned.nodes().expect("atrule has body").get(0).expect("rule exists");
        assert!(cloned_rule.attrs.get("_autoprefixerPrefix").is_none(), "nested rule still has key — recursion broken");
        assert!(cloned_rule.attrs.get("kept").is_some());
        let cloned_decl = cloned_rule.nodes().expect("rule has body").get(0).expect("decl exists");
        assert!(cloned_decl.attrs.get("_autoprefixerPrefix").is_none(), "nested decl still has key — recursion broken");
        assert!(cloned_decl.attrs.get("kept").is_some());

        // Original tree is untouched (deep clone, not a move).
        let orig_rule = node_at_path(&r.root, &path_rule).unwrap();
        assert!(orig_rule.attrs.get("_autoprefixerPrefix").is_some());
    }

    #[test]
    fn walk_up_can_be_called_from_visitor() {
        // Plugin pattern that autoprefixer needs:
        // `parentPrefix(node) → if any ancestor's name has a vendor prefix, return it`.
        // We test the read pattern here: visitor walks up and inspects each
        // ancestor's kind while holding only `&mut root`.
        let mut r = parse("@media print { a { color: red; } }").unwrap();
        let mut ancestor_kinds_seen: Vec<&'static str> = Vec::new();
        walk_decls_mut_with_parent(&mut r.root, |root, path, _ctx| {
            walk_up_with(root, path, |anc| {
                ancestor_kinds_seen.push(match &anc.kind {
                    NodeKind::Root(_) => "root",
                    NodeKind::AtRule(_) => "atrule",
                    NodeKind::Rule(_) => "rule",
                    _ => "other",
                });
                true
            });
            DeferredMutation::Keep
        });
        assert_eq!(ancestor_kinds_seen, vec!["rule", "atrule", "root"]);
    }
}
