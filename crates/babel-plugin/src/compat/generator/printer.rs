//! 1:1 port of `@babel/generator@7.23.0/lib/printer.js` (the parts
//! reachable from our 5 call-site corpus).
//!
//! The full upstream is 651 LOC handling source maps, retain-lines /
//! retain-function-parens, compact / concise / minified flag matrix,
//! aux comments, a print stack used by `isFirstInContext`, etc.
//! We inherit only the byte-output contract — the corpus exercises a
//! tight subset, and forms outside it (Program-as-input,
//! ExpressionStatement, ClassDeclaration, etc.) belong to a separate
//! Drift event if they ever appear.
//!
//! Helper API kept verbatim: `word`, `space`, `token`, `tokenChar`,
//! `print`, `printList`, `printJoin`. Behaviour-defining state kept:
//! `_endsWithWord`, `_endsWithInteger` (token-collision avoidance).
//!
//! Comments: Babel preserves attached `_innerComments` /
//! `_leadingComments` / `_trailingComments` on each node. SWC stores
//! comments out-of-band in a `Comments` impl keyed by `BytePos`. The
//! corpus's `comment-*` fixtures parse the leading/trailing/inner
//! comments off the SWC tree manually and re-attach during printing
//! — see `comment_store.rs` (forthcoming, §4.3 follow-up). For the
//! initial port pass we skip comment emission entirely; the
//! comment-axis fixtures will fail until the comment store lands.

use super::buffer::Buffer;
use super::generators;
use super::node::parentheses;

use swc_core::common::comments::{Comment, CommentKind, Comments};
use swc_core::common::{BytePos, Spanned};
use swc_core::ecma::ast::Expr;

#[derive(Default)]
pub struct Format {
    /// Babel's `format.jsescOption.quotes`. We only honour `"double"`
    /// (the upstream default for source-output mode) for now.
    pub _quotes_double: bool,
}

pub struct Printer<'c> {
    pub buf: Buffer,
    pub format: Format,
    /// Token-collision guard: was the last appended token a word?
    /// Used by `word()` to insert a space if the next thing is also a
    /// word (so `return foo` doesn't collapse to `returnfoo`).
    pub ends_with_word: bool,
    /// Used by `tokenChar` / `token` to avoid `1.toString()` →
    /// `1.toString()` (which is an error — a `.` after an integer
    /// without a leading space could be parsed as a decimal point).
    pub ends_with_integer: bool,
    /// Current indent depth, in `indent_char` × `indent_repeat`
    /// units. Mirrors upstream `_indent`. `indent()` / `dedent()`
    /// adjust; `_maybe_indent()` runs on every queue/append call to
    /// auto-insert indentation after newlines.
    indent_depth: u32,
    indent_char: u8,
    indent_repeat: u32,
    /// SWC comment store. Comments were captured at parse time and
    /// keyed by `BytePos`. Babel's printer queries `node.leadingComments`
    /// / `node.trailingComments` from the AST itself; SWC stores
    /// out-of-band, so we query at every `print(node, ...)` boundary.
    /// `None` for callers that don't have a comment store (e.g.,
    /// synthetic-AST internal calls).
    comments: Option<&'c dyn Comments>,
    /// De-duplication: the same Comment can be reachable from both
    /// the trailing-position of one node and the leading-position of
    /// the next. Babel mirrors this with `_printedComments: Set`.
    /// We hash by `Span.lo` since BytePos uniquely identifies a comment.
    printed_comments: std::collections::HashSet<u32>,
}

impl<'c> Printer<'c> {
    pub fn new() -> Self {
        Self::with_comments(None)
    }

    pub fn with_comments(comments: Option<&'c dyn Comments>) -> Self {
        Self {
            buf: Buffer::new(),
            format: Format { _quotes_double: true },
            ends_with_word: false,
            ends_with_integer: false,
            indent_depth: 0,
            indent_char: b' ',
            indent_repeat: 2,
            comments,
            printed_comments: std::collections::HashSet::new(),
        }
    }

    pub fn finish(self) -> String {
        self.buf.get()
    }

    // ---------- Indent + newline ----------

    pub fn indent(&mut self) {
        self.indent_depth += 1;
    }

    pub fn dedent(&mut self) {
        if self.indent_depth > 0 {
            self.indent_depth -= 1;
        }
    }

    fn current_indent(&self) -> u32 {
        self.indent_depth * self.indent_repeat
    }

