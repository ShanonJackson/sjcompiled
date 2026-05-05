//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-function.ts`.
//!
//! ```ts
//! export const traverseFunction = (
//!   expression: t.Function,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   let value: t.Node | undefined | null = undefined;
//!   let updatedMeta: Metadata = meta;
//!
//!   if (t.isBlockStatement(expression.body)) {
//!     traverse(expression.body, {
//!       noScope: true,
//!       ReturnStatement(path) {
//!         const { argument } = path.node;
//!
//!         if (argument) {
//!           ({ value, meta: updatedMeta } = evaluateExpression(argument, meta));
//!         }
//!
//!         path.stop();
//!       },
//!     });
//!   } else {
//!     ({ value, meta: updatedMeta } = evaluateExpression(expression.body, meta));
//!   }
//!
//!   return createResultPair(value as t.Expression, updatedMeta);
//! };
//! ```
//!
//! ## SWC mapping notes
//!
//! * Babel `t.Function` → SWC `Expr::Fn(FnExpr)` or `Expr::Arrow(ArrowExpr)`.
//!   These are the only function-like `Expr` variants `evaluate-expression.ts`
//!   dispatches into via `t.isFunction(targetExpression)` — function
//!   declarations / object methods / class methods are statements or
//!   properties, not Exprs, and never reach this function.
//! * Babel `expression.body: t.BlockStatement | t.Expression` →
//!   * `FnExpr.function.body: Option<BlockStmt>` — always `Some` in practice;
//!     the `Option` accommodates TS overload declarations which are FnDecl,
//!     not FnExpr.
//!   * `ArrowExpr.body: Box<BlockStmtOrExpr>` — either branch reachable.
//! * Babel `traverse(node, { noScope: true, ReturnStatement(path) { ... path.stop(); } })`
//!   → SWC `Visit` impl with a `done` flag in `visit_return_stmt`. SWC's
//!   default child-traversal walks into nested function bodies (matching
//!   Babel's default behaviour), so a `ReturnStatement` nested in an inner
//!   function fires the same DFS pre-order as Babel — the FIRST hit wins,
//!   subsequent ReturnStmts are early-returned. The argument of the first
//!   ReturnStmt is captured by clone (cheap — `Option<Box<Expr>>`); we then
//!   call `evaluate_expression` on it AFTER the walk, sidestepping the
//!   `&mut self` aliasing problem of nesting closure calls inside a
//!   `Visit` callback.
//!
//! ## JS-undefined fall-through
//!
//! The JS `let value = undefined` initial state plus `let updatedMeta =
//! meta` mirrors a "no return found" / "non-block body" deopt path. The
//! `value as t.Expression` cast at the bottom is a TS lie — at runtime
//! `value` may be JS `undefined`. Rust models this with
//! `ResultPair { value: Option<Box<Expr>> }`; a None value propagates
//! the JS-undefined semantics into downstream consumers (see
//! `create_result_pair.rs` module docs for the full reasoning).
//!
//! `meta` is threaded by `&mut` reference; the JS "returned meta" is the
//! same object. The Rust port mutates state through `meta` in place; the
//! JS `updatedMeta = meta` reassignment is a no-op at the reference level
//! and has no Rust analog.

use swc_core::ecma::ast::{BlockStmt, BlockStmtOrExpr, Expr, ReturnStmt};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};

