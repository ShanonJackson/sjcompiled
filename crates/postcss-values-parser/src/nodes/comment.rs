//! Port of `postcss-values-parser/lib/nodes/Comment.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Comment {
    pub common: Common,
    pub text: String,
    pub inline: bool,
    pub left: String,
    pub right: String,
}

impl Comment {
    pub fn test_inline(value: &str) -> bool { value.starts_with("//") }
}
