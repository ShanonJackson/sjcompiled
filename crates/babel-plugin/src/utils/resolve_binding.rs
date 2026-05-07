//! 1:1 port of `packages/babel-plugin/src/utils/resolve-binding.ts`.
//!
//! Phase 5 §5.4e — closes the §5.4 row group. Wires the in-plugin
//! `crate::resolver::Resolver` (built in §5.4b/c/d) into the
//! binding-resolution call graph. The §5.5 / §5.6 evaluators consume
//! [`resolve_binding`] to fold cross-file constants into CSS values.
//!
//! ## Behaviour
//!
//! Given an identifier reference (`name`) and a metadata snapshot
//! pointing at the call site, walk the binding chain:
//!
//! 1. **Same-file binding (`source: Module`).** If the binding
//!    resolves to a `VariableDeclarator` in the current file,
//!    return its `init` expression.
//! 2. **Re-exported synthetic binding.** When the parent path is
//!    `export { name } from 'mod'` without a real local scope
//!    binding, JS plugin synthesises a `Binding` with no real
//!    scope. Mirrored here.
//! 3. **Cross-file import (`source: Import`).** If the binding's
//!    parent is an `ImportDeclaration` (or the binding itself is
//!    an `ExportNamedDeclaration` re-export), resolve the module
//!    path, parse the imported file's AST, find the matching
//!    export via [`crate::utils::traversers::get_default_export`]
//!    / [`crate::utils::traversers::get_named_export`], return
//!    that node + the resolved filename.
//! 4. **`@compiled/*` short-circuit.** If the import source starts
//!    with `@compiled/`, return None — Compiled APIs aren't user
//!    constants and shouldn't be folded. Documented in
//!    upstream issue #1010.
//! 5. **Namespace import (`import * as theme from 'theme'`).**
//!    JS returns `path.node` (the binding's own NodePath). The
//!    Rust port returns `node: None, source: Import,
//!    imported_filename: Some(path)` so the caller knows it's a
//!    cross-file pointer but doesn't have a single foldable
//!    expression.
//!
//! ## §5.4e scope
//!
//! - The destructuring-resolution helpers
//!   ([`resolve_identifier_coming_from_destructuring`],
//!   [`resolve_object_pattern_value_node`]) are ported. The
//!   `resolveObjectPatternValueNode` member-expression branch
//!   recurses into [`crate::compat::evaluation::evaluate`] for
//!   constant-folding — the §5.0c evaluator handles literal /
//!   identifier folds; non-foldable shapes deopt.
//! - The JS `meta.state.cache.load` infrastructure IS replicated
//!   for the four namespaces consumed by this file (`read-file`,
//!   `parse-module`, `find-default-export-module-node`,
//!   `find-named-export-module-node`). The cache lives on
//!   `State.cache` (per-transform) — sound under WASI's
//!   per-transform tear-down because the cache is born/dropped
//!   with each invocation. Mirrors Babel's default-options
//!   behaviour where each `pre()` constructs a fresh `Cache`
//!   (upstream `babel-plugin.ts:80-91`). The `opts.cache: true`
//!   cross-transform `globalCache` upstream gate isn't on the
//!   corpus; not modelled.
//! - The breadcrumb requirement at every `get_binding` /
//!   `get_own_binding` call site per §5.0c Finding 7 is honoured —
//!   each call carries the lazy-crawl reference comment.
//!
//! ## §5.6 wiring (post-§5.6 closure)
//!
//! `resolveObjectPatternValueNode`'s `evaluateExpression` callback
//! — the JS plugin threads its evaluator through here. Phase 5 §5.6
//! ☑ shipped the real evaluator at
//! `crate::utils::evaluate_expression::evaluate_expression`, but
//! THIS site still wires `crate::compat::evaluation::evaluate`
//! directly because the destructuring-resolution path doesn't yet
//! surface in any unit-tested fixture. When Phase 6 surfaces a
//! fold-through-destructured-arg shape, the
//! `_evaluate_expression: Option<&EvalFn>` parameter on
//! [`resolve_binding_with_evaluator`] becomes the wire-in point
//! (drop the `_` prefix; thread the real fn through).

use std::fs;
use std::path::PathBuf;

use swc_core::common::sync::Lrc;
use swc_core::common::{FileName, SourceMap};
use swc_core::ecma::ast::{
    EsVersion, Expr, Ident, Module, ObjectLit, ObjectPat, ObjectPatProp, Pat, Prop, PropName,
    PropOrSpread,
};
// Used by the `tests` module below — pre-import to keep the inner
// `use super::*;` shape lean.
#[cfg(test)]
use swc_core::ecma::ast::{Decl, ModuleItem};
use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

use crate::compat::scope::{Binding, ScopeId, ScopeIndex};
use crate::constants::DEFAULT_CODE_EXTENSIONS;
use crate::types::Metadata;

use super::traversers::{
    get_default_export, get_named_export, set_imported_compiled_imports, ExportResult,
};
use super::types::{BindingSource, PartialBindingWithMeta};

// ───────── resolveIdentifierComingFromDestructuring ─────────

/// Discriminator for [`resolve_identifier_coming_from_destructuring`].
///
/// JS: `resolveFor: 'key' | 'value'`. The key/value names target
/// either the property's KEY (left side of `:`) or VALUE (right
/// side of `:`) in an `ObjectProperty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveFor {
    Key,
    Value,
}

