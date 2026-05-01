//! Port of `postcss-values-parser/lib/nodes/Quoted.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Quoted {
    pub common: Common,
    pub quote: char,
    pub unclosed: bool,
}
