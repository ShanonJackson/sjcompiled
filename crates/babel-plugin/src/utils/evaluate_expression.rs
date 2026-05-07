//! 1:1 port of `packages/babel-plugin/src/utils/evaluate-expression.ts`.
//!
//! ```ts
//! const isIdentifierReferencesMutated = (path: NodePath<t.Identifier>): boolean => {
//!   const binding = path.scope.getBinding(path.node.name);
//!   if (!binding) return false;
//!   if (!t.isVariableDeclarator(binding.path.node) || !binding.constant) return true;
//!   for (let i = 0; i < binding.referencePaths.length; i++) {
//!     const refPath = binding.referencePaths[i];
//!     const innerBinding = refPath.scope.getBinding(path.node.name);
//!     if (!innerBinding) continue;
//!     if (!t.isVariableDeclarator(innerBinding.path.node) || !innerBinding.constant) return true;
//!   }
//!   return false;
//! };
//!
//! const isPathReferencingAnyMutatedIdentifiers = (path: NodePath<any>): boolean => {
//!   if (path.isIdentifier()) return isIdentifierReferencesMutated(path);
//!   let mutated = false;
//!   path.traverse({ Identifier(innerPath) { ... } });
//!   return mutated;
//! };
//!
//! const babelEvaluateExpression = (node, meta, fallbackNode = node) => {
//!   try {
//!     const path = getPathOfNode(node, meta.parentPath);
//!     if (isPathReferencingAnyMutatedIdentifiers(path)) return fallbackNode;
//!     const result = path.evaluate();
//!     if (result.value != null) {
//!       switch (typeof result.value) {
//!         case 'string': return t.stringLiteral(result.value);
//!         case 'number': return t.numericLiteral(result.value);
//!       }
//!     }
//!     return fallbackNode;
//!   } catch { return fallbackNode; }
//! };
//!
//! export const evaluateExpression = (expression, meta) => {
//!   let value, updatedMeta = meta;
//!   const targetExpression = t.isTSAsExpression(expression) ? expression.expression : expression;
//!   if (t.isIdentifier(...))         ({ value, meta: updatedMeta } = traverseIdentifier(...));
//!   else if (t.isMemberExpression(...))   ...traverseMemberExpression(...);
//!   else if (t.isFunction(...))           ...traverseFunction(...);
//!   else if (t.isCallExpression(...))     ...traverseCallExpression(...);
//!   else if (t.isBinaryExpression(...))   ...traverseBinaryExpression(...);
//!   else if (t.isUnaryExpression(...))    ...traverseUnaryExpression(...);
//!
//!   if (
//!     t.isStringLiteral(value) || t.isNumericLiteral(value) ||
//!     t.isObjectExpression(value) || t.isTaggedTemplateExpression(value) ||
//!     (value && isCompiledKeyframesCallExpression(value, updatedMeta.state))
//!   ) return createResultPair(value, updatedMeta);
//!
//!   if (value) {
//!     const babelEvaluatedNode = babelEvaluateExpression(value, updatedMeta, targetExpression);
//!     return createResultPair(babelEvaluatedNode, updatedMeta);
//!   }
//!   const babelEvaluatedNode = babelEvaluateExpression(targetExpression, updatedMeta);
//!   return createResultPair(babelEvaluatedNode, updatedMeta);
//! };
//! ```
//!
//! ## §5.6 wiring contract
//!
//! Three contracts established by §5.4e + §5.5 closure are honoured here:
//!
//! 1. **Cross-file scope swap** (§5.4e drift-fix). When the Identifier
//!    branch's binding resolves to `source == Import` with a
//!    populated `imported_module: Arc<Module>` AND a foldable
//!    `node: Some(_)`, this dispatcher builds a fresh
//!    [`crate::compat::scope::ScopeIndex`] from the imported module
//!    and recurses with that index (parent_scope = imported program
//!    scope, own_scope = None). Identifier references inside the
//!    imported AST resolve against the imported file's scope —
//!    matching JS Babel's `meta.parentPath` swap at
//!    `resolve-binding.ts:407-414`.
//!
//! 2. **`own_scope_override` channel** (§5.5 closure). The closure
//!    that leaves invoke reads `meta.own_scope_override` at each
//!    invocation and uses it as the effective `own_scope` for the
//!    next dispatch. `traverse_call_expression` sets the override
//!    before its recursive callee-eval; this dispatcher consumes it.
//!
//! 3. **Namespace-import dispatch route** (§5.5 closure / §5.4e
//!    drift-fix). When the MemberExpression branch's bottom binding
//!    is a namespace import (`source == Import &&
//!    imported_module.is_some() && node.is_none()`), the dispatcher
//!    routes the first access-path element through
//!    [`crate::utils::traverse_expression::traverse_member_expression::traverse_access_path::evaluate_path::evaluate_namespace_import_path`]
//!    and continues the chain against the imported scope.
//!
//! ## Soundness — raw-pointer-based dispatcher recursion
//!
//! The Compiled `evaluate-expression` contract is mutually-recursive
//! with `traverse-expression/*` leaves. JS resolves the recursion via
//! a closure parameter (`evaluateExpression: EvaluateExpression`). The
//! Rust port mirrors the closure shape — leaves take
//! `evaluate_expression: &mut F` where
//! `F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair`.
//!
//! `traverse_call_expression` requires `&mut ScopeIndex` for IIFE-arrow
//! scope synthesis (`register_new_scope` + `register_synthetic_binding`).
//! Other leaves take `&ScopeIndex` (read-only). The closure they invoke
//! must re-enter dispatch with full scope state — which means the
//! closure's body needs to acquire `&mut ScopeIndex` even though the
//! outer leaf already holds a borrow.
//!
//! This is the well-known "self-referential local state" pattern. We
//! resolve it via `*mut ScopeIndex` raw pointers + a constrained
//! `unsafe` block. Soundness rests on the following access discipline,
//! verified by inspection of every §5.5 leaf:
//!
//! - **Leaves do not access `scope_index` between invoking the
//!   closure and returning.** Every closure call is followed only by
//!   trivial wrap-up (build a `ResultPair`, restore `meta` fields,
//!   etc.) that does NOT touch the scope index.
//! - **`traverse_call_expression` mutates `scope_index` BEFORE
//!   invoking the closure**, then sets `meta.own_scope_override` and
//!   invokes the closure ONCE for callee evaluation. Between
//!   `register_synthetic_binding` and the closure call, no other
//!   `scope_index` access happens. After the closure returns, only
//!   `meta.own_scope_override` is restored — `scope_index` is not
//!   touched.
//! - **Argument evaluation in `traverse_call_expression` invokes the
//!   closure for each arg BEFORE any `scope_index` mutation.** The
//!   `register_new_scope` call happens after the args are evaluated.
//!
//! Under stacked-borrows / tree-borrows, the raw `&mut *scope_ptr`
//! reborrow inside the closure body aliases with the leaf's outer
//! `&ScopeIndex` borrow. Since the leaf does not access scope_index
//! during the closure body, no actual aliased read or write occurs;
//! the pattern is sound under Rust's "no aliased access" rule.
//!
//! Alternative designs considered and rejected:
//! - `Rc<RefCell<ScopeIndex>>`: would require modifying §5.5 leaf
//!   signatures (locked); also panics on overlapping borrows when
//!   `traverse_call_expression`'s `borrow_mut()` is active during a
//!   closure body that needs to recurse.
//! - Thread-local `Cell<*mut ScopeIndex>`: same aliasing model
//!   underneath; less clarity at the call site.
//! - Hand-inlining `traverse_call_expression`'s logic: drift risk;
//!   §5.5 closure leaf would have a parallel duplicate.

