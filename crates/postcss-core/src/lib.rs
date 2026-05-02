//! crates/postcss-core
//! Byte-for-byte Rust port of `postcss@8.5.6`.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.
//!
//! ## Version note
//!
//! The original target was `postcss@8.4.31` (the version pinned in the
//! Compiled monorepo this code originated from). When the consuming
//! monorepo's actual postcss version was confirmed as `8.5.6`, an
//! empirical diff (see `crates/_vendor/test-postcss-versions/`) showed:
//!
//!   * 5 of 13 source files (`stringifier`, `root`, `at-rule`, `comment`,
//!     `list`) are byte-identical between 8.4.31 and 8.5.6.
//!   * The remaining 8 files differ only in cosmetic reorderings,
//!     diagnostic/sourcemap surface, and defensive null-checks that
//!     don't reach the `parse → stringify` hashing path.
//!   * 26/26 raw round-trips and 30/30 plugin × input pairs produced
//!     byte-identical output across both versions.
//!
//! Conclusion: this port covers both 8.4.31 and 8.5.6 byte-output
//! correctly. Pin updated to 8.5.6 to match the actual deployment target.
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
pub use node::{AttrValue, Node, NodeAttrs, NodeKind, RawValue, Raws, Source, SourcePosition};
pub use parser::Parser;
pub use root::Root;
pub use rule::Rule;
pub use stringifier::{stringify, stringify_node, Stringifier};
pub use js_number::js_number_to_string;
pub use plugin_error::{PluginError, PluginResult};
pub use container::{
    DeferredMutation, Mutation, NodePath, Visit, WalkCtx,
    insert_before_at_path, node_at_path, node_at_path_mut, parent_every,
    parent_index_of, parent_nodes, parent_path, parent_some, sibling_at,
    sibling_relative, walk_at_rules_mut_with_parent, walk_comments_mut_with_parent,
    walk_decls_mut_with_parent, walk_mut_with_parent, walk_rules_mut_with_parent,
    walk_up_with,
};

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
                    ..Node::default()
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
            ..Node::default()
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
                    ..Node::default()
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
            ..Node::default()
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
                    ..Node::default()
                }],
            }),
            raws: Raws::default(),
            source: Source::default(),
            ..Node::default()
        });
        let out = stringify(&r);
        assert!(out.contains("color: red"), "got: {out:?}");
        assert!(!out.contains("color: red;"), "got: {out:?}");
    }
}

/// `root.rawCache` priority-chain regression suite.
///
/// Mirrors upstream `stringifier.js::raw()` step 3: when a plugin writes
/// `root.rawCache[detect]`, the stringifier returns that VERBATIM
/// regardless of what the tree-scan would produce. This is the path that
/// makes `cssnano-util-raw-cache`-style minification work — write 11
/// empty strings to rawCache, get back compact output without rewriting
/// every node's raws individually.
///
/// These tests would NOT surface from any test in `cssnano-utils` because
/// the producer (rawCache plugin) and the consumer (stringifier) live in
/// different crates with no integration test between them. They live here.
#[cfg(test)]
mod raw_cache_tests {
    use super::*;

    fn build_simple_root() -> Root {
        // Two rules: `a { color: red; }` and `b { color: blue; }`.
        // Built fresh (no parser) so every raws.before/between is None.
        let mut r = Root::new();
        for (sel, val) in [(".a", "red"), (".b", "blue")] {
            r.nodes_mut().push(Node {
                kind: NodeKind::Rule(rule::Rule {
                    selector: sel.to_string(),
                    nodes: vec![Node {
                        kind: NodeKind::Declaration(declaration::Declaration {
                            prop: "color".to_string(),
                            value: val.to_string(),
                            important: false,
                            variable: false,
                        }),
                        ..Node::default()
                    }],
                }),
                ..Node::default()
            });
        }
        r
    }

    #[test]
    fn raw_cache_colon_overrides_tree_scan() {
        // Without rawCache: decl emits ".a { color: red; }" (default ": ").
        // With rawCache.colon = ":" (cssnano-style minified): emits "color:red".
        let mut r = build_simple_root();
        r.set_raw_cache("colon", ":");
        let out = stringify(&r);
        assert!(out.contains("color:red"), "expected `color:red` (colon override): {out:?}");
        assert!(!out.contains("color: red"), "rawCache override should win: {out:?}");
    }

