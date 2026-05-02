//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/old-value.js`.

use crate::utils;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct OldValue {
    pub unprefixed: String,
    pub prefixed: String,
    pub string: String,
    pub regexp: Regex,
}

impl OldValue {
    /// JS: `constructor(unprefixed, prefixed, string, regexp)`.
    /// `string` defaults to `prefixed`; `regexp` defaults to `utils.regexp(prefixed)`.
    pub fn new(
        unprefixed: impl Into<String>,
        prefixed: impl Into<String>,
        string: Option<String>,
        regexp: Option<Regex>,
    ) -> Self {
        let prefixed = prefixed.into();
        let string = string.unwrap_or_else(|| prefixed.clone());
        let regexp = regexp.unwrap_or_else(|| utils::regexp(&prefixed, true));
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
