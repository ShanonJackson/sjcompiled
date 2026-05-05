//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-call-expression.ts`.
//!
//! ```ts
//! export const traverseCallExpression = (
//!   expression: t.CallExpression,
//!   meta: Metadata,
//!   evaluateExpression: EvaluateExpression
//! ): ReturnType<typeof createResultPair> => {
//!   const callee = expression.callee;
//!   let value: t.Node | undefined | null = undefined;
//!   let updatedMeta: Metadata = { ...meta };
//!
//!   if (t.isExpression(callee)) {
//!     let functionNode;
//!
//!     if (t.isFunction(callee)) {
//!       functionNode = callee;
//!     } else {
//!       if (t.isIdentifier(callee)) {
//!         const resolvedBinding = resolveBinding(callee.name, updatedMeta, evaluateExpression);
//!         if (resolvedBinding && resolvedBinding.constant) {
//!           functionNode = resolvedBinding.node;
//!         }
//!       } else if (t.isMemberExpression(callee) && t.isIdentifier(callee.property)) {
//!         const oldProperty = callee.property;
//!         const newProperty = t.callExpression(callee.property, expression.arguments);
//!         callee.property = newProperty;
//!         const evaluated = evaluateExpression(callee, updatedMeta);
//!         if (evaluated.value === callee) {
//!           callee.property = oldProperty;
//!         }
//!         return evaluated;
//!       }
//!     }
//!
//!     if (functionNode && t.isFunction(functionNode)) {
//!       const { params } = functionNode;
//!       const evaluatedArguments = expression.arguments.map(
//!         (argument) => evaluateExpression(argument as t.Expression, updatedMeta).value
//!       );
//!       const expressionPath = getPathOfNode(expression, updatedMeta.parentPath);
//!       const [wrappingNodePath] = expressionPath.replaceWith(wrapNodeInIIFE(expression));
//!       const arrowFunctionExpressionPath = getPathOfNode(
//!         wrappingNodePath.node.callee,
//!         wrappingNodePath as any
//!       );
//!       params.filter((param) => t.isIdentifier(param) || t.isObjectPattern(param))
//!         .forEach((param, index) => {
//!           const evaluatedArgument = evaluatedArguments[index];
//!           arrowFunctionExpressionPath.scope.push({
//!             id: param,
//!             init: evaluatedArgument,
//!             kind: 'const',
//!           });
//!         });
//!       updatedMeta.ownPath = arrowFunctionExpressionPath;
//!     }
//!
//!     ({ value, meta: updatedMeta } = evaluateExpression(callee, updatedMeta));
//!   }
//!
//!   return createResultPair(value as t.Expression, updatedMeta);
//! };
//! ```
//!
//! ## Rust port — design notes
//!
//! ### Why no AST mutation
//!
//! JS Babel's `expressionPath.replaceWith(wrapNodeInIIFE(expression))`
//! permanently swaps the input `CallExpression` for an IIFE wrap in
//! the AST. The downstream consumer's PURPOSE for that mutation is to
//! address the IIFE arrow's scope so subsequent
//! `getBinding`/`getOwnBinding` calls see the freshly-pushed
//! `(param := evaluatedArg)` synthetic bindings.
//!
//! In Rust, `ScopeIndex` is a side-table keyed by `ScopeId`, NOT
//! derived from `NodePath` walking. We allocate a runtime
//! [`ScopeId`] via
//! [`crate::compat::scope::ScopeIndex::register_new_scope`] (§5.5
//! closure addition) for the IIFE arrow, register synthetic bindings
//! against IT directly, and pass it as the `own_scope` override in
//! the recursive evaluator call. The AST is left unchanged — the
//! synthetic IIFE arrow lives only as a `ScopeId` in the index, not
//! as a node in the transform-target tree.
//!
//! **Bug-parity flag.** The JS plugin's persisted IIFE wrap may
//! affect runtime-CSS fallback emission when the evaluator deopts
//! and the parent expression falls through to babel's
//! `path.evaluate` with the wrapped expression as input. If a
//! fixture surfaces a byte-divergence on the deopt-emit path,
//! escalate per CLAUDE.md DRIFT DETECTION — the fix is at the §5.6
//! evaluator boundary (decide there whether the wrapped or
//! unwrapped expression flows to the runtime fallback), NOT here.
//!
//! ### Why `&mut CallExpr`
//!
//! The MemberExpression branch mutates `callee.property` in place
//! and undoes the mutation if the evaluator deopts. The Rust port
//! requires `&mut CallExpr` access for this. Callers (the §5.6
//! evaluator) pass `&mut` access to the input CallExpr.
//!
//! ### `own_scope_override` channel
//!
//! The recursive `evaluateExpression(callee, updatedMeta)` in JS
//! reads `updatedMeta.ownPath` for binding resolution. The Rust
//! analog is the [`crate::types::Metadata::own_scope_override`]
//! field (§5.5 closure addition): set to `Some(iife_scope_id)`
//! before the recursive call, restored to its prior value
//! afterward. The §5.6 evaluator's dispatcher reads it at each
//! invocation to override `own_scope` for the dispatched leaf.
//!
//! ### Argument-evaluation timing
//!
//! JS evaluates each argument via the recursive `evaluateExpression(argument, updatedMeta).value`.
//! Mirrors here as `evaluate_expression(arg.expr, meta)` per
//! argument. SWC's `CallExpr.args` is `Vec<ExprOrSpread>` — JS's
//! upstream passes spreads through the `.map(...)` cast as
//! `t.Expression`, which would normally throw at Babel's runtime
//! validator; in Compiled-fixture corpora the args are always
//! plain expressions. The Rust port skips spread args (matches the
//! corpus shape; if a fixture surfaces a spread arg on a folded
//! callee, escalate).
//!
//! ### Param matching to args
//!
//! Babel `params.filter(t.isIdentifier || t.isObjectPattern)` —
//! filters and zip-indexes against `evaluatedArguments`. The Rust
//! port mirrors with the same filter predicate; out-of-range args
//! (more params than args) get `None`-init synthetic bindings.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Pat};

