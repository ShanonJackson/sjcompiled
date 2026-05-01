//! Port of `postcss-values-parser/lib/nodes/Operator.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Operator {
    pub common: Common,
}

pub static OPERATOR_CHARS: &[&str] = &["*", "-", "%", "+", "/"];
pub static OPERATOR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\*\-%+/]$").unwrap());