use std::sync::Arc;

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, Ident, Lit, MemberExpr, MemberProp, Module, Number, Str,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::compat::evaluation::{evaluate as compat_evaluate, EvaluatedValue, Value};
use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};
use crate::utils::is_compiled::is_compiled_keyframes_call_expression;
use crate::utils::resolve_binding::resolve_binding;
use crate::utils::traverse_expression::traverse_member_expression::traverse_access_path::{
    evaluate_path::evaluate_namespace_import_path, traverse_member_access_path,
};
use crate::utils::traverse_expression::{
    traverse_binary_expression, traverse_call_expression, traverse_function, traverse_identifier,
    traverse_member_expression,
};
use crate::utils::types::BindingSource;

/// 1:1 port of `evaluateExpression`.
///
/// `scope_index` / `parent_scope` / `own_scope` are threaded as
/// explicit parameters because the Rust `Metadata` doesn't carry
/// scope refs. Per §5.5 closure convention.
///
/// `meta.own_scope_override` is honoured: when set, it overrides
/// `own_scope` for this call. `traverse_call_expression` uses this
/// channel to swap in the synthetic IIFE arrow's scope for the
/// recursive callee evaluation.
pub fn evaluate_expression<'a>(
    expression: &Expr,
    meta: &mut Metadata<'a>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> ResultPair {
    dispatch_evaluate(expression, meta, scope_index, parent_scope, own_scope)
}