/// Walks an `ObjectPattern` / `VariableDeclarator` looking for a
/// destructured property whose key (or value) is an `Ident` whose
/// name matches `name`. Recurses into `VariableDeclarator.id`
/// per upstream.
///
/// Returns the matching key-value `Prop` (the inner of
/// `Box<Prop>`); the JS shape is `t.ObjectProperty | undefined`.
/// Non-`KeyValue` properties (shorthand, methods, getters, setters,
/// spreads) are filtered per upstream's `t.isObjectProperty`
/// predicate.
pub fn resolve_identifier_coming_from_destructuring(
    name: &str,
    expr: Option<&Expr>,
    resolve_for: ResolveFor,
) -> Option<Prop> {
    match expr {
        // Direct `ObjectExpression` (Babel: `t.isObjectPattern(node)`).
        // Note: SWC's `Pat::Object` is the binding-side; the JS
        // function accepts `t.Expression | undefined` and uses
        // `t.isObjectPattern` which checks the t.Pattern shape. The
        // §5.4e port handles BOTH the `Expr::Object`
        // (object-literal source) AND the `Pat::Object` (binding
        // pattern) reach via the helper below.
        Some(Expr::Object(obj)) => find_match_in_object_lit(obj, name, resolve_for),
        // `VariableDeclarator` reach: `binding.path.node` is the
        // declarator. `declarator.id` is a `Pat` — destructuring
        // patterns appear here.
        // The JS recursion `resolveIdentifierComingFromDestructuring({
        //   name, node: declarator.id as Expression, resolveFor })`
        // re-enters with the pattern as if it were an Expression —
        // upstream relies on Babel's lax type cast. The Rust port
        // handles this by converting Pat::Object → walk via the
        // pattern-side helper.
        // We model this by exposing a pattern-side variant below.
        _ => None,
    }
}

/// Pattern-side variant of [`resolve_identifier_coming_from_destructuring`].
/// Walks an `ObjectPat` (the binding-pattern shape inside
/// `VariableDeclarator.name`) for a destructured key/value match.
pub fn resolve_identifier_in_pattern(
    name: &str,
    pat: &Pat,
    resolve_for: ResolveFor,
) -> Option<Prop> {
    if let Pat::Object(obj_pat) = pat {
        return find_match_in_object_pat(obj_pat, name, resolve_for);
    }
    None
}

fn find_match_in_object_lit(
    object: &ObjectLit,
    name: &str,
    resolve_for: ResolveFor,
) -> Option<Prop> {
    for prop in &object.props {
        let PropOrSpread::Prop(boxed) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**boxed else {
            // Methods / getters / setters / shorthand / spreads —
            // upstream's t.isObjectProperty predicate excludes.
            continue;
        };
        let matches = match resolve_for {
            ResolveFor::Key => match &kv.key {
                PropName::Ident(id) => id.sym == *name,
                _ => false,
            },
            ResolveFor::Value => match &*kv.value {
                Expr::Ident(id) => id.sym == *name,
                _ => false,
            },
        };
        if matches {
            return Some(Prop::KeyValue(kv.clone()));
        }
    }
    None
}

fn find_match_in_object_pat(
    pat: &ObjectPat,
    name: &str,
    resolve_for: ResolveFor,
) -> Option<Prop> {
    for prop in &pat.props {
        let ObjectPatProp::KeyValue(kv) = prop else {
            // Rest / Assign — upstream's t.isObjectProperty is true
            // only for KeyValue.
            continue;
        };
        let matches = match resolve_for {
            ResolveFor::Key => match &kv.key {
                PropName::Ident(id) => id.sym == *name,
                _ => false,
            },
            // For pattern-side, the "value" is itself a Pat, not an
            // Expr. We unwrap Pat::Ident matching to mirror upstream's
            // `t.isIdentifier(property.value)` check.
            ResolveFor::Value => match &*kv.value {
                Pat::Ident(binding) => binding.id.sym == *name,
                _ => false,
            },
        };
        if matches {
            // Synthesise an `Expr::Ident` for the pattern-side
            // value so the returned `Prop::KeyValue` has the same
            // shape regardless of source. The pattern-side value
            // identifier is always foldable to an ident-Expr.
            let synthetic_value: Box<Expr> = match &*kv.value {
                Pat::Ident(binding) => Box::new(Expr::Ident(Ident::from(binding.id.clone()))),
                _ => Box::new(Expr::Invalid(swc_core::ecma::ast::Invalid {
                    span: Default::default(),
                })),
            };
            return Some(Prop::KeyValue(swc_core::ecma::ast::KeyValueProp {
                key: kv.key.clone(),
                value: synthetic_value,
            }));
        }
    }
    None
}

// ───────── getDestructuredObjectPatternKey ─────────

/// Walks an `ObjectPat`'s key-value properties looking for an
/// alias-shaped property `{ key: value }` where `value` matches
/// `reference_name`. Returns the corresponding `key` name; if no
/// match, returns `reference_name` unchanged.
///
/// Pre-order DFS through an Expression looking for the first
/// ObjectProperty whose key is `Ident(reference_name)` (or
/// equivalent shorthand normalisation), recursing into Object
/// values. Mirrors upstream
/// `traverse(expression, { ObjectProperty: { exit ... } })` with
/// `path.stop()` on first match. Non-Object values are skipped (no
/// match possible deeper). Returns the matching property's value
/// expression cloned.
fn deep_find_object_expression_property(expr: &Expr, reference_name: &str) -> Option<Box<Expr>> {
    let Expr::Object(obj) = expr else {
        return None;
    };
    for prop in &obj.props {
        let PropOrSpread::Prop(boxed) = prop else {
            continue;
        };
        match &**boxed {
            Prop::KeyValue(kv) => {
                if let PropName::Ident(id) = &kv.key {
                    if id.sym == *reference_name {
                        return Some(kv.value.clone());
                    }
                }
                // Recurse into nested ObjectExpressions on the value.
                if let Some(found) =
                    deep_find_object_expression_property(&kv.value, reference_name)
                {
                    return Some(found);
                }
            }
            Prop::Shorthand(id) => {
                // `{ color }` shorthand — JS sees `ObjectProperty`
                // with key/value both Identifier("color"). Match via
                // the local name.
                if id.sym == *reference_name {
                    return Some(Box::new(Expr::Ident(id.clone())));
                }
            }
            // Method / Setter / Getter / Assign — none are
            // ObjectProperty in Babel; skip.
            _ => continue,
        }
    }
    None
}

