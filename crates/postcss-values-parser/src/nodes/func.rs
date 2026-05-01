//! Port of `postcss-values-parser/lib/nodes/Func.js`.

use super::node::Common;
use super::Node;

#[derive(Debug, Clone, Default)]
pub struct Func {
    pub common: Common,
    pub name: String,
    pub is_color: bool,
    pub is_var: bool,
    pub is_url: bool,
    pub nodes: Vec<Node>,
    pub raws_after: String,
    pub unclosed: bool,
}