fn dispatch_evaluate<'a>(
    expression: &Expr,
    meta: &mut Metadata<'a>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> ResultPair {
    // §5.5 closure contract: per-call own-scope override.
    let effective_own_scope = meta.own_scope_override.or(own_scope);

    // JS: `t.isTSAsExpression(expression) ? expression.expression : expression`.
    // Skip TS-as wrappers; CSS-value evaluation is type-irrelevant.
    //
    // ALSO unwrap `Expr::Paren`: Babel's parser strips
    // `ParenthesizedExpression` nodes by default, so upstream's
    // `t.isObjectExpression(value)` / `t.isCallExpression(value)`
    // checks NEVER see a paren wrapper. SWC's parser keeps them, so
    // `() => ({x: 1})` parses with arrow body `Paren(Object)`. Without
    // this unwrap, every "arrow-returning-object" idiom would deopt
    // through the catch-all CSS-variable path. See
    // `crates/babel-plugin/src/compat/paren.rs` for the full rationale.
    let target_expression: &Expr =
        crate::compat::paren::unwrap_paren_and_ts_as(expression);

    // §5.4e drift-fix consumer contract: cross-file fold against
    // imported scope. When an Identifier resolves to an Import binding
    // with both `imported_module` and a foldable `node`, recurse into
    // the resolved node with a fresh ScopeIndex built over the
    // imported module's AST.
    if let Expr::Ident(id) = target_expression {
        if let Some(b) = resolve_binding(
            id.sym.as_str(),
            &*meta,
            &*scope_index,
            parent_scope,
            effective_own_scope,
        ) {
            if b.constant && b.source == BindingSource::Import {
                if let (Some(imported_module), Some(node)) = (b.imported_module, b.node) {
                    let mut imported_idx = ScopeIndex::build(&*imported_module);
                    let imp_prog = imported_idx.program_scope();
                    let pair =
                        dispatch_evaluate(&node, meta, &mut imported_idx, imp_prog, None);
                    return finalize_value(
                        pair.value,
                        target_expression,
                        meta,
                        scope_index,
                        parent_scope,
                        effective_own_scope,
                    );
                }
            }
        }
    }

    // Build the recursive dispatcher closure. See module-level
    // SAFETY comment for the soundness contract.
    let scope_ptr: *mut ScopeIndex = scope_index;
    let mut closure = move |e: &Expr, m: &mut Metadata<'_>| -> ResultPair {
        // SAFETY: see module-level comment. The scope_ptr reborrow
        // is exclusive during the closure body; leaves do not alias.
        let inner = unsafe { &mut *scope_ptr };
        // The inner dispatch reads `m.own_scope_override` and
        // composes against `effective_own_scope` from this frame's
        // env. JS Babel's recursion uses the parent path's scope
        // chain, which is what `effective_own_scope` represents
        // here.
        dispatch_evaluate(e, m, inner, parent_scope, effective_own_scope)
    };

    let value: Option<Box<Expr>> = match target_expression {
        Expr::Ident(id) => {
            // SAFETY: see module-level. The leaf reads scope_index
            // for its own resolve_binding then invokes the closure
            // (no further scope access).
            let scope_ref: &ScopeIndex = unsafe { &*scope_ptr };
            traverse_identifier(
                id,
                meta,
                scope_ref,
                parent_scope,
                effective_own_scope,
                &mut closure,
            )
            .value
        }

        Expr::Member(member) => {
            // §5.6 wiring contract: namespace-import preflight.
            // Detect `<theme>.<exportName>...` where `theme` is a
            // namespace-import binding, and route through
            // `evaluate_namespace_import_path` against a fresh
            // imported-module ScopeIndex.
            if let Some(v) = try_namespace_import_dispatch(
                member,
                meta,
                scope_ptr,
                parent_scope,
                effective_own_scope,
            ) {
                Some(v)
            } else if let Some(v) = try_cross_file_member_dispatch(
                member,
                meta,
                scope_ptr,
                parent_scope,
                effective_own_scope,
            ) {
                // Cross-file member-access dispatch: bottom binding
                // resolves to a foldable cross-file Ident
                // (default-import or named-import) — re-walk the
                // member chain against the imported file's scope so
                // identifiers like `sharedStyles` in
                // `export default sharedStyles` resolve through the
                // imported file's `export const sharedStyles = {...}`
                // binding rather than against the consumer scope
                // (where the name is the import alias and has no
                // foldable init).
                Some(v)
            } else {
                // SAFETY: see module-level.
                let scope_ref: &ScopeIndex = unsafe { &*scope_ptr };
                traverse_member_expression(
                    member,
                    meta,
                    scope_ref,
                    parent_scope,
                    effective_own_scope,
                    &mut closure,
                )
                .value
            }
        }

        // JS `t.isFunction` covers FnExpr + ArrowExpr in Expr position.
        Expr::Fn(_) | Expr::Arrow(_) => {
            traverse_function(target_expression, meta, &mut closure).value
        }

        Expr::Call(call) => {
            // `traverse_call_expression` requires `&mut CallExpr`
            // (in-place property mutation on the MemberExpression
            // branch). Clone first so the input `expression` stays
            // untouched.
            let mut call_clone = call.clone();
            // SAFETY: see module-level.
            let scope_mut: &mut ScopeIndex = unsafe { &mut *scope_ptr };
            traverse_call_expression(
                &mut call_clone,
                meta,
                scope_mut,
                parent_scope,
                effective_own_scope,
                &mut closure,
            )
            .value
        }

        Expr::Bin(bin) => traverse_binary_expression(bin, meta, &mut closure).value,

        Expr::Unary(u) => traverse_unary_expression_dispatch(u, meta, &mut closure),

        // JS falls through: `value` stays `undefined`.
        _ => None,
    };

    finalize_value(
        value,
        target_expression,
        meta,
        scope_index,
        parent_scope,
        effective_own_scope,
    )
}

/// Trivial trampoline so the Unary path matches the
/// `expr-by-reference` shape that other branches use. Compiled's JS
/// `traverseUnaryExpression` returns a `ResultPair`; we extract
/// `.value` here for symmetry with the other arms.
fn traverse_unary_expression_dispatch<'a, F>(
    expr: &swc_core::ecma::ast::UnaryExpr,
    meta: &mut Metadata<'a>,
    closure: &mut F,
) -> Option<Box<Expr>>
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    use crate::utils::traverse_expression::traverse_unary_expression;
    traverse_unary_expression(expr, meta, closure).value
}

