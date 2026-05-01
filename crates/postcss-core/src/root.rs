//! Port of `postcss/lib/root.js`.

use crate::node::{Node, NodeKind, Raws, Source};

#[derive(Debug, Clone, Default)]
pub struct RootInner {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct Root {
    pub root: Node,
}

impl Default for Root {
    fn default() -> Self { Root::new() }
}

impl Root {
    pub fn new() -> Self {
        Root {
            root: Node {
                kind: NodeKind::Root(RootInner::default()),
                raws: Raws::default(),
                source: Source::default(),
            },
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
}
