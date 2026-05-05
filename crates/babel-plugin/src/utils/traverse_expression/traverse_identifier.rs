//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-identifier.ts`.
//!
//! ```ts
//! export const traverseIdentifier = (
//!   expression: t.Identifier,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   let value: t.Node | undefined | null = undefined;
//!   let updatedMeta: Metadata = meta;
//!
//!   const resolvedBinding = resolveBinding(expression.name, updatedMeta, evaluateExpression);
//!
//!   if (resolvedBinding && resolvedBinding.constant && resolvedBinding.node) {
//!     ({ value, meta: updatedMeta } = evaluateExpression(
//!       resolvedBinding.node as t.Expression,
//!       resolvedBinding.meta
//!     ));
//!   }
//!
//!   return createResultPair(value as t.Expression, updatedMeta);
//! };
//! ```
//!
//! ## Cross-file scope swap — §5.6 wires the consumer
//!
//! The JS port returns `resolved.meta` from `resolveBinding` for
//! cross-file resolutions: a fresh `Metadata` whose `parentPath`
//! points into the imported module's AST. The §5.5 recursive
//! `evaluateExpression(node, resolved.meta)` re-enters with that
//! imported-file context, so any `getBinding` lookups inside the
//! recursion target the imported file's scope.
//!
//! The §5.4e Rust port drops this cross-file `meta` synthesis
//! (documented at `utils/types.rs:115-145`); cross-file scope
//! routing happens at the §5.6 evaluator boundary instead. The
//! §5.6 dispatcher (`utils::evaluate_expression::dispatch_evaluate`)
//! detects `binding.source == Import &&
//! binding.imported_module.is_some() && binding.node.is_some()` AT
//! THE IDENTIFIER ENTRY and recurses with a fresh `ScopeIndex`
//! built over the imported module BEFORE delegating to this leaf.
//! The leaf's same-file path therefore always sees same-file scope
//! info — no cross-file misroute.
//!
//! For this leaf's recursive `evaluate_expression` call on a
//! same-file binding, scope info is the caller's by design.

use swc_core::ecma::ast::{Expr, Ident};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::resolve_binding::resolve_binding;

/// 1:1 port of `traverseIdentifier`.
///
/// Takes `scope_index` / `parent_scope` / `own_scope` as explicit
/// parameters — JS derives them from `meta.parentPath.scope`, but
/// the Rust `Metadata` doesn't carry scope refs (would require a
/// new lifetime parameter on `Metadata` and reach across the entire
/// callgraph).
pub fn traverse_identifier<'a, F>(
    expression: &Ident,
    meta: &mut Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let name = expression.sym.as_str();

    // If a fixture surfaces lazy-crawl observability here, see
    // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
    let resolved = resolve_binding(name, meta, scope_index, parent_scope, own_scope);

    if let Some(binding) = resolved {
        if binding.constant {
            if let Some(node) = binding.node {
                // JS: `evaluateExpression(resolvedBinding.node, resolvedBinding.meta)`.
                // Rust: pass the same `meta` — the §5.6 dispatcher detects
                // cross-file resolutions BEFORE delegating here and recurses
                // with a fresh imported `ScopeIndex`, so this leaf only ever
                // sees same-file folds. See module docs.
                let result = evaluate_expression(&node, meta);
                return create_result_pair(result.value, meta);
            }
        }
    }

    // Fall-through: JS `value as t.Expression` is `undefined` when
    // resolution / constancy / node-presence fails. Mirror with
    // `Option::None`.
    create_result_pair(None, meta)
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

    fn identity_evaluator<'a>(expr: &Expr, meta: &mut Metadata<'a>) -> ResultPair {
        create_result_pair(Some(Box::new(expr.clone())), meta)
    }

    #[test]
    fn unresolved_identifier_returns_none() {
        // No bindings in scope_index → resolve_binding returns None →
        // value stays JS-undefined.
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
        let id = Ident::new("nonexistent".into(), DUMMY_SP, Default::default());
        let mut eval = identity_evaluator;
        let pair = traverse_identifier(
            &id,
            &mut meta,
            &scope_index,
            scope_index.program_scope(),
            None,
            &mut eval,
        );
        assert!(pair.value.is_none());
    }

    #[test]
    fn resolves_const_to_init_expr_via_recursion() {
        // const x = 'blue'; → resolve_binding returns the StringLit;
        // evaluate_expression (identity here) re-emits it.
        let module = parse_module("const x = 'blue';");
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
        let mut eval = identity_evaluator;
        let pair = traverse_identifier(
            &id,
            &mut meta,
            &scope_index,
            scope_index.program_scope(),
            None,
            &mut eval,
        );
        let v = pair.value.expect("resolved value");
        match *v {
            Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => {
                assert_eq!(s.value.to_atom_lossy().as_str(), "blue");
            }
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn deopts_when_binding_is_non_constant() {
        // let x = 1; x = 2; → binding.constant == false → deopt.
        let module = parse_module("let x = 1; x = 2;");
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
        let mut eval = identity_evaluator;
        let pair = traverse_identifier(
            &id,
            &mut meta,
            &scope_index,
            scope_index.program_scope(),
            None,
            &mut eval,
        );
        // Non-const → JS-undefined fall-through.
        assert!(pair.value.is_none());
    }
}