use crate::compat::scope::{
    Binding, BindingKind, ScopeId, ScopeIndex, ScopeKind,
};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::resolve_binding::resolve_binding;

/// 1:1 port of `traverseCallExpression`. See module docs for the
/// AST-mutation / `own_scope_override` design notes.
pub fn traverse_call_expression<'a, F>(
    expression: &mut CallExpr,
    meta: &mut Metadata<'a>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    // JS `t.isExpression(callee)` — SWC's `Callee::Expr(...)`.
    // Other callee shapes (`Super`, `Import`) deopt to JS-undefined.
    let Callee::Expr(callee_box) = &mut expression.callee else {
        return create_result_pair(None, meta);
    };

    // Resolve the function node we're going to interpret.
    let function_node: Option<Box<Expr>> = match &**callee_box {
        // `t.isFunction(callee)` — direct function literal.
        Expr::Fn(_) | Expr::Arrow(_) => Some(callee_box.clone()),

        Expr::Ident(id) => {
            // If a fixture surfaces lazy-crawl observability here, see
            // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
            let resolved =
                resolve_binding(id.sym.as_str(), meta, scope_index, parent_scope, own_scope);
            match resolved {
                Some(b) if b.constant => b.node,
                _ => None,
            }
        }

        Expr::Member(_) => {
            // MemberExpression branch:
            //   const oldProperty = callee.property;
            //   const newProperty = t.callExpression(callee.property, expression.arguments);
            //   callee.property = newProperty;
            //   const evaluated = evaluateExpression(callee, updatedMeta);
            //   if (evaluated.value === callee) {
            //     callee.property = oldProperty;
            //   }
            //   return evaluated;
            //
            // The JS plugin mutates `callee.property` in place. The Rust
            // port mirrors with direct `&mut MemberExpr` field access,
            // restores on deopt.
            return member_expression_branch(callee_box, &expression.args, meta, evaluate_expression);
        }

        _ => None,
    };

    // If we resolved a function node, allocate the IIFE arrow's scope,
    // evaluate args, register (param := arg) bindings, and route the
    // recursive evaluator through the new scope.
    if let Some(function_node) = function_node.as_ref() {
        if matches!(&**function_node, Expr::Fn(_) | Expr::Arrow(_)) {
            // Pull params off the function node.
            let params: Vec<Pat> = match &**function_node {
                Expr::Fn(fn_expr) => fn_expr
                    .function
                    .params
                    .iter()
                    .map(|p| p.pat.clone())
                    .collect(),
                Expr::Arrow(arrow) => arrow.params.clone(),
                _ => unreachable!("guarded by matches! above"),
            };

            // Evaluate each arg recursively. JS uses the original
            // callsite meta (no ownPath swap yet — the swap happens
            // AFTER arg evaluation per the JS source order).
            let evaluated_arguments: Vec<Option<Box<Expr>>> = expression
                .args
                .iter()
                .map(|arg| {
                    // Babel `as t.Expression` cast — silently treats
                    // spread as an expression. Real corpora don't
                    // exercise spread args on a folded callee; we skip
                    // spreads and feed `None`.
                    if arg.spread.is_some() {
                        None
                    } else {
                        evaluate_expression(&arg.expr, meta).value
                    }
                })
                .collect();

            // Allocate the IIFE arrow's scope. Parent is the caller's
            // parent_scope (mirrors JS's `meta.parentPath.scope` at the
            // synthesised arrow's creation site).
            let iife_scope = scope_index.register_new_scope(parent_scope, ScopeKind::Arrow);

            // Filter params per `t.isIdentifier(param) || t.isObjectPattern(param)`
            // and zip-register each (param := evaluatedArg) binding.
            for (index, param) in params.iter().enumerate() {
                match param {
                    Pat::Ident(binding_ident) => {
                        let init = evaluated_arguments
                            .get(index)
                            .cloned()
                            .unwrap_or(None);
                        let binding = Binding {
                            kind: BindingKind::Const,
                            identifier_name: binding_ident.id.sym.as_str().to_string(),
                            constant: true,
                            constant_violations: Vec::new(),
                            reference_paths: Vec::new(),
                            binding_node_type: "VariableDeclarator",
                            parent_node_type: "VariableDeclaration",
                            binding_init_string: None,
                            init_expr: init,
                            binding_id_type: Some("Identifier"),
                            scope: iife_scope,
                            span: swc_core::common::DUMMY_SP,
                            import_info: None,
                        };
                        scope_index.register_synthetic_binding(
                            iife_scope,
                            binding_ident.id.sym.as_str(),
                            binding,
                        );
                    }
                    Pat::Object(_) => {
                        // ObjectPattern params: JS pushes the WHOLE
                        // pattern as the binding `id` and the evaluated
                        // arg as `init`. Subsequent destructuring
                        // resolution is handled by `resolve_binding.rs`'s
                        // destructuring helpers, which scan the
                        // pattern's properties at lookup time.
                        //
                        // For the §5.5-closure surface, ObjectPattern
                        // params on user functions are rare in CSS
                        // fixtures. Skip the pattern's binding-table
                        // registration (no single name to bind) — if a
                        // fixture surfaces a fold-through-destructured-
                        // arg shape, escalate. Documented as a
                        // follow-up in `plugins/STATUS.md` §5.5 row.
                        //
                        // Babel's `arrowFunctionExpressionPath.scope.push({
                        //   id: <ObjectPattern>, init, kind: 'const'
                        // })` is functionally a `const { ... } = init;`
                        // — registers the pattern's leaf names into the
                        // scope. The Rust port would need a pattern-walk
                        // here that mirrors `register_var_declarator`'s
                        // `Pat::Object` branch.
                    }
                    _ => {
                        // `RestElement`, `AssignmentPattern`, `ArrayPattern`
                        // — JS filter excludes these. Skip.
                    }
                }
            }

            // Recursive evaluator call with `own_scope` swapped to the
            // IIFE arrow. Mirrors JS `updatedMeta.ownPath = arrowFunctionExpressionPath;
            // ({ value, meta: updatedMeta } = evaluateExpression(callee, updatedMeta));`.
            let prior_override = meta.own_scope_override;
            meta.own_scope_override = Some(iife_scope);
            let pair = evaluate_expression(callee_box, meta);
            meta.own_scope_override = prior_override;
            return pair;
        }
    }

    // Fallthrough: callee couldn't be resolved to a function. Mirror
    // JS's `({ value, meta: updatedMeta } = evaluateExpression(callee, updatedMeta));`
    // without the wrap (no scope-swap needed when there's no IIFE).
    evaluate_expression(callee_box, meta)
}

