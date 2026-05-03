//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/old-value.js`.

use crate::fast_match::WordRegexp;
use regex::Regex;

/// Either the standard WORD-shape matcher (`(^|[\s,(])(NAME($|[\s(,]))`)
/// or a caller-supplied custom regex (currently used only by Intrinsic
/// hacks, which match a different trailing-boundary class `[\s),]`).
///
/// Bypasses the fast path only on the explicit `Custom` variant —
/// the default construction path uses [`WordRegexp`], so the dominant
/// `OldValue` traffic still goes through the fast matcher.
#[derive(Debug, Clone)]
pub enum OldValueRegexp {
    Word(WordRegexp),
    Custom(Regex),
}

impl OldValueRegexp {
    pub fn is_match(&self, haystack: &str) -> bool {
        match self {
            Self::Word(r) => r.is_match(haystack),
            Self::Custom(r) => r.is_match(haystack),
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
