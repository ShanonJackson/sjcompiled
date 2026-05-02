//! crates/cssnano-postcss-discard-comments
//! Byte-for-byte Rust port of `postcss-discard-comments@5.1.2`.
//!
//! Folder/file mapping (1:1 with upstream
//! `node_modules/postcss-discard-comments@5.1.2/src/`, with one
//! Rust-mandated rename):
//!   - `index.js`               -> `src/lib.rs` (this file)
//!   - `lib/commentParser.js`   -> `src/comment_parser.rs`
//!   - `lib/commentRemover.js`  -> `src/comment_remover.rs`
//!
//! The `lib/` parent directory is dropped because Rust's crate-root
//! file is itself `lib.rs`; a child module literally named `lib`
//! collides. Behavior is unaffected.
//!
//! All bugs of upstream 5.1.2 are intentionally preserved (notably
//! the unclosed-comment edge case in `comment_parser.rs`).
//!
//! ## Behavior (1:1 with upstream `OnceExit(css)`)
//!
//! 1. Pre-order DFS over `css`'s descendants (NOT `css` itself).
//! 2. Each `comment` node whose body satisfies `canRemove` is removed
//!    via `node.remove()` (which uses Root.removeChild semantics for
//!    root-level removal — raws.before transfers to the next sibling
//!    when removing the first child of root).
//! 3. For ANY surviving node, `raws.between` (when defined) is run
//!    through `replaceComments(value, list.space)` — comments inside
//!    are dropped or kept per `canRemove`, then the result is
//!    space-collapsed.
//! 4. Decl-specific:
//!    - If `raws.value` exists with a non-empty raw, replace the
//!      stored value with the comment-stripped raw and clear the
//!      cached `raws.value`.
//!    - If `raws.important` is set, run it through `replaceComments`;
//!      if the result has no surviving comments, collapse to the
//!      canonical `"!important"`.
//!    - Else apply `replaceComments` to `decl.value`.
//! 5. Rule-specific:
//!    - If `raws.selector` exists with a non-empty raw, replace it
//!      via `replaceComments` with **separator `''`** (not the default
//!      `' '`) — this can join previously-separated selector tokens.
//! 6. Atrule-specific:
//!    - If `raws.afterName` is set, run through `replaceComments`. If
//!      the result is empty, set to `' '`. Otherwise pad with `' '` on
//!      both sides.
//!    - If `raws.params.raw` is set, run through `replaceComments`.

pub mod comment_parser;
pub mod comment_remover;

use indexmap::IndexMap;

use postcss_core::container::{remove_at, Mutation};
use postcss_core::list;
use postcss_core::node::{Node, NodeKind};
use postcss_core::{PluginResult, Root};

use self::comment_parser::{comment_parser, token_text, Token, TokenKind};
use self::comment_remover::CommentRemover;

/// Plugin options. Default behavior keeps `/*! ...*/` important comments
/// and drops all others.
pub struct DiscardCommentsOpts {
    /// Default `false`. When `true`, every comment is removed including
    /// `/*!` ones.
    pub remove_all: bool,
    /// Default `false`. When `true`, keep ONLY the FIRST `/*!` important
    /// comment in document order; remove all subsequent comments.
    pub remove_all_but_first: bool,
    /// Optional callback override — when set, every comment is passed
    /// to it and the boolean return value decides removal.
    pub remove: Option<Box<dyn Fn(&str) -> bool>>,
}

impl Default for DiscardCommentsOpts {
    fn default() -> Self {
        DiscardCommentsOpts { remove_all: false, remove_all_but_first: false, remove: None }
    }
}

impl std::fmt::Debug for DiscardCommentsOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscardCommentsOpts")
            .field("remove_all", &self.remove_all)
            .field("remove_all_but_first", &self.remove_all_but_first)
            .field("remove", &self.remove.is_some())
            .finish()
    }
}

/// Plugin entrypoint.
pub fn postcss_discard_comments(root: &mut Root, opts: &DiscardCommentsOpts) -> PluginResult {
    let mut remover = CommentRemover::new(opts);
    let mut matcher_cache: IndexMap<String, Vec<Token>> = IndexMap::new();
    let mut replacer_cache: IndexMap<String, String> = IndexMap::new();
    walk_tree(&mut root.root, &mut remover, &mut matcher_cache, &mut replacer_cache);
    Ok(())
}

