//! 1:1 port of `packages/babel-plugin/src/utils/comments.ts`.
//!
//! Upstream `getNodeComments(path, meta)` returns
//! `{before, current}` `CommentLine[]` from
//! `meta.state.file.ast.comments`, filtered by line-number against
//! `path.node.loc.start.line` and `lineNumber - 1`. The Rust port
//! mirrors this against [`LineComment`]s pre-resolved at
//! `Program::enter` — see [`collect_line_comments`] for the
//! AST-walk + source-map lookup that builds the store.
//!
//! ### Why a pre-pass?
//!
//! SWC's [`PluginCommentsProxy`] is `BytePos`-keyed and does not
//! expose iteration over the whole file's comments. Babel walks
//! `file.ast.comments` (a flat list); the equivalent in SWC is
//! "for every span seen during a Visit, query
//! `get_leading(span.lo)` and `get_trailing(span.hi)`". The
//! collector dedupes per-`BytePos` so each unique attachment point
//! is queried once, and dedupes per-comment by `(span.lo, span.hi)`
//! so a single comment attached as both leading-of-X and
//! trailing-of-Y is captured once.
//!
//! ### Source-map dependency
//!
//! Resolving `BytePos` → 1-indexed line requires a `SourceMapper`.
//! The SWC plugin runtime exposes this via
//! `meta.source_map: PluginSourceMapProxy` (see
//! `lib.rs::process`). Tests that don't go through the plugin entry
//! get an empty `comment_lines` vec on `State` — the disable check
//! then returns `false` for every input, matching upstream's
//! "no-directive" fast path.

use std::collections::{HashMap, HashSet};

use swc_core::common::comments::{Comment, CommentKind, Comments};
use swc_core::common::{BytePos, SourceMapper, Spanned};
use swc_core::ecma::ast::{
    ArrowExpr, BlockStmt, Class, Expr, Function, JSXAttr, JSXClosingElement, JSXElement,
    JSXExprContainer, JSXOpeningElement, Module, Pat, Program, Script, Stmt, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::constants::{
    COMPILED_DIRECTIVE_DISABLE_LINE, COMPILED_DIRECTIVE_DISABLE_NEXT_LINE,
    COMPILED_DIRECTIVE_TRANSFORM_CSS_PROP,
};
use crate::state::{LineComment, State};

/// §6.5 pre-pass output — pre-resolved comment list AND a
/// `BytePos → 1-indexed line` index covering every span the
/// collector visited. Both go onto `State` (via
/// `set_comment_lines` / `set_span_lines`). Producing them in one
/// pass avoids two AST walks at `Program::enter`.
pub struct LineIndex {
    pub comments: Vec<LineComment>,
    pub spans: HashMap<u32, usize>,
}

/// Walk the program once, querying `comments.get_leading(span.lo)` /
/// `comments.get_trailing(span.hi)` at every node and resolving each
/// `BytePos` to a 1-indexed line via `source_map`. Returns a deduped
/// `LineIndex` for the visitor to install on `State`.
///
/// Span dedupe: each unique `BytePos.0` is queried once. Comment
/// dedupe: each unique `(span.lo, span.hi)` is recorded once (a
/// comment attached as both leading-of-X and trailing-of-Y otherwise
/// shows up twice).
pub fn collect_line_comments<C, S>(program: &Program, comments: &C, source_map: &S) -> LineIndex
where
    C: Comments,
    S: SourceMapper,
{
    let mut collector = Collector {
        comments,
        source_map,
        seen_pos: HashSet::new(),
        seen_comments: HashSet::new(),
        comments_out: Vec::new(),
        spans_out: HashMap::new(),
    };
    program.visit_with(&mut collector);
    LineIndex {
        comments: collector.comments_out,
        spans: collector.spans_out,
    }
}

struct Collector<'a, C, S> {
    comments: &'a C,
    source_map: &'a S,
    seen_pos: HashSet<u32>,
    seen_comments: HashSet<(u32, u32)>,
    comments_out: Vec<LineComment>,
    spans_out: HashMap<u32, usize>,
}

impl<C: Comments, S: SourceMapper> Collector<'_, C, S> {
    fn collect_at(&mut self, pos: BytePos) {
        // BytePos(0) is the dummy span sentinel; max value is the
        // out-of-range sentinel some swc helpers use. Skip both.
        if pos.0 == 0 || pos.0 == u32::MAX {
            return;
        }
        if !self.seen_pos.insert(pos.0) {
            return;
        }
        // Record this position's line for cheap dispatch-time lookup.
        let line = self.source_map.lookup_char_pos(pos).line;
        self.spans_out.insert(pos.0, line);
        if let Some(cmts) = self.comments.get_leading(pos) {
            for c in cmts {
                self.push_comment(c);
            }
        }
        if let Some(cmts) = self.comments.get_trailing(pos) {
            for c in cmts {
                self.push_comment(c);
            }
        }
    }

    fn push_comment(&mut self, c: Comment) {
        let key = (c.span.lo.0, c.span.hi.0);
        if !self.seen_comments.insert(key) {
            return;
        }
        let start_line = self.source_map.lookup_char_pos(c.span.lo).line;
        let end_line = self.source_map.lookup_char_pos(c.span.hi).line;
        self.comments_out.push(LineComment {
            start_line,
            end_line,
            kind: c.kind,
            text: c.text.to_string(),
        });
    }
}

