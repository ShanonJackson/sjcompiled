//! 1:1 port of `@babel/generator@7.23.0/lib/generators/{base,statements}.js`
//! reachable from arrow function block bodies in our corpus.
//!
//! Surfaced by §6.8e (styled/behaviour cluster): `props => { return X; }`
//! shaped arrows landed BlockStmt nodes through `expressions.rs::arrow`,
//! which previously emitted the placeholder `/*UNHANDLED-BLOCK*/` and
//! produced a divergent hash from upstream. This module ports the
//! BlockStatement printer and the Statement variants reachable from the
//! styled / css-prop dynamic-arrow-body cluster.
//!
//! Coverage scope: `BlockStatement`, `ReturnStatement`, `ThrowStatement`,
//! `ExpressionStatement`, `IfStatement`. Other statement variants emit a
//! distinct placeholder per kind so future fixtures surface as their own
//! cluster (drift-detection per CLAUDE.md). Port them 1:1 from upstream
//! when surfaced.

use swc_core::ecma::ast::{BlockStmt, Expr, IfStmt, ReturnStmt, Stmt, ThrowStmt};

use super::super::printer::Printer;

/// 1:1 port of `base.js::BlockStatement`.
///
/// ```js
/// function BlockStatement(node) {
///   this.tokenChar(123);  // {
///   if (node.body.length || node.directives.length) {
///     this.printSequence(node.directives, node, { indent: true, ... });
///     this.printSequence(node.body, node, { indent: true });
///     this.rightBrace(node);
///   } else {
///     this.printInnerComments();
///     this.tokenChar(125);
///   }
/// }
/// ```
///
/// SWC's `BlockStmt` has no `directives` field (Directive prologues land
/// inline as `Stmt::Expr(ExprStmt(Lit::Str))` with the directive flag).
/// For our cluster the body never carries a real directive prologue —
/// upstream's directives array is always empty. We thus take the body-
/// only branch.
pub fn block_statement(p: &mut Printer, node: &BlockStmt) {
    p.token_char(b'{');
    if !node.stmts.is_empty() {
        // upstream `printSequence(body, { indent: true })`:
        //   indent();
        //   for stmt: newline(); print(stmt);
        //   dedent();
        //   if (!endsWith('\n')) newline();
        p.indent();
        for stmt in &node.stmts {
            p.newline(1);
            print_statement(p, stmt);
        }
        p.dedent();
        if !p.ends_with(b'\n') {
            p.newline(1);
        }
    }
    p.token_char(b'}');
}

/// Statement dispatcher. Mirrors upstream's `print(node, parent)` when
/// dispatched on a Stmt-typed node (the registry is built dynamically
/// from each generator file's `exports`). Statements that don't print
/// expressions inside themselves don't need a parent; statements that
/// do (e.g. ReturnStatement.argument) pass `None` because the
/// expression-needsParens policy keys off Expr-shaped parents only.
pub fn print_statement(p: &mut Printer, stmt: &Stmt) {
    match stmt {
        Stmt::Block(b) => block_statement(p, b),
        Stmt::Return(r) => return_statement(p, r),
        Stmt::Throw(t) => throw_statement(p, t),
        Stmt::Expr(e) => expression_statement(p, &e.expr),
        Stmt::If(i) => if_statement(p, i),
        Stmt::Empty(_) => p.semicolon_force(),
        // Per CLAUDE.md drift-detection: emit a distinct placeholder
        // per kind so unported variants surface as their own cluster.
        // Port from upstream `base.js` / `statements.js` 1:1 when a
        // fixture lands here.
        Stmt::Debugger(_) => p.buf.append("/*UNHANDLED-STMT-DEBUGGER*/"),
        Stmt::With(_) => p.buf.append("/*UNHANDLED-STMT-WITH*/"),
        Stmt::Labeled(_) => p.buf.append("/*UNHANDLED-STMT-LABELED*/"),
        Stmt::Break(_) => p.buf.append("/*UNHANDLED-STMT-BREAK*/"),
        Stmt::Continue(_) => p.buf.append("/*UNHANDLED-STMT-CONTINUE*/"),
        Stmt::Switch(_) => p.buf.append("/*UNHANDLED-STMT-SWITCH*/"),
        Stmt::Try(_) => p.buf.append("/*UNHANDLED-STMT-TRY*/"),
        Stmt::While(_) => p.buf.append("/*UNHANDLED-STMT-WHILE*/"),
        Stmt::DoWhile(_) => p.buf.append("/*UNHANDLED-STMT-DOWHILE*/"),
        Stmt::For(_) => p.buf.append("/*UNHANDLED-STMT-FOR*/"),
        Stmt::ForIn(_) => p.buf.append("/*UNHANDLED-STMT-FORIN*/"),
        Stmt::ForOf(_) => p.buf.append("/*UNHANDLED-STMT-FOROF*/"),
        Stmt::Decl(_) => p.buf.append("/*UNHANDLED-STMT-DECL*/"),
    }
}

