//! 1:1 port of `jsesc@2.5.2` (the version pinned by `@babel/generator@7.x`).
//!
//! Source: `node_modules/jsesc/jsesc.js` (string-handling branch only).
//! Babel's `@babel/generator/lib/index.js` defaults `jsescOption` to
//! `{ quotes: 'double', wrap: true, minimal: false }`. The `StringLiteral`
//! printer (`generators/types.js::StringLiteral`) calls
//! `_jsesc(node.value, this.format.jsescOption)` whenever
//! `getPossibleRaw(node)` returns `undefined` — i.e., for synthetic
//! StringLiterals that have no source-anchored `extra.raw`.
//!
//! Our consumer is `utils/hoist_sheet.rs::emit_hoisted_sheets`: when it
//! synthesises the `const _<n> = "<sheet>";` VarDecl, the Str node has
//! no `raw`. Babel's printer would jsesc-escape any non-ASCII code units
//! (non-breaking spaces, BMP non-ASCII, astral surrogate pairs) into
//! `\xXX` / `\uXXXX` form. SWC's emitter (with `ascii_only: false`) emits
//! those raw. To match Babel byte-for-byte we pre-compute the escaped
//! `raw` here and let SWC's emitter print it verbatim (`lit.rs:91`
//! short-circuits to `write_str_lit(raw)` when raw is set and ascii-safe).
//!
//! Options pinned to Babel's defaults:
//! - `quotes: 'double'`
//! - `wrap: true` — output is wrapped in the chosen quote
//! - `minimal: false` — every code unit outside the printable-ASCII
//!   whitelist is escaped
//! - `escapeEverything: false`
//! - `lowercaseHex: false` — uppercase hex digits
//! - `es6: false` — astral chars naturally split into UTF-16 surrogate
//!   pairs since the loop iterates code units, not codepoints
//! - `json: false`

/// Babel's default `jsesc(value, jsescOption)` shape: returns the
/// quoted string literal (including surrounding `"..."`).
pub fn babel_default_string(value: &str) -> String {
    // Iterate over UTF-16 code units (matches jsesc's `string.charAt(i)` /
    // `string.charCodeAt(i)` behaviour with es6:false). Astral chars
    // surface as their high+low surrogate code units in this iteration,
    // each individually >= 0xD800, so they fall through to the
    // `\uXXXX` hex branch — producing `\uD83D\uDE0E` for U+1F60E etc.
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        // Printable-ASCII whitelist (jsesc.js:74:
        //   /[ !#-&\(-\[\]-_a-~]/) — excludes 0x22 ("), 0x27 ('),
        // 0x5C (\), 0x60 (`), and everything outside 0x20-0x7E.
        if is_whitelisted(u) {
            out.push(u as u8 as char);
            i += 1;
            continue;
        }
        // Quote/backtick/apostrophe handling: only the chosen quote is
        // escaped; the others stay literal. Quote here is `"`.
        if u == 0x22 {
            out.push_str("\\\"");
            i += 1;
            continue;
        }
        if u == 0x60 || u == 0x27 {
            // backtick / single-quote: literal (we're in double-quote mode).
            out.push(u as u8 as char);
            i += 1;
            continue;
        }
        // `\0` followed by non-digit → `\0`; followed by digit → `\x00`.
        if u == 0x00 {
            let next_is_digit = units
                .get(i + 1)
                .map(|n| (0x30..=0x39).contains(n))
                .unwrap_or(false);
            if !next_is_digit {
                out.push_str("\\0");
                i += 1;
                continue;
            }
            // fall through to hex branch
        }
        // Single-escape table (jsesc.js:59-70). `\v` is intentionally
        // omitted (IE<9 quirk noted in upstream comment).
        if let Some(esc) = single_escape(u) {
            out.push_str(esc);
            i += 1;
            continue;
        }
        // Hex branch. `\xXX` for code units <= 0xFF, `\uXXXX` otherwise.
        // Uppercase hex (lowercaseHex:false).
        if u <= 0xFF {
            out.push_str(&format!("\\x{:02X}", u));
        } else {
            out.push_str(&format!("\\u{:04X}", u));
        }
        i += 1;
    }
    out.push('"');
    out
}

fn is_whitelisted(u: u16) -> bool {
    // 0x20 (space), 0x21 (!), 0x23-0x26, 0x28-0x5B, 0x5D-0x5F, 0x61-0x7E.
    matches!(
        u,
        0x20 | 0x21 | 0x23..=0x26 | 0x28..=0x5B | 0x5D..=0x5F | 0x61..=0x7E
    )
}