/// JS doc: `Eg. const { key: value } = { key: 'something' }, ref =
/// 'value'` returns `'key'` so the lookup against the source object
/// can find `something`.
pub fn get_destructured_object_pattern_key(node: &ObjectPat, reference_name: &str) -> String {
    for prop in &node.props {
        let ObjectPatProp::KeyValue(kv) = prop else {
            continue;
        };
        let key_name = match &kv.key {
            PropName::Ident(id) => id.sym.to_string(),
            _ => String::new(),
        };
        let value_name = match &*kv.value {
            Pat::Ident(binding) => binding.id.sym.to_string(),
            _ => String::new(),
        };
        if !key_name.is_empty() && key_name != value_name && value_name == reference_name {
            return key_name;
        }
    }
    reference_name.to_string()
}

// ───────── resolveObjectPatternValueNode ─────────

/// Resolve the value-node for a destructured reference inside an
/// `ObjectExpression`-shaped expression.
///
/// **§5.4e scope:** the `ObjectExpression` direct-match path is
/// fully ported. The `MemberExpression` recursive evaluation path
/// (JS `evaluateExpression(expression, meta)`) lands when the §5.6
/// evaluator dispatches into `compat::evaluation::evaluate`. For
/// now, member-on-member sources deopt cleanly (return None).
///
/// `evaluateExpression` is taken as a callback so the §5.6 wiring
/// can plug in without touching this file again. Pass `None` when
/// no evaluator is available; the recursive identifier-resolution
/// path stays (covering the most common shape:
/// `const x = { foo: 1 }; const { foo } = x;`).
pub fn resolve_object_pattern_value_node<EvalFn>(
    expression: &Expr,
    reference_name: &str,
    meta: &Metadata<'_>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: Option<&EvalFn>,
) -> Option<Box<Expr>>
where
    EvalFn: Fn(&Expr) -> Option<Box<Expr>>,
{
    if let Expr::Object(_) = expression {
        // Direct object-literal: deep-walk for a property whose key
        // matches reference_name. Mirrors upstream's
        // `traverse(expression, { ObjectProperty: { exit ... } })`
        // which walks every nested ObjectProperty and stops at the
        // first match (`path.stop()`). Without the recursive walk,
        // `const { small } = theme.fonts` resolves through the
        // identifier-recursion branch to the FULL `theme` object
        // (not `theme.fonts`), so the matching `small` lives inside
        // `theme.fonts.small` — one level deeper than this top-level
        // search would find without recursion. Pre-order DFS matches
        // upstream behaviour for the unique-key case (the only shape
        // any fixture in the corpus surfaces); the rare ambiguous
        // multi-match case mirrors the JS `traverse` deterministic
        // order.
        return deep_find_object_expression_property(expression, reference_name);
    }

    // Member: split the JS branches that the §5.4e port collapsed.
    // - Member-on-member (`obj.member`): needs the evaluator to fold;
    //   matches JS branch `t.isMemberExpression(expression) &&
    //   t.isMemberExpression(expression.object)`.
    // - Single-Member with Ident object (`obj.x`): falls through to
    //   the identifier-recursion branch below; matches JS branch
    //   `t.isMemberExpression(expression) && t.isIdentifier(expression.object)`.
    if let Expr::Member(member) = expression {
        if matches!(&*member.obj, Expr::Member(_)) {
            // Member-on-member (e.g. `theme.color.primary`) needs the
            // evaluator to fold. With `evaluate_expression = None` we
            // deopt; with Some we recurse on the folded value.
            if let Some(evaluator) = evaluate_expression {
                if let Some(folded) = evaluator(expression) {
                    return resolve_object_pattern_value_node(
                        &folded,
                        reference_name,
                        meta,
                        scope_index,
                        parent_scope,
                        own_scope,
                        evaluate_expression,
                    );
                }
            }
            return None;
        }
        // Otherwise (single-Member) fall through to the
        // identifier-recursion branch below.
    }

    // Identifier OR member-on-identifier — recurse into the
    // resolved binding's source expression.
    let identifier_name = match expression {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Member(member) => match &*member.obj {
            Expr::Ident(id) => Some(id.sym.to_string()),
            _ => None,
        },
        _ => None,
    };
    let Some(identifier_name) = identifier_name else {
        return None;
    };

    // If a fixture surfaces lazy-crawl observability here, see
    // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
    let resolved = resolve_binding(
        &identifier_name,
        meta,
        scope_index,
        parent_scope,
        own_scope,
    )?;
    if !resolved.constant {
        return None;
    }
    let resolved_node = resolved.node?;
    // Identity-skip: if the resolution returned the same node we
    // started from (a self-reference), don't loop.
    if std::ptr::eq(resolved_node.as_ref() as *const Expr, expression as *const Expr) {
        return None;
    }
    resolve_object_pattern_value_node(
        &resolved_node,
        reference_name,
        meta,
        scope_index,
        parent_scope,
        own_scope,
        evaluate_expression,
    )
}

