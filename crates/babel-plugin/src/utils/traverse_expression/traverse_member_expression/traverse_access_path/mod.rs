//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/index.ts`.
//!
//! ```ts
//! export const traverseMemberAccessPath = (
//!   expression: t.Expression,
//!   meta: Metadata,
//!   expressionName: string,
//!   accessPath: t.Identifier[],
//!   memberExpression: t.MemberExpression,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   const { value: resolvedExpression, meta: updatedMeta } = resolveExpressionInMember(
//!     expression,
//!     meta,
//!     expressionName,
//!     memberExpression,
//!     evaluateExpression
//!   );
//!
//!   if (accessPath.length) {
//!     const pathName = accessPath[0].name;
//!     const result = evaluatePath(resolvedExpression, updatedMeta, pathName);
//!
//!     return traverseMemberAccessPath(
//!       result.value,
//!       result.meta,
//!       pathName,
//!       accessPath.slice(1),
//!       memberExpression,
//!       evaluateExpression
//!     );
//!   }
//!
//!   return createResultPair(resolvedExpression, updatedMeta);
//! };
//! ```

pub mod evaluate_path;
pub mod resolve_expression;

pub use evaluate_path::evaluate_path;
pub use resolve_expression::resolve_expression_in_member;

use swc_core::ecma::ast::{Expr, Ident, MemberExpr};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};

/// 1:1 port of `traverseMemberAccessPath`.
pub fn traverse_member_access_path<'a, F>(
    expression: &Expr,
    meta: &mut Metadata<'a>,
    expression_name: &str,
    access_path: &[Ident],
    member_expression: &MemberExpr,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let resolved_pair = resolve_expression_in_member(
        expression,
        meta,
        expression_name,
        member_expression,
        scope_index,
        parent_scope,
        own_scope,
        evaluate_expression,
    );
    let resolved_expression: Box<Expr> = resolved_pair
        .value
        .unwrap_or_else(|| Box::new(expression.clone()));

    if !access_path.is_empty() {
        let path_name = access_path[0].sym.as_str().to_string();
        let next_pair = evaluate_path(&resolved_expression, meta, &path_name);
        let next_expr: Box<Expr> = next_pair
            .value
            .unwrap_or_else(|| resolved_expression.clone());
        return traverse_member_access_path(
            &next_expr,
            meta,
            &path_name,
            &access_path[1..],
            member_expression,
            scope_index,
            parent_scope,
            own_scope,
            evaluate_expression,
        );
    }

    create_result_pair(Some(resolved_expression), meta)
}
