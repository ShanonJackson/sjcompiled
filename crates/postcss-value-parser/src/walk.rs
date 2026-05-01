//! Port of `postcss-value-parser/lib/walk.js`.

use crate::parse::{Node, NodeKind};

/// Mirrors upstream `walk(nodes, cb, bubble)`. The callback returns `false`
/// to skip the function-body recursion for this node.
pub fn walk<F: FnMut(&mut Node, usize) -> Option<bool>>(nodes: &mut [Node], mut cb: F, bubble: bool) {
    walk_impl(nodes, &mut cb, bubble);
}

fn walk_impl<F: FnMut(&mut Node, usize) -> Option<bool>>(nodes: &mut [Node], cb: &mut F, bubble: bool) {
    let len = nodes.len();
    for i in 0..len {
        let n = &mut nodes[i];
        let result = if !bubble { cb(n, i) } else { Some(true) };
        if result != Some(false) && n.kind == NodeKind::Function {
            let children: &mut Vec<Node> = &mut n.nodes;
            walk_impl(children, cb, bubble);
        }
        if bubble {
            let n = &mut nodes[i];
            cb(n, i);
        }
    }
}
