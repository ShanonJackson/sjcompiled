//! Phase 6 §6.3 — `cssMap` handler.
//!
//! Upstream source: `packages/babel-plugin/src/css-map/index.ts`
//! (the `visitCssMapPath` function) — first handler that emits real
//! CSS output and writes back into the AST. Replaces the inline
//! `unimplemented!()` panic at `utils/css_builders.rs:921` (the
//! `generate_cache_for_css_map` dispatch site for cssMap-references
//! seen mid-traversal).
//!
//! Behaviour:
//!
//! 1. Detect a free-standing `cssMap({...})` call at top-most module
//!    scope (i.e. as a `VariableDeclarator` init).
//! 2. Validate shape: exactly 1 argument, the argument is an
//!    ObjectExpression, the parent IS a VariableDeclarator with an
//!    Identifier id (not a destructuring pattern).
//! 3. Tagged template form (`` cssMap`...` ``) is NOT supported —
//!    error with `NO_TAGGED_TEMPLATE`.
//! 4. For each property of the input object:
//!    - run `merge_extended_selectors_into_properties` on the value
//!      (lifts the `selectors:` shorthand and expands at-rule blocks)
//!    - call `build_css` on the processed value
//!    - reject any `variables` (the variant must be statically defined)
//!    - call `transform_css_items` to get `(sheets, classNames)`
//!    - reject any classNames count > 1
//!    - emit a new `(key: classNames[0] || "")` property
//! 5. Replace the cssMap call's argument-shape with the new
//!    `ObjectExpression` of `(variantKey: className)` pairs.
//! 6. Publish `state.cssMap[binding] = totalSheets` via the
//!    MutationRecorder (StateDiff::CssMapInsert; site 5 — already
//!    wired in `state.rs`).
//!
//! Cardinal rule: `cssMap` MUST be at top-most module scope as a
//! `const X = cssMap(...)` declaration. Anything else throws
//! `DEFINE_MAP`. The dispatch in `babel_plugin.rs::visit_mut_var_declarator`
//! is the entry point — `visit_mut_expr` at the call-expression
//! position is the error path that fires when the call isn't
//! consumed by the var-declarator handler.
//!
//! Drift watch points:
//! - SWC's `Ident` cannot hold spaces / parens, so the upstream
//!   `t.identifier(atRuleName)` (where `atRuleName` =
//!   `@media screen and (min-width: 500px)`) becomes a string-
//!   literal key in the Rust port. See `process_selectors.rs`
//!   `collapse_at_rule` for the divergence note. Bytes through
//!   `build_css` are equal because `build_css` reads the key via
//!   `get_key_value`.
//! - `state.cssMap[binding] = totalSheets` is upstream a per-key
//!   whole-array overwrite. The `StateDiff::CssMapInsert` arm at
//!   `state.rs:368` mirrors with `IndexMap::insert` (same semantics).

pub mod process_selectors;

use swc_core::ecma::ast::{
    CallExpr, Expr, KeyValueProp, Lit, ObjectLit, Pat, Prop, PropOrSpread, Str, VarDeclarator,
};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::{MutationRecorder, StateDiff};
use crate::types::{Metadata, MetadataContext};
use crate::utils::ast::{build_code_frame_error, CssBuildError};
use crate::utils::css_builders::build_css;
use crate::utils::css_map::{create_error_message, error_if_not_valid_object_property, ErrorMessages};
use crate::utils::transform_css_items::transform_css_items;

use self::process_selectors::merge_extended_selectors_into_properties;

/// Errors produced by `visit_css_map_path` and its helpers. Wraps
/// `CssBuildError` so the visitor's error channel sees a single
/// shape.
pub type CssMapError = CssBuildError;