/// 1:1 port of `traverseFunction`. Accepts an `Expr` of variant
/// `Fn` or `Arrow`; other variants are treated as the JS-undefined
/// fall-through (value stays `None`).
pub fn traverse_function<'a, F>(
    expression: &Expr,
    meta: &mut Metadata<'a>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let body_kind: Option<BodyKind<'_>> = match expression {
        Expr::Fn(fn_expr) => fn_expr.function.body.as_ref().map(BodyKind::Block),
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(b) => Some(BodyKind::Block(b)),
            BlockStmtOrExpr::Expr(e) => Some(BodyKind::Expr(e)),
        },
        _ => None,
    };

    let value: Option<Box<Expr>> = match body_kind {
        Some(BodyKind::Block(block)) => {
            // Babel `traverse(body, { noScope: true, ReturnStatement })`
            // analog: SWC `Visit` walks the BlockStmt, captures the
            // first `ReturnStmt`'s argument and short-circuits via the
            // `done` flag.
            let mut finder = FirstReturnFinder {
                captured_arg: None,
                done: false,
            };
            block.visit_with(&mut finder);
            finder
                .captured_arg
                .map(|arg| evaluate_expression(&arg, meta).value)
                .unwrap_or(None)
        }
        Some(BodyKind::Expr(expr)) => evaluate_expression(expr, meta).value,
        None => None,
    };

    create_result_pair(value, meta)
}