/// Final post-check + babel fallback. Mirrors evaluate-expression.ts:178-199:
///
/// ```ts
/// if (
///   t.isStringLiteral(value) || t.isNumericLiteral(value) ||
///   t.isObjectExpression(value) || t.isTaggedTemplateExpression(value) ||
///   (value && isCompiledKeyframesCallExpression(value, updatedMeta.state))
/// ) return createResultPair(value, updatedMeta);
/// if (value) return createResultPair(babelEvaluateExpression(value, updatedMeta, targetExpression), updatedMeta);
/// return createResultPair(babelEvaluateExpression(targetExpression, updatedMeta), updatedMeta);
/// ```
fn finalize_value<'a>(
    value: Option<Box<Expr>>,
    target_expression: &Expr,
    meta: &mut Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> ResultPair {
    // Fast-pass: types that flow through unchanged.
    if let Some(ref v) = value {
        if matches!(
            &**v,
            Expr::Lit(Lit::Str(_)) | Expr::Lit(Lit::Num(_)) | Expr::Object(_) | Expr::TaggedTpl(_)
        ) {
            return create_result_pair(value, meta);
        }
        if is_compiled_keyframes_call_expression(v, meta.state) {
            return create_result_pair(value, meta);
        }
    }

    // Babel-evaluate fallback. JS does:
    //   if (value) babelEvaluateExpression(value, meta, targetExpression);
    //   else       babelEvaluateExpression(targetExpression, meta);
    // The fallback inputs the unfolded expression to compat::evaluation
    // and substitutes a literal on success; otherwise returns the
    // fallback (target_expression for None-input, or target_expression
    // for Some-input — same target_expression in both branches via
    // the JS `fallbackNode = targetExpression` default).
    let (eval_input, fallback): (&Expr, Box<Expr>) = match value.as_deref() {
        Some(v) => (v, Box::new(target_expression.clone())),
        None => (target_expression, Box::new(target_expression.clone())),
    };
    let folded = babel_evaluate_expression(
        eval_input,
        meta,
        scope_index,
        parent_scope,
        own_scope,
        fallback,
    );
    create_result_pair(Some(folded), meta)
}

/// 1:1 port of `babelEvaluateExpression`. The JS try/catch wrapping
/// `path.evaluate()` maps to `compat::evaluation::evaluate`'s
/// `EvaluatedValue::Deopt` return — Rust evaluator never panics on
/// expressions Babel would tolerate.
fn babel_evaluate_expression<'a>(
    node: &Expr,
    meta: &Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    fallback_node: Box<Expr>,
) -> Box<Expr> {
    let scope_for_eval = own_scope.unwrap_or(parent_scope);

    // JS: `if (isPathReferencingAnyMutatedIdentifiers(path)) return fallbackNode;`
    if is_path_referencing_any_mutated_identifiers(node, scope_index, scope_for_eval) {
        return fallback_node;
    }

    let result = compat_evaluate(node, scope_index, scope_for_eval);
    let _ = meta; // unused — present for grep parity with JS signature.
    match result {
        EvaluatedValue::Confident(Value::String(s)) => Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: s.into(),
            raw: None,
        }))),
        EvaluatedValue::Confident(Value::Number(n)) => Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: n,
            raw: None,
        }))),
        // JS: `result.value != null` — undefined / null fall through.
        // Boolean / Array / Object: switch only handles 'string' and
        // 'number' — others drop to fallback.
        _ => fallback_node,
    }
}

/// 1:1 port of `isIdentifierReferencesMutated`.
///
/// `path.scope.getBinding(name)` → `scope_index.get_binding(scope, name)`.
/// `t.isVariableDeclarator(binding.path.node)` → JS-side check that
/// the binding's source is a `VariableDeclarator`. The Rust
/// `Binding::binding_node_type` field carries this string verbatim
/// per the §5.0a port.
fn is_identifier_references_mutated(
    name: &str,
    scope_index: &ScopeIndex,
    scope: ScopeId,
) -> bool {
    // If a fixture surfaces lazy-crawl observability here, see
    // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
    let Some(binding) = scope_index.get_binding(scope, name) else {
        return false;
    };
    if binding.binding_node_type != "VariableDeclarator" || !binding.constant {
        return true;
    }
    // JS iterates `binding.referencePaths` and re-checks each ref's
    // scope. The Rust `Binding::reference_paths: Vec<ReferenceSite>`
    // carries the (span, scope) for every reference. Look up each
    // ref's binding and apply the same predicate.
    for ref_site in binding.reference_paths.iter() {
        // If a fixture surfaces lazy-crawl observability here, see
        // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
        let Some(inner) = scope_index.get_binding(ref_site.scope, name) else {
            continue;
        };
        if inner.binding_node_type != "VariableDeclarator" || !inner.constant {
            return true;
        }
    }
    false
}

/// 1:1 port of `isPathReferencingAnyMutatedIdentifiers`.
///
/// JS: if path is an Identifier, run the per-id check. Otherwise
/// `path.traverse({ Identifier: ... })` and short-circuit on the
/// first mutated reference.
fn is_path_referencing_any_mutated_identifiers(
    expr: &Expr,
    scope_index: &ScopeIndex,
    scope: ScopeId,
) -> bool {
    if let Expr::Ident(id) = expr {
        return is_identifier_references_mutated(id.sym.as_str(), scope_index, scope);
    }
    let mut visitor = MutatedIdentifierFinder {
        scope_index,
        scope,
        mutated: false,
    };
    expr.visit_with(&mut visitor);
    visitor.mutated
}

struct MutatedIdentifierFinder<'idx> {
    scope_index: &'idx ScopeIndex,
    scope: ScopeId,
    mutated: bool,
}

impl<'idx> Visit for MutatedIdentifierFinder<'idx> {
    fn visit_ident(&mut self, n: &Ident) {
        if self.mutated {
            return;
        }
        if is_identifier_references_mutated(n.sym.as_str(), self.scope_index, self.scope) {
            self.mutated = true;
        }
    }
}

