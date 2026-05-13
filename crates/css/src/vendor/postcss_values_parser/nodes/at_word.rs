//! Port of `postcss-values-parser/lib/nodes/AtWord.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct AtWord {
    pub common: Common,
    pub name: String,
}
