//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-unary-expression.ts`.
//!
//! ```ts
//! export const traverseUnaryExpression = (
//!   expression: t.UnaryExpression,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   const { operator, argument } = expression;
//!
//!   // If argument is already a numeric literal like -8 then skip
//!   if (operator === '-' && !hasNumericValue(argument)) {
//!     // Convert something like -getSpacing() to -1 * getSpacing()
//!     return createResultPair(
//!       t.binaryExpression('*', t.numericLiteral(-1), evaluateExpression(argument, meta).value),
//!       meta
//!     );
//!   }
//!
//!   return createResultPair(expression, meta);
//! };
//! ```
//!
//! ## SWC mapping notes
//!
//! * Babel `t.UnaryExpression.operator: '-' | '+' | '!' | '~' | 'typeof' | 'void' | 'delete' | 'throw'`
//!   → SWC `UnaryOp::{Minus, Plus, Bang, Tilde, TypeOf, Void, Delete}`.
//!   The JS check `operator === '-'` maps to `op == UnaryOp::Minus`.
//! * Babel `t.numericLiteral(-1)` → SWC
//!   `Expr::Lit(Lit::Num(Number { value: -1.0, .. }))`. Note: SWC
//!   represents numeric literals with `f64` only; `-1` round-trips
//!   correctly because `-1.0_f64` has an exact representation.
//! * Babel `t.binaryExpression('*', ...)` → SWC `BinExpr` with
//!   `op: BinaryOp::Mul`.
//!
//! ## JS-undefined fall-through
//!
//! `evaluateExpression(argument, meta).value` can be `undefined` when
//! the recursive evaluator can't fold the argument (most plausibly
//! a `traverse-function.ts` empty-body fallthrough propagating up).
//! In JS this would feed `undefined` into `t.binaryExpression('*', ...,
//! undefined)`, which Babel's AST builder validates at runtime and
//! throws `TypeError: Property right of BinaryExpression expected
//! node to be of a type ["Expression","PrivateName"] but instead
//! got undefined`. The Rust port preserves this crash semantically
//! via `.expect(...)` — see CLAUDE.md "BUGS in OLD = BUGS in NEW".
//! In practice this branch is unreachable on real fixtures because
//! `evaluateExpression` returns the input expression unchanged on
//! deopt rather than `undefined`.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{BinExpr, BinaryOp, Expr, Lit, Number, UnaryExpr, UnaryOp};

use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::has_numeric_value::has_numeric_value;

/// 1:1 port of `traverseUnaryExpression`.
pub fn traverse_unary_expression<'a, F>(
    expression: &UnaryExpr,
    meta: &mut Metadata<'a>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let UnaryExpr { op, arg, .. } = expression;

    // If argument is already a numeric literal like -8 then skip.
    if *op == UnaryOp::Minus && !has_numeric_value(arg) {
        // Convert something like -getSpacing() to -1 * getSpacing().
        let inner = evaluate_expression(arg, meta);
        // JS-undefined sneak-through would crash Babel's builder; the
        // Rust port mirrors with `.expect(...)`. See module docs.
        let right = inner.value.expect(
            "evaluateExpression returned undefined for unary minus argument — \
             matches JS Babel TypeError on t.binaryExpression('*', -1, undefined)",
        );
        let folded = Box::new(Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::Mul,
            left: Box::new(Expr::Lit(Lit::Num(Number {
                span: DUMMY_SP,
                value: -1.0,
                raw: None,
            }))),
            right,
        }));
        return create_result_pair(Some(folded), meta);
    }

    create_result_pair(Some(Box::new(Expr::Unary(expression.clone()))), meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::ecma::ast::{CallExpr, Callee, Ident, Str};

    fn make_meta_state() -> State {
        State::default()
    }

    fn unary(op: UnaryOp, arg: Box<Expr>) -> UnaryExpr {
        UnaryExpr {
            span: DUMMY_SP,
            op,
            arg,
        }
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

    fn ident_call(name: &str) -> Box<Expr> {
        Box::new(Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                name.into(),
                DUMMY_SP,
                Default::default(),
            )))),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        }))
    }

    fn identity_evaluator<'a>(expr: &Expr, meta: &mut Metadata<'a>) -> ResultPair {
        create_result_pair(Some(Box::new(expr.clone())), meta)
    }

    #[test]
    fn passes_through_unary_minus_with_numeric_argument() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        // `-8` — argument is a numeric literal already, so the
        // operator-minus branch is skipped and the input is returned
        // unchanged.
        let expr = unary(UnaryOp::Minus, num_lit(8.0));
        let mut eval = identity_evaluator;
        let pair = traverse_unary_expression(&expr, &mut meta, &mut eval);
        let result = pair.value.expect("result");
        match *result {
            Expr::Unary(u) => {
                assert_eq!(u.op, UnaryOp::Minus);
                assert!(matches!(*u.arg, Expr::Lit(Lit::Num(_))));
            }
            _ => panic!("expected Unary"),
        }
    }

    #[test]
    fn rewrites_unary_minus_with_non_numeric_argument_to_mul_by_minus_one() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        // `-getSpacing()` → `-1 * getSpacing()`.
        let expr = unary(UnaryOp::Minus, ident_call("getSpacing"));
        let mut eval = identity_evaluator;
        let pair = traverse_unary_expression(&expr, &mut meta, &mut eval);
        let result = pair.value.expect("result");
        match *result {
            Expr::Bin(b) => {
                assert_eq!(b.op, BinaryOp::Mul);
                match *b.left {
                    Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, -1.0),
                    _ => panic!("expected -1 numeric literal on left"),
                }
                assert!(matches!(*b.right, Expr::Call(_)));
            }
            _ => panic!("expected BinExpr"),
        }
    }

    #[test]
    fn passes_through_non_minus_unary_operators() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        // `!getSpacing()` — operator isn't `-`, branch skipped.
        let expr = unary(UnaryOp::Bang, ident_call("getSpacing"));
        let mut eval = identity_evaluator;
        let pair = traverse_unary_expression(&expr, &mut meta, &mut eval);
        let result = pair.value.expect("result");
        match *result {
            Expr::Unary(u) => assert_eq!(u.op, UnaryOp::Bang),
            _ => panic!("expected Unary"),
        }
    }

    #[test]
    fn passes_through_unary_minus_with_numeric_string_argument() {
        // JS `hasNumericValue("8") === true`, so the branch is skipped
        // for `-"8"` (an unusual shape but representable).
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        let expr = unary(UnaryOp::Minus, str_lit("8"));
        let mut eval = identity_evaluator;
        let pair = traverse_unary_expression(&expr, &mut meta, &mut eval);
        let result = pair.value.expect("result");
        assert!(matches!(*result, Expr::Unary(_)));
    }
}
