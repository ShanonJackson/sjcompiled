//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/evaluate-path/index.ts`.
//!
//! ```ts
//! export const evaluatePath = (
//!   expression: t.Expression,
//!   meta: Metadata,
//!   pathName: string
//! ): ReturnType<typeof createResultPair> => {
//!   if (t.isObjectExpression(expression)) {
//!     return evaluateObjectPath(expression, meta, pathName);
//!   } else if (t.isTSAsExpression(expression)) {
//!     return evaluatePath(expression.expression, meta, pathName);
//!   } else if (t.isImportNamespaceSpecifier(expression)) {
//!     return evaluateNamespaceImportPath(expression, meta.state.file, meta, pathName);
//!   }
//!
//!   return createResultPair(expression, meta);
//! };
//! ```
//!
//! ## SWC mapping
//!
//! - Babel `t.isObjectExpression` → SWC `Expr::Object(_)`.
//! - Babel `t.isTSAsExpression` → SWC `Expr::TsAs(_)` (TS `as`
//!   wrapper). The `expression` field on `TsAsExpr` is the inner
//!   `Box<Expr>` — recurse into it.
//! - Babel `t.isImportNamespaceSpecifier` is structurally tricky on
//!   the SWC side: SWC parses `import * as theme from 'mod'` into
//!   an `ImportStarAsSpecifier` ModuleDecl item, NOT an `Expr`
//!   variant. There's no `Expr::ImportNamespaceSpecifier` analog
//!   that flows through `evaluatePath`. The JS `t.isImportNamespaceSpecifier(expression)`
//!   check works because Babel's `evaluateExpression` loop walks
//!   the binding's `path.node` which CAN be an
//!   `ImportNamespaceSpecifier` (a non-`Expression` Node sneaking
//!   through the `as t.Expression` cast). The Rust analog would
//!   require either:
//!   1. A dedicated marker `Expr` variant or sentinel (invention).
//!   2. Threading a sidecar discriminator from `traverse_access_path`
//!      that says "this resolved expr is from a namespace import,
//!      route via `evaluate_namespace_import_path`".
//!
//!   Phase 5 §5.6 ☑ chose option 3: route the namespace-import
//!   dispatch AT THE MEMBER-EXPRESSION ENTRY of
//!   `utils::evaluate_expression::dispatch_evaluate` (preflight via
//!   `try_namespace_import_dispatch`), so this dispatcher's
//!   `t.isImportNamespaceSpecifier`-equivalent branch stays
//!   unreachable by design. The [`namespace_import`] leaf body is
//!   real and reachable through the §5.6 preflight route.

pub mod namespace_import;
pub mod object;

pub use namespace_import::evaluate_namespace_import_path;
pub use object::evaluate_object_path;

use swc_core::ecma::ast::Expr;

use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};

/// 1:1 port of `evaluatePath`.
pub fn evaluate_path(expression: &Expr, meta: &mut Metadata<'_>, path_name: &str) -> ResultPair {
    match expression {
        Expr::Object(obj) => evaluate_object_path(obj, meta, path_name),
        // Babel's `t.isTSAsExpression` covers both `x as T` AND
        // `x as const`. SWC splits these into two AST variants, so
        // we recurse on either.
        Expr::TsAs(ts_as) => evaluate_path(&ts_as.expr, meta, path_name),
        Expr::TsConstAssertion(ts_const) => evaluate_path(&ts_const.expr, meta, path_name),
        // The `t.isImportNamespaceSpecifier` JS branch is unreachable
        // from this dispatcher — see module docs. When the §5.6
        // cross-file owner lands the sidecar discriminator, this
        // arm gets a real route.
        _ => create_result_pair(Some(Box::new(expression.clone())), meta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, ExprStmt, Lit, ModuleItem, Stmt};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_first_expr(src: &str) -> Box<Expr> {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"));
        let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = &module.body[0] else {
            panic!("expected expr stmt");
        };
        expr.clone()
    }

    #[test]
    fn dispatches_object_to_object_path() {
        let expr = parse_first_expr(r#"({ red: 'r' });"#);
        // Strip the outer Paren that the parser inserts for top-level
        // object expressions.
        let inner = match *expr {
            Expr::Paren(p) => p.expr,
            other => Box::new(other),
        };
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let pair = evaluate_path(&inner, &mut meta, "red");
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "r"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn unwraps_ts_as_expression() {
        // `({ red: 'r' } as Theme).red` — the outer is TsAsExpr; we
        // recurse into its inner.
        let expr = parse_first_expr(r#"({ red: 'r' } as { red: string });"#);
        let inner = match *expr {
            Expr::Paren(p) => p.expr,
            other => Box::new(other),
        };
        // inner is TsAsExpr.
        assert!(matches!(*inner, Expr::TsAs(_)));
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let pair = evaluate_path(&inner, &mut meta, "red");
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "r"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn passes_through_unknown_expression() {
        let expr = parse_first_expr("42;");
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let pair = evaluate_path(&expr, &mut meta, "anyName");
        let v = pair.value.expect("value");
        assert!(matches!(*v, Expr::Lit(Lit::Num(_))));
    }
}