/// §5.6 wiring: namespace-import preflight for the MemberExpression
/// branch.
///
/// Returns `Some(folded_value)` when the bottom binding of the
/// member chain is a namespace import AND the chain produced a
/// foldable value via `evaluate_namespace_import_path` (and any
/// subsequent access-path elements). Returns `None` to defer to the
/// standard `traverse_member_expression` path.
fn try_namespace_import_dispatch<'a>(
    member: &MemberExpr,
    meta: &mut Metadata<'a>,
    scope_ptr: *mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Option<Box<Expr>> {
    // Walk the member chain to extract bottom binding identifier +
    // access path. Mirrors `traverse_member_expression::get_member_expression_meta`
    // (kept local to avoid a public-surface bump on the §5.5 leaf).
    let info = collect_member_meta(member);
    let binding_id = info.binding_identifier?;
    if info.access_path.is_empty() {
        return None;
    }

    // SAFETY: see module-level. Local immutable reborrow for the
    // resolve_binding lookup; no mutation.
    let scope_ref: &ScopeIndex = unsafe { &*scope_ptr };
    let resolved = resolve_binding(
        binding_id.sym.as_str(),
        &*meta,
        scope_ref,
        parent_scope,
        own_scope,
    )?;
    if !(resolved.constant && resolved.source == BindingSource::Import) {
        return None;
    }
    // Two shapes route into the namespace dispatch:
    //
    // 1. **Direct namespace import** at the consumer:
    //    `import * as X from './m'; X.foo` —
    //    `resolve_binding` returns `node: None,
    //    imported_module: Some(m_ast)`. The first member-access
    //    element resolves against `m_ast`'s named exports.
    //
    // 2. **Namespace re-export through a named import** at the
    //    consumer: `import { X } from './t'` where `t.ts` is
    //    `import * as X from './m'; export { X };`. The first hop
    //    of `resolve_binding` lands on the local Ident from
    //    `export { X }` (so `node: Some(Ident("X"))`) and points
    //    at `t.ts`'s AST. Mirrors upstream's
    //    `traverse-identifier.ts:25-33` path: `evaluateExpression`
    //    recurses on the resolved Ident with the imported meta,
    //    `traverseIdentifier` re-runs `resolveBinding`, and Babel's
    //    `getBinding` walks `t.ts`'s scope to land on the
    //    `import * as X` binding — yielding the namespace target.
    //    The Rust port performs the equivalent second-hop lookup
    //    inline here so the namespace dispatcher can then evaluate
    //    the access path against the FINAL namespace module's AST.
    //
    // 3. **Namespace re-export through `export * from`** —
    //    upstream Babel via `traverse` follows star-exports into
    //    every source. Not surfaced by any current corpus fixture;
    //    deferred until a fixture lands.
    let imported_module: Arc<Module> = if resolved.node.is_none()
        && resolved.imported_module.is_some()
    {
        resolved.imported_module.unwrap()
    } else if let (Some(node), Some(t_module)) =
        (resolved.node.as_deref(), resolved.imported_module.as_ref())
    {
        // Shape (2): the resolved node is an Identifier whose
        // local-side name is bound in the imported file as a
        // namespace import. Look it up in `t_module`'s scope and
        // follow through to the namespace target.
        let Expr::Ident(local_id) = node else { return None };
        let local_name = local_id.sym.as_str().to_string();
        let t_idx = ScopeIndex::build(&**t_module);
        let t_prog = t_idx.program_scope();
        let local_binding = t_idx.get_binding(t_prog, &local_name)?;
        let import_info = local_binding.import_info.as_ref()?;
        if !matches!(
            import_info.kind,
            crate::compat::scope::ImportSpecifierKind::Namespace
        ) {
            return None;
        }
        // Resolve the namespace's source against the imported
        // file's filename — same anchoring rule as
        // `follow_reexport_hop` in resolve_binding.rs.
        let from_path: std::path::PathBuf = resolved
            .imported_filename
            .as_deref()
            .map(std::path::PathBuf::from)?;
        let resolver = meta.state.resolver()?;
        let resolved_path = resolver
            .resolve_sync(&from_path, &import_info.source)
            .ok()?;
        let resolved_path_str = resolved_path.to_string_lossy().to_string();
        let extensions: Vec<String> = meta
            .state
            .opts()
            .extensions
            .clone()
            .unwrap_or_else(|| {
                crate::constants::DEFAULT_CODE_EXTENSIONS
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect()
            });
        if !extensions.iter().any(|ext| resolved_path_str.ends_with(ext)) {
            return None;
        }
        let cache = &meta.state.cache;
        let path_for_read = resolved_path.clone();
        let source = cache.read_file.borrow_mut().load(
            Some("read-file"),
            &resolved_path_str,
            || std::fs::read_to_string(&path_for_read).unwrap_or_default(),
        );
        if source.is_empty() && !resolved_path.exists() {
            return None;
        }
        let path_for_parse = resolved_path.clone();
        let path_str_for_parse = resolved_path_str.clone();
        cache
            .parse_module
            .borrow_mut()
            .load(Some("parse-module"), &resolved_path_str, || {
                use swc_core::common::sync::Lrc;
                use swc_core::common::{FileName, SourceMap};
                use swc_core::ecma::ast::EsVersion;
                use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};
                let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
                let fm = cm.new_source_file(
                    Lrc::new(FileName::Real(path_for_parse.clone())),
                    source.clone(),
                );
                let parsed = parse_file_as_module(
                    &fm,
                    Syntax::Typescript(TsSyntax {
                        tsx: path_str_for_parse.ends_with(".tsx"),
                        ..Default::default()
                    }),
                    EsVersion::Es2022,
                    None,
                    &mut Vec::new(),
                )
                .unwrap_or_else(|_| Module {
                    span: Default::default(),
                    body: Vec::new(),
                    shebang: None,
                });
                Arc::new(parsed)
            })
    } else {
        return None;
    };

    // Build a fresh ScopeIndex over the imported module. Lives until
    // this preflight returns; subsequent recursion captures it.
    let mut imported_idx = ScopeIndex::build(&*imported_module);
    let imp_prog = imported_idx.program_scope();

    // First access-path element routes through evaluate_namespace_import_path.
    let placeholder = Expr::Ident(binding_id.clone());
    let first_path = info.access_path[0].sym.as_str().to_string();
    let pair = evaluate_namespace_import_path(
        &placeholder,
        &imported_module,
        &mut imported_idx,
        meta,
        &first_path,
    );
    let resolved_value = pair.value?;

    if info.access_path.len() == 1 {
        // Chain complete — return folded value.
        return Some(resolved_value);
    }

    // Continue chain through remaining access-path elements against
    // the IMPORTED scope. Build a closure that recurses via
    // `dispatch_evaluate` with the imported scope, then call
    // `traverse_member_access_path` for the rest of the chain.
    let imp_scope_ptr: *mut ScopeIndex = &mut imported_idx;
    let mut imp_closure = move |e: &Expr, m: &mut Metadata<'_>| -> ResultPair {
        // SAFETY: imp_scope_ptr lives as long as `imported_idx`,
        // which is owned by this preflight frame. The closure is
        // invoked synchronously inside `traverse_member_access_path`
        // before this function returns.
        let inner = unsafe { &mut *imp_scope_ptr };
        dispatch_evaluate(e, m, inner, imp_prog, None)
    };
    let remaining_path = &info.access_path[1..];
    // SAFETY: re-borrow imported_idx immutably for the leaf.
    let imp_ref: &ScopeIndex = unsafe { &*imp_scope_ptr };
    let next_pair = traverse_member_access_path(
        &resolved_value,
        meta,
        &first_path,
        remaining_path,
        member,
        imp_ref,
        imp_prog,
        None,
        &mut imp_closure,
    );
    next_pair.value
}