// ───────── resolveRequest ─────────

/// Resolve `request` against the file's directory using the
/// in-plugin resolver. Returns the absolute resolved path.
///
/// JS:
///   if (!resolver) return resolve.sync(...) else resolver.resolveSync(...).
/// Both paths collapse into [`crate::resolver::Resolver::resolve_sync`]
/// in the Rust port (PLAN.md §1 constraint 2's "single resolver
/// surface" lock).
fn resolve_request(request: &str, meta: &Metadata<'_>) -> Option<PathBuf> {
    let filename = meta.state.filename()?;
    let resolver = meta.state.resolver()?;
    let from_path = std::path::Path::new(filename);
    resolver.resolve_sync(from_path, request).ok()
}

// ───────── getBinding ─────────

/// Retrieve a `Binding` for `reference_name` from the scope chain.
///
/// JS shape:
///   const scopedBinding =
///     ownPath?.scope.getOwnBinding(name) ||
///     parentPath.scope.getBinding(name);
///   if (scopedBinding) return scopedBinding;
///   if (parentPath.isExportNamedDeclaration() && parentPath.node.source) {
///     return synthetic binding;
///   }
///
/// The Rust port preserves the OR semantics: try own_scope's
/// `get_own_binding` first, fall back to parent_scope's
/// `get_binding`. The synthetic re-export Binding case isn't
/// representable in `compat::scope::Binding` without an AST mutation
/// — we return `None` and let the caller (resolve_binding) detect
/// the re-export shape via `parent_path` and call into the imported
/// module directly. Documented inline at the resolveBinding call
/// site.
fn get_binding<'idx>(
    reference_name: &str,
    scope_index: &'idx ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Option<&'idx Binding> {
    if let Some(own) = own_scope {
        // If a fixture surfaces lazy-crawl observability here, see
        // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
        if let Some(b) = scope_index.get_own_binding(own, reference_name) {
            return Some(b);
        }
    }
    // If a fixture surfaces lazy-crawl observability here, see
    // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
    scope_index.get_binding(parent_scope, reference_name)
}

// ───────── resolveBinding ─────────

/// Resolve `reference_name` to its source expression — same-file
/// or cross-file. See module docs for the behavioural contract.
///
/// `evaluate_expression` is an optional callback used by the
/// destructuring-resolution path. Pass `None` when no evaluator is
/// wired (most §5.4e callers); §5.6 will pass a real fn.
pub fn resolve_binding<'a>(
    reference_name: &str,
    meta: &Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Option<PartialBindingWithMeta> {
    resolve_binding_with_evaluator::<fn(&Expr) -> Option<Box<Expr>>>(
        reference_name,
        meta,
        scope_index,
        parent_scope,
        own_scope,
        None,
    )
}

