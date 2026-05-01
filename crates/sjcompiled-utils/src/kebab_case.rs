//! Port of `packages/utils/src/kebab-case.ts`.

use once_cell::sync::Lazy;
use regex::Regex;

/// Upstream regex: `/[A-Z\u00C0-\u00D6\u00D8-\u00DE]/g`.
static KEBAB_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Z\u{00C0}-\u{00D6}\u{00D8}-\u{00DE}]").unwrap()
});

/// Mirrors upstream `kebabCase(str)` — replaces each upper-case letter
/// (ASCII A-Z plus the Latin-1 range) with `"-" + lowercase`.
pub fn kebab_case(input: &str) -> String {
    KEBAB_RE.replace_all(input, |caps: &regex::Captures| {
        let m = caps.get(0).unwrap().as_str();
        format!("-{}", m.to_lowercase())
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_to_kebab() {
        assert_eq!(kebab_case("backgroundColor"), "background-color");
    }

    #[test]
    fn pascal_to_kebab() {
        // Leading capital becomes leading dash.
        assert_eq!(kebab_case("FontSize"), "-font-size");
    }

    #[test]
    fn already_kebab_is_passthrough() {
        assert_eq!(kebab_case("font-size"), "font-size");
    }

    #[test]
    fn empty_string() {
        assert_eq!(kebab_case(""), "");
    }

    #[test]
    fn latin_1_uppercase_chars() {
        // Ç (U+00C7) is in the upstream range.
        assert_eq!(kebab_case("Çedilla"), "-çedilla");
    }
}
