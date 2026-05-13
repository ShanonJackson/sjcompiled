//! crates/cssnano-postcss-normalize-string
//! Byte-for-byte Rust port of `postcss-normalize-string@5.1.0`.
//!
//! Folder/file mapping (1:1 with upstream):
//!   - `src/index.js` -> `src/lib.rs` (this file).
//!
//! All bugs of upstream 5.1.0 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! Pre-order DFS over `css`'s descendants. For each:
//!   - rule  → `selector = minify(selector, cache, preferredQuote)`.
//!   - decl  → `value    = minify(value,    cache, preferredQuote)`.
//!   - atrule → `params   = minify(params,   cache, preferredQuote)`.
//!
//! `minify` is value-parser-walked and caches by `original + '|' + quote`.
//! `normalize` walks every value-parser String node and:
//!   1. Parses the string body with the bespoke string-AST parser.
//!   2. If the body contains any quote token (bare OR escaped),
//!      consider re-wrapping (`changeWrappingQuotes`).
//!   3. Otherwise, set the node's wrapping quote to `preferredQuote`.
//!   4. Re-emit the body via `stringify(ast)` — collapsing escaped
//!      newlines (`\\\n`) into nothing.

use indexmap::IndexMap;

use postcss_core::node::{Node, NodeKind};
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, stringify as vp_stringify, walk as vp_walk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredQuote {
    Single,
    Double,
}

impl PreferredQuote {
    fn key(&self) -> &'static str {
        match self { PreferredQuote::Single => "single", PreferredQuote::Double => "double" }
    }
    fn char(&self) -> char {
        match self { PreferredQuote::Single => '\'', PreferredQuote::Double => '"' }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizeStringOpts {
    pub preferred_quote: PreferredQuote,
}

impl Default for NormalizeStringOpts {
    fn default() -> Self { NormalizeStringOpts { preferred_quote: PreferredQuote::Double } }
}

// ---------------------------------------------------------------------------
// String AST — bespoke per upstream `parse(str)`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum StringAstNode {
    Space(String),
    /// Bare unescaped `'` — has fixed value `'`.
    SingleQuote,
    /// Bare unescaped `"` — has fixed value `"`.
    DoubleQuote,
    /// `\\'` — value `\\'` (2 chars).
    EscapedSingleQuote,
    /// `\\"` — value `\\"` (2 chars).
    EscapedDoubleQuote,
    /// `\\\n` — value `\\\n`. Stringify SKIPS this entirely (collapses
    /// multi-line strings).
    Newline,
    /// Default word/string token.
    String(String),
}