/// `visitCssMapPath` upstream lines 33–116. Two-arg shape:
/// `(call_expr, parent_id_name)`; the parent VariableDeclarator
/// context is supplied by the caller (the `babel_plugin.rs` dispatch
/// site checks the parent shape and threads `parent_id_name`).
///
/// On success, mutates `call.args[0]` from the original input
/// ObjectExpression to the output `(variantKey: className)`
/// ObjectExpression. The caller then replaces the call expression
/// with the mutated argument's value (matches upstream's
/// `path.replaceWith(t.objectExpression(...))`).
///
/// Returns:
/// - `Ok(replacement_expr)` on success — the caller replaces the
///   cssMap CallExpr with this `Expr::Object` value.
/// - `Err(...)` on shape validation or inner build failure.
///
/// **State writes** (via `MutationRecorder`):
/// - `StateDiff::CssMapInsert { binding, sheets }` — single
///   per-binding whole-array overwrite. Site 5 in
///   `STATE_MUTATIONS.md`.
pub fn visit_css_map_path(
    call: &CallExpr,
    parent_binding_name: &str,
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
) -> Result<Expr, CssMapError> {
    // Upstream line 56: `if (path.node.arguments.length !== 1)`.
    if call.args.len() != 1 {
        return Err(build_code_frame_error(
            create_error_message(ErrorMessages::NumberOfArgument.text()),
            Some(call.span),
        ));
    }
    let first_arg = &call.args[0];
    if first_arg.spread.is_some() {
        // Spread destroys the "1 ObjectExpression argument" invariant
        // upstream relies on. Mirror with NUMBER_OF_ARGUMENT — Babel
        // would surface this through the same shape check (Babel
        // doesn't allow t.spreadElement at CallExpression args
        // length-checking; the spread argument-array contributes
        // one to length but the resulting evaluation is invalid).
        return Err(build_code_frame_error(
            create_error_message(ErrorMessages::ArgumentType.text()),
            Some(call.span),
        ));
    }

    // Upstream line 65: `if (!t.isObjectExpression(path.node.arguments[0]))`.
    let Expr::Object(input_obj) = first_arg.expr.as_ref() else {
        return Err(build_code_frame_error(
            create_error_message(ErrorMessages::ArgumentType.text()),
            Some(call.span),
        ));
    };

    let mut total_sheets: Vec<String> = Vec::new();
    let mut output_props: Vec<PropOrSpread> = Vec::with_capacity(input_obj.props.len());

    for property in &input_obj.props {
        // Upstream line 77: `errorIfNotValidObjectProperty(property, meta)`.
        error_if_not_valid_object_property(property)?;

        // After the guard, only KeyValue / Shorthand / Assign reach
        // here. Upstream's path reads `property.value` which only
        // exists on KeyValue (Shorthand/Assign in upstream Babel
        // can't pass error_if_not_valid_object_property given they
        // aren't valid in the cssMap input shape). Mirror.
        let PropOrSpread::Prop(boxed) = property else {
            unreachable!("guarded above");
        };
        let Prop::KeyValue(kv) = boxed.as_ref() else {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticVariantObject.text()),
                None,
            ));
        };

        // Upstream lines 79–85: `if (!t.isObjectExpression(property.value))`.
        let Expr::Object(variant_obj) = kv.value.as_ref() else {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticVariantObject.text()),
                Some(expr_span(kv.value.as_ref())),
            ));
        };

        // Upstream line 87: `mergeExtendedSelectorsIntoProperties(...)`.
        let processed = merge_extended_selectors_into_properties(variant_obj)?;

        // Upstream line 88: `buildCss(processedPropertyValue, meta)`.
        let css_output = build_css(
            &Expr::Object(processed),
            meta,
            scope_index,
            parent_scope,
            own_scope,
            recorder,
        )?;

        // Upstream lines 90–96: variables MUST be empty for static
        // cssMap variants — reject if not.
        if !css_output.variables.is_empty() {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticVariantObject.text()),
                Some(expr_span(kv.value.as_ref())),
            ));
        }

        // Upstream line 98: `transformCssItems(css, meta)`.
        let transformed = transform_css_items(&css_output.css, meta);
        total_sheets.extend(transformed.sheets);

        // Upstream lines 101–107: classNames.length > 1 is an error.
        if transformed.class_names.len() > 1 {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticVariantObject.text()),
                None,
            ));
        }

        // Upstream line 109: `t.objectProperty(property.key,
        // classNames[0] || t.stringLiteral(''))`.
        let class_value: Box<Expr> = match transformed.class_names.into_iter().next() {
            Some(cn) => cn,
            None => Box::new(Expr::Lit(Lit::Str(Str {
                span: swc_core::common::DUMMY_SP,
                value: "".into(),
                raw: None,
            }))),
        };

        output_props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: kv.key.clone(),
            value: class_value,
        }))));
    }

    // Upstream line 115: `meta.state.cssMap[path.parent.id.name] = totalSheets;`
    // Whole-array publish per binding.
    recorder.apply(
        StateDiff::CssMapInsert {
            binding: parent_binding_name.to_string(),
            sheets: total_sheets,
        },
        meta.state,
    );

    Ok(Expr::Object(ObjectLit {
        span: input_obj.span,
        props: output_props,
    }))
}

