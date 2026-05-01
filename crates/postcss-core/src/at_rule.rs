//! Port of `postcss/lib/at-rule.js`.

use crate::node::Node;

#[derive(Debug, Clone, Default)]
pub struct AtRule {
    pub name: String,
    pub params: String,
    /// `true` when the at-rule has a `{ ... }` body. postcss models this by
    /// `node.nodes` being either an array (block) or `undefined` (statement).
    pub has_block: bool,
    pub nodes: Vec<Node>,
}