/// `replaceComments(source, space, separator)` upstream. Caches by
/// `source + '@|@' + separator`. Builds a stripped string by walking
/// the comment tokens, replacing removable comments with `separator`
/// and keeping non-removable comments as `/*body*/`. The result is
/// then space-collapsed via `list.space(parsed).join(' ')`.
fn replace_comments(
    source: &str,
    separator: &str,
    remover: &mut CommentRemover,
    cache: &mut IndexMap<String, String>,
) -> String {
    let key = format!("{source}@|@{separator}");
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let toks = comment_parser(source);
    let mut acc = String::new();
    for t in &toks {
        let contents = token_text(source, *t);
        match t.kind {
            TokenKind::Text => acc.push_str(contents),
            TokenKind::Comment => {
                if let Some(true) = remover.can_remove(contents) {
                    acc.push_str(separator);
                } else {
                    acc.push_str("/*");
                    acc.push_str(contents);
                    acc.push_str("*/");
                }
            }
        }
    }
    let pieces = list::space(&acc);
    let result = pieces.join(" ");
    cache.insert(key, result.clone());
    result
}

/// `matchesComments(source)` — returns the comment-token subset of the
/// parser's output. Used by the `!important` collapse logic to detect
/// whether any comment survived `replaceComments`.
fn matches_comments(
    source: &str,
    cache: &mut IndexMap<String, Vec<Token>>,
) -> Vec<Token> {
    if let Some(cached) = cache.get(source) {
        return cached.clone();
    }
    let result: Vec<Token> = comment_parser(source)
        .into_iter()
        .filter(|t| t.kind == TokenKind::Comment)
        .collect();
    cache.insert(source.to_string(), result.clone());
    cache.get(source).cloned().unwrap_or_default()
}

/// Pre-order DFS walker that supports mid-walk removal. Mirrors postcss
/// `Container.walk(cb)` — visits child, then descends. Does NOT visit
/// `parent` itself. Removals shift sibling indices; we re-borrow the
/// parent on each iteration so the index cursor stays correct.
fn walk_tree(
    parent: &mut Node,
    remover: &mut CommentRemover,
    matcher_cache: &mut IndexMap<String, Vec<Token>>,
    replacer_cache: &mut IndexMap<String, String>,
) {
    let mut i: usize = 0;
    loop {
        let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len { break; }
        // Process child[i]. May produce a removal request.
        let removed = {
            let child = &mut parent.nodes_mut().unwrap()[i];
            process_node(child, remover, matcher_cache, replacer_cache)
        };
        if matches!(removed, Mutation::Remove) {
            // Use `remove_at` so Root.removeChild's raws-transfer fires
            // when removing the first child of root.
            remove_at(parent, i);
            // Cursor stays at i — what was at i+1 is now at i.
            continue;
        }
        // Descend before advancing (pre-order).
        {
            let child = &mut parent.nodes_mut().unwrap()[i];
            walk_tree(child, remover, matcher_cache, replacer_cache);
        }
        i += 1;
    }
}

