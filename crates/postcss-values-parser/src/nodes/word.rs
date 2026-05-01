//! Port of `postcss-values-parser/lib/nodes/Word.js`.

use super::node::Common;

#[derive(Debug, Clone, Default)]
pub struct Word {
    pub common: Common,
    pub is_variable: bool,
    pub is_hex: bool,
    pub is_color: bool,
    pub is_url: bool,
}

impl Word {
    pub fn is_variable_name(value: &str) -> bool { value.starts_with("--") }
}