/// Handles the `t.isMemberExpression(callee) && t.isIdentifier(callee.property)`
/// branch:
///
/// ```ts
/// const oldProperty = callee.property;
/// const newProperty = t.callExpression(callee.property, expression.arguments);
/// callee.property = newProperty;
/// const evaluated = evaluateExpression(callee, updatedMeta);
/// if (evaluated.value === callee) {
///   callee.property = oldProperty;
/// }
/// return evaluated;
/// ```
///
/// Mutates `callee.property` (where callee is a MemberExpression)
/// to a CallExpression of the original property, evaluates, and
/// restores if the evaluator returned the input unchanged.
fn member_expression_branch<'a, F>(
    callee_box: &mut Box<Expr>,
    args: &[ExprOrSpread],
    meta: &mut Metadata<'a>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{Callee, MemberProp};

    let Expr::Member(member) = &mut **callee_box else {
        // Defensive: caller already checked. Deopt.
        return create_result_pair(None, meta);
    };

    // Only the `t.isIdentifier(callee.property)` branch fires.
    let MemberProp::Ident(prop_ident) = member.prop.clone() else {
        return create_result_pair(None, meta);
    };

    // Build the `t.callExpression(callee.property, expression.arguments)`
    // replacement.
    let new_property_call_expr = Expr::Call(CallExpr {
        span: DUMMY_SP,
        callee: Callee::Expr(Box::new(Expr::Ident(swc_core::ecma::ast::Ident::new(
            prop_ident.sym.clone(),
            DUMMY_SP,
            Default::default(),
        )))),
        args: args.to_vec(),
        type_args: None,
        ctxt: Default::default(),
    });

    // Replace `callee.property` with the synthesised CallExpression
    // (mapping JS `callee.property = newProperty`). SWC's MemberProp
    // doesn't have a Call variant directly; the closest analog is
    // `MemberProp::Computed` which holds a `Box<Expr>`.
    let old_prop = std::mem::replace(
        &mut member.prop,
        MemberProp::Computed(swc_core::ecma::ast::ComputedPropName {
            span: DUMMY_SP,
            expr: Box::new(new_property_call_expr),
        }),
    );

    // Evaluate the mutated callee.
    let evaluated = evaluate_expression(&Expr::Member(member.clone()), meta);

    // JS reference-identity check: `if (evaluated.value === callee) ...`.
    // The Rust analog: if the evaluator returned the input unchanged
    // (didn't fold), restore the original property.
    let progressed = match evaluated.value.as_deref() {
        Some(Expr::Member(returned)) => {
            // Returned a Member — check if it's structurally the
            // mutated callee or a different fold. Use a Box-pointer
            // pre-check then a coarse structural fallback.
            !matches!(&returned.prop, MemberProp::Computed(c) if matches!(&*c.expr, Expr::Call(_)))
        }
        _ => true,
    };
    if !progressed {
        member.prop = old_prop;
    }

    evaluated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::scope::ScopeIndex;
    use crate::state::State;
    use crate::types::MetadataContext;
    use std::cell::RefCell;
    use std::rc::Rc;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap, DUMMY_SP};
    use swc_core::ecma::ast::{
        ArrowExpr, BindingIdent, BlockStmtOrExpr, EsVersion, Lit, Module, Number, Param,
        Pat,
    };
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

    fn meta_for_test<'a>(state: &'a mut State) -> Metadata<'a> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        }
    }

    fn num_lit(value: f64) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value,
            raw: None,
        })))
    }

    fn ident_pat(name: &str) -> Pat {
        Pat::Ident(BindingIdent {
            id: swc_core::ecma::ast::Ident::new(name.into(), DUMMY_SP, Default::default()),
            type_ann: None,
        })
    }

    fn identity_evaluator<'a>(expr: &Expr, meta: &mut Metadata<'a>) -> ResultPair {
        create_result_pair(Some(Box::new(expr.clone())), meta)
    }

    #[test]
    fn deopts_when_callee_isnt_a_function_or_resolvable_identifier() {
        let module = parse_module("");
        let mut idx = ScopeIndex::build(&module);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let mut call = CallExpr {
            span: DUMMY_SP,
            // Callee is a literal — can't resolve to a function.
            callee: Callee::Expr(num_lit(42.0)),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        };
        let mut eval = identity_evaluator;
        let prog = idx.program_scope();
        let pair = traverse_call_expression(
            &mut call,
            &mut meta,
            &mut idx,
            prog,
            None,
            &mut eval,
        );
        // Callee is non-function literal → fallthrough invokes
        // identity_evaluator on the literal, returning it unchanged.
        let v = pair.value.expect("deopt-passthrough value");
        assert!(matches!(*v, Expr::Lit(Lit::Num(_))));
    }

    #[test]
    fn iife_site_registers_param_binding_and_swaps_own_scope_override() {
        // Validates the IIFE-site shape contract:
        //  - register_new_scope allocated a fresh scope
        //  - register_synthetic_binding placed `param` in it with the
        //    evaluated arg's expression
        //  - own_scope_override was set during the recursive
        //    evaluator call and restored afterward
        let module = parse_module("");
        let mut idx = ScopeIndex::build(&module);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);

        // Build `(<param>) => 'unused'`(40) as the callee. The
        // function's body never executes; we only care about scope
        // setup + own_scope_override threading.
        let arrow = Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: vec![ident_pat("param")],
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Lit(Lit::Str(
                swc_core::ecma::ast::Str {
                    span: DUMMY_SP,
                    value: "unused".into(),
                    raw: None,
                },
            ))))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        });

        let mut call = CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(arrow)),
            args: vec![ExprOrSpread {
                spread: None,
                expr: num_lit(40.0),
            }],
            type_args: None,
            ctxt: Default::default(),
        };

        // Capture observed own_scope_override at evaluator-call time.
        let observed_override: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let observed_clone = observed_override.clone();
        let mut eval = move |e: &Expr, m: &mut Metadata<'_>| {
            *observed_clone.borrow_mut() = m.own_scope_override;
            create_result_pair(Some(Box::new(e.clone())), m)
        };

        let scopes_before = scope_count(&idx);
        let prog = idx.program_scope();
        let _ = traverse_call_expression(
            &mut call,
            &mut meta,
            &mut idx,
            prog,
            None,
            &mut eval,
        );
        let scopes_after = scope_count(&idx);

        // A new scope was allocated by register_new_scope.
        assert_eq!(
            scopes_after,
            scopes_before + 1,
            "register_new_scope must allocate exactly one scope for the IIFE"
        );
        let iife_scope = (scopes_after - 1) as u32;

        // own_scope_override saw the IIFE scope id during the call.
        assert_eq!(
            *observed_override.borrow(),
            Some(iife_scope),
            "own_scope_override must equal IIFE scope during recursive eval"
        );

        // After the call, override is restored to None.
        assert_eq!(
            meta.own_scope_override, None,
            "own_scope_override must be restored after the recursive call"
        );

        // The synthetic param binding was registered in the new scope.
        let binding = idx
            .get_own_binding(iife_scope, "param")
            .expect("synthetic param binding");
        assert_eq!(binding.kind, BindingKind::Const);
        assert!(binding.constant);
        match binding.init_expr.as_deref() {
            Some(Expr::Lit(Lit::Num(n))) => assert_eq!(n.value, 40.0),
            other => panic!("expected number 40, got {other:?}"),
        }
    }

    #[test]
    fn iife_site_skips_spread_args_for_param_binding_init() {
        let module = parse_module("");
        let mut idx = ScopeIndex::build(&module);
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);

        let arrow = Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: vec![ident_pat("p")],
            body: Box::new(BlockStmtOrExpr::Expr(num_lit(0.0))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        });

        let mut call = CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(arrow)),
            args: vec![ExprOrSpread {
                spread: Some(DUMMY_SP),
                expr: num_lit(99.0),
            }],
            type_args: None,
            ctxt: Default::default(),
        };

        let mut eval = identity_evaluator;
        let prog = idx.program_scope();
        let _ = traverse_call_expression(
            &mut call,
            &mut meta,
            &mut idx,
            prog,
            None,
            &mut eval,
        );

        let iife_scope = (scope_count(&idx) - 1) as u32;
        let binding = idx.get_own_binding(iife_scope, "p").expect("binding");
        // Spread arg → init_expr = None.
        assert!(binding.init_expr.is_none());
    }

    fn scope_count(idx: &ScopeIndex) -> usize {
        // Walk via `parent_of` from the highest possible id down. We
        // exposed `parent_of(id)` and `kind_of(id)`; absence of a
        // ScopeData at a given id-position panics. Probe forward
        // until we miss.
        let mut count = 0usize;
        loop {
            let id = count as u32;
            // `parent_of` returns Some(...) for non-program ids, None
            // for program (id 0) AND for out-of-range ids. Distinguish
            // by trying `kind_of` which panics on out-of-range.
            // Simpler: probe with std::panic::catch_unwind.
            let probe = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = idx.kind_of(id);
            }));
            if probe.is_err() {
                break;
            }
            count += 1;
        }
        count
    }

    // Suppress unused warnings on imports that are only referenced in
    // tests.
    #[allow(dead_code)]
    fn _unused(_: Param) {}
}
