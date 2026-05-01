//! Port of `postcss-values-parser/lib/nodes/Container.js`.

use super::Node;

#[derive(Debug, Clone, Default)]
pub struct Container {
    pub nodes: Vec<Node>,
}
