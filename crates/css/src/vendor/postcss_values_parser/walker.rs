//! Port of `postcss-values-parser/lib/walker.js`.

use super::nodes::{Node, NodeKind};

/// Recursively walk every descendant. Returns `false` to abort early.
pub fn walk<F: FnMut(&Node) -> bool>(nodes: &[Node], f: &mut F) -> bool {
    for n in nodes {
        if !f(n) { return false; }
        if let NodeKind::Func(func) = &n.kind {
            if !walk(&func.nodes, f) { return false; }
        }
    }
    true
}