    /// `_maybeIndent(firstChar)` — if we're at indent depth > 0 AND
    /// the buffer's tail is `\n` AND we're not about to write another
    /// `\n`, queue the indent chars first. Called by every output
    /// primitive (`word`, `token`, `tokenChar`, `space`) before write.
    fn maybe_indent(&mut self, first_char: u8) {
        if self.indent_depth > 0 && first_char != b'\n' && self.buf.get_last_char() == b'\n' {
            let n = self.current_indent();
            self.buf.queue_indentation(self.indent_char, n);
        }
    }

    /// `newline(count)` — emit `count` newlines (max 2 in non-force
    /// mode; we ignore the cap since our caller `printList` only ever
    /// asks for 1). Also collapses with already-pending newlines via
    /// `getNewlineCount()` so we don't double-up.
    pub fn newline(&mut self, count: u32) {
        if count == 0 {
            return;
        }
        let existing = self.buf.get_newline_count();
        let needed = count.saturating_sub(existing);
        for _ in 0..needed {
            self.buf.queue(b'\n');
        }
        // Mirror upstream `_queue` — see `space()` for the rationale.
        // Without this, a `word(X)` immediately after a `newline()` (e.g.
        // `if (cond)\nelse foo;` shapes) would queue an extra space.
        self.ends_with_word = false;
        self.ends_with_integer = false;
    }

    // ---------- Output primitives ----------

    /// `space(force=false)` — emit a single space unless one is already
    /// at the buffer tail. `force=true` always emits.
    pub fn space(&mut self) {
        if self.buf.has_content() {
            let last = self.buf.get_last_char();
            if last != b' ' && last != b'\n' {
                self.buf.queue(b' ');
                // Mirror upstream `_queue`: every queue op resets the
                // word/integer collision-guard flags. Without this,
                // `word("return") + space() + word("__cmplp")` would
                // double-space because `word()` re-queues a space when
                // `ends_with_word` is still true.
                self.ends_with_word = false;
                self.ends_with_integer = false;
            }
        } else {
            self.buf.queue(b' ');
            self.ends_with_word = false;
            self.ends_with_integer = false;
        }
    }

    pub fn space_force(&mut self) {
        self.buf.queue(b' ');
        self.ends_with_word = false;
        self.ends_with_integer = false;
    }

    /// `word(str)` — emits an identifier-like word; inserts a leading
    /// space if the previous token was also a word.
    pub fn word(&mut self, s: &str) {
        if self.ends_with_word
            || (s.starts_with('/') && self.buf.get_last_char() == b'/')
        {
            self.buf.queue(b' ');
        }
        self.maybe_indent(s.as_bytes().first().copied().unwrap_or(0));
        self.buf.append(s);
        self.ends_with_word = true;
        self.ends_with_integer = false;
    }

    /// `number(str)` — emits a numeric literal token. Sets
    /// `ends_with_integer` on the way out so a following `.` triggers
    /// a space (`1 .toString()` style).
    pub fn number(&mut self, s: &str) {
        self.word(s);
        let bytes = s.as_bytes();
        let is_int = !s.contains(['e', 'E', '.'])
            && !s.starts_with("0x")
            && !s.starts_with("0X")
            && !s.starts_with("0b")
            && !s.starts_with("0B")
            && !s.starts_with("0o")
            && !s.starts_with("0O");
        let last = bytes.last().copied().unwrap_or(0);
        self.ends_with_integer = is_int && last != b'.';
    }

    /// `token(str)` — emits a multi-char punctuation token. Inserts
    /// a leading space when the previous tail char would token-collide
    /// (e.g., `+` after `+` becomes `++`, `-` after `-` becomes `--`).
    pub fn token(&mut self, s: &str) {
        let last = self.buf.get_last_char();
        let first = s.as_bytes().first().copied().unwrap_or(0);
        // `!` followed by `--` or `=` collides; `+` after `+`; `-` after `-`;
        // `.` after integer literal.
        let collide = (last == b'!' && (s == "--" || first == b'='))
            || (first == b'+' && last == b'+')
            || (first == b'-' && last == b'-')
            || (first == b'.' && self.ends_with_integer);
        if collide {
            self.buf.queue(b' ');
        }
        self.maybe_indent(first);
        self.buf.append(s);
        self.ends_with_word = false;
        self.ends_with_integer = false;
    }

    /// `semicolon(force=false)` — upstream's
    /// `printer.js::semicolon(force)`. Without `force` Babel queues the
    /// `;` so a subsequent same-token append collapses; we map both
    /// modes to a direct char emit because the buffer already de-dupes
    /// trailing semicolons via `_endsWith`. For our cluster the
    /// distinction collapses to a plain emit.
    pub fn semicolon(&mut self) {
        self.token_char(b';');
    }

