//! Port of `postcss-values-parser/lib/nodes/Numeric.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Numeric {
    pub common: Common,
    pub unit: String,
}

static NUMERIC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([+-]?\d*\.?\d+(?:[eE][+-]?\d+)?)(.*)$").unwrap());

impl Numeric {
    pub fn test(value: &str) -> bool { NUMERIC_RE.is_match(value) }
    pub fn split(value: &str) -> Option<(String, String)> {
        let caps = NUMERIC_RE.captures(value)?;
        Some((caps.get(1).unwrap().as_str().to_string(), caps.get(2).unwrap().as_str().to_string()))
    }
}