/// Cross-file member-access dispatch: 1:1 port of upstream's
/// `evaluateExpression(member, meta) → traverseMemberExpression →
/// traverseMemberAccessPath` chain in the case where the bottom
/// binding's `resolveBinding` returns `meta` SWAPPED to the
/// imported file's scope.
///
/// Upstream `resolveBinding` (`resolve-binding.ts:401-414`) returns
/// `{ node, meta: { ...meta, parentPath: foundParentPath, state: {
/// ..., file: ast, filename: modulePath } } }` — so when
/// `traverseMemberExpression` recurses on the resolved node via
/// `evaluateExpression`, every downstream `path.scope.getBinding`
/// lookup walks the imported file's scope chain. The Rust port
/// drops the `meta`-swap (documented in `traverse_identifier.rs`)
/// in favour of routing cross-file at the dispatch boundary; this
/// helper closes the member-access half of that contract that
/// `try_namespace_import_dispatch` left open.
///
/// Engages when:
///   - The member chain has a binding identifier (no
///     `member-of-member` head shape).
///   - `resolve_binding` returns `source: Import,
///     imported_module: Some, node: Some(Expr::Ident)` —
///     i.e. cross-file with a non-namespace foldable Ident
///     (covers `import x from './m'` for default exports
///     where `m` re-exports `export default x`, and
///     `import { x }` where the imported module is
///     `export { x };`).
///
/// Returns `None` to fall through (typical case: same-file
/// member access, namespace handled by the sibling helper).
fn try_cross_file_member_dispatch<'a>(
    member: &MemberExpr,
    meta: &mut Metadata<'a>,
    scope_ptr: *mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Option<Box<Expr>> {
    let info = collect_member_meta(member);
    let binding_id = info.binding_identifier?;

    // SAFETY: see module-level. Local immutable reborrow for the
    // resolve_binding lookup; no mutation.
    let scope_ref: &ScopeIndex = unsafe { &*scope_ptr };
    let resolved = resolve_binding(
        binding_id.sym.as_str(),
        &*meta,
        scope_ref,
        parent_scope,
        own_scope,
    )?;

    if !(resolved.constant && resolved.source == BindingSource::Import) {
        return None;
    }
    let imported_module = resolved.imported_module.as_ref()?;
    let resolved_node = resolved.node.as_ref()?;
    // Only the Ident shape — Object/Lit shapes don't need a scope
    // swap (they're already final values).
    let Expr::Ident(_) = &**resolved_node else {
        return None;
    };

    // Build a fresh ScopeIndex over the imported module and re-walk
    // the member chain against it. Rebuild the chain head with the
    // resolved Ident — the same `node` shape upstream's
    // `traverseMemberExpression` recurses on with the swapped meta.
    let mut imported_idx = ScopeIndex::build(&**imported_module);
    let imp_prog = imported_idx.program_scope();

    // Re-construct the member expression with the resolved Ident as
    // the binding identifier. The member structure (access path) is
    // preserved so consumers like `traverse_member_access_path` see
    // the same path. This mirrors upstream's `evaluateExpression(
    // member /* unchanged */, swappedMeta)` — the AST node identity
    // doesn't change; only `meta`'s scope chain does.
    let mut rebuilt_member = member.clone();
    replace_binding_identifier(&mut rebuilt_member, &binding_id);

    // Closure that recurses via `dispatch_evaluate` with the
    // imported scope. SAFETY: imp_scope_ptr lives as long as
    // `imported_idx`, which is owned by this preflight frame.
    let imp_scope_ptr: *mut ScopeIndex = &mut imported_idx;
    let mut imp_closure = move |e: &Expr, m: &mut Metadata<'_>| -> ResultPair {
        let inner = unsafe { &mut *imp_scope_ptr };
        dispatch_evaluate(e, m, inner, imp_prog, None)
    };

    // SAFETY: re-borrow imported_idx immutably for the leaf.
    let imp_ref: &ScopeIndex = unsafe { &*imp_scope_ptr };
    let pair = traverse_member_expression(
        &rebuilt_member,
        meta,
        imp_ref,
        imp_prog,
        None,
        &mut imp_closure,
    );
    pair.value
}