/// 1:1 port of `statements.js::ReturnStatement`.
///
/// ```js
/// function ReturnStatement(node) {
///   this.word("return");
///   printStatementAfterKeyword(this, node.argument, node, false);
/// }
/// // printStatementAfterKeyword:
/// //   if (node) { printer.space(); printer.printTerminatorless(node, parent, isLabel); }
/// //   printer.semicolon();
/// ```
///
/// `printTerminatorless` preserves no-line-terminator semantics for
/// `return <expr>` (a newline between `return` and `<expr>` would be an
/// ASI hazard). Babel's implementation calls
/// `_printForLineTerminatorPrevention(arg, parent, isLabel)` which
/// disables the sourcemap-driven retainLines for the expression. Our
/// printer doesn't track retainLines / source positions, so the
/// distinction collapses to plain `print(arg, None)`.
pub fn return_statement(p: &mut Printer, node: &ReturnStmt) {
    p.word("return");
    if let Some(arg) = &node.arg {
        p.space();
        p.print(arg, None);
    }
    p.semicolon();
}

/// 1:1 port of `statements.js::ThrowStatement`.
///
/// ```js
/// function ThrowStatement(node) {
///   this.word("throw");
///   printStatementAfterKeyword(this, node.argument, node, false);
/// }
/// ```
pub fn throw_statement(p: &mut Printer, node: &ThrowStmt) {
    p.word("throw");
    p.space();
    p.print(&node.arg, None);
    p.semicolon();
}

/// 1:1 port of `expressions.js::ExpressionStatement`.
///
/// ```js
/// function ExpressionStatement(node) {
///   this.print(node.expression, node);
///   this.semicolon();
/// }
/// ```
pub fn expression_statement(p: &mut Printer, expr: &Expr) {
    p.print(expr, None);
    p.semicolon();
}

/// 1:1 port of `statements.js::IfStatement`.
///
/// ```js
/// function IfStatement(node) {
///   this.word("if");
///   this.space();
///   this.tokenChar(40);  // (
///   this.print(node.test, node);
///   this.tokenChar(41);  // )
///   this.space();
///   const needsBlock = node.alternate && isIfStatement(getLastStatement(node.consequent));
///   if (needsBlock) { tokenChar 123; newline(); indent(); }
///   this.printAndIndentOnComments(node.consequent, node);
///   if (needsBlock) { dedent(); newline(); tokenChar 125; }
///   if (node.alternate) {
///     if (this.endsWith(125)) this.space();
///     this.word("else"); this.space();
///     this.printAndIndentOnComments(node.alternate, node);
///   }
/// }
/// ```
///
/// `printAndIndentOnComments` is `print` plus a comment-driven indent
/// — without a comment store at the stmt level we collapse to plain
/// `print_statement`. The `needsBlock` branch handles `if-else-if` ASI
/// where the consequent ends in another IfStatement; we mirror it.
pub fn if_statement(p: &mut Printer, node: &IfStmt) {
    p.word("if");
    p.space();
    p.token_char(b'(');
    p.print(&node.test, None);
    p.token_char(b')');
    p.space();

    let needs_block = node.alt.is_some() && consequent_ends_in_if(&node.cons);
    if needs_block {
        p.token_char(b'{');
        p.newline(1);
        p.indent();
    }
    print_statement(p, &node.cons);
    if needs_block {
        p.dedent();
        p.newline(1);
        p.token_char(b'}');
    }
    if let Some(alt) = &node.alt {
        if p.ends_with(b'}') {
            p.space();
        }
        p.word("else");
        p.space();
        print_statement(p, alt);
    }
}

