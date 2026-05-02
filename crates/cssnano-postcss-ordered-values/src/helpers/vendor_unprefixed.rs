//! Port of `src/lib/vendorUnprefixed.js`.
//!
//! Upstream: `prop.replace(/^-\w+-/, '')`. JS regex without the `u` flag
//! treats `\w` as ASCII `[A-Za-z0-9_]`. We hand-scan to mirror that
//! ASCII-only semantic — using Rust's `regex` crate with default
//! Unicode-aware `\w` would over-match (e.g. `-übér-` would strip).

pub fn vendor_unprefixed(prop: &str) -> String {
    let bytes = prop.as_bytes();
    if bytes.first() != Some(&b'-') {
        return prop.to_string();
    }
    // Find one or more ASCII word chars, then a closing `-`.
    let mut i = 1usize;
    while i < bytes.len() {
        let c = bytes[i];
        let is_word = c.is_ascii_alphanumeric() || c == b'_';
        if !is_word { break; }
        i += 1;
    }
    if i == 1 || bytes.get(i) != Some(&b'-') {
        return prop.to_string();
    }
    prop[i + 1..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_webkit() {
        assert_eq!(vendor_unprefixed("-webkit-animation"), "animation");
    }

    #[test]
    fn strips_moz_with_underscore() {
        assert_eq!(vendor_unprefixed("-moz_thing-prop"), "prop");
    }

    #[test]
    fn no_prefix_passes_through() {
        assert_eq!(vendor_unprefixed("animation"), "animation");
    }

    #[test]
    fn ascii_only_unicode_word_not_stripped() {
        // `ü` is not in the ASCII `\w` class, so the JS regex bails;
        // Rust default-Unicode `\w` would match. Verify our hand-scan
        // mirrors JS.
        assert_eq!(vendor_unprefixed("-übér-prop"), "-übér-prop");
    }

    #[test]
    fn empty_inner_does_not_strip() {
        // `--prop` — no word chars between leading `-` and closing `-`.
        assert_eq!(vendor_unprefixed("--prop"), "--prop");
    }
}