    #[test]
    fn raw_cache_before_open_overrides_tree_scan() {
        // rawCache.beforeOpen = "" should remove the space before `{`.
        let mut r = build_simple_root();
        r.set_raw_cache("beforeOpen", "");
        let out = stringify(&r);
        assert!(out.contains(".a{"), "expected `.a{{` (beforeOpen=\"\"): {out:?}");
        assert!(out.contains(".b{"));
    }

    #[test]
    fn raw_cache_before_decl_overrides_tree_scan() {
        // Without override: tree-scan finds no decl `before` (fresh tree),
        // falls back to DEFAULT_RAW.beforeDecl = "\n".
        // With override = "": no leading whitespace before each decl.
        let mut r = build_simple_root();
        r.set_raw_cache("beforeDecl", "");
        let out = stringify(&r);
        // Decl emits without leading newline. The exact bytes around
        // `color:` vary, but the whole rule should be on one logical line
        // with no `\n` before `color`.
        assert!(!out.contains("\ncolor"), "beforeDecl override should suppress newline: {out:?}");
    }

    #[test]
    fn raw_cache_before_close_overrides_tree_scan() {
        // rawCache.beforeClose = "" → `}` immediately after last decl.
        let mut r = build_simple_root();
        r.set_raw_cache("beforeClose", "");
        let out = stringify(&r);
        // Both rules close with `}` directly after their last byte.
        // We sniff for `red}` and `blue}` — no whitespace between.
        assert!(out.contains("red}") || out.contains("red;}"), "got: {out:?}");
        assert!(out.contains("blue}") || out.contains("blue;}"), "got: {out:?}");
    }

    #[test]
    fn raw_cache_full_minify_all_eleven_keys() {
        // Apply the full `cssnano-util-raw-cache` payload: 11 keys, all
        // empty except colon (`:`) and indent (`""`). Output should be
        // fully minified.
        let mut r = build_simple_root();
        for (k, v) in [
            ("colon", ":"),
            ("indent", ""),
            ("beforeDecl", ""),
            ("beforeRule", ""),
            ("beforeOpen", ""),
            ("beforeClose", ""),
            ("beforeComment", ""),
            ("after", ""),
            ("emptyBody", ""),
            ("commentLeft", ""),
            ("commentRight", ""),
        ] {
            r.set_raw_cache(k, v);
        }
        let out = stringify(&r);
        // Every space the default tree-scan would have inserted is gone.
        assert!(!out.contains(" {"), "beforeOpen should be empty: {out:?}");
        assert!(!out.contains(": "), "colon should be `:`: {out:?}");
        assert!(!out.contains("\n"), "no newlines in fully-minified output: {out:?}");
    }

    #[test]
    fn raw_cache_empty_does_not_change_output() {
        // Empty rawCache → identical to unset → tree-scan + DEFAULT_RAW path.
        // For freshly-built nodes (no parser raws), tree-scan finds no
        // sample → falls back to DEFAULT_RAW: beforeDecl="\n",
        // beforeClose="\n", beforeOpen=" ", colon=": ". Output bytes
        // match what upstream JS produces for the same fresh tree.
        let r = build_simple_root();
        let out = stringify(&r);
        assert_eq!(out, ".a {\ncolor: red\n}\n.b {\ncolor: blue\n}",
            "no rawCache: DEFAULT_RAW chain produces canonical fresh-tree output");
    }

    #[test]
    fn node_raws_still_wins_over_raw_cache() {
        // rawCache is step 3 in upstream; node.raws.<key> is step 1.
        // If a node has its own raws.between, it must NOT be overridden.
        let mut r = Root::new();
        r.set_raw_cache("colon", ":");
        // Decl with explicit `raws.between = " /* important */ "`.
        r.nodes_mut().push(Node {
            kind: NodeKind::Rule(rule::Rule {
                selector: ".a".to_string(),
                nodes: vec![Node {
                    kind: NodeKind::Declaration(declaration::Declaration {
                        prop: "color".to_string(),
                        value: "red".to_string(),
                        important: false,
                        variable: false,
                    }),
                    raws: Raws {
                        between: Some(" /* x */ ".to_string()),
                        ..Raws::default()
                    },
                    ..Node::default()
                }],
            }),
            ..Node::default()
        });
        let out = stringify(&r);
        assert!(out.contains("color /* x */ red"),
            "node.raws.between (step 1) must win over rawCache.colon (step 3): {out:?}");
    }
}
