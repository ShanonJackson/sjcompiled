//! Port of `postcss-values-parser/lib/nodes/UnicodeRange.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct UnicodeRange {
    pub common: Common,
}

static UNICODE_RANGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[uU]\+[a-fA-F0-9?]+(?:-[a-fA-F0-9]+)?$").unwrap());

impl UnicodeRange {
    pub fn test(value: &str) -> bool { UNICODE_RANGE_RE.is_match(value) }
}
