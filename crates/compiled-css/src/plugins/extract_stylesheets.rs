//! Port of `packages/css/src/plugins/extract-stylesheets.ts`.
//!
//! Upstream JS:
//! ```ts
//! OnceExit(root) {
//!   root.each((node) => {
//!     opts?.callback(node.toString());
//!   });
//! }
//! ```
//!
//! Iteration order matters — sheets feed into downstream hashing and
//! must arrive in document order. `root.each` is non-recursive, so we
//! only stringify direct children of root. Each sheet is a self-contained
//! `node.toString()` — see `postcss_core::stringify_node` for the
//! "no leading raws.before, no trailing semicolon" semantics that
//! mirror upstream.

use postcss_core::{stringify_node, PluginResult, Root};

#[derive(Debug, Clone, Default)]
pub struct ExtractStyleSheetsOpts {
    /// Sheets produced by the run are pushed here in document order.
    pub sheets: Vec<String>,
}

pub fn extract_stylesheets(root: &Root, opts: &mut ExtractStyleSheetsOpts) -> PluginResult {
    if let Some(children) = root.root.nodes() {
        for child in children {
            opts.sheets.push(stringify_node(child));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn run(css: &str) -> Vec<String> {
        let root = parse(css).unwrap();
        let mut opts = ExtractStyleSheetsOpts::default();
        extract_stylesheets(&root, &mut opts).unwrap();
        opts.sheets
    }

    #[test]
    fn one_rule_one_sheet() {
        let sheets = run("a { color: red; }");
        assert_eq!(sheets, vec!["a { color: red; }"]);
    }

    #[test]
    fn multiple_rules_in_doc_order() {
        let sheets = run("a { color: red; }\nb { color: blue; }\nc { color: green; }");
        assert_eq!(
            sheets,
            vec![
                "a { color: red; }".to_string(),
                "b { color: blue; }".to_string(),
                "c { color: green; }".to_string(),
            ]
        );
    }

    #[test]
    fn at_rule_emitted_as_sheet() {
        let sheets = run("@media (min-width: 100px) { a { color: red; } }");
        assert_eq!(sheets.len(), 1);
        assert!(sheets[0].starts_with("@media"));
    }

    #[test]
    fn top_level_decl_no_trailing_semicolon() {
        // `node.toString()` upstream calls dispatch with semicolon=undefined,
        // which is falsy — no `;` is emitted.
        let sheets = run("color: red;");
        assert_eq!(sheets, vec!["color: red"]);
    }

    #[test]
    fn comment_emitted_as_sheet() {
        let sheets = run("/* hi */");
        assert_eq!(sheets, vec!["/* hi */"]);
    }

    #[test]
    fn empty_input_no_sheets() {
        let sheets = run("");
        assert!(sheets.is_empty());
    }

    #[test]
    fn does_not_mutate_root() {
        // Plugin is read-only — round-trip after extract must equal input.
        let css = "a { color: red; }\nb { color: blue; }\n";
        let root = parse(css).unwrap();
        let mut opts = ExtractStyleSheetsOpts::default();
        extract_stylesheets(&root, &mut opts).unwrap();
        assert_eq!(postcss_core::stringify(&root), css);
    }
}