/// Internal body-kind discriminator. Mirrors Babel's
/// `t.BlockStatement | t.Expression` body shape after JS dispatches
/// on `t.isBlockStatement(expression.body)`.
enum BodyKind<'a> {
    Block(&'a BlockStmt),
    Expr(&'a Expr),
}

/// SWC analog of Babel's `traverse(body, { ReturnStatement })` +
/// `path.stop()`. Captures the first `ReturnStmt`'s argument by clone
/// and short-circuits on subsequent encounters via the `done` flag.
///
/// SWC's default `visit_*_children_with` recursion descends into
/// nested function bodies (matching Babel's `traverse` default), so
/// the DFS pre-order ordering of "first ReturnStmt encountered" is
/// preserved — including the case where a nested arrow's
/// `return` precedes the outer function's `return` in source order.
/// This is a 1:1 behavioural inheritance of upstream's `path.stop()`
/// semantics; see CLAUDE.md "BUGS in OLD = BUGS in NEW".
struct FirstReturnFinder {
    captured_arg: Option<Box<Expr>>,
    done: bool,
}

impl Visit for FirstReturnFinder {
    fn visit_return_stmt(&mut self, n: &ReturnStmt) {
        if self.done {
            return;
        }
        // Mirrors JS `if (argument) { ... }` — only capture when the
        // ReturnStmt has an argument. `return;` (no argument) leaves
        // `captured_arg = None` and still sets `done`, mirroring
        // `path.stop()` after a no-op `if`.
        self.captured_arg = n.arg.clone();
        self.done = true;
        // Intentionally do NOT call `n.visit_children_with(self)` —
        // Babel's `path.stop()` halts further descent. Our flag-based
        // short-circuit gives the same effect across siblings; not
        // descending into the argument here matches the
        // "no traversal past the stopped node" contract.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{
        ArrowExpr, BlockStmtOrExpr, FnExpr, Function, IfStmt, Lit, Number, Stmt, Str,
    };

    fn make_meta_state() -> State {
        State::default()
    }

    fn num_lit(value: f64) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value,
            raw: None,
        })))
    }

    fn str_lit(value: &str) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        })))
    }

    fn return_stmt(arg: Option<Box<Expr>>) -> Stmt {
        Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg,
        })
    }

    fn block(stmts: Vec<Stmt>) -> BlockStmt {
        BlockStmt {
            span: DUMMY_SP,
            stmts,
            ctxt: Default::default(),
        }
    }

    fn fn_expr_with_block(stmts: Vec<Stmt>) -> Expr {
        Expr::Fn(FnExpr {
            ident: None,
            function: Box::new(Function {
                params: vec![],
                decorators: vec![],
                span: DUMMY_SP,
                body: Some(block(stmts)),
                is_generator: false,
                is_async: false,
                type_params: None,
                return_type: None,
                ctxt: Default::default(),
            }),
        })
    }

    fn arrow_with_block(stmts: Vec<Stmt>) -> Expr {
        Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: vec![],
            body: Box::new(BlockStmtOrExpr::BlockStmt(block(stmts))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        })
    }

    fn arrow_with_expr_body(body: Box<Expr>) -> Expr {
        Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: vec![],
            body: Box::new(BlockStmtOrExpr::Expr(body)),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        })
    }

    fn identity_evaluator<'a>(expr: &Expr, meta: &mut Metadata<'a>) -> ResultPair {
        create_result_pair(Some(Box::new(expr.clone())), meta)
    }

    #[test]
    fn captures_first_return_argument_in_block_body() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = fn_expr_with_block(vec![return_stmt(Some(num_lit(10.0)))]);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        let v = pair.value.expect("captured value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 10.0),
            _ => panic!("expected numeric literal"),
        }
    }

    #[test]
    fn arrow_with_concise_expr_body_evaluates_directly() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = arrow_with_expr_body(num_lit(42.0));
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        let v = pair.value.expect("captured value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 42.0),
            _ => panic!("expected numeric literal"),
        }
    }

    #[test]
    fn empty_block_body_yields_js_undefined_value() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = fn_expr_with_block(vec![]);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        // No ReturnStmt → JS `value` remains `undefined` → Rust None.
        assert!(pair.value.is_none());
    }

    #[test]
    fn return_without_argument_yields_js_undefined_value() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        // `return;` — no argument. JS captures `argument === undefined`,
        // skips the `if (argument)` body, calls `path.stop()`. value
        // stays JS undefined → Rust None.
        let expr = fn_expr_with_block(vec![return_stmt(None)]);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        assert!(pair.value.is_none());
    }

    #[test]
    fn first_return_in_dfs_order_wins_over_later_siblings() {
        // function () { if (x) { return 'inner'; } return 'outer'; }
        // Babel pre-order DFS visits IfStmt → consequent BlockStmt →
        // ReturnStmt('inner') first, captures 'inner', stops. Outer
        // return is never captured.
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let inner_return = return_stmt(Some(str_lit("inner")));
        let outer_return = return_stmt(Some(str_lit("outer")));
        let if_stmt = Stmt::If(IfStmt {
            span: DUMMY_SP,
            test: Box::new(Expr::Lit(Lit::Bool(swc_core::ecma::ast::Bool {
                span: DUMMY_SP,
                value: true,
            }))),
            cons: Box::new(Stmt::Block(block(vec![inner_return]))),
            alt: None,
        });
        let expr = fn_expr_with_block(vec![if_stmt, outer_return]);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        let v = pair.value.expect("captured value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(&*s.value, "inner"),
            _ => panic!("expected string literal"),
        }
    }

    #[test]
    fn arrow_with_block_body_walks_for_first_return() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = arrow_with_block(vec![return_stmt(Some(num_lit(7.0)))]);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        let v = pair.value.expect("captured value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 7.0),
            _ => panic!("expected numeric literal"),
        }
    }

    #[test]
    fn evaluator_invoked_at_most_once_when_first_return_short_circuits() {
        // Validates `path.stop()` semantics — even if multiple
        // ReturnStatements exist, the recursive evaluator runs exactly
        // once (on the first capture). State-mutating evaluators in
        // the §5.6 chain rely on this.
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = fn_expr_with_block(vec![
            return_stmt(Some(num_lit(1.0))),
            return_stmt(Some(num_lit(2.0))),
        ]);
        let mut call_count = 0u32;
        let mut eval = |e: &Expr, m: &mut Metadata<'_>| {
            call_count += 1;
            create_result_pair(Some(Box::new(e.clone())), m)
        };
        let _ = traverse_function(&expr, &mut meta, &mut eval);
        assert_eq!(call_count, 1, "evaluator must run exactly once");
    }

    #[test]
    fn non_function_expr_yields_js_undefined_value() {
        // Defensive shape: caller dispatching is supposed to filter
        // on Fn/Arrow, but if a non-function Expr reaches here we
        // mirror JS undefined-value (no AST shape recognition possible).
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = *num_lit(5.0);
        let mut eval = identity_evaluator;
        let pair = traverse_function(&expr, &mut meta, &mut eval);
        assert!(pair.value.is_none());
    }
}
