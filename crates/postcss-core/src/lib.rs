//! crates/postcss-core
//! Byte-for-byte Rust port of `postcss@8.4.31`.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss/lib/`):
//!   - `tokenize.js`         -> `src/tokenize.rs`
//!   - `parser.js`           -> `src/parser.rs`
//!   - `stringifier.js`      -> `src/stringifier.rs`
//!   - `node.js`             -> `src/node.rs`
//!   - `container.js`        -> `src/container.rs`
//!   - `root.js`             -> `src/root.rs`
//!   - `rule.js`             -> `src/rule.rs`
//!   - `at-rule.js`          -> `src/at_rule.rs`
//!   - `declaration.js`      -> `src/declaration.rs`
//!   - `comment.js`          -> `src/comment.rs`
//!   - `input.js`            -> `src/input.rs`
//!   - `css-syntax-error.js` -> `src/css_syntax_error.rs`
//!   - `list.js`             -> `src/list.rs`
//!
//! All bugs of the upstream version are intentionally preserved.

pub mod tokenize;
pub mod input;
pub mod css_syntax_error;
pub mod list;
pub mod node;
pub mod container;
pub mod root;
pub mod rule;
pub mod at_rule;
pub mod declaration;
pub mod comment;
pub mod parser;
pub mod stringifier;
pub mod js_number;
pub mod plugin_error;

pub use at_rule::AtRule;
pub use comment::Comment;
pub use css_syntax_error::CssSyntaxError;
pub use declaration::Declaration;
pub use input::Input;
pub use node::{Node, NodeKind, RawValue, Raws, Source, SourcePosition};
pub use parser::Parser;
pub use root::Root;
pub use rule::Rule;
pub use stringifier::{stringify, stringify_node, Stringifier};
pub use js_number::js_number_to_string;
pub use plugin_error::{PluginError, PluginResult};
pub use container::{Mutation, Visit, WalkCtx};

/// `parse(css)` — entry that mirrors `node_modules/postcss/lib/parse.js`.
pub fn parse(css: &str) -> Result<Root, CssSyntaxError> {
    let input = Input::new(css.to_string(), None);
    let mut p = Parser::new(input);
    p.parse()?;
    Ok(p.into_root())
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    /// Exit gate for Phase 1a: `stringify(parse(css)) == css` for every CSS
    /// input in this corpus.
    fn assert_round_trip(css: &str) {
        let root = parse(css).expect(&format!("parse failed for: {:?}", css));
        let out = stringify(&root);
        assert_eq!(
            out, css,
            "round-trip mismatch\n  input:  {:?}\n  output: {:?}",
            css, out
        );
    }

    #[test]
    fn simple_decl() { assert_round_trip("a { color: red; }"); }

    #[test]
    fn no_trailing_semi() { assert_round_trip("a { color: red }"); }

    #[test]
    fn multiple_decls() { assert_round_trip("a {\n  color: red;\n  font-size: 12px;\n}"); }

    #[test]
    fn nested_atrule() {
        assert_round_trip("@media (max-width: 100px) {\n  a { color: red; }\n}");
    }

    #[test]
    fn comment_in_value() {
        assert_round_trip("a { color: /* hi */ red; }");
    }

    #[test]
    fn statement_atrule() {
        assert_round_trip("@charset \"utf-8\";");
    }

    #[test]
    fn empty_rule() { assert_round_trip("a {}"); }

    #[test]
    fn important_decl() { assert_round_trip("a { color: red !important; }"); }

    #[test]
    fn url_value() { assert_round_trip("a { background: url(foo.png); }"); }

    #[test]
    fn leading_underscore_hack() { assert_round_trip("a { _color: red; }"); }

    /// Regression for the rawCache `beforeOpen` fallback. A freshly-built
    /// Rule with no `raws.between` set should emit `selector ` + `{`
    /// (single space) — the upstream `DEFAULT_RAW.beforeOpen = " "`
    /// fallback. Hits when a plugin (e.g. `postcss-nested`) builds a
    /// wrapper rule from scratch.
    #[test]
    fn fresh_rule_gets_default_between_space() {
        let mut r = Root::new();
        r.nodes_mut().push(Node {
            kind: NodeKind::Rule(rule::Rule {
                selector: ".x".to_string(),
                nodes: vec![Node {
                    kind: NodeKind::Declaration(declaration::Declaration {
                        prop: "color".to_string(),
                        value: "red".to_string(),
                        important: false,
                        variable: false,
                    }),
                    raws: Raws::default(),
                    source: Source::default(),
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
        });

        let out = stringify(&r);
        assert!(out.contains(".x {"), "expected `.x {{` in output: {out:?}");
    }

    /// Regression for the rawCache `semicolon` fallback. Sibling rule
    /// in the tree has `raws.semicolon = true`; the freshly-built rule
    /// inherits via the cache scan and emits a trailing `;`.
    #[test]
    fn fresh_rule_inherits_semicolon_from_sibling() {
        let mut r = parse("a { color: red; }").unwrap();
        r.nodes_mut().push(Node {
            kind: NodeKind::Rule(rule::Rule {
                selector: ".x".to_string(),
                nodes: vec![Node {
                    kind: NodeKind::Declaration(declaration::Declaration {
                        prop: "color".to_string(),
                        value: "blue".to_string(),
                        important: false,
                        variable: false,
                    }),
                    raws: Raws::default(),
                    source: Source::default(),
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
        });
        let out = stringify(&r);
        // Sibling sample → `raws.semicolon = Some(true)` → emit `;`.
        assert!(out.contains(".x { color: blue;"), "got: {out:?}");
    }

    /// Regression: with no sibling sample for `raws.semicolon`, the
    /// fallback is `DEFAULT_RAW.semicolon = false` → no trailing `;`.
    #[test]
    fn fresh_rule_no_sibling_omits_trailing_semicolon() {
        let mut r = Root::new();
        r.nodes_mut().push(Node {
            kind: NodeKind::Rule(rule::Rule {
                selector: ".x".to_string(),
                nodes: vec![Node {
                    kind: NodeKind::Declaration(declaration::Declaration {
                        prop: "color".to_string(),
                        value: "red".to_string(),
                        important: false,
                        variable: false,
                    }),
                    raws: Raws::default(),
                    source: Source::default(),
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
        });
        let out = stringify(&r);
        assert!(out.contains("color: red"), "got: {out:?}");
        assert!(!out.contains("color: red;"), "got: {out:?}");
    }
}
