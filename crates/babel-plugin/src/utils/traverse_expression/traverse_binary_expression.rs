//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-binary-expression.ts`.
//!
//! ```ts
//! export const traverseBinaryExpression = (
//!   expression: t.BinaryExpression,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   if (!t.isPrivateName(expression.left)) {
//!     const { value: left } = evaluateExpression(expression.left, meta);
//!     const { value: right } = evaluateExpression(expression.right, meta);
//!
//!     if (hasNumericValue(left) && hasNumericValue(right)) {
//!       return createResultPair(t.binaryExpression(expression.operator, left, right), meta);
//!     }
//!   }
//!
//!   return createResultPair(expression, meta);
//! };
//! ```
//!
//! ## SWC mapping notes
//!
//! * Babel `t.BinaryExpression.left: Expression | PrivateName` → SWC
//!   `BinExpr.left: Box<Expr>` where the PrivateName case is
//!   `Expr::PrivateName(_)`. The `t.isPrivateName(expression.left)`
//!   guard maps to `matches!(*expression.left, Expr::PrivateName(_))`.
//! * Babel `t.binaryExpression(operator, left, right)` constructs a
//!   `BinaryExpression` node with the binary-only operator subset.
//!   SWC `BinExpr` unifies binary + logical ops in one `BinaryOp` enum;
//!   here `expression.op` is the input binary expression's op (already
//!   a binary op since the input *is* a `BinExpr`), so the new
//!   `BinExpr` reuses it directly.
//!
//! ## Recursive evaluator parameter
//!
//! JS injects `evaluateExpression` as a parameter to break the
//! circular module dep between `traverse-expression/*` and
//! `evaluate-expression.ts`. Rust mirrors with a generic
//! `F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair` parameter; the
//! lifetime `'a` is bound to the caller's `Metadata` so the closure
//! can mutate state through the same `&mut State` reference.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{BinExpr, Expr};

use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::has_numeric_value::has_numeric_value;

/// 1:1 port of `traverseBinaryExpression`.
pub fn traverse_binary_expression<'a, F>(
    expression: &BinExpr,
    meta: &mut Metadata<'a>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let left_is_private_name = matches!(*expression.left, Expr::PrivateName(_));
    if !left_is_private_name {
        let left_pair = evaluate_expression(&expression.left, meta);
        let right_pair = evaluate_expression(&expression.right, meta);

        // JS destructures `{ value: left/right }`, then runs
        // `hasNumericValue(left) && hasNumericValue(right)`. When
        // either evaluator returned `undefined` (Rust None),
        // `hasNumericValue(undefined)` is false in JS — so the AND
        // short-circuits. The Rust analog is the `if let Some(...) ...`
        // pair gate; falls through to the unchanged-expression path
        // when either is None.
        if let (Some(left), Some(right)) = (left_pair.value.as_ref(), right_pair.value.as_ref()) {
            if has_numeric_value(left) && has_numeric_value(right) {
                let folded = Box::new(Expr::Bin(BinExpr {
                    span: DUMMY_SP,
                    op: expression.op,
                    left: left.clone(),
                    right: right.clone(),
                }));
                return create_result_pair(Some(folded), meta);
            }
        }
    }

    create_result_pair(Some(Box::new(Expr::Bin(expression.clone()))), meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use crate::utils::create_result_pair::ResultPair;
    use swc_core::ecma::ast::{BinaryOp, Lit, Number, Str};

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

    fn bin(op: BinaryOp, left: Box<Expr>, right: Box<Expr>) -> BinExpr {
        BinExpr {
            span: DUMMY_SP,
            op,
            left,
            right,
        }
    }

    /// Identity evaluator — returns the input expression as-is. Mirrors
    /// the deopt-path behaviour of `evaluate-expression.ts` for nodes
    /// it can't simplify.
    fn identity_evaluator<'a>(expr: &Expr, meta: &mut Metadata<'a>) -> ResultPair {
        create_result_pair(Some(Box::new(expr.clone())), meta)
    }

    #[test]
    fn folds_numeric_literal_pair() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = bin(BinaryOp::Add, num_lit(2.0), num_lit(3.0));
        let mut eval = identity_evaluator;
        let pair = traverse_binary_expression(&expr, &mut meta, &mut eval);
        let folded = pair.value.expect("folded result");
        match *folded {
            Expr::Bin(b) => {
                assert_eq!(b.op, BinaryOp::Add);
                assert!(matches!(*b.left, Expr::Lit(Lit::Num(_))));
                assert!(matches!(*b.right, Expr::Lit(Lit::Num(_))));
            }
            _ => panic!("expected BinExpr"),
        }
    }

    #[test]
    fn folds_numeric_string_pair() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = bin(BinaryOp::Add, str_lit("4"), str_lit("5"));
        let mut eval = identity_evaluator;
        let pair = traverse_binary_expression(&expr, &mut meta, &mut eval);
        // Numeric-string pair both pass `hasNumericValue` so the fold
        // reconstructs a BinExpr with the original (string) operands.
        let folded = pair.value.expect("folded result");
        match *folded {
            Expr::Bin(b) => {
                assert!(matches!(*b.left, Expr::Lit(Lit::Str(_))));
                assert!(matches!(*b.right, Expr::Lit(Lit::Str(_))));
            }
            _ => panic!("expected BinExpr"),
        }
    }

    #[test]
    fn deopts_when_either_operand_is_non_numeric_string() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = bin(BinaryOp::Add, num_lit(2.0), str_lit("hello"));
        let mut eval = identity_evaluator;
        let pair = traverse_binary_expression(&expr, &mut meta, &mut eval);
        // Falls through to `createResultPair(expression, meta)` —
        // returns the input expression unchanged.
        let result = pair.value.expect("result");
        assert!(matches!(*result, Expr::Bin(_)));
    }

    #[test]
    fn deopts_when_evaluator_returns_undefined() {
        let mut state = make_meta_state();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let expr = bin(BinaryOp::Add, num_lit(2.0), num_lit(3.0));
        // JS-undefined emulator: returns None.
        let mut eval = |_e: &Expr, m: &mut Metadata<'_>| create_result_pair(None, m);
        let pair = traverse_binary_expression(&expr, &mut meta, &mut eval);
        // Either-None gates the fold off; result is the unchanged
        // input BinExpr.
        let result = pair.value.expect("result");
        assert!(matches!(*result, Expr::Bin(_)));
    }
}