    /// `semicolon(true)` — used by `EmptyStatement`.
    pub fn semicolon_force(&mut self) {
        self.token_char(b';');
    }

    /// `endsWith(charCode)` — upstream's printer state introspection
    /// used by IfStatement (insert space after `}` before `else`) and
    /// BlockStatement (suppress trailing newline if one is already at
    /// the buffer tail).
    pub fn ends_with(&self, c: u8) -> bool {
        self.buf.get_last_char() == c
    }

    /// `tokenChar(c)` — single-char punctuation.
    pub fn token_char(&mut self, c: u8) {
        let last = self.buf.get_last_char();
        let collide = (c == b'+' && last == b'+')
            || (c == b'-' && last == b'-')
            || (c == b'.' && self.ends_with_integer);
        if collide {
            self.buf.queue(b' ');
        }
        self.maybe_indent(c);
        self.buf.append_char(c);
        self.ends_with_word = false;
        self.ends_with_integer = false;
    }

    // ---------- Print dispatch ----------

    /// `print(node, parent)` — main expression dispatcher. Wraps the
    /// child in parens if the parent context demands it
    /// (`needsParens`), then delegates to the per-node generator.
    pub fn print(&mut self, node: &Expr, parent: Option<&Expr>) {
        // `Expr::Paren` is SWC's source-shape preservation node; Babel
        // flattens that layer (storing `extra.parenthesized` on the
        // inner). To match Babel's bytes we treat ParenExpr as
        // transparent — recurse on the inner with the SAME parent so
        // paren policy decides emit-or-drop on the flattened shape.
        // This is the fix for `(a && b) || c → a && b || c`.
        if let Expr::Paren(p_node) = node {
            self.print(&p_node.expr, parent);
            return;
        }

        let span = node.span();

        // Leading comments — Babel emits these BEFORE the node body.
        self.print_leading_comments_at(span.lo);

        let needs_parens = parent
            .map(|p| needs_parens_for(node, p))
            .unwrap_or(false);

        if needs_parens {
            self.token_char(b'(');
        }

        match node {
            Expr::Ident(n) => generators::types::ident(self, n),
            Expr::Lit(lit) => generators::types::literal(self, lit),
            Expr::Bin(b) => generators::expressions::binary(self, b, node),
            Expr::Cond(c) => generators::expressions::conditional(self, c, node),
            Expr::Unary(u) => generators::expressions::unary(self, u, node),
            Expr::Member(m) => generators::expressions::member(self, m, node),
            Expr::Call(c) => generators::expressions::call(self, c, node),
            Expr::Object(o) => generators::types::object(self, o),
            Expr::Array(a) => generators::types::array(self, a, node),
            Expr::Tpl(t) => generators::template_literals::tpl(self, t, node),
            Expr::TaggedTpl(t) => generators::template_literals::tagged_tpl(self, t, node),
            Expr::Arrow(a) => generators::expressions::arrow(self, a, node),
            Expr::Paren(_) => unreachable!("handled above"),
            Expr::JSXElement(e) => generators::jsx::jsx_element(self, e),
            Expr::JSXFragment(f) => generators::jsx::jsx_fragment(self, f),
            Expr::JSXEmpty(e) => generators::jsx::jsx_empty_expression(self, e),
            Expr::JSXMember(m) => generators::jsx::jsx_member_expression(self, m),
            Expr::JSXNamespacedName(n) => generators::jsx::jsx_namespaced_name(self, n),
            Expr::TsAs(t) => generators::typescript::ts_as_expr(self, t, node),
            Expr::TsSatisfies(t) => generators::typescript::ts_satisfies_expr(self, t, node),
            Expr::TsTypeAssertion(t) => generators::typescript::ts_type_assertion(self, t, node),
            Expr::TsNonNull(t) => generators::typescript::ts_non_null_expr(self, t, node),
            Expr::TsConstAssertion(t) => generators::typescript::ts_const_assertion(self, t, node),
            Expr::TsInstantiation(t) => generators::typescript::ts_instantiation(self, t, node),
            other => {
                let _ = other;
                self.buf.append("/*UNHANDLED-EXPR*/");
            }
        }

        if needs_parens {
            self.token_char(b')');
        }

        // Trailing comments — Babel emits these AFTER the node body
        // (and after any close-paren).
        self.print_trailing_comments_at(span.hi);
    }