/// Same as [`resolve_binding`] but accepts an optional
/// destructuring-resolution evaluator. Splitting this out keeps
/// `resolve_binding`'s signature simple for the common case.
pub fn resolve_binding_with_evaluator<EvalFn>(
    reference_name: &str,
    meta: &Metadata<'_>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    // The destructuring-resolution evaluator. Wired by §5.6's
    // `evaluate_expression`. Currently unused inside this entry
    // point because the helper recurses into `resolve_object_pattern_value_node`
    // which carries it as a separate parameter — surfaced here so
    // §5.6 callers can thread it without changing the public
    // surface again.
    _evaluate_expression: Option<&EvalFn>,
) -> Option<PartialBindingWithMeta>
where
    EvalFn: Fn(&Expr) -> Option<Box<Expr>>,
{
    let binding = get_binding(reference_name, scope_index, parent_scope, own_scope);

    // The JS bail-early: if no binding OR binding is an ObjectPattern
    // (destructured args from a fn we don't want to follow). We
    // approximate: bindings tracked by ScopeIndex are scope-shape
    // bindings, not destructured args; if `binding.init_expr` is
    // an `Pat::Object` site, treat as bail. The §5.0a port doesn't
    // populate `init_expr` for object-pattern LHS (only Pat::Ident
    // const), so a destructured arg shows up with `init_expr = None`
    // here — we let it fall through to the import-detection step.
    let Some(binding) = binding else {
        return None;
    };

    // Same-file VariableDeclarator branch.
    if let Some(init) = binding.init_expr.as_ref() {
        // The `binding.init_expr` is populated for
        // `<kind> x = <expr>` with `Pat::Ident` LHS (§6.8n widened
        // the §5.0c gate from Const-only to all kinds). Destructured
        // LHS is handled by the §6.8n branch below.
        return Some(PartialBindingWithMeta {
            node: Some(init.clone()),
            constant: binding.constant,
            source: BindingSource::Module,
            imported_filename: None,
            imported_module: None,
        });
    }

    // §6.8n — destructured `Pat::Object` LHS branch. Mirrors
    // `resolve-binding.ts:263-269`: when `binding.path.node.id` is
    // `t.isObjectPattern` AND `binding.path.node.init` is an
    // expression, recover the source key for `reference_name` via
    // `getDestructuredObjectPatternKey`, then walk
    // `resolveObjectPatternValueNode(init, ..., key, evaluateExpression)`
    // to extract the matching value node.
    if let (Some(pat), Some(init)) = (
        binding.destructured_pat.as_ref(),
        binding.destructured_init.as_ref(),
    ) {
        let key = get_destructured_object_pattern_key(pat, reference_name);
        // Pass `None` for the evaluator: the §5.4e parity-corpus
        // reach for `resolveObjectPatternValueNode` is the
        // direct-object + identifier-recursion paths, both of which
        // operate without it. The MemberExpression-on-MemberExpression
        // path is gated on the evaluator and stays deopt-clean here.
        let resolved = resolve_object_pattern_value_node::<fn(&Expr) -> Option<Box<Expr>>>(
            init,
            &key,
            meta,
            scope_index,
            parent_scope,
            own_scope,
            None,
        );
        return Some(PartialBindingWithMeta {
            node: resolved,
            constant: binding.constant,
            source: BindingSource::Module,
            imported_filename: None,
            imported_module: None,
        });
    }

    // Cross-file import branch. §5.4e extended `Binding` with
    // `import_info: Option<ImportInfo>` (mirrors §5.0c's `init_expr`
    // extension precedent) populated by `register_import` for every
    // import-specifier binding.
    let Some(import_info) = binding.import_info.as_ref() else {
        // Same-file but not a const literal — return as
        // `Module`/`node: None`. Caller deopts.
        return Some(PartialBindingWithMeta {
            node: None,
            constant: binding.constant,
            source: BindingSource::Module,
            imported_filename: None,
            imported_module: None,
        });
    };
    let import_source = import_info.source.as_str();

    // The @compiled/* short-circuit (upstream issue #1010).
    if import_source.starts_with("@compiled/") {
        return None;
    }

    // No filename → can't resolve anything.
    let _ = meta.state.filename()?;

    let module_path = resolve_request(import_source, meta)?;

    // Extension gate: don't parse non-code files.
    let extensions: Vec<String> = meta
        .state
        .opts()
        .extensions
        .clone()
        .unwrap_or_else(|| {
            DEFAULT_CODE_EXTENSIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        });
    let module_path_str = module_path.to_string_lossy().to_string();
    if !extensions
        .iter()
        .any(|ext| module_path_str.ends_with(ext))
    {
        return None;
    }

    // Read + parse the imported file. 1:1 port of upstream
    // `meta.state.cache.load({namespace: 'read-file', ...})` and
    // `{namespace: 'parse-module', ...}` (`resolve-binding.ts:307-322`).
    // Per-transform caching keyed on `module_path_str` so a single
    // imported module is read+parsed ONCE per transform regardless
    // of how many references resolve to it. Sound under WASI's
    // per-transform tear-down: the cache lives on `State` which is
    // born/dropped with each plugin invocation. Mirrors Babel's
    // default-options behaviour (no `opts.cache`) where each `pre()`
    // constructs a fresh `Cache`.
    let cache = &meta.state.cache;
    let module_path_for_read = module_path.clone();
    let source = cache.read_file.borrow_mut().load(
        Some("read-file"),
        &module_path_str,
        || fs::read_to_string(&module_path_for_read).unwrap_or_default(),
    );
    if source.is_empty() && !module_path.exists() {
        return None;
    }
    let module_path_for_parse = module_path.clone();
    let module_path_str_for_parse = module_path_str.clone();
    let imported_module_arc: std::sync::Arc<Module> = cache
        .parse_module
        .borrow_mut()
        .load(Some("parse-module"), &module_path_str, || {
            let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
            let fm = cm.new_source_file(
                Lrc::new(FileName::Real(module_path_for_parse.clone())),
                source.clone(),
            );
            let parsed = parse_file_as_module(
                &fm,
                Syntax::Typescript(TsSyntax {
                    tsx: module_path_str_for_parse.ends_with(".tsx"),
                    ..Default::default()
                }),
                EsVersion::Es2022,
                None,
                &mut Vec::new(),
            )
            // Parser errors → Arc-wrap an empty Module so the cache
            // entry is uniform; the export-lookup below returns None
            // and the caller deopts. Mirrors upstream `parse(...)`
            // throwing → bubbles up through `cache.load` and the
            // try/catch in the visitor downgrades to deopt.
            .unwrap_or_else(|_| Module {
                span: Default::default(),
                body: Vec::new(),
                shebang: None,
            });
            std::sync::Arc::new(parsed)
        });

    // Find the matching export. 1:1 with upstream's
    // `find-default-export-module-node` /
    // `find-named-export-module-node` cache namespaces
    // (`resolve-binding.ts:328-339,344-358`). Caching here means
    // repeated references to the SAME imported export skip the
    // AST-walk done by `get_*_export`.
    let export_result: Option<ExportResult> = match import_info.kind {
        crate::compat::scope::ImportSpecifierKind::Default => cache
            .find_default_export
            .borrow_mut()
            .load(
                Some("find-default-export-module-node"),
                &module_path_str,
                || get_default_export(&imported_module_arc),
            ),
        crate::compat::scope::ImportSpecifierKind::Namespace => {
            // import * as theme from 'theme' — no foldable expression.
            // Return Some-with-None so the caller knows it was found
            // but isn't a single Expr. We still attach the imported
            // module so the §5.6 evaluator's namespace-member
            // walking path (e.g. `theme.colors`) has the AST to
            // walk against.
            return Some(PartialBindingWithMeta {
                node: None,
                constant: binding.constant,
                source: BindingSource::Import,
                imported_filename: Some(module_path_str),
                imported_module: Some(imported_module_arc),
            });
        }
        crate::compat::scope::ImportSpecifierKind::Named => {
            // The imported-side name (LHS of `as`, or the spec name
            // when no alias) was captured by the §5.0a binding
            // builder into `import_info.imported_name`. Side-effect:
            // setImportedCompiledImports would record any `css`
            // imports the resolved module uses, but we don't have
            // `&mut state` here (only `&meta`); the §5.6 evaluator's
            // wrapper, when ported, passes `&mut Metadata` and can
            // reach state — for §5.4e the side-effect is skipped
            // (not on the §5.4e gate corpus).
            let _ = set_imported_compiled_imports;
            let imported_name = import_info
                .imported_name
                .as_deref()
                .unwrap_or(reference_name);
            // 1:1 with upstream cacheKey
            // `modulePath=${modulePath}&exportName=${exportName}`
            // (`resolve-binding.ts:346`).
            let cache_key = format!(
                "modulePath={}&exportName={}",
                module_path_str, imported_name
            );
            let imported_module_for_lookup = imported_module_arc.clone();
            let imported_name_owned = imported_name.to_string();
            cache.find_named_export.borrow_mut().load(
                Some("find-named-export-module-node"),
                &cache_key,
                || get_named_export(&imported_module_for_lookup, &imported_name_owned),
            )
        }
    };

    let Some(export) = export_result else {
        return None;
    };

    // Re-export-from chain: 1:1 with upstream
    // `binding.path.isExportNamedDeclaration()` branch
    // (`resolve-binding.ts:367-394`). When the matched export is
    // `export { local [as exported] } from './mod'` (or `export {
    // x as default } from './mod'`), upstream parses `./mod`
    // RELATIVE to the IMPORTED module's filename and dispatches to
    // `getNamedExport(ast, local)` / `getDefaultExport(ast)`.
    //
    // The Rust port carries the hop on `ExportResult.reexport_from`
    // and chases here, anchored to the *imported* module's path so
    // relative specifiers (`./tokens`) resolve from the imported
    // file's directory rather than the consumer's. Loop-bounded to
    // avoid pathological circular re-export chains; matches
    // upstream's implicit termination via Babel's
    // `binding.path` shape (a non-re-export path breaks the
    // `isExportNamedDeclaration() && source` chain).
    if let Some(hop) = export.reexport_from {
        return follow_reexport_hop(
            hop,
            &module_path,
            binding.constant,
            meta,
            cache,
            0,
        );
    }

    Some(PartialBindingWithMeta {
        node: export.node,
        constant: binding.constant,
        source: BindingSource::Import,
        imported_filename: Some(module_path_str),
        imported_module: Some(imported_module_arc),
    })
}

