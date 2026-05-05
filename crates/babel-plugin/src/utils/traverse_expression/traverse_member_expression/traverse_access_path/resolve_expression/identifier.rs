//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/resolve-expression/identifier.ts`.
//!
//! ```ts
//! export const evaluateIdentifier = (
//!   expression: t.Identifier,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   const { name } = expression;
//!   const resolvedBinding = resolveBinding(name, meta, evaluateExpression);
//!
//!   if (resolvedBinding) {
//!     const { constant, node, meta: updatedMeta } = resolvedBinding;
//!
//!     if (constant && node) {
//!       return createResultPair(node as t.Expression, updatedMeta);
//!     }
//!   }
//!
//!   return createResultPair(expression, meta);
//! };
//! ```
//!
//! Cross-file scope swap: §5.6 wires the consumer at the dispatch
//! entry (`utils::evaluate_expression::dispatch_evaluate`). When the
//! input identifier resolves to a foldable cross-file import, the
//! §5.6 evaluator builds a fresh `ScopeIndex` from the imported
//! module's AST and recurses with that index BEFORE delegating into
//! the access-path chain that reaches this leaf. So this leaf's
//! same-file path always sees same-file scope info — see
//! `traverse_identifier.rs` module docs for the full design.

use swc_core::ecma::ast::{Expr, Ident};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::resolve_binding::resolve_binding;

/// 1:1 port of `evaluateIdentifier`.
pub fn evaluate_identifier<'a>(
    expression: &Ident,
    meta: &mut Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> ResultPair {
    let name = expression.sym.as_str();

    // If a fixture surfaces lazy-crawl observability here, see
    // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
    let resolved = resolve_binding(name, meta, scope_index, parent_scope, own_scope);

    if let Some(binding) = resolved {
        if binding.constant {
            if let Some(node) = binding.node {
                return create_result_pair(Some(node), meta);
            }
        }
    }

    // Fall-through: JS returns the input identifier unchanged
    // (`createResultPair(expression, meta)`). Rust mirrors with
    // a cloned `Expr::Ident`.
    create_result_pair(Some(Box::new(Expr::Ident(expression.clone()))), meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::scope::ScopeIndex;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap, DUMMY_SP};
    use swc_core::ecma::ast::{EsVersion, Module};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_module(src: &str) -> Module {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"))
    }

    #[test]
    fn resolves_const_identifier_to_init_expr() {
        let module = parse_module("const x = 42;");
        let scope_index = ScopeIndex::build(&module);
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let id = Ident::new("x".into(), DUMMY_SP, Default::default());
        let pair = evaluate_identifier(
            &id,
            &mut meta,
            &scope_index,
            scope_index.program_scope(),
            None,
        );
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(swc_core::ecma::ast::Lit::Num(n)) => assert_eq!(n.value, 42.0),
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn returns_input_identifier_when_unresolved() {
        let module = parse_module("");
        let scope_index = ScopeIndex::build(&module);
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let id = Ident::new("nope".into(), DUMMY_SP, Default::default());
        let pair = evaluate_identifier(
            &id,
            &mut meta,
            &scope_index,
            scope_index.program_scope(),
            None,
        );
        let v = pair.value.expect("value (unchanged input)");
        match *v {
            Expr::Ident(out) => assert_eq!(out.sym.as_str(), "nope"),
            other => panic!("expected identifier, got {other:?}"),
        }
    }
}
