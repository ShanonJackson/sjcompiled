//! crates/postcss-values-parser
//! Byte-for-byte Rust port of `postcss-values-parser@6.0.2` (plural).
//! Distinct from `postcss-value-parser@4.2.0` — different AST nodes.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-values-parser/lib/`):
//!   - `index.js`            -> `src/lib.rs` (this file)
//!   - `tokenize.js`         -> `src/tokenize.rs`
//!   - `walker.js`           -> `src/walker.rs`
//!   - `ValuesParser.js`     -> `src/values_parser.rs`
//!   - `ValuesStringifier.js`-> `src/values_stringifier.rs`
//!   - `nodes/AtWord.js`     -> `src/nodes/at_word.rs`
//!   - `nodes/Comment.js`    -> `src/nodes/comment.rs`
//!   - `nodes/Container.js`  -> `src/nodes/container.rs`
//!   - `nodes/Func.js`       -> `src/nodes/func.rs`
//!   - `nodes/Interpolation.js` -> `src/nodes/interpolation.rs`
//!   - `nodes/Node.js`       -> `src/nodes/node.rs`
//!   - `nodes/Numeric.js`    -> `src/nodes/numeric.rs`
//!   - `nodes/Operator.js`   -> `src/nodes/operator.rs`
//!   - `nodes/Punctuation.js`-> `src/nodes/punctuation.rs`
//!   - `nodes/Quoted.js`     -> `src/nodes/quoted.rs`
//!   - `nodes/UnicodeRange.js` -> `src/nodes/unicode_range.rs`
//!   - `nodes/Word.js`       -> `src/nodes/word.rs`

pub mod tokenize;
pub mod walker;
pub mod values_parser;
pub mod values_stringifier;
pub mod nodes;

pub use values_parser::ValuesParser;
pub use values_stringifier::{stringify_standalone, ValuesStringifier};
pub use nodes::{Node, NodeKind, Root};

/// Mirrors upstream `parse(css, options)` entry point.
pub fn parse(input: &str) -> Root {
    let mut p = ValuesParser::new(input.to_string());
    p.parse();
    p.into_root()
}

