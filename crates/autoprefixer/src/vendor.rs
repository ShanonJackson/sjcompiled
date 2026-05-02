//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/vendor.js`.

use once_cell::sync::Lazy;
use regex::Regex;

static PREFIX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(-\w+-)").unwrap());

/// Extract `-webkit-` from `-webkit-foo`, returns `""` when no prefix.
pub fn prefix(prop: &str) -> String {
    PREFIX_RE
        .find(prop)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

/// Strip the leading vendor prefix.
pub fn unprefixed(prop: &str) -> String {
    PREFIX_RE.replace(prop, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_extracts_or_empty() {
        assert_eq!(prefix("-webkit-foo"), "-webkit-");
        assert_eq!(prefix("-moz-foo"), "-moz-");
        assert_eq!(prefix("foo"), "");
    }

    #[test]
    fn unprefixed_strips() {
        assert_eq!(unprefixed("-webkit-foo"), "foo");
        assert_eq!(unprefixed("foo"), "foo");
    }
}