/// `getLastStatement(stmt)`: walks down `body` until reaching a non-
/// Statement-bodied node, then returns it. Used to decide whether the
/// consequent ends in an IfStatement (ASI-prone shape).
fn consequent_ends_in_if(stmt: &Stmt) -> bool {
    let mut current = stmt;
    loop {
        match current {
            Stmt::If(_) => return true,
            Stmt::With(w) => current = &w.body,
            Stmt::Labeled(l) => current = &l.body,
            Stmt::While(w) => current = &w.body,
            Stmt::DoWhile(d) => current = &d.body,
            Stmt::For(f) => current = &f.body,
            Stmt::ForIn(f) => current = &f.body,
            Stmt::ForOf(f) => current = &f.body,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::generator::generate_stmt;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{
        BindingIdent, BlockStmt, BlockStmtOrExpr, Expr, Ident, IdentName, MemberExpr, MemberProp,
        Pat, ReturnStmt, Stmt,
    };

    fn ident_expr(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(Ident::new(name.into(), DUMMY_SP, Default::default())))
    }

    /// `__cmplp => { return __cmplp.color; }` should hash to the
    /// upstream-target `63bh2t` (post-`normalize_props_usage` shape).
    /// Test the BlockStmt portion: `{ return __cmplp.color; }`.
    #[test]
    fn block_with_return_member_expr() {
        let body = Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: ident_expr("__cmplp"),
                prop: MemberProp::Ident(IdentName::new("color".into(), DUMMY_SP)),
            }))),
        });
        let block = BlockStmt {
            span: DUMMY_SP,
            stmts: vec![body],
            ctxt: Default::default(),
        };
        let out = generate_stmt(&Stmt::Block(block));
        assert_eq!(out, "{\n  return __cmplp.color;\n}");
    }

    /// Empty block `{}` — just open/close.
    #[test]
    fn empty_block_no_directives() {
        let block = BlockStmt { span: DUMMY_SP, stmts: vec![], ctxt: Default::default() };
        assert_eq!(generate_stmt(&Stmt::Block(block)), "{}");
    }

    /// Bare `return;` (no argument).
    #[test]
    fn return_no_argument() {
        let r = Stmt::Return(ReturnStmt { span: DUMMY_SP, arg: None });
        assert_eq!(generate_stmt(&r), "return;");
    }

    /// `return __cmplp.color;` — the dominant shape in styled/behaviour.
    #[test]
    fn return_member_expr() {
        let r = Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: ident_expr("__cmplp"),
                prop: MemberProp::Ident(IdentName::new("color".into(), DUMMY_SP)),
            }))),
        });
        assert_eq!(generate_stmt(&r), "return __cmplp.color;");
    }

    /// Hash parity check — confirms the byte output matches the upstream
    /// hash target identified during §6.8e investigation. The hash
    /// `63bh2t` was confirmed via `bun parity-harness/babel-plugin/probe-hash.mjs`
    /// to correspond to `"__cmplp => {\n  return __cmplp.color;\n}"`.
    #[test]
    fn arrow_block_body_byte_target() {
        use compiled_utils::hash;
        use swc_core::ecma::ast::{ArrowExpr, BlockStmtOrExpr};

        let body = Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: ident_expr("__cmplp"),
                prop: MemberProp::Ident(IdentName::new("color".into(), DUMMY_SP)),
            }))),
        });
        let arrow = Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: vec![Pat::Ident(BindingIdent {
                id: Ident::new("__cmplp".into(), DUMMY_SP, Default::default()),
                type_ann: None,
            })],
            body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                stmts: vec![body],
                ctxt: Default::default(),
            })),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        });
        let printed = crate::compat::generator::generate(&arrow);
        assert_eq!(printed, "__cmplp => {\n  return __cmplp.color;\n}");
        assert_eq!(hash(&printed), "63bh2t");
    }
}