/// Per-node processor. Returns `Mutation::Remove` if this node should be
/// dropped from its parent; `Mutation::Keep` otherwise.
fn process_node(
    node: &mut Node,
    remover: &mut CommentRemover,
    matcher_cache: &mut IndexMap<String, Vec<Token>>,
    replacer_cache: &mut IndexMap<String, String>,
) -> Mutation {
    // 1) Comment node — early removal path.
    //
    // Upstream `index.js` lines 73-77:
    //   if (node.type === 'comment' && remover.canRemove(node.text)) {
    //     node.remove();
    //     return;
    //   }
    //
    // The `return` sits INSIDE the `if`. When `canRemove` returns
    // `false`/`undefined` (kept-comment path), upstream falls through to
    // the `raws.between` check below. Postcss core never produces a
    // `raws.between` on comment nodes in practice, but we mirror the
    // control flow exactly so that any future drift in postcss-core's
    // raws population can't silently cause a hash divergence here.
    if let NodeKind::Comment(c) = &node.kind {
        let body_for_can_remove = c.text.clone();
        if let Some(true) = remover.can_remove(&body_for_can_remove) {
            return Mutation::Remove;
        }
        // Fall through — upstream has no early return for kept comments.
    }

    // 2) raws.between — applies to decl/rule/atrule. Upstream check is
    //    `typeof node.raws.between === 'string'` — we already model
    //    raws.between as `Option<String>`, so `Some(_)` is the analogue.
    if let Some(between) = node.raws.between.clone() {
        let new_between = replace_comments(&between, " ", remover, replacer_cache);
        node.raws.between = Some(new_between);
    }

    // 3) Decl-specific.
    if matches!(node.kind, NodeKind::Declaration(_)) {
        // 3a) raws.value with non-empty raw.
        let has_raw_value = matches!(&node.raws.value, Some(rv) if !rv.raw.is_empty());
        if has_raw_value {
            let (raw, raw_value, current_value) = {
                let rv = node.raws.value.as_ref().unwrap();
                let cur = match &node.kind { NodeKind::Declaration(d) => d.value.clone(), _ => String::new() };
                (rv.raw.clone(), rv.value.clone(), cur)
            };
            let new_value = if raw_value == current_value {
                replace_comments(&raw, " ", remover, replacer_cache)
            } else {
                replace_comments(&current_value, " ", remover, replacer_cache)
            };
            if let NodeKind::Declaration(d) = &mut node.kind {
                d.value = new_value;
            }
            node.raws.value = None;
        }
        // 3b) raws.important — non-empty truthy in JS.
        let important_raw = node.raws.important.clone().filter(|s| !s.is_empty());
        if let Some(important) = important_raw {
            let processed = replace_comments(&important, " ", remover, replacer_cache);
            let surviving = matches_comments(&processed, matcher_cache);
            let final_important = if !surviving.is_empty() { processed } else { "!important".to_string() };
            node.raws.important = Some(final_important);
        } else {
            // 3c) Apply replace_comments to decl.value.
            let cur = match &node.kind { NodeKind::Declaration(d) => d.value.clone(), _ => String::new() };
            let new_value = replace_comments(&cur, " ", remover, replacer_cache);
            if let NodeKind::Declaration(d) = &mut node.kind {
                d.value = new_value;
            }
        }
        return Mutation::Keep;
    }

    // 4) Rule-specific.
    if matches!(node.kind, NodeKind::Rule(_)) {
        let has_raw_selector = matches!(&node.raws.selector, Some(rv) if !rv.raw.is_empty());
        if has_raw_selector {
            let (raw, val) = {
                let rv = node.raws.selector.as_ref().unwrap();
                (rv.raw.clone(), rv.value.clone())
            };
            // Separator is '' (empty string), NOT ' '.
            let new_raw = replace_comments(&raw, "", remover, replacer_cache);
            node.raws.selector = Some(postcss_core::node::RawValue { value: val, raw: new_raw });
            return Mutation::Keep;
        }
        return Mutation::Keep;
    }

    // 5) AtRule-specific.
    if matches!(node.kind, NodeKind::AtRule(_)) {
        // 5a) raws.afterName — truthy non-empty.
        let after_name = node.raws.after_name.clone().filter(|s| !s.is_empty());
        if let Some(after_name) = after_name {
            let cr = replace_comments(&after_name, " ", remover, replacer_cache);
            let new_after = if cr.is_empty() {
                // upstream: `cr + ' '` = ' '
                " ".to_string()
            } else {
                format!(" {cr} ")
            };
            node.raws.after_name = Some(new_after);
        }
        // 5b) raws.params.raw — non-empty.
        let has_raw_params = matches!(&node.raws.params, Some(rv) if !rv.raw.is_empty());
        if has_raw_params {
            let (raw, val) = {
                let rv = node.raws.params.as_ref().unwrap();
                (rv.raw.clone(), rv.value.clone())
            };
            let new_raw = replace_comments(&raw, " ", remover, replacer_cache);
            node.raws.params = Some(postcss_core::node::RawValue { value: val, raw: new_raw });
        }
    }

    Mutation::Keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_discard_comments(&mut root, &DiscardCommentsOpts::default()).unwrap();
        stringify(&root)
    }

    #[test]
    fn drops_non_important_top_level_comment() {
        let out = run("/* foo */ a { color: red; }");
        eprintln!("out: {out:?}");
        assert!(!out.contains("/* foo */"), "got: {out:?}");
        // After comment drop the rule body remains. Selector raws may
        // strip leading whitespace; assert the rule survives.
        assert!(out.contains("color"), "got: {out:?}");
        assert!(out.contains("red"), "got: {out:?}");
    }

    #[test]
    fn keeps_important_comment() {
        let out = run("/*! keep me */ a { color: red; }");
        assert!(out.contains("/*! keep me */"), "got: {out:?}");
    }

    #[test]
    fn drops_inline_comment_between_decls() {
        let out = run("a { color: red; /* x */ background: blue; }");
        assert!(!out.contains("/* x */"), "got: {out:?}");
    }

    #[test]
    fn no_op_blank_input() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn drops_comment_in_decl_value() {
        let out = run("a { color: red /* hi */ blue; }");
        assert!(!out.contains("/* hi */"), "got: {out:?}");
    }
}