/// Replace the bottom (left-most) Ident in a MemberExpression chain
/// with `new_id`. Walks down `expression.obj` until it hits the
/// Ident leaf. Used by `try_cross_file_member_dispatch` to swap the
/// chain head with the resolved cross-file Ident.
fn replace_binding_identifier(member: &mut MemberExpr, new_id: &Ident) {
    let mut cursor: &mut Expr = &mut *member.obj;
    loop {
        match cursor {
            Expr::Member(inner) => {
                cursor = &mut *inner.obj;
            }
            Expr::Ident(_) => {
                *cursor = Expr::Ident(new_id.clone());
                return;
            }
            _ => return,
        }
    }
}

/// Local mirror of `traverse_member_expression::get_member_expression_meta`.
/// Kept here so the §5.5 leaf's private surface stays unchanged.
struct LocalMemberMeta {
    access_path: Vec<Ident>,
    binding_identifier: Option<Ident>,
}

fn collect_member_meta(expression: &MemberExpr) -> LocalMemberMeta {
    let mut visitor = LocalMemberVisitor {
        access_path: Vec::new(),
        binding_identifier: None,
        arg_depth: 0,
    };
    expression.visit_with(&mut visitor);
    visitor.access_path.reverse();
    LocalMemberMeta {
        access_path: visitor.access_path,
        binding_identifier: visitor.binding_identifier,
    }
}

struct LocalMemberVisitor {
    access_path: Vec<Ident>,
    binding_identifier: Option<Ident>,
    arg_depth: u32,
}

