//! 1:1 port of `packages/babel-plugin-strip-runtime/src/utils/to-uri-component.ts`.
//!
//! ```ts
//! export const toURIComponent = (rule: string): string => {
//!   const component = encodeURIComponent(rule).replace(/!/g, '%21');
//!   return component;
//! };
//! ```
//!
//! JavaScript `encodeURIComponent` escapes every byte of the UTF-8
//! representation EXCEPT the unreserved set
//! `A–Z a–z 0–9 - _ . ! ~ * ' ( )`. The upstream `.replace(/!/g, '%21')`
//! removes `!` from that set. Hex digits emitted by
//! `encodeURIComponent` are UPPERCASE; the byte contract requires we
//! match that.

#[inline]
fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')'
    )
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Escape a CSS rule string to be a valid URI query param. Also escape
/// `!` (so the result is safe to inline in webpack `require(...)`
/// strings — webpack treats `!` as a loader separator).
///
/// 1:1 with `toURIComponent` in upstream
/// `packages/babel-plugin-strip-runtime/src/utils/to-uri-component.ts`.
pub fn to_uri_component(rule: &str) -> String {
    let mut out = String::with_capacity(rule.len());
    for &byte in rule.as_bytes() {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX_UPPER[(byte >> 4) as usize] as char);
            out.push(HEX_UPPER[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_uri_component;

    #[test]
    fn css_rule_color_blue() {
        // Mirrors the regex match from strip-runtime-source-code.test.ts:
        //   require('@compiled/.../compiled-css.css?style=._syaz13q2%7Bcolor%3Ablue%7D')
        assert_eq!(
            to_uri_component("._syaz13q2{color:blue}"),
            "._syaz13q2%7Bcolor%3Ablue%7D"
        );
    }

    #[test]
    fn css_rule_font_size() {
        assert_eq!(
            to_uri_component("._1wyb1fwx{font-size:12px}"),
            "._1wyb1fwx%7Bfont-size%3A12px%7D"
        );
    }

    #[test]
    fn unreserved_passthrough() {
        let unreserved =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~*'()";
        assert_eq!(to_uri_component(unreserved), unreserved);
    }

    #[test]
    fn exclamation_is_escaped() {
        // JS encodeURIComponent leaves `!` alone; the upstream ts then
        // forces `.replace(/!/g, '%21')`. We must match the post-replace
        // shape.
        assert_eq!(to_uri_component("a!b!c"), "a%21b%21c");
    }

    #[test]
    fn space_and_braces() {
        assert_eq!(to_uri_component(" {}"), "%20%7B%7D");
    }

    #[test]
    fn utf8_multibyte() {
        // ñ is C3 B1, € is E2 82 AC.
        assert_eq!(to_uri_component("ñ"), "%C3%B1");
        assert_eq!(to_uri_component("€"), "%E2%82%AC");
    }

    #[test]
    fn hex_uppercase() {
        // encodeURIComponent emits uppercase hex; lowercase would be a
        // byte-level divergence and break the parity oracle.
        assert_eq!(to_uri_component("\u{000F}"), "%0F");
        assert_eq!(to_uri_component("\u{00FF}"), "%C3%BF");
    }

    #[test]
    fn empty_string() {
        assert_eq!(to_uri_component(""), "");
    }

    #[test]
    fn embedded_nul() {
        assert_eq!(to_uri_component("a\0b"), "a%00b");
    }

    #[test]
    fn webpack_loader_separator() {
        // The upstream comment specifically calls out `!` as a webpack
        // loader separator. Real input from styleSheetPath fixture:
        let style_sheet_path =
            "@compiled/webpack-loader/css-loader!@compiled/webpack-loader/css-loader/compiled-css.css";
        assert_eq!(
            to_uri_component(style_sheet_path),
            "%40compiled%2Fwebpack-loader%2Fcss-loader%21%40compiled%2Fwebpack-loader%2Fcss-loader%2Fcompiled-css.css"
        );
    }
}
