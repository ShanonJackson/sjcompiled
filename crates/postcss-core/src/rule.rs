//! Port of `postcss/lib/rule.js`.

use crate::node::Node;

#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub selector: String,
    pub nodes: Vec<Node>,
}
