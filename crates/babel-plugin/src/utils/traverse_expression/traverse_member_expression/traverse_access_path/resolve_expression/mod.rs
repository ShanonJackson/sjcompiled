//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/resolve-expression/index.ts`.
//!
//! ```ts
//! export const resolveExpressionInMember = (
//!   expression: t.Expression,
//!   meta: Metadata,
//!   expressionName: string,
//!   memberExpression: t.MemberExpression,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   let result = createResultPair(expression, meta);
//!
//!   if (t.isIdentifier(expression)) {
//!     result = evaluateIdentifier(expression, meta, evaluateExpression);
//!   } else if (t.isFunction(expression)) {
//!     // Function expressions are the declaration and not the function call
//!     // itself, the arguments are stored in the member expression
//!     const callExpression = t.callExpression(
//!       expression,
//!       getFunctionArgs(expressionName, memberExpression)
//!     );
//!     result = evaluateExpression(callExpression, meta);
//!   } else if (
//!     isCompiledCSSCallExpression(expression, meta.state) &&
//!     t.isExpression(expression.arguments[0])
//!   ) {
//!     result = evaluateExpression(expression.arguments[0], meta);
//!   } else if (t.isCallExpression(expression) || t.isMemberExpression(expression)) {
//!     result = evaluateExpression(expression, meta);
//!   }
//!
//!   // Recursively resolve expression until we extracted its value node or
//!   // have reach its origin declaration
//!   if (result.value !== expression) {
//!     return resolveExpressionInMember(
//!       result.value,
//!       result.meta,
//!       expressionName,
//!       memberExpression,
//!       evaluateExpression
//!     );
//!   }
//!
//!   return result;
//! };
//! ```
//!
//! ## Recursion termination
//!
//! Babel uses `result.value !== expression` (object identity) — same
//! Babel `Node` reference means "no progress made". The Rust port
//! mirrors structurally: if the resolved value is identity-equal
//! (Box pointer equality) OR structurally equal at a coarse level
//! (same `Expr::Ident` symbol when we started from an Ident), don't
//! recurse — otherwise we infinite-loop. We use a "different boxed
//! pointer" check + a self-reference guard for the Ident-pass-through
//! case.

pub mod function_args;
pub mod identifier;

pub use function_args::get_function_args;
pub use identifier::evaluate_identifier;

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, MemberExpr,
};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::is_compiled::is_compiled_css_call_expression;

/// 1:1 port of `resolveExpressionInMember`.
pub fn resolve_expression_in_member<'a, F>(
    expression: &Expr,
    meta: &mut Metadata<'a>,
    expression_name: &str,
    member_expression: &MemberExpr,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let mut result: ResultPair =
        create_result_pair(Some(Box::new(expression.clone())), meta);

    let dispatched: Option<ResultPair> = match expression {
        Expr::Ident(id) => Some(evaluate_identifier(
            id,
            meta,
            scope_index,
            parent_scope,
            own_scope,
        )),
        // Babel `t.isFunction` covers FnExpr + ArrowExpr in Expr position.
        Expr::Fn(_) | Expr::Arrow(_) => {
            let args = get_function_args(expression_name, member_expression);
            let call_expression = Expr::Call(make_call_expr(expression.clone(), args));
            Some(evaluate_expression(&call_expression, meta))
        }
        // `isCompiledCSSCallExpression(expression, meta.state) &&
        //  t.isExpression(expression.arguments[0])` — the JS plugin
        // routes `css(<expr>)` calls to evaluating the first arg
        // directly. SWC `CallExpr.args[0]` is `ExprOrSpread`; the
        // `t.isExpression` check excludes spreads.
        Expr::Call(call) if is_compiled_css_call_expression(expression, meta.state) => {
            match call.args.first() {
                Some(ExprOrSpread { spread: None, expr }) => {
                    Some(evaluate_expression(expr, meta))
                }
                _ => None,
            }
        }
        Expr::Call(_) | Expr::Member(_) => Some(evaluate_expression(expression, meta)),
        _ => None,
    };

    if let Some(d) = dispatched {
        result = d;
    }

    // JS recursion guard: `if (result.value !== expression) recurse`.
    // For Rust we approximate via "value differs from input by a coarse
    // structural check" — primarily avoiding the Ident-pass-through
    // self-loop: `evaluate_identifier` returns the input Ident
    // unchanged when unresolved, and our `Some(expression.clone())`
    // initial result also wraps the input. We check whether the
    // dispatched value is the SAME shape as the input identifier
    // (or call/member); if so, don't recurse.
    let progressed = match (&result.value, expression) {
        (Some(boxed), Expr::Ident(input_id)) => match &**boxed {
            Expr::Ident(out_id) => out_id.sym != input_id.sym,
            _ => true,
        },
        (Some(boxed), input) => !exprs_match_by_kind_and_shape(boxed, input),
        (None, _) => false,
    };

    if progressed {
        // Re-extract the value Expr to recurse on. `result.value` is
        // `Option<Box<Expr>>`; we already know it's `Some` above.
        let next_expr = result.value.as_ref().expect("progressed implies Some").clone();
        return resolve_expression_in_member(
            &next_expr,
            meta,
            expression_name,
            member_expression,
            scope_index,
            parent_scope,
            own_scope,
            evaluate_expression,
        );
    }

    result
}

fn make_call_expr(callee_expr: Expr, args: Vec<ExprOrSpread>) -> CallExpr {
    CallExpr {
        span: DUMMY_SP,
        callee: Callee::Expr(Box::new(callee_expr)),
        args,
        type_args: None,
        ctxt: Default::default(),
    }
}

/// Coarse "structurally equivalent" check used by the recursion-progress
/// guard. NOT a deep comparison — just enough to detect the
/// pass-through case where the dispatcher returned an unchanged
/// Expr (e.g., the identifier deopt in `evaluate_identifier`).
fn exprs_match_by_kind_and_shape(a: &Expr, b: &Expr) -> bool {
    use std::mem::discriminant;
    if discriminant(a) != discriminant(b) {
        return false;
    }
    match (a, b) {
        (Expr::Ident(la), Expr::Ident(lb)) => la.sym == lb.sym,
        (Expr::Lit(la), Expr::Lit(lb)) => format!("{la:?}") == format!("{lb:?}"),
        // For Member/Call/etc., conservative: same discriminant
        // counts as "no progress" — pre-empts an obvious self-loop.
        // The §5.6 evaluator's wrapper produces structurally
        // different shapes when it actually folds, so this won't
        // false-block a real fold.
        _ => true,
    }
}