impl<C: Comments, S: SourceMapper> Visit for Collector<'_, C, S> {
    fn visit_module(&mut self, n: &Module) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_script(&mut self, n: &Script) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_stmt(&mut self, n: &Stmt) {
        self.collect_at(n.span().lo);
        self.collect_at(n.span().hi);
        n.visit_children_with(self);
    }

    fn visit_expr(&mut self, n: &Expr) {
        self.collect_at(n.span().lo);
        self.collect_at(n.span().hi);
        n.visit_children_with(self);
    }

    fn visit_jsx_element(&mut self, n: &JSXElement) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_jsx_opening_element(&mut self, n: &JSXOpeningElement) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_jsx_closing_element(&mut self, n: &JSXClosingElement) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_jsx_attr(&mut self, n: &JSXAttr) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_jsx_expr_container(&mut self, n: &JSXExprContainer) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, n: &VarDeclarator) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_pat(&mut self, n: &Pat) {
        self.collect_at(n.span().lo);
        self.collect_at(n.span().hi);
        n.visit_children_with(self);
    }

    fn visit_block_stmt(&mut self, n: &BlockStmt) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_function(&mut self, n: &Function) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }

    fn visit_class(&mut self, n: &Class) {
        self.collect_at(n.span.lo);
        self.collect_at(n.span.hi);
        n.visit_children_with(self);
    }
}

/// 1:1 port of `getNodeComments(path, meta)`
/// (`packages/babel-plugin/src/utils/comments.ts`).
///
/// Returns `(before, current)` line-comments where `before` matches
/// `lineNumber - 1` on both ends and `current` matches `lineNumber`
/// on both ends. Filtered to `CommentKind::Line` only — upstream's
/// `comment.type === 'CommentLine'` predicate.
///
/// Returns empty vecs when the node spans multiple lines. This
/// mirrors the upstream early-return:
///
/// ```js
/// if (!lineNumber || lineNumber !== path.node?.loc?.end.line) {
///   return { before: [], current: [] };
/// }
/// ```
pub fn get_node_comments<'a>(
    state: &'a State,
    start_line: usize,
    end_line: usize,
) -> (Vec<&'a LineComment>, Vec<&'a LineComment>) {
    if start_line == 0 || start_line != end_line {
        return (Vec::new(), Vec::new());
    }
    let line = start_line;
    let line_above = line.saturating_sub(1);
    let mut before = Vec::new();
    let mut current = Vec::new();
    for c in state.comment_lines() {
        if c.kind != CommentKind::Line {
            continue;
        }
        if line_above != 0 && c.start_line == line_above && c.end_line == line_above {
            before.push(c);
        }
        if c.start_line == line && c.end_line == line {
            current.push(c);
        }
    }
    (before, current)
}

/// 1:1 port of `isCssPropDisabled(path, meta)`
/// (`packages/babel-plugin/src/css-prop/index.ts:26-44`).
///
/// `start_line` / `end_line` are the 1-indexed lines of the path
/// being checked. Pass `(0, 0)` (or any pair where `start != end`)
/// to no-op.
pub fn is_css_prop_disabled(state: &State, start_line: usize, end_line: usize) -> bool {
    let (before, current) = get_node_comments(state, start_line, end_line);
    let next_line_directive = format!(
        "{} {}",
        COMPILED_DIRECTIVE_DISABLE_NEXT_LINE, COMPILED_DIRECTIVE_TRANSFORM_CSS_PROP
    );
    let same_line_directive = format!(
        "{} {}",
        COMPILED_DIRECTIVE_DISABLE_LINE, COMPILED_DIRECTIVE_TRANSFORM_CSS_PROP
    );
    before
        .iter()
        .any(|c| c.text.trim().starts_with(&next_line_directive))
        || current
            .iter()
            .any(|c| c.text.trim().starts_with(&same_line_directive))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;

    fn line_comment(line: usize, text: &str) -> LineComment {
        LineComment {
            start_line: line,
            end_line: line,
            kind: CommentKind::Line,
            text: text.into(),
        }
    }

    #[test]
    fn no_comment_lines_means_no_disable() {
        let s = State::default();
        assert!(!is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn disable_next_line_directive_above_target_line_disables() {
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            4,
            " @compiled-disable-next-line transform-css-prop",
        )]);
        assert!(is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn disable_line_directive_on_target_line_disables() {
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            5,
            " @compiled-disable-line transform-css-prop",
        )]);
        assert!(is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn directive_two_lines_above_does_not_disable() {
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            3,
            " @compiled-disable-next-line transform-css-prop",
        )]);
        assert!(!is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn multi_line_path_skips_check() {
        // Upstream early-return: if the path spans multiple lines,
        // get_node_comments returns ({},{}) regardless of directives.
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            4,
            " @compiled-disable-next-line transform-css-prop",
        )]);
        assert!(!is_css_prop_disabled(&s, 5, 7));
    }

    #[test]
    fn directive_for_a_different_rule_does_not_disable_css_prop() {
        // Upstream `startsWith` requires the rule name to match.
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            4,
            " @compiled-disable-next-line transform-other-rule",
        )]);
        assert!(!is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn block_comments_do_not_disable() {
        // Upstream filters to CommentLine only. Block comments with
        // the same text are ignored.
        let mut s = State::default();
        s.set_comment_lines(vec![LineComment {
            start_line: 4,
            end_line: 4,
            kind: CommentKind::Block,
            text: " @compiled-disable-next-line transform-css-prop ".into(),
        }]);
        assert!(!is_css_prop_disabled(&s, 5, 5));
    }

    #[test]
    fn line_zero_or_mismatched_lines_returns_empty() {
        let mut s = State::default();
        s.set_comment_lines(vec![line_comment(
            1,
            " @compiled-disable-next-line transform-css-prop",
        )]);
        // line == 0 (no loc) returns empty per upstream's `!lineNumber` guard.
        assert!(!is_css_prop_disabled(&s, 0, 0));
    }
}
