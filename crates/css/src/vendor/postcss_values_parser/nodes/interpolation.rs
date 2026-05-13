//! Port of `postcss-values-parser/lib/nodes/Interpolation.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Interpolation {
    pub common: Common,
    pub prefix: String,
    pub params: String,
}