/// Helper: extract the binding name + initializer from a
/// `VariableDeclarator` if it matches the `const x = cssMap({...})`
/// shape. Returns `None` if any prerequisite fails.
///
/// Upstream lines 47–53 enforce the `(VariableDeclarator,
/// Identifier id)` shape. The Rust port mirrors here so the visitor
/// dispatch site can early-return cleanly.
pub fn extract_var_decl_target(decl: &VarDeclarator) -> Option<&str> {
    let Pat::Ident(bind) = &decl.name else {
        return None;
    };
    Some(bind.id.sym.as_ref())
}

fn expr_span(expr: &Expr) -> swc_core::common::Span {
    match expr {
        Expr::Object(o) => o.span,
        Expr::Lit(Lit::Str(s)) => s.span,
        Expr::Lit(Lit::Num(n)) => n.span,
        Expr::Lit(Lit::Bool(b)) => b.span,
        Expr::Lit(Lit::Null(n)) => n.span,
        Expr::Lit(Lit::BigInt(b)) => b.span,
        Expr::Lit(Lit::Regex(r)) => r.span,
        Expr::Lit(Lit::JSXText(j)) => j.span,
        Expr::Ident(i) => i.span,
        Expr::Call(c) => c.span,
        _ => swc_core::common::DUMMY_SP,
    }
}

