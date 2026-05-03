//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/old-value.js`.

use crate::fast_match::{IntrinsicRegexp, WordRegexp};

/// Either the standard WORD-shape matcher
/// (`(^|[\s,(])(NAME($|[\s(,]))`) — the dominant case — or the
/// Intrinsic-shape matcher (`(^|[\s,(])(NAME($|[\s),]))`) used by the
/// 6 Intrinsic-hack names. Both variants are fast-match-backed and
/// serializable; this is what makes V2 precomputed snapshots viable.
///
/// Replaced an earlier `Custom(regex::Regex)` variant — `regex::Regex`
/// is not serde-able, which would have blocked the V2 populated-table
/// snapshot. The Intrinsic variant is byte-equal to the prior regex
/// for every input on the Intrinsic name corpus (see
/// `tests/intrinsic_regexp_parity.rs`).
#[derive(Debug, Clone)]
pub enum OldValueRegexp {
    Word(WordRegexp),
    Intrinsic(IntrinsicRegexp),
}

impl OldValueRegexp {
    pub fn is_match(&self, haystack: &str) -> bool {
        match self {
            Self::Word(r) => r.is_match(haystack),
            Self::Intrinsic(r) => r.is_match(haystack),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OldValue {
    pub unprefixed: String,
    pub prefixed: String,
    pub string: String,
    pub regexp: OldValueRegexp,
}

impl OldValue {
    /// JS: `constructor(unprefixed, prefixed, string, regexp)`.
    /// `string` defaults to `prefixed`; `regexp` defaults to `utils.regexp(prefixed)`.
    pub fn new(
        unprefixed: impl Into<String>,
        prefixed: impl Into<String>,
        string: Option<String>,
        regexp: Option<OldValueRegexp>,
    ) -> Self {
        let prefixed = prefixed.into();
        let string = string.unwrap_or_else(|| prefixed.clone());
        let regexp = regexp.unwrap_or_else(|| {
            OldValueRegexp::Word(WordRegexp::new(&prefixed))
        });
        Self {
            unprefixed: unprefixed.into(),
            prefixed,
            string,
            regexp,
        }
    }

    /// Check that value contains old value.
    pub fn check(&self, value: &str) -> bool {
        if value.contains(&self.string) {
            self.regexp.is_match(value)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_matches_substring_and_regexp() {
        let v = OldValue::new("flex", "-webkit-flex", None, None);
        assert!(v.check("display: -webkit-flex"));
        assert!(!v.check("display: flex"));
    }
}
