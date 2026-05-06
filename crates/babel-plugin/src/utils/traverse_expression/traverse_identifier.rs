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

use swc_core::ecma::ast::{Expr, Prop, PropName, PropOrSpread, Ident};

use crate::compat::scope::{Binding, ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::resolve_binding::{get_destructured_object_pattern_key, resolve_binding};

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

    // §6.8r — Member-on-member destructure fallback. Mirrors upstream
    // `resolve-binding.ts:resolveObjectPatternValueNode`'s
    // member-on-member branch (lines surrounding the
    // `t.isMemberExpression(expression) &&
    // t.isMemberExpression(expression.object)` check) which calls
    // `evaluateExpression(expression, meta)` to fold the chain into
    // an ObjectExpression before extracting the destructure key.
    //
    // Why here and not inside `resolve_object_pattern_value_node`:
    // that function is reached via `resolve_binding`, whose public
    // surface takes `&Metadata` / `&ScopeIndex` (immutable). The
    // recursive evaluator requires `&mut Metadata` / `&mut
    // ScopeIndex`. Threading mutable refs through every
    // `resolve_binding` caller (12+ sites across css-builders,
    // traverse-expression, evaluate-expression) would be invasive.
    // This leaf already holds an `evaluate_expression` closure and
    // mutable `Metadata` — the exact frame upstream's JS closes over
    // — so the targeted fallback lands the missing branch without
    // restructuring the call graph.
    //
    // Applies only when:
    //   1. `resolve_binding` returned `None` OR `node = None`
    //      (otherwise the upstream branch was satisfied)
    //   2. Local binding is a destructure (`destructured_pat` +
    //      `destructured_init` both Some)
    //   3. `destructured_init` is a member-on-member chain (depth ≥ 2)
    //   4. Binding is constant (matches upstream's
    //      `binding.constantViolations.length > 0` deopt)
    //
    // The folded ObjectExpression is then walked with the same
    // KeyValue+Ident-key lookup `resolve_object_pattern_value_node`
    // uses, and the matched value is recursively evaluated to handle
    // nested folds (e.g. matched value is itself an Identifier).
    let effective_own_scope = meta.own_scope_override.or(own_scope);
    if let Some(b) =
        lookup_binding_for_destructure(scope_index, parent_scope, effective_own_scope, name)
    {
        if !b.constant {
            return create_result_pair(None, meta);
        }
        if let (Some(pat), Some(init)) =
            (b.destructured_pat.as_ref(), b.destructured_init.as_ref())
        {
            // Gate on member-on-member shape — single-Member init is
            // already handled by `resolve_object_pattern_value_node`'s
            // identifier-recursion path.
            let is_member_on_member =
                matches!(&**init, Expr::Member(m) if matches!(&*m.obj, Expr::Member(_)));
            if is_member_on_member {
                let key = get_destructured_object_pattern_key(pat, name);
                // Clone the init so we don't hold a borrow on `b`
                // while invoking the closure (which mutably borrows
                // through `meta`).
                let init_owned = init.clone();
                let folded = evaluate_expression(&init_owned, meta).value;
                if let Some(folded_expr) = folded {
                    if let Expr::Object(obj) = &*folded_expr {
                        for prop in &obj.props {
                            let PropOrSpread::Prop(boxed) = prop else { continue };
                            let Prop::KeyValue(kv) = &**boxed else { continue };
                            let PropName::Ident(id) = &kv.key else { continue };
                            if id.sym == *key {
                                let result = evaluate_expression(&kv.value, meta);
                                return create_result_pair(result.value, meta);
                            }
                        }
                    }
                }
            }
        }
    }

    // Fall-through: JS `value as t.Expression` is `undefined` when
    // resolution / constancy / node-presence fails. Mirror with
    // `Option::None`.
    create_result_pair(None, meta)
}

/// Walk own-scope first then parent-scope to find a binding by name —
/// matches the lookup order in `resolve_binding::get_binding`. Surfaced
/// here so the §6.8r fallback can read `Binding::destructured_pat` /
/// `destructured_init` directly without re-routing through
/// `resolve_binding` (which strips that information into
/// `PartialBindingWithMeta` and discards the destructure init).
fn lookup_binding_for_destructure<'idx>(
    scope_index: &'idx ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    name: &str,
) -> Option<&'idx Binding> {
    if let Some(own) = own_scope {
        if let Some(b) = scope_index.get_own_binding(own, name) {
            return Some(b);
        }
    }
    scope_index.get_binding(parent_scope, name)
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