    /// `printList(elements, parent, sep=", ")` — comma-separated print.
    pub fn print_list(&mut self, items: &[Box<Expr>], parent: &Expr) {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.token_char(b',');
                self.space();
            }
            self.print(item, Some(parent));
        }
    }

    // ---------- Comments ----------

    /// `_printLeadingComments(node)` analog. Queries the SWC comment
    /// store at `pos` (the node's `span.lo`).
    pub fn print_leading_comments_at(&mut self, pos: BytePos) {
        let comments = match self.comments {
            Some(c) => c,
            None => return,
        };
        if let Some(list) = comments.take_leading(pos) {
            for c in list {
                self.print_comment(&c);
            }
        }
    }

    /// `_printTrailingComments(node)` analog. SWC stores trailing
    /// comments at the node's `span.hi`.
    pub fn print_trailing_comments_at(&mut self, pos: BytePos) {
        let comments = match self.comments {
            Some(c) => c,
            None => return,
        };
        if let Some(list) = comments.take_trailing(pos) {
            for c in list {
                self.print_comment(&c);
            }
        }
    }

    /// `_printComment(comment)` — Babel's comment emission rules
    /// (lib/printer.js:519). Key invariants reproduced:
    /// - For block comments at non-`[` / non-`{` tail: emit a leading
    ///   space, then the `/* … */` body, NO trailing space (the next
    ///   token's emit handles its own leading-space policy).
    /// - For line comments: emit `// …` then a newline.
    /// - Skip if already printed (Babel's `_printedComments` Set).
    pub fn print_comment(&mut self, comment: &Comment) {
        // Dedup by Span.lo (BytePos uniquely identifies a comment).
        if !self.printed_comments.insert(comment.span.lo.0) {
            return;
        }

        let last = self.buf.get_last_char();
        // Babel's rule: insert a leading space unless the buffer's
        // tail is `[` or `{`. This is the policy that produces
        // `cond ? /* yes */'a-class' : 'b-class'` (no space between
        // `*/` and `'a-class'`) — the leading-space-before-comment
        // rule fires once before `/*`, but no trailing space is
        // emitted after `*/`. The next print() call's first
        // operation (e.g. `token('a-class')`) handles its own
        // leading-token policy.
        if last != b'[' && last != b'{' && self.buf.has_content() {
            self.space();
        }

        match comment.kind {
            CommentKind::Block => {
                let val = format!("/*{}*/", comment.text);
                // Honor indent: if buffer tail is `\n` and we're at
                // indent depth > 0, queue indent chars before the
                // comment body (matches upstream's `_maybeIndent`
                // running on every `_append` call).
                self.maybe_indent(val.as_bytes()[0]);
                self.buf.append(&val);
                self.ends_with_word = false;
                self.ends_with_integer = false;
            }
            CommentKind::Line => {
                let val = format!("//{}", comment.text);
                self.maybe_indent(val.as_bytes()[0]);
                self.buf.append(&val);
                // Line comments must be followed by a newline so the
                // following code isn't commented-out. Babel does
                // `newline(1, true)` (force) to enforce this even in
                // compact mode.
                self.newline(1);
                self.ends_with_word = false;
                self.ends_with_integer = false;
            }
        }
    }
}

/// `needsParens(node, parent)` — top-level dispatch. Returns true when
/// the child must be parenthesised given the parent's shape. Mirrors
/// `node/index.js::needsParens` (which dispatches on `node.type`).
pub fn needs_parens_for(child: &Expr, parent: &Expr) -> bool {
    match child {
        Expr::Bin(b) => {
            if parentheses::logical_needs_parens(b, parent) {
                return true;
            }
            // BinaryExpression-specific edge: `in` operator inside a for/var
            // declarator. Not reachable from our 5 call sites — covered for
            // completeness when it shows up.
            parentheses::binary_needs_parens(b, parent, child)
        }
        Expr::Cond(_) => parentheses::conditional_needs_parens(child as *const Expr, parent),
        Expr::Seq(_) => {
            // SequenceExpression in expression-position always needs parens
            // unless it's the immediate child of an ExpressionStatement /
            // For/Return/Throw etc. None of those are reachable from our
            // entry point (we always start at an Expression node, not a
            // Statement), so default to true.
            true
        }
        // TS expression wrappers — upstream `parentheses.js:165-167`
        // (`function TSAsExpression() { return true; }`) — these
        // unconditionally wrap in parens, regardless of parent. The
        // same handler covers TSAsExpression, TSSatisfiesExpression,
        // and TSTypeAssertion (re-exported on the same line in
        // `parentheses.js:22`).
        Expr::TsAs(_) | Expr::TsSatisfies(_) | Expr::TsTypeAssertion(_) => {
            let _ = parent;
            true
        }
        _ => false,
    }
}

impl<'c> Default for Printer<'c> {
    fn default() -> Self {
        Self::new()
    }
}