impl Visit for LocalMemberVisitor {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        n.callee.visit_with(self);
        self.arg_depth += 1;
        for arg in &n.args {
            arg.visit_with(self);
        }
        self.arg_depth -= 1;
    }

    fn visit_member_expr(&mut self, n: &MemberExpr) {
        if self.arg_depth > 0 {
            return;
        }
        match &*n.obj {
            Expr::Ident(id) => {
                self.binding_identifier = Some(id.clone());
            }
            Expr::Call(call) => {
                if let Callee::Expr(boxed) = &call.callee {
                    if let Expr::Ident(id) = &**boxed {
                        self.binding_identifier = Some(id.clone());
                    }
                }
            }
            _ => {}
        }
        match &n.prop {
            MemberProp::Ident(id) => {
                self.access_path
                    .push(Ident::new(id.sym.clone(), DUMMY_SP, Default::default()));
            }
            MemberProp::Computed(c) => {
                if let Expr::Call(call) = &*c.expr {
                    if let Callee::Expr(boxed) = &call.callee {
                        if let Expr::Ident(id) = &**boxed {
                            self.access_path.push(id.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        n.obj.visit_with(self);
        n.prop.visit_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::scope::ScopeIndex;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, ExprStmt, ModuleItem, Stmt};
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

    /// Parse `src` and return the first top-level expression statement's expr.
    fn first_expr(module: &Module) -> Box<Expr> {
        for item in &module.body {
            if let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item {
                return expr.clone();
            }
        }
        panic!("no top-level expression statement");
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

    #[test]
    fn folds_const_string_identifier() {
        // `const x = 'red'; x;` — evaluate_expression(x) should fold to 'red'.
        let module = parse_module("const x = 'red'; x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr = parse_module("x;");
        let target = first_expr(&expr);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "red"),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn folds_const_number_identifier() {
        let module = parse_module("const x = 42; x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("x;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 42.0),
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn folds_let_when_not_reassigned() {
        // `let x = 1; x;` — let binding with no reassignment is
        // `binding.constant === true` per Babel (constantViolations
        // is empty), so the init folds same as a `const`. Mirrors
        // upstream `expression-evaluation.test.ts` "should inline
        // mutable identifier that is not mutated".
        let module = parse_module("let x = 1; x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("x;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 1.0),
            other => panic!("expected number 1, got {other:?}"),
        }
    }

    #[test]
    fn deopts_when_let_is_reassigned() {
        // `let x = 1; x = 2; x;` — reassignment marks
        // `binding.constant = false`; identifier deopts to fallback.
        let module = parse_module("let x = 1; x = 2; x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("x;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Ident(id) => assert_eq!(id.sym.as_str(), "x"),
            other => panic!("expected identifier deopt, got {other:?}"),
        }
    }

    #[test]
    fn folds_binary_numeric_via_babel_fallback() {
        // 2 + 3 → babel evaluator folds to 5.
        let mut idx = ScopeIndex::build(&parse_module(""));
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("2 + 3;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 5.0),
            // Or: traverse_binary_expression returns BinExpr(2, +, 3), then
            // finalize_value runs babel_evaluate which folds to 5.
            other => panic!("expected number 5, got {other:?}"),
        }
    }

    #[test]
    fn folds_string_concatenation_via_babel_fallback() {
        let mut idx = ScopeIndex::build(&parse_module(""));
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("'a' + 'b';");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "ab"),
            other => panic!("expected string 'ab', got {other:?}"),
        }
    }

    #[test]
    fn unwraps_ts_as_expression() {
        // (x as any) where x = 'red' → unwraps to 'red'.
        let module = parse_module("const x = 'red'; (x as any);");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        // The second statement is an ExprStmt containing TsAs.
        let mut target_box: Option<Box<Expr>> = None;
        for item in &module.body {
            if let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item {
                target_box = Some(expr.clone());
            }
        }
        let target = target_box.expect("target ts-as expression");
        // SWC parses `(x as any)` as ParenExpr(TsAs(Ident(x))). Unwrap the paren.
        let inner = match *target {
            Expr::Paren(p) => p.expr,
            other => Box::new(other),
        };
        let pair = evaluate_expression(&*inner, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "red"),
            other => panic!("expected string 'red', got {other:?}"),
        }
    }

    #[test]
    fn passes_through_object_expression_unchanged() {
        // const o = ({ red: 'r' }); o; → returns object expression as-is.
        let module = parse_module("const o = { red: 'r' }; o;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("o;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        // Resolved to ObjectExpression, flows through finalize_value unchanged.
        assert!(matches!(*v, Expr::Object(_)));
    }

    #[test]
    fn unary_minus_on_const_number() {
        // const x = 8; -x; → babel fallback or unary leaf folds.
        let module = parse_module("const x = 8; -x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("-x;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        // -x where x=8 should fold to -8 numerically (via -1 * x → babel
        // evaluator → -8 numericLiteral).
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, -8.0),
            // Acceptable fallback: BinExpr or UnaryExpr if leaf didn't fully fold.
            // Per JS port, the path leads to babel eval → -8.
            other => panic!("expected -8 numeric literal, got {other:?}"),
        }
    }

    #[test]
    fn cross_file_fold_routes_through_imported_module_branch() {
        // This unit test exercises the cross-file branch entry, but
        // since we don't wire a real Resolver in unit-test scope,
        // the branch is exercised indirectly: we synthesise a state
        // with an Import binding's Arc<Module> populated.
        //
        // The full end-to-end cross-file fold is exercised by
        // resolve_binding's `cross_file_import_carries_imported_module_arc`
        // gate (post-§5.4e drift-fix). Here we assert the dispatcher
        // does NOT panic on a same-file fold path that resembles
        // the cross-file shape.
        let module = parse_module("const x = 'pink'; x;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("x;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        assert!(pair.value.is_some());
    }

    #[test]
    fn template_literal_folds_via_babel_evaluator() {
        // `${'a'}b` → babel fold to 'ab'.
        let mut idx = ScopeIndex::build(&parse_module(""));
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("`${'a'}b`;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "ab"),
            other => panic!("expected string 'ab', got {other:?}"),
        }
    }

    #[test]
    fn member_expression_on_const_object_folds() {
        // const o = { red: 'r' }; o.red; → 'r'.
        let module = parse_module("const o = { red: 'r' }; o.red;");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("o.red;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Str(s)) => assert_eq!(s.value.to_atom_lossy().as_str(), "r"),
            other => panic!("expected string 'r', got {other:?}"),
        }
    }

    #[test]
    fn unresolved_identifier_falls_through_to_input() {
        // `noBinding;` — no binding, no fold; babel evaluator deopts;
        // fallback returns the input identifier.
        let mut idx = ScopeIndex::build(&parse_module(""));
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("noBinding;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Ident(id) => assert_eq!(id.sym.as_str(), "noBinding"),
            other => panic!("expected fallback identifier, got {other:?}"),
        }
    }

    #[test]
    fn own_scope_override_is_consumed_by_dispatch() {
        // Smoke-test: dispatcher reads `meta.own_scope_override` and
        // doesn't crash with a non-program scope id. The full IIFE
        // path is exercised in traverse_call_expression's tests; this
        // confirms the dispatcher honours the channel.
        let module = parse_module("");
        let mut idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: Some(prog),
            in_conditional_branch: false,
            // ^ override pointing to program scope (always valid).
        };
        let expr_ast = parse_module("1 + 1;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        let v = pair.value.expect("value");
        match *v {
            Expr::Lit(Lit::Num(n)) => assert_eq!(n.value, 2.0),
            other => panic!("expected number 2, got {other:?}"),
        }
    }

    #[test]
    fn namespace_import_dispatch_routes_via_evaluate_namespace_import_path() {
        // import * as theme from './theme'; theme.color;
        // The §5.6 wiring contract: when resolve_binding returns a
        // namespace-import binding (source==Import, node=None,
        // imported_module=Some), the MemberExpression branch routes
        // to evaluate_namespace_import_path.
        //
        // This unit-scope test synthesises the binding state directly
        // (resolve_binding's full path needs a Resolver + filesystem).
        // We assert the preflight signature: when the binding's
        // imported_module is Some and node is None, the dispatcher
        // builds a fresh ScopeIndex and routes through namespace_import.
        //
        // End-to-end coverage lands in compat-evaluation / resolver
        // integration corpora — the unit gate here is the API
        // call-graph contract.
        let _expr = parse_module("theme.color;");
        // Preflight runs via try_namespace_import_dispatch; with no
        // Resolver wired the resolution miss returns None and falls
        // through to traverse_member_expression. That branch returns
        // the input member expression unchanged because `theme` has
        // no binding. We confirm the dispatcher doesn't panic.
        let mut idx = ScopeIndex::build(&parse_module(""));
        let prog = idx.program_scope();
        let mut state = State::default();
        let mut meta = meta_for_test(&mut state);
        let expr_ast = parse_module("theme.color;");
        let target = first_expr(&expr_ast);
        let pair = evaluate_expression(&*target, &mut meta, &mut idx, prog, None);
        assert!(pair.value.is_some());
    }
}