/// Suppress unused-import warning when MetadataContext is referenced
/// only in module docs.
const _: Option<MetadataContext> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{BytePos, Span, SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        BindingIdent, Callee, ExprOrSpread, Ident, KeyValueProp, ObjectLit, Prop, PropName,
        PropOrSpread,
    };

    use crate::mutation_recorder::ApiKind;
    use crate::state::State;
    use crate::types::{Metadata, MetadataContext};

    fn ident_key(name: &str) -> PropName {
        PropName::Ident(Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()).into())
    }
    fn str_value(value: &str) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        })))
    }
    fn key_value(k: PropName, v: Box<Expr>) -> PropOrSpread {
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp { key: k, value: v })))
    }
    fn object_value(props: Vec<PropOrSpread>) -> Box<Expr> {
        Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props,
        }))
    }

    fn css_map_call(arg: Box<Expr>) -> CallExpr {
        CallExpr {
            span: Span::new(BytePos(100), BytePos(200)),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "cssMap".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: arg,
            }],
            type_args: None,
        }
    }

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        }
    }

    fn fresh_scope_index() -> ScopeIndex {
        let module = swc_core::ecma::ast::Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        ScopeIndex::build(&module)
    }

    #[test]
    fn happy_path_simple_variant_emits_classname_and_publishes_sheets() {
        // cssMap({ root: { color: 'red' } })
        let arg = object_value(vec![key_value(
            ident_key("root"),
            object_value(vec![key_value(ident_key("color"), str_value("red"))]),
        )]);
        let call = css_map_call(arg);

        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);

        let replacement = visit_css_map_path(
            &call,
            "styles",
            &mut meta,
            &mut recorder,
            &mut idx,
            parent,
            None,
        )
        .expect("happy path should succeed");

        // Replacement is an ObjectExpression with one entry.
        let Expr::Object(obj) = replacement else {
            panic!("expected ObjectExpression replacement");
        };
        assert_eq!(obj.props.len(), 1);

        // state.cssMap was published under "styles".
        assert!(state.css_map().contains_key("styles"));
        assert!(!state.css_map().get("styles").unwrap().is_empty());

        // recorder captured the diff.
        let log = recorder.diff_log();
        assert_eq!(log.len(), 1);
        assert!(matches!(
            &log[0],
            StateDiff::CssMapInsert { binding, sheets }
                if binding == "styles" && !sheets.is_empty()
        ));
    }

    #[test]
    fn rejects_zero_arguments() {
        let mut call = css_map_call(object_value(vec![]));
        call.args.clear();
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);
        let err = visit_css_map_path(
            &call, "x", &mut meta, &mut recorder, &mut idx, parent, None,
        )
        .unwrap_err();
        assert!(err.message.contains("only receive one argument"));
    }

    #[test]
    fn rejects_two_arguments() {
        let mut call = css_map_call(object_value(vec![]));
        call.args.push(ExprOrSpread {
            spread: None,
            expr: object_value(vec![]),
        });
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);
        let err = visit_css_map_path(
            &call, "x", &mut meta, &mut recorder, &mut idx, parent, None,
        )
        .unwrap_err();
        assert!(err.message.contains("one argument"));
    }

    #[test]
    fn rejects_non_object_argument() {
        let call = css_map_call(str_value("oops"));
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);
        let err = visit_css_map_path(
            &call, "x", &mut meta, &mut recorder, &mut idx, parent, None,
        )
        .unwrap_err();
        assert!(err.message.contains("only receive an object"));
    }

    #[test]
    fn rejects_variant_value_not_object() {
        // cssMap({ root: 'red' }) — value must be an object.
        let arg = object_value(vec![key_value(ident_key("root"), str_value("red"))]);
        let call = css_map_call(arg);
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);
        let err = visit_css_map_path(
            &call, "x", &mut meta, &mut recorder, &mut idx, parent, None,
        )
        .unwrap_err();
        assert!(err.message.contains("statically defined"));
    }

    #[test]
    fn extract_var_decl_target_returns_ident_name() {
        let decl = VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Ident(BindingIdent {
                id: Ident::new("styles".into(), DUMMY_SP, SyntaxContext::empty()),
                type_ann: None,
            }),
            init: None,
            definite: false,
        };
        assert_eq!(extract_var_decl_target(&decl), Some("styles"));
    }

    #[test]
    fn extract_var_decl_target_rejects_destructure() {
        // const { styles } = ...
        let decl = VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Object(swc_core::ecma::ast::ObjectPat {
                span: DUMMY_SP,
                props: vec![],
                optional: false,
                type_ann: None,
            }),
            init: None,
            definite: false,
        };
        assert!(extract_var_decl_target(&decl).is_none());
    }

    #[test]
    fn happy_path_two_variants_preserves_order() {
        // cssMap({ a: { color: 'red' }, b: { color: 'blue' } })
        let arg = object_value(vec![
            key_value(
                ident_key("a"),
                object_value(vec![key_value(ident_key("color"), str_value("red"))]),
            ),
            key_value(
                ident_key("b"),
                object_value(vec![key_value(ident_key("color"), str_value("blue"))]),
            ),
        ]);
        let call = css_map_call(arg);
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = fresh_scope_index();
        let parent = idx.program_scope();
        let mut meta = fresh_meta(&mut state);
        let replacement = visit_css_map_path(
            &call,
            "styles",
            &mut meta,
            &mut recorder,
            &mut idx,
            parent,
            None,
        )
        .unwrap();
        let Expr::Object(obj) = replacement else { panic!("expected obj") };
        assert_eq!(obj.props.len(), 2);
        let keys: Vec<String> = obj
            .props
            .iter()
            .filter_map(|p| match p {
                PropOrSpread::Prop(b) => match b.as_ref() {
                    Prop::KeyValue(kv) => crate::utils::css_map::get_key_value(&kv.key),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }

    // suppress dead-import warning when ApiKind is referenced only in
    // tests.
    const _: Option<ApiKind> = None;
}