pub fn stringify(root: &Root) -> String { ValuesStringifier::stringify(root) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_with_unit() {
        let root = parse("16px");
        assert_eq!(root.nodes.len(), 1);
        if let NodeKind::Numeric(n) = &root.nodes[0].kind {
            assert_eq!(n.common.value, "16");
            assert_eq!(n.unit, "px");
        } else { panic!("expected Numeric, got {:?}", root.nodes[0].kind); }
    }

    #[test]
    fn parses_function() {
        let root = parse("rgb(1,2,3)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert_eq!(f.name, "rgb");
            // 1, comma, 2, comma, 3 = 5 children.
            assert_eq!(f.nodes.len(), 5);
        } else { panic!("expected Func, got {:?}", root.nodes[0].kind); }
    }

    #[test]
    fn parses_word() {
        let root = parse("red");
        if let NodeKind::Word(_w) = &root.nodes[0].kind {} else { panic!(); }
    }

    #[test]
    fn parses_quoted() {
        let root = parse("\"hello\"");
        if let NodeKind::Quoted(q) = &root.nodes[0].kind {
            assert_eq!(q.quote, '"');
            assert!(!q.unclosed);
        } else { panic!(); }
    }

    #[test]
    fn parses_variable() {
        let root = parse("--foo");
        if let NodeKind::Word(w) = &root.nodes[0].kind {
            assert!(w.is_variable);
        } else { panic!(); }
    }

    #[test]
    fn detects_unicode_range() {
        let root = parse("U+0025-00FF");
        if let NodeKind::UnicodeRange(_) = &root.nodes[0].kind {} else {
            panic!("expected UnicodeRange, got {:?}", root.nodes[0].kind);
        }
    }

    /// Drift fix: lowercase `u+0025` is NOT a UnicodeRange per upstream
    /// (`Word.js:18` regex is `U\+...`, capital-U only). It must classify
    /// as a Word.
    #[test]
    fn lowercase_u_is_not_unicode_range() {
        let root = parse("u+0025");
        // The wrapped tokenizer splits on `+` (operator), so `u+0025` becomes
        // tokens [u, +, 0025]. The first node is therefore Word `u`. The key
        // assertion is that NO node in the tree is UnicodeRange.
        let any_ur = root.nodes.iter().any(|n| matches!(n.kind, NodeKind::UnicodeRange(_)));
        assert!(!any_ur, "lowercase u+xxxx must not classify as UnicodeRange");
    }

    /// Drift fix: `Func.is_color` is set from `Func.js:90` regex
    /// `^(hsla?|hwb|lab|lch|rgba?)$/i` — color functions only.
    #[test]
    fn func_is_color_for_rgb() {
        let root = parse("rgb(1,2,3)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert!(f.is_color, "rgb() must have is_color=true");
        } else { panic!(); }
    }

    #[test]
    fn func_is_color_case_insensitive() {
        let root = parse("RGBA(1,2,3,1)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert!(f.is_color, "RGBA() must have is_color=true (case-insensitive)");
        } else { panic!(); }
    }

    #[test]
    fn func_not_color_for_calc() {
        let root = parse("calc(1px + 2px)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert!(!f.is_color, "calc() must NOT have is_color=true");
        } else { panic!(); }
    }

    /// Drift fix: `Func.is_var` requires name=`var` (case-insensitive)
    /// AND first child value matching `/^--[^\s]+$/`.
    #[test]
    fn func_is_var_for_css_variable() {
        let root = parse("var(--foo)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert!(f.is_var, "var(--foo) must have is_var=true");
        } else { panic!(); }
    }

    #[test]
    fn func_not_var_when_no_dash_dash_arg() {
        let root = parse("var(red)");
        if let NodeKind::Func(f) = &root.nodes[0].kind {
            assert!(!f.is_var, "var() with non-`--*` first arg must not have is_var=true");
        } else { panic!(); }
    }

    /// Drift fix: `Word.is_hex` uses `/^#(.+)/` — bare `"#"` must NOT
    /// classify as is_hex (old port used `starts_with('#')`).
    #[test]
    fn bare_hash_is_not_hex_word() {
        // `#` alone may or may not parse as a Word depending on tokenizer;
        // the property under test is that any Word with value="#"
        // has is_hex=false. Construct via the tokenizer surface.
        let root = parse("#");
        for n in &root.nodes {
            if let NodeKind::Word(w) = &n.kind {
                if w.common.value == "#" {
                    assert!(!w.is_hex, "bare # must not have is_hex=true");
                }
            }
        }
    }
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    /// `expand-shorthands` mutates this AST and re-emits via stringify;
    /// any drift here changes hashed bytes downstream. Lock the round-trip.
    fn assert_roundtrip(input: &str) {
        let root = parse(input);
        let out = stringify(&root);
        assert_eq!(out, input,
            "values-parser round-trip mismatch\n  input:  {:?}\n  output: {:?}",
            input, out);
    }

    #[test] fn keyword() { assert_roundtrip("red"); }
    #[test] fn px_value() { assert_roundtrip("16px"); }
    #[test] fn hex_value() { assert_roundtrip("#ff0000"); }
    #[test] fn space_separated() { assert_roundtrip("1px 2px 3px 4px"); }
    #[test] fn comma_list() { assert_roundtrip("a,b,c"); }
    #[test] fn function_call() { assert_roundtrip("rgb(1,2,3)"); }
    #[test] fn variable() { assert_roundtrip("--foo"); }
    #[test] fn quoted_string() { assert_roundtrip("\"hello\""); }
    #[test] fn at_word() { assert_roundtrip("@import"); }
}