fn single_escape(u: u16) -> Option<&'static str> {
    match u {
        0x5C => Some("\\\\"), // `\` → `\\`
        0x08 => Some("\\b"),  // backspace
        0x0C => Some("\\f"),  // form feed
        0x0A => Some("\\n"),  // newline
        0x0D => Some("\\r"),  // carriage return
        0x09 => Some("\\t"),  // tab
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_printable_ascii_passes_through() {
        assert_eq!(babel_default_string("hello world"), r#""hello world""#);
        assert_eq!(babel_default_string("a-z 0-9 .,;"), r#""a-z 0-9 .,;""#);
    }

    #[test]
    fn double_quote_is_escaped_single_and_backtick_kept() {
        assert_eq!(babel_default_string(r#"he said "hi""#), r#""he said \"hi\"""#);
        assert_eq!(babel_default_string("it's a `tag`"), r#""it's a `tag`""#);
    }

    #[test]
    fn backslash_is_escaped() {
        assert_eq!(babel_default_string(r"a\b"), r#""a\\b""#);
    }

    #[test]
    fn standard_control_chars_use_single_escapes() {
        assert_eq!(babel_default_string("\t"), r#""\t""#);
        assert_eq!(babel_default_string("\n"), r#""\n""#);
        assert_eq!(babel_default_string("\r"), r#""\r""#);
        assert_eq!(babel_default_string("\x08"), r#""\b""#);
        assert_eq!(babel_default_string("\x0C"), r#""\f""#);
    }

    #[test]
    fn vertical_tab_is_hex_not_v_escape() {
        // `\v` is intentionally omitted from singleEscapes (jsesc.js:68-69).
        assert_eq!(babel_default_string("\x0B"), r#""\x0B""#);
    }

    #[test]
    fn null_followed_by_digit_uses_hex() {
        assert_eq!(babel_default_string("\0"), r#""\0""#);
        assert_eq!(babel_default_string("\01"), r#""\x001""#);
        assert_eq!(babel_default_string("\0a"), r#""\0a""#);
    }

    #[test]
    fn non_ascii_below_256_uses_x_form() {
        // 0xA0 (non-breaking space) → `\xA0`; 0xFF → `\xFF`.
        assert_eq!(babel_default_string("\u{A0}"), r#""\xA0""#);
        assert_eq!(babel_default_string("\u{FF}"), r#""\xFF""#);
    }

    #[test]
    fn bmp_above_256_uses_u_form() {
        assert_eq!(babel_default_string("\u{0100}"), r#""\u0100""#);
        assert_eq!(babel_default_string("\u{FFFF}"), r#""\uFFFF""#);
    }

    #[test]
    fn astral_chars_split_into_surrogate_pair() {
        // U+1F60E (😎) → high 0xD83D, low 0xDE0E.
        assert_eq!(babel_default_string("\u{1F60E}"), r#""\uD83D\uDE0E""#);
        // U+10000 → high 0xD800, low 0xDC00.
        assert_eq!(babel_default_string("\u{10000}"), r#""\uD800\uDC00""#);
        // U+10FFFF → high 0xDBFF, low 0xDFFF.
        assert_eq!(babel_default_string("\u{10FFFF}"), r#""\uDBFF\uDFFF""#);
    }

    #[test]
    fn matches_target_sheet_string_byte_for_byte() {
        // The fixture's CSS bytes:
        //   `._aetrib82:after{content:"😎"}`
        // Babel's emitted string literal:
        //   `"._aetrib82:after{content:\"\uD83D\uDE0E\"}"`
        let css = "._aetrib82:after{content:\"\u{1F60E}\"}";
        let want = "\"._aetrib82:after{content:\\\"\\uD83D\\uDE0E\\\"}\"";
        assert_eq!(babel_default_string(css), want);
    }

    #[test]
    fn del_char_uses_hex() {
        assert_eq!(babel_default_string("\x7F"), r#""\x7F""#);
    }

    #[test]
    fn other_control_chars_use_x_form() {
        assert_eq!(babel_default_string("\x01"), r#""\x01""#);
        assert_eq!(babel_default_string("\x07"), r#""\x07""#);
        assert_eq!(babel_default_string("\x1F"), r#""\x1F""#);
    }
}