impl StringAstNode {
    fn value_str(&self) -> &str {
        match self {
            StringAstNode::Space(s) | StringAstNode::String(s) => s,
            StringAstNode::SingleQuote => "'",
            StringAstNode::DoubleQuote => "\"",
            StringAstNode::EscapedSingleQuote => "\\'",
            StringAstNode::EscapedDoubleQuote => "\\\"",
            StringAstNode::Newline => "\\\n",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct StringAstTypes {
    escaped_single_quote: usize,
    escaped_double_quote: usize,
    single_quote: usize,
    double_quote: usize,
}

#[derive(Debug, Clone, Default)]
struct StringAst {
    nodes: Vec<StringAstNode>,
    types: StringAstTypes,
    quotes: bool,
}

/// Mirrors upstream `stringify(ast)` — concatenate values, skipping
/// the escaped-newline token.
fn ast_stringify(ast: &StringAst) -> String {
    let mut out = String::new();
    for n in &ast.nodes {
        if matches!(n, StringAstNode::Newline) { continue; }
        out.push_str(n.value_str());
    }
    out
}

/// Mirrors upstream `parse(str)` byte-for-byte. The `WORD_END` regex
/// is replaced by an explicit byte-scan with the same character class
/// `[ \n\t\r\f'"\\]`.
fn ast_parse(s: &str) -> StringAst {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut pos: usize = 0;
    let mut ast = StringAst::default();

    while pos < len {
        let code = bytes[pos];
        match code {
            b' ' | b'\t' | b'\r' | 0x0C => {
                // SPACE | TAB | CR | FEED — not LF here. The do/while
                // includes LF as a continuation char though.
                let mut next = pos;
                loop {
                    next += 1;
                    if next >= len { break; }
                    let c = bytes[next];
                    if !matches!(c, b' ' | b'\n' | b'\t' | b'\r' | 0x0C) { break; }
                }
                ast.nodes.push(StringAstNode::Space(s[pos..next].to_string()));
                // Upstream: `pos = next - 1; pos++` at the end of switch.
                pos = next.saturating_sub(1);
            }
            b'\'' => {
                ast.nodes.push(StringAstNode::SingleQuote);
                ast.types.single_quote += 1;
                ast.quotes = true;
            }
            b'"' => {
                ast.nodes.push(StringAstNode::DoubleQuote);
                ast.types.double_quote += 1;
                ast.quotes = true;
            }
            b'\\' => {
                let next_pos = pos + 1;
                let next_code = if next_pos < len { Some(bytes[next_pos]) } else { None };
                match next_code {
                    Some(b'\'') => {
                        ast.nodes.push(StringAstNode::EscapedSingleQuote);
                        ast.types.escaped_single_quote += 1;
                        ast.quotes = true;
                        pos = next_pos;
                    }
                    Some(b'"') => {
                        ast.nodes.push(StringAstNode::EscapedDoubleQuote);
                        ast.types.escaped_double_quote += 1;
                        ast.quotes = true;
                        pos = next_pos;
                    }
                    Some(b'\n') => {
                        ast.nodes.push(StringAstNode::Newline);
                        pos = next_pos;
                    }
                    _ => {
                        // Fall through — upstream uses an intentional
                        // missing `break` to fall into the default
                        // (word) branch. Replicate that here.
                        let next = scan_word_end(bytes, pos + 1, len);
                        let value = &s[pos..=next];
                        ast.nodes.push(StringAstNode::String(value.to_string()));
                        pos = next;
                    }
                }
            }
            _ => {
                // default: word scan from pos+1.
                let next = scan_word_end(bytes, pos + 1, len);
                let value = &s[pos..=next];
                ast.nodes.push(StringAstNode::String(value.to_string()));
                pos = next;
            }
        }
        pos += 1;
    }

    ast
}

/// Mirrors upstream's `WORD_END = /[ \n\t\r\f'"\\]/g` scan starting at
/// `from`. Returns the index of the character BEFORE the next word-end
/// char. If no word-end is found, returns `len - 1` (matches upstream's
/// `WORD_END.lastIndex === 0 → next = len - 1`).
fn scan_word_end(bytes: &[u8], from: usize, len: usize) -> usize {
    let mut i = from;
    while i < len {
        let c = bytes[i];
        if matches!(c, b' ' | b'\n' | b'\t' | b'\r' | 0x0C | b'\'' | b'"' | b'\\') {
            return i.saturating_sub(1);
        }
        i += 1;
    }
    // No match — upstream sets `next = len - 1`.
    len.saturating_sub(1)
}

// ---------------------------------------------------------------------------
// Quote rewrap logic.
// ---------------------------------------------------------------------------

/// `changeWrappingQuotes(node, ast)` — upstream `src/index.js:183-207`.
///
/// Each `if` reads `node.quote` freshly so the second branch sees the
/// post-flip quote (a snapshot would deviate from upstream verbatim
/// even though the second branch can never fire after a flip — the
/// first `if` requires the opposite escape count to be zero, so the
/// second `if`'s precondition is unsatisfiable post-flip).
fn change_wrapping_quotes(node: &mut VNode, ast: &mut StringAst) {
    if ast.types.single_quote != 0 || ast.types.double_quote != 0 {
        return;
    }
    if node.quote == Some('\'')
        && ast.types.escaped_single_quote > 0
        && ast.types.escaped_double_quote == 0
    {
        node.quote = Some('"');
    }
    if node.quote == Some('"')
        && ast.types.escaped_double_quote > 0
        && ast.types.escaped_single_quote == 0
    {
        node.quote = Some('\'');
    }
    let new_quote = node.quote.unwrap_or('"');
    let updated = change_child_quotes(std::mem::take(&mut ast.nodes), new_quote);
    ast.nodes = updated;
}

/// `changeChildQuotes(childNodes, parentQuote)` upstream.
fn change_child_quotes(nodes: Vec<StringAstNode>, parent_quote: char) -> Vec<StringAstNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for child in nodes {
        match child {
            StringAstNode::EscapedDoubleQuote if parent_quote == '\'' => {
                out.push(StringAstNode::DoubleQuote);
            }
            StringAstNode::EscapedSingleQuote if parent_quote == '"' => {
                out.push(StringAstNode::SingleQuote);
            }
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// normalize / minify / plugin entry.
// ---------------------------------------------------------------------------

fn normalize(value: &str, preferred_quote: PreferredQuote) -> String {
    if value.is_empty() {
        return value.to_string();
    }
    let mut parsed = vp_parse(value);
    vp_walk(
        &mut parsed,
        |child: &mut VNode, _i: usize| -> Option<bool> {
            if child.kind != VKind::String { return None; }
            let mut ast = ast_parse(&child.value);
            if ast.quotes {
                change_wrapping_quotes(child, &mut ast);
            } else {
                child.quote = Some(preferred_quote.char());
            }
            child.value = ast_stringify(&ast);
            None
        },
        false,
    );
    vp_stringify(&parsed)
}

fn minify(
    original: &str,
    cache: &mut IndexMap<String, String>,
    preferred_quote: PreferredQuote,
) -> String {
    let key = format!("{original}|{}", preferred_quote.key());
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let new_value = normalize(original, preferred_quote);
    cache.insert(key, new_value.clone());
    new_value
}

pub fn postcss_normalize_string(root: &mut Root, opts: &NormalizeStringOpts) -> PluginResult {
    let mut cache: IndexMap<String, String> = IndexMap::new();
    walk_tree(&mut root.root, opts.preferred_quote, &mut cache);
    Ok(())
}

/// Pre-order DFS that mirrors postcss `Container.walk(cb)`.
fn walk_tree(parent: &mut Node, pq: PreferredQuote, cache: &mut IndexMap<String, String>) {
    let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
    for i in 0..len {
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            process_node(child, pq, cache);
        }
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            walk_tree(child, pq, cache);
        }
    }
}

fn process_node(node: &mut Node, pq: PreferredQuote, cache: &mut IndexMap<String, String>) {
    // Mirrors upstream `OnceExit(css)` in `src/index.js:298-313`: a plain
    // field assignment, NOT a raws-clearing op. The postcss stringifier
    // (JS `lib/stringifier.js#rawValue` and our `postcss-core::stringifier::raw_value_str`)
    // already compares `raws.{prop}.value === node.{prop}` and emits the
    // cached `raw` form only when they match — which is the correct
    // behavior when `minify` returns an unchanged value (e.g. for any
    // selector/decl/atrule whose source contains comments or trailing
    // whitespace captured into raws). Clearing raws here would lose
    // those source bytes on no-op normalization, diverging from JS.
    match &mut node.kind {
        NodeKind::Rule(r) => {
            let new_sel = minify(&r.selector, cache, pq);
            r.selector = new_sel;
        }
        NodeKind::Declaration(d) => {
            let new_val = minify(&d.value, cache, pq);
            d.value = new_val;
        }
        NodeKind::AtRule(a) => {
            let new_params = minify(&a.params, cache, pq);
            a.params = new_params;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_normalize_string(&mut root, &NormalizeStringOpts::default()).unwrap();
        stringify(&root)
    }

    #[test]
    fn flips_single_to_double_for_plain_string() {
        let out = run("a { content: 'foo'; }");
        assert!(out.contains("\"foo\""), "got: {out:?}");
    }

    #[test]
    fn keeps_double_for_plain_string_under_default() {
        let out = run("a { content: \"foo\"; }");
        assert!(out.contains("\"foo\""), "got: {out:?}");
    }

    #[test]
    fn no_op_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn handles_empty_string_decl_value() {
        let out = run("a { content: ''; }");
        assert!(out.contains("\"\""), "got: {out:?}");
    }

    #[test]
    fn flips_when_escapes_are_redundant() {
        // `"\""` body has an escaped double-quote → wrap should flip
        // to `'` so the escape becomes a bare `"`.
        let out = run("a { content: \"\\\"\"; }");
        assert!(out.contains("'\"'"), "got: {out:?}");
    }

    #[test]
    fn collapses_escaped_newline() {
        // String body `foo\\\nbar` should collapse to `foobar`.
        let out = run("a { content: \"foo\\\nbar\"; }");
        assert!(out.contains("\"foobar\""), "got: {out:?}");
    }

    #[test]
    fn change_wrapping_quotes_post_flip_second_if_inert() {
        // Regression for the verbatim re-read of `node.quote` between
        // the two `if`s in `change_wrapping_quotes`. Body is `\'` inside
        // a `'`-wrapped string: first if flips wrap to `"`. With a stale
        // snapshot that path would still pass; we cover the OPPOSITE
        // path here too (body `\"` inside `"`-wrap) where stale-vs-fresh
        // are equally fine, just to lock in the symmetry.
        let out_a = run("a { content: '\\''; }");
        assert!(out_a.contains("\"'\""), "got: {out_a:?}");
        let out_b = run("a { content: \"\\\"\"; }");
        assert!(out_b.contains("'\"'"), "got: {out_b:?}");
    }

    #[test]
    fn backslash_followed_by_non_special_falls_through() {
        // Upstream: BACKSLASH branch's missing `break` falls through to
        // default when the next char is NOT `'`, `"`, or `\n`. The whole
        // run including the leading backslash becomes a single string
        // node and stringifies unchanged.
        let out = run("a { content: \"\\g\"; }");
        assert!(out.contains("\"\\g\""), "got: {out:?}");
    }

    #[test]
    fn preserves_raws_on_noop_normalization() {
        // Regression for the raws-clearing drift. Source has a trailing
        // comment after the value; postcss-core captures it into
        // `raws.value.raw`. Normalize-string is a no-op (string is
        // already double-quoted), so `raws.value.value == node.value`
        // and the stringifier should emit the raw form (with comment).
        // Prior code cleared raws → comment was lost.
        let out = run("a { content: \"foo\" /* trailing */; }");
        assert!(
            out.contains("/* trailing */"),
            "trailing comment must survive no-op normalization; got: {out:?}"
        );
    }

    #[test]
    fn cache_key_collision_resistant_with_pipe_in_value() {
        // Cache key is `original + '|' + preferredQuote`. A value that
        // already contains `|` must still produce a unique key (default
        // quote is "double" so the key suffix is constant) — and the
        // normalized output must equal what we'd get without caching.
        let css = "a { content: '|||'; } b { content: '|||'; }";
        let out = run(css);
        // Both strings get rewrapped to `"|||"`.
        let count = out.matches("\"|||\"").count();
        assert_eq!(count, 2, "got: {out:?}");
    }

    #[test]
    fn unescapes_redundant_quote_in_mixed_escapes() {
        // Body `\"\\'` (escaped double + escaped single, no bare quotes).
        // Bail check is on BARE quotes only, so `change_wrapping_quotes`
        // proceeds. Wrap stays `"` (neither flip condition matches —
        // both escape kinds present). `changeChildQuotes` then unescapes
        // `\\'` to bare `'` inside the now-`"`-wrapped body.
        let css = "a { content: \"\\\"\\'\"; }";
        let out = run(css);
        assert!(out.contains("\"\\\"'\""), "got: {out:?}");
    }
}
