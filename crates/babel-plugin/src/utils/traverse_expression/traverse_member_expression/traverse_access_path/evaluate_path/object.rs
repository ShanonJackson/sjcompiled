//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/evaluate-path/object.ts`.
//!
//! ```ts
//! export const evaluateObjectPath = (
//!   expression: t.ObjectExpression,
//!   meta: Metadata,
//!   propertyName: string
//! ): ReturnType<typeof createResultPair> => {
//!   const result = getObjectPropertyValue(expression, propertyName);
//!
//!   return createResultPair(result ? (result.node as t.Expression) : expression, meta);
//! };
//! ```
//!
//! Single dispatch into the §5.4e
//! [`crate::utils::traversers::get_object_property_value`] helper.

use swc_core::ecma::ast::{Expr, ObjectLit};

use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::traversers::get_object_property_value;

/// 1:1 port of `evaluateObjectPath`.
pub fn evaluate_object_path(
    expression: &ObjectLit,
    meta: &mut Metadata<'_>,
    property_name: &str,
) -> ResultPair {
    let result = get_object_property_value(expression, property_name);
    let value: Box<Expr> = match result.and_then(|r| r.node) {
        Some(node) => node,
        None => Box::new(Expr::Object(expression.clone())),
    };
    create_result_pair(Some(value), meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, ExprStmt, ModuleItem, Stmt};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_object_expression(src: &str) -> ObjectLit {
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
        let inner = match &**expr {
            Expr::Paren(p) => &p.expr,
            other => panic!("expected paren, got {other:?}"),
        };
        match &**inner {
            Expr::Object(o) => o.clone(),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn returns_value_for_known_property() {
        let obj = parse_object_expression(r#"({ red: 'r', blue: 'b' });"#);
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        let pair = evaluate_object_path(&obj, &mut meta, "blue");
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => {
                assert_eq!(s.value.to_atom_lossy().as_str(), "b");
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn returns_input_object_for_missing_property() {
        let obj = parse_object_expression(r#"({ red: 'r' });"#);
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        };
        let pair = evaluate_object_path(&obj, &mut meta, "blue");
        let v = pair.value.expect("value (unchanged)");
        // Falls through to `expression` per upstream — Object stays.
        assert!(matches!(*v, Expr::Object(_)));
    }
}