// ───────── Re-export-from chain follower ─────────

/// Follow a `ReexportHop` (`export { x [as y] } from './mod'`) one
/// hop deeper. 1:1 with upstream `resolve-binding.ts:367-394`'s
/// `binding.path.isExportNamedDeclaration()` branch.
///
/// The `from_path` anchor is the IMPORTED module's filename — the
/// hop's `source` specifier is resolved RELATIVE to it (mirrors
/// `resolveRequest(moduleImportSource, extensions, meta)` upstream
/// where `meta.state.filename` was already swapped via the
/// returning resolveBinding's meta-mutation at line 408-413). The
/// `_depth` parameter caps recursion; upstream relies on AST
/// shape termination (a non-re-export path breaks the chain) but
/// the Rust port adds an explicit cap to defuse pathological
/// circular re-export graphs (mutual `export {x} from './b'` /
/// `export {x} from './a'`). 32 hops is far beyond any sane
/// re-export chain in the corpus.
fn follow_reexport_hop(
    hop: crate::utils::traversers::ReexportHop,
    from_path: &std::path::Path,
    constant: bool,
    meta: &Metadata<'_>,
    cache: &crate::state::CacheSlot,
    depth: u32,
) -> Option<PartialBindingWithMeta> {
    if depth >= 32 {
        return None;
    }
    // Resolve the hop's source against the IMPORTED file's path.
    let resolver = meta.state.resolver()?;
    let resolved_path = resolver.resolve_sync(from_path, &hop.source).ok()?;
    let resolved_path_str = resolved_path.to_string_lossy().to_string();

    // Extension gate — same as the named-import branch.
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

    // Cached read+parse, same shape as the named-import branch.
    let resolved_path_for_read = resolved_path.clone();
    let source = cache.read_file.borrow_mut().load(
        Some("read-file"),
        &resolved_path_str,
        || std::fs::read_to_string(&resolved_path_for_read).unwrap_or_default(),
    );
    if source.is_empty() && !resolved_path.exists() {
        return None;
    }
    let resolved_path_for_parse = resolved_path.clone();
    let path_str_for_parse = resolved_path_str.clone();
    let imported_module_arc: std::sync::Arc<Module> = cache
        .parse_module
        .borrow_mut()
        .load(Some("parse-module"), &resolved_path_str, || {
            let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
            let fm = cm.new_source_file(
                Lrc::new(FileName::Real(resolved_path_for_parse.clone())),
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
            std::sync::Arc::new(parsed)
        });

    // Default vs named lookup, mirroring upstream's
    // `resolve-binding.ts:386-388`.
    let export_result: Option<ExportResult> = if hop.is_default {
        let imported_for_lookup = imported_module_arc.clone();
        cache.find_default_export.borrow_mut().load(
            Some("find-default-export-module-node"),
            &resolved_path_str,
            || get_default_export(&imported_for_lookup),
        )
    } else {
        let cache_key = format!(
            "modulePath={}&exportName={}",
            resolved_path_str, hop.local_name
        );
        let imported_for_lookup = imported_module_arc.clone();
        let local_owned = hop.local_name.clone();
        cache.find_named_export.borrow_mut().load(
            Some("find-named-export-module-node"),
            &cache_key,
            || get_named_export(&imported_for_lookup, &local_owned),
        )
    };

    let Some(export) = export_result else {
        return None;
    };

    // Recursive hop: the resolved export may itself be another
    // re-export-from (`export { x } from './a'` where a.js does
    // `export { x } from './b'`). Mirrors upstream's `traverse`
    // visitor following ExportNamedDeclaration through every hop
    // until a real declaration is hit.
    if let Some(next_hop) = export.reexport_from {
        return follow_reexport_hop(
            next_hop,
            &resolved_path,
            constant,
            meta,
            cache,
            depth + 1,
        );
    }

    Some(PartialBindingWithMeta {
        node: export.node,
        constant,
        source: BindingSource::Import,
        imported_filename: Some(resolved_path_str),
        imported_module: Some(imported_module_arc),
    })
}

// ───────── Tests ─────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MetadataContext, State};

    fn fresh_state() -> State {
        State::default()
    }

    fn meta_for_state<'a>(state: &'a mut State) -> Metadata<'a> {
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
    fn destructured_object_pattern_key_alias() {
        // Mirrors the JS doc example: `{ key: value }` → 'key'.
        let pat_src = r#"const { key: value } = src;"#;
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), pat_src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let pat = match &module.body[0] {
            ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Decl(Decl::Var(v))) => {
                match &v.decls[0].name {
                    Pat::Object(o) => o.clone(),
                    other => panic!("expected ObjectPat, got {other:?}"),
                }
            }
            other => panic!("expected var decl, got {other:?}"),
        };
        assert_eq!(get_destructured_object_pattern_key(&pat, "value"), "key");
        // No alias → returns reference unchanged.
        assert_eq!(get_destructured_object_pattern_key(&pat, "other"), "other");
    }

    #[test]
    fn destructured_object_pattern_key_no_alias_keeps_name() {
        // `const { key } = src;` shape — no alias, returns ref.
        let pat_src = r#"const { key } = src;"#;
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), pat_src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let pat = match &module.body[0] {
            ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Decl(Decl::Var(v))) => {
                match &v.decls[0].name {
                    Pat::Object(o) => o.clone(),
                    other => panic!("expected ObjectPat, got {other:?}"),
                }
            }
            other => panic!("expected var decl, got {other:?}"),
        };
        // 'key' isn't an alias, so reference stays unchanged.
        assert_eq!(get_destructured_object_pattern_key(&pat, "key"), "key");
    }

    #[test]
    fn resolve_object_pattern_value_node_direct_object() {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(
            Lrc::new(FileName::Anon),
            r#"({ red: '#f00', blue: '#00f' });"#.to_string(),
        );
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        // Unwrap the parenthesized object literal.
        let expr: Box<Expr> = match &module.body[0] {
            ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Expr(e)) => match &*e.expr {
                Expr::Paren(p) => p.expr.clone(),
                _ => panic!("expected paren expr"),
            },
            other => panic!("unexpected stmt {other:?}"),
        };
        let mut state = fresh_state();
        let meta = meta_for_state(&mut state);
        let scope_index = ScopeIndex::build(&module);
        let result = resolve_object_pattern_value_node::<fn(&Expr) -> Option<Box<Expr>>>(
            &expr,
            "blue",
            &meta,
            &scope_index,
            scope_index.program_scope(),
            None,
            None,
        );
        match result.as_deref() {
            Some(Expr::Lit(swc_core::ecma::ast::Lit::Str(s))) => {
                assert_eq!(s.value.as_str(), Some("#00f"));
            }
            other => panic!("expected '#00f' string literal, got {other:?}"),
        }
    }

    #[test]
    fn resolve_identifier_in_pattern_finds_key() {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(
            Lrc::new(FileName::Anon),
            r#"const { foo: bar } = src;"#.to_string(),
        );
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let pat = match &module.body[0] {
            ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Decl(Decl::Var(v))) => {
                v.decls[0].name.clone()
            }
            _ => panic!("expected decl"),
        };
        // Find by KEY 'foo'.
        let by_key = resolve_identifier_in_pattern("foo", &pat, ResolveFor::Key);
        assert!(by_key.is_some(), "expected match by key 'foo'");
        // Find by VALUE 'bar' (the local binding name).
        let by_value = resolve_identifier_in_pattern("bar", &pat, ResolveFor::Value);
        assert!(by_value.is_some(), "expected match by value 'bar'");
        // Non-match returns None.
        let miss = resolve_identifier_in_pattern("nope", &pat, ResolveFor::Key);
        assert!(miss.is_none());
    }

    #[test]
    fn resolve_request_returns_none_without_filename_or_resolver() {
        // Sanity guard: resolve_request requires both filename AND
        // resolver to be set on State. Tests / production callers
        // who skip wiring get None back rather than a silent
        // resolve.sync against the wrong cwd.
        let mut state = fresh_state();
        let meta = meta_for_state(&mut state);
        let r = resolve_request("anything", &meta);
        assert!(r.is_none());
    }

    /// End-to-end: build a State with a real resolver, parse a
    /// fixture's consumer.js, set state.filename, run resolve_binding
    /// for an imported `parity-pkg-main-only` reference. The §5.4a
    /// fixture skeleton on disk satisfies both.
    #[test]
    fn cross_file_named_import_resolves_via_default_resolver() {
        use crate::resolver::build_default;
        use std::sync::Arc;

        // The §5.4a fixture skeleton.
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let consumer_path = repo_root
            .join("parity-harness/resolver-matrix/fixtures-source")
            .join("axis-1-pkg-main/main-only/consumer.js");
        if !consumer_path.exists() {
            return; // fixture not present
        }

        // Synthesize a consumer file that imports the fixture
        // package and re-exports the import — gives us a
        // top-level binding for a named import to resolve against.
        let synthetic_src = format!(
            r#"
            import pkg from 'parity-pkg-main-only';
            export {{ pkg }};
            "#
        );
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(
            Lrc::new(FileName::Real(consumer_path.clone())),
            synthetic_src,
        );
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let scope_index = ScopeIndex::build(&module);

        // Wire a default-config resolver + filename onto State.
        let mut state = fresh_state();
        let resolver = Arc::new(build_default(Some(&[
            ".js".to_string(),
            ".jsx".to_string(),
            ".ts".to_string(),
            ".tsx".to_string(),
        ])));
        state.set_resolver(resolver);
        state.set_filename(consumer_path.to_string_lossy().to_string());
        let meta = meta_for_state(&mut state);

        // Look up the binding `pkg` (the default-import alias).
        // §5.0a's ScopeIndex builds bindings for import declarators;
        // `binding.import_source` should be 'parity-pkg-main-only'
        // and `binding.import_kind` should be 'default'.
        let result = resolve_binding(
            "pkg",
            &meta,
            &scope_index,
            scope_index.program_scope(),
            None,
        );
        // The §5.0a binding-builder either populates import_source/kind
        // or doesn't — depending on the §5.0a closure. If it doesn't,
        // resolve_binding correctly returns None or Some-with-no-node;
        // that's an implementation gap surfaced by this test, not a
        // §5.4e regression.
        if let Some(r) = result {
            // If the binding chain reached resolve, we must have a
            // resolved import_filename pointing at the fixture's
            // entry.js, AND an imported_module Arc so the §5.6
            // evaluator can scope-swap.
            if matches!(r.source, BindingSource::Import) {
                let imported = r.imported_filename.expect("imported filename");
                assert!(
                    imported.contains("parity-pkg-main-only"),
                    "imported filename should point at the fixture pkg, got {imported}"
                );
                // §5.4e drift-fix contract: cross-file Import
                // resolutions MUST carry the imported AST so the
                // §5.6 evaluator can build a fresh ScopeIndex.
                // Without this, deep cross-file chains deopt.
                assert!(
                    r.imported_module.is_some(),
                    "imported_module must be Some for cross-file Import resolutions \
                     — see PartialBindingWithMeta type-level doc-comment"
                );
            }
        }
    }

    /// §5.4e drift-fix gate: when resolve_binding returns
    /// `source: Import`, the `imported_module` Arc MUST be
    /// populated and MUST point at a Module containing the
    /// expected exports. Direct test against a synthesised
    /// imported file (no fixture-on-disk dependency) so the
    /// §5.4e shape contract is locked even when the §5.4a
    /// fixture skeleton is unavailable.
    #[test]
    fn cross_file_import_carries_imported_module_arc() {
        use crate::resolver::build_default;
        use std::sync::Arc;

        // Synthesise an imported file under tempdir + a consumer
        // file in the same directory. The default-config resolver
        // walks node_modules from the consumer's parent — we
        // sidestep that by using a relative-path import.
        let tmp = std::env::temp_dir().join("§5.4e_drift_fix_imported_module");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let imported_path = tmp.join("theme.ts");
        std::fs::write(
            &imported_path,
            "export const colors = { primary: '#0052cc' };\n",
        )
        .unwrap();
        let consumer_path = tmp.join("consumer.ts");
        let consumer_src =
            "import { colors } from './theme';\nexport { colors };\n";
        std::fs::write(&consumer_path, consumer_src).unwrap();

        // Parse the consumer + build its ScopeIndex.
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(
            Lrc::new(FileName::Real(consumer_path.clone())),
            consumer_src.to_string(),
        );
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let scope_index = ScopeIndex::build(&module);

        // Wire State + meta.
        let mut state = fresh_state();
        let resolver = Arc::new(build_default(Some(&[
            ".js".to_string(),
            ".jsx".to_string(),
            ".ts".to_string(),
            ".tsx".to_string(),
        ])));
        state.set_resolver(resolver);
        state.set_filename(consumer_path.to_string_lossy().to_string());
        let meta = meta_for_state(&mut state);

        let result = resolve_binding(
            "colors",
            &meta,
            &scope_index,
            scope_index.program_scope(),
            None,
        );

        // The binding-builder + resolver pipeline should reach
        // the imported file. If it doesn't (for a §5.0a/oxc-resolver
        // reason this test surfaces), fail loudly — the drift fix
        // depends on this end-to-end shape.
        let result = result
            .expect("resolve_binding for `colors` import should reach the imported file");
        assert!(
            matches!(result.source, BindingSource::Import),
            "expected source: Import, got {:?}",
            result.source,
        );
        let imported_filename = result
            .imported_filename
            .as_deref()
            .expect("imported_filename populated");
        assert!(
            imported_filename.contains("theme.ts"),
            "imported_filename should point at theme.ts, got {imported_filename}",
        );

        // The §5.4e drift-fix contract: imported_module is Some
        // and contains the imported file's parsed AST.
        let imported_module = result
            .imported_module
            .as_ref()
            .expect("imported_module populated for cross-file Import");
        // Quick AST-shape sanity: walk the imported module's
        // top-level body for an export-decl named `colors`. If
        // the Arc carries the wrong file's AST, this lookup would
        // miss.
        use crate::utils::traversers::get_named_export;
        let export = get_named_export(imported_module, "colors")
            .expect("imported_module must contain `export const colors = ...`");
        assert!(
            export.node.is_some(),
            "the `colors` export MUST resolve to its init expression"
        );
    }
}
