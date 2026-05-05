//! 1:1 port of `packages/babel-plugin/src/utils/build-compiled-component.ts`.
//!
//! Builds the `<CC><CS>{cssNode}</CS>{jsxNode}</CC>` wrapper for the
//! css-prop and classNames handlers. The upstream implementation
//! delegates the JSX template construction to `@babel/template`; the
//! Rust port hand-builds the equivalent SWC AST shape directly. No
//! template parser dependency — see `compat/template.rs` (NOT yet
//! ported) for context.
//!
//! Babel→SWC field-name divergences:
//!
//! * `JSXOpeningElement.attributes` → `JSXOpeningElement.attrs`.
//! * `JSXElement.openingElement` → `JSXElement.opening`.
//! * `JSXElement.closingElement` (required) → `JSXElement.closing`
//!   (Option). We always emit `Some(_)` to match Babel.
//! * `JSXExpressionContainer.expression` → `JSXExprContainer.expr`
//!   (typed as a `JSXExpr` enum: `Empty | Expr`).
//! * `JSXAttribute` → `JSXAttr` inside `JSXAttrOrSpread::JSXAttr`.
//!
//! ### Byte-equality scope
//!
//! This is the §4.6 SHELL deliverable. The AST shape matches Babel's
//! `@babel/template`-generated shape, but emit byte-equality vs. the
//! Babel oracle is the §4.8 fixture-corpus problem. Differences known
//! at port time:
//!
//! * Babel's template parser inserts JSXText whitespace between
//!   children when the source string spans multiple lines. The Rust
//!   hand-built tree has NO whitespace JSXText children — the
//!   downstream printer collapses them anyway, but if a fixture
//!   exercises this, the JSXText inserts go here.
//! * `nonceAttribute` upstream is built as a string fragment
//!   (`nonce={mynonce}`) and re-parsed by `@babel/template`. We
//!   construct the JSXAttr directly with a JSXExprContainer holding
//!   `Ident(opts.nonce)`. Same byte output.
//!
//! ### Phase 5/6 reach
//!
//! `build_compiled_component` itself does NOT reach
//! `evaluate_expression` / `resolve_binding`. It consumes a
//! pre-evaluated `CSSOutput` and a JSXElement reference. The visitor
//! dispatch site (css-prop / classNames handler) that CALLS this
//! function reaches the Phase 5 stubs via `buildCss`; that's a
//! Phase 6 §6.5 / §6.6 concern.

use compiled_utils::unique;
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, Ident, IdentName, JSXAttr, JSXAttrName,
    JSXAttrOrSpread, JSXAttrValue, JSXClosingElement, JSXElement, JSXElementChild, JSXElementName,
    JSXExpr, JSXExprContainer, JSXOpeningElement, ObjectLit, PropOrSpread, SpreadElement,
};

use crate::mutation_recorder::MutationRecorder;
use crate::types::Metadata;

use crate::utils::build_css_variables::build_css_variables;
use crate::utils::get_jsx_attribute::get_jsx_attribute;
use crate::utils::get_runtime_class_name_library::get_runtime_class_name_library;
use crate::utils::hoist_sheet::hoist_sheet;
use crate::utils::transform_css_items::{transform_css_items, TransformCssItemsResult};
use crate::utils::types::CSSOutput;

// ───────── Helpers ─────────

fn ident_expr(name: &str) -> Box<Expr> {
    Box::new(Expr::Ident(Ident::new(
        name.into(),
        DUMMY_SP,
        Default::default(),
    )))
}

fn jsx_element_name_ident(name: &str) -> JSXElementName {
    JSXElementName::Ident(Ident::new(name.into(), DUMMY_SP, Default::default()))
}

fn jsx_attr_name_ident(name: &str) -> JSXAttrName {
    JSXAttrName::Ident(IdentName::new(name.into(), DUMMY_SP))
}

fn jsx_expr_container(expr: Box<Expr>) -> JSXExprContainer {
    JSXExprContainer {
        span: DUMMY_SP,
        expr: JSXExpr::Expr(expr),
    }
}

fn make_call(callee_name: &str, args: Vec<ExprOrSpread>) -> Expr {
    Expr::Call(CallExpr {
        span: DUMMY_SP,
        callee: Callee::Expr(ident_expr(callee_name)),
        args,
        type_args: None,
        ctxt: Default::default(),
    })
}

/// Returns the actual value of a JSX attribute value. Mirrors
/// upstream `getExpression` (lines 49–59). Used to thread the user's
/// existing `className` value into the wrapping `ax`/`ac` call.
///
/// Panics on `JSXEmptyExpression`, matching upstream's
/// `throw new Error('Empty expression not supported.')`.
fn get_expression(value: &JSXAttrValue) -> Box<Expr> {
    match value {
        // SWC's JSXAttrValue::Str holds Str directly (not Lit-wrapped).
        // Babel's `t.StringLiteral` is `Lit::Str(Str)` — same byte
        // output, different enum nesting.
        JSXAttrValue::Str(s) => Box::new(Expr::Lit(swc_core::ecma::ast::Lit::Str(s.clone()))),
        JSXAttrValue::JSXExprContainer(container) => match &container.expr {
            JSXExpr::JSXEmptyExpr(_) => panic!("Empty expression not supported."),
            JSXExpr::Expr(e) => e.clone(),
        },
        JSXAttrValue::JSXElement(e) => Box::new(Expr::JSXElement(e.clone())),
        JSXAttrValue::JSXFragment(f) => Box::new(Expr::JSXFragment(f.clone())),
    }
}

// ───────── compiledTemplate ─────────

/// Build the `<CC><CS>{cssNode}</CS>{jsxNode}</CC>` JSX wrapper.
///
/// Mirrors upstream `compiledTemplate` (lines 23–42). `node` is the
/// user's JSX element (already mutated with the new className/style
/// attrs by `build_compiled_component`); `sheets` is the deduplicated
/// list of stylesheet strings — each gets hoisted via `hoist_sheet`
/// and the resulting identifier names go into the `<CS>` array.
pub fn compiled_template(
    node: Box<Expr>,
    sheets: &[String],
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
) -> Expr {
    // ───── Build the `cssNode` array: hoisted-sheet identifiers ─────
    //
    // Upstream: `t.arrayExpression(unique(sheets).map(sheet =>
    //   hoistSheet(sheet, meta)))` — note hoistSheet returns
    //   t.Identifier; our port returns the symbol name (per
    //   state.rs's "store name, reconstruct Ident on emit" contract).
    let unique_sheets = unique(sheets);
    let css_array_elems: Vec<Option<ExprOrSpread>> = unique_sheets
        .iter()
        .map(|sheet| {
            let name = hoist_sheet(sheet, meta, recorder);
            Some(ExprOrSpread {
                spread: None,
                expr: ident_expr(&name),
            })
        })
        .collect();
    let css_array = Expr::Array(ArrayLit {
        span: DUMMY_SP,
        elems: css_array_elems,
    });

    // ───── <CS [nonce={...}]>{cssArray}</CS> ─────
    let mut cs_attrs: Vec<JSXAttrOrSpread> = Vec::new();
    if let Some(nonce_expr) = build_nonce_value(meta) {
        cs_attrs.push(JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("nonce"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                nonce_expr,
            ))),
        }));
    }
    let cs_element = JSXElement {
        span: DUMMY_SP,
        opening: JSXOpeningElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CS"),
            attrs: cs_attrs,
            self_closing: false,
            type_args: None,
        },
        children: vec![JSXElementChild::JSXExprContainer(jsx_expr_container(
            Box::new(css_array),
        ))],
        closing: Some(JSXClosingElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CS"),
        }),
    };

    // ───── <CC [key={keyAttribute}]>...</CC> ─────
    let mut cc_attrs: Vec<JSXAttrOrSpread> = Vec::new();
    if let (Some(key_attr), _) = get_jsx_attribute(&node, "key") {
        // Babel: `<CC ${generate(keyAttribute).code}>` — re-emits the
        // entire attribute string (`key=...`). The Rust port clones
        // the JSXAttr directly; the printer renders the same bytes.
        cc_attrs.push(JSXAttrOrSpread::JSXAttr(key_attr.clone()));
    }
    let cc_element = JSXElement {
        span: DUMMY_SP,
        opening: JSXOpeningElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CC"),
            attrs: cc_attrs,
            self_closing: false,
            type_args: None,
        },
        children: vec![
            JSXElementChild::JSXElement(Box::new(cs_element)),
            // `{%%jsxNode%%}` placeholder — wraps the user's JSXElement
            // in an Expression Container, matching `@babel/template`'s
            // expansion of a `{...}` placeholder around an Expression.
            JSXElementChild::JSXExprContainer(jsx_expr_container(node)),
        ],
        closing: Some(JSXClosingElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CC"),
        }),
    };

    Expr::JSXElement(Box::new(cc_element))
}

/// Builds the inner expression of `nonce={...}`. Returns `None` when
/// `opts.nonce` is unset.
///
/// Upstream emits `nonce={${meta.state.opts.nonce}}` as a string
/// fragment and lets `@babel/template` parse it. The value of
/// `opts.nonce` is interpolated DIRECTLY as code (not as a string
/// literal) — so e.g. `nonce: '__webpack_nonce__'` produces
/// `nonce={__webpack_nonce__}` in the output. Rust mirrors with an
/// `Ident` carrying the configured name verbatim.
fn build_nonce_value(meta: &Metadata<'_>) -> Option<Box<Expr>> {
    meta.state.opts().nonce.as_ref().map(|name| ident_expr(name))
}

// ───────── buildCompiledComponent ─────────

/// Splice className + style attributes onto the user's JSXElement
/// then wrap with `compiled_template`. Mirrors upstream
/// `buildCompiledComponent` (lines 68–144).
///
/// Returns the new `<CC>` Expr replacing the user's original JSX
/// element at the visitor dispatch site. The caller is responsible
/// for substituting it via the SWC visitor's mutator API.
pub fn build_compiled_component(
    node: Box<JSXElement>,
    css_output: &CSSOutput,
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
) -> Expr {
    let TransformCssItemsResult { sheets, class_names } =
        transform_css_items(&css_output.css, meta);

    // Mutate a clone of the user's element. SWC's visit_mut would
    // mutate in place, but at this layer we want a clean owned
    // value to thread into compiled_template.
    let mut node = node;

    // ───── className splice ─────
    let class_name_lib = get_runtime_class_name_library(meta);
    let (existing_classname, classname_idx) = {
        // Split the borrow of `node` for the lookup; we take an
        // owned clone of the value to avoid an aliasing borrow when
        // we mutate the attrs vector below.
        let probe = Expr::JSXElement(node.clone());
        let (attr, idx) = get_jsx_attribute(&probe, "className");
        (attr.cloned(), idx)
    };

    if let Some(existing) = existing_classname {
        // `classNames.concat(classNameExpression)` — the user's
        // existing className value joins the array as the trailing
        // element.
        if let Some(value) = &existing.value {
            let user_value_expr = get_expression(value);
            let mut values: Vec<Box<Expr>> = class_names;
            values.push(user_value_expr);

            let new_call = make_call(
                class_name_lib,
                vec![ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Array(ArrayLit {
                        span: DUMMY_SP,
                        elems: values
                            .into_iter()
                            .map(|expr| Some(ExprOrSpread { spread: None, expr }))
                            .collect(),
                    })),
                }],
            );

            // Replace value in place.
            if let JSXAttrOrSpread::JSXAttr(attr) =
                &mut node.opening.attrs[classname_idx as usize]
            {
                attr.value = Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                    Box::new(new_call),
                )));
            }
        }
    } else {
        // No existing className — push our own.
        let new_call = make_call(
            class_name_lib,
            vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Array(ArrayLit {
                    span: DUMMY_SP,
                    elems: class_names
                        .into_iter()
                        .map(|expr| Some(ExprOrSpread { spread: None, expr }))
                        .collect(),
                })),
            }],
        );
        node.opening.attrs.push(JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("className"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                Box::new(new_call),
            ))),
        }));
    }

    // ───── style splice (only when variables exist) ─────
    if !css_output.variables.is_empty() {
        let mut dynamic_props: Vec<PropOrSpread> = build_css_variables(&css_output.variables, |e| e);

        // Find existing style attr (if any) — splice or push.
        let (existing_style, style_idx) = {
            let probe = Expr::JSXElement(node.clone());
            let (attr, idx) = get_jsx_attribute(&probe, "style");
            (attr.cloned(), idx)
        };

        if let Some(style_attr) = existing_style {
            // Remove the pre-existing style attribute.
            node.opening.attrs.remove(style_idx as usize);

            if let Some(JSXAttrValue::JSXExprContainer(container)) = &style_attr.value {
                if let JSXExpr::Expr(expr) = &container.expr {
                    match &**expr {
                        Expr::Object(obj) => {
                            // Splice each property into our list at
                            // its original index — mirrors the
                            // upstream `forEach((prop, index) =>
                            // splice(index, 0, prop))` shape.
                            for (index, prop) in obj.props.iter().enumerate() {
                                // Skip ObjectMethod (no SWC equivalent
                                // beyond `Prop::Method` / Setter /
                                // Getter — upstream's
                                // `t.isObjectMethod` filter).
                                let is_method = matches!(
                                    prop,
                                    PropOrSpread::Prop(boxed) if matches!(
                                        &**boxed,
                                        swc_core::ecma::ast::Prop::Method(_)
                                            | swc_core::ecma::ast::Prop::Getter(_)
                                            | swc_core::ecma::ast::Prop::Setter(_)
                                    )
                                );
                                if is_method {
                                    continue;
                                }
                                if index <= dynamic_props.len() {
                                    dynamic_props.insert(index, prop.clone());
                                }
                            }
                        }
                        _ => {
                            // Non-object expression → spread it.
                            dynamic_props.insert(
                                0,
                                PropOrSpread::Spread(SpreadElement {
                                    dot3_token: DUMMY_SP,
                                    expr: expr.clone(),
                                }),
                            );
                        }
                    }
                }
            }
        }

        // Push the new style prop.
        let style_obj = Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: dynamic_props,
        });
        node.opening.attrs.push(JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("style"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                Box::new(style_obj),
            ))),
        }));
    }

    compiled_template(Box::new(Expr::JSXElement(node)), &sheets, meta, recorder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::{Metadata, MetadataContext};
    use crate::utils::types::{CssItem, UnconditionalCssItem, Variable};
    use swc_core::ecma::ast::{Lit, Number, Str};

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        }
    }

    fn empty_div() -> Box<JSXElement> {
        Box::new(JSXElement {
            span: DUMMY_SP,
            opening: JSXOpeningElement {
                span: DUMMY_SP,
                name: jsx_element_name_ident("div"),
                attrs: vec![],
                self_closing: false,
                type_args: None,
            },
            children: vec![],
            closing: Some(JSXClosingElement {
                span: DUMMY_SP,
                name: jsx_element_name_ident("div"),
            }),
        })
    }

    fn unconditional(css: &str) -> CssItem {
        CssItem::Unconditional(UnconditionalCssItem {
            css: css.to_string(),
        })
    }

    fn extract_jsx_element(expr: &Expr) -> &JSXElement {
        let Expr::JSXElement(e) = expr else {
            panic!("not JSXElement")
        };
        e
    }

    fn name_of(name: &JSXElementName) -> String {
        let JSXElementName::Ident(i) = name else {
            panic!("expected Ident name")
        };
        i.sym.as_str().to_string()
    }

    // ───────── compiled_template ─────────

    #[test]
    fn compiled_template_wraps_in_cc_with_cs_child() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let inner = Expr::JSXElement(empty_div());
        let result = compiled_template(Box::new(inner), &["._a{color:red}".to_string()], &mut meta, &mut recorder);
        let elem = extract_jsx_element(&result);
        assert_eq!(name_of(&elem.opening.name), "CC");
        assert_eq!(elem.children.len(), 2);
        // First child: <CS>...</CS>
        let JSXElementChild::JSXElement(cs) = &elem.children[0] else {
            panic!("first child not JSXElement")
        };
        assert_eq!(name_of(&cs.opening.name), "CS");
        // Second child: {jsxNode}
        let JSXElementChild::JSXExprContainer(_) = &elem.children[1] else {
            panic!("second child not ExprContainer")
        };
        // CC has no key attribute (none was on the input).
        assert_eq!(elem.opening.attrs.len(), 0);
        // Hoist created one entry in state.sheets.
        assert_eq!(meta.state.sheets().len(), 1);
    }

    #[test]
    fn compiled_template_threads_nonce_when_opts_nonce_set() {
        let mut state = State::default();
        let mut opts = crate::types::PluginOptions::default();
        opts.nonce = Some("__webpack_nonce__".to_string());
        state.set_opts(opts);
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = compiled_template(Box::new(Expr::JSXElement(empty_div())), &[], &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        let JSXElementChild::JSXElement(cs) = &cc.children[0] else {
            panic!()
        };
        assert_eq!(cs.opening.attrs.len(), 1);
        let JSXAttrOrSpread::JSXAttr(nonce_attr) = &cs.opening.attrs[0] else {
            panic!("not JSXAttr")
        };
        let JSXAttrName::Ident(id) = &nonce_attr.name else { panic!() };
        assert_eq!(id.sym.as_str(), "nonce");
        // The value is `{__webpack_nonce__}` — JSXExprContainer holding Ident.
        let Some(JSXAttrValue::JSXExprContainer(container)) = &nonce_attr.value else { panic!() };
        let JSXExpr::Expr(expr) = &container.expr else { panic!() };
        let Expr::Ident(id) = &**expr else { panic!() };
        assert_eq!(id.sym.as_str(), "__webpack_nonce__");
    }

    #[test]
    fn compiled_template_propagates_key_attribute() {
        // A JSXElement carrying `key="abc"` should pass it onto <CC>.
        let key_attr = JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("key"),
            value: Some(JSXAttrValue::Str(Str {
                span: DUMMY_SP,
                value: "abc".into(),
                raw: None,
            })),
        });
        let mut div = empty_div();
        div.opening.attrs.push(key_attr);

        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = compiled_template(Box::new(Expr::JSXElement(div)), &[], &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        assert_eq!(cc.opening.attrs.len(), 1);
        let JSXAttrOrSpread::JSXAttr(propagated) = &cc.opening.attrs[0] else {
            panic!()
        };
        let JSXAttrName::Ident(id) = &propagated.name else { panic!() };
        assert_eq!(id.sym.as_str(), "key");
    }

    #[test]
    fn compiled_template_dedupes_sheets_via_unique() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let _ = compiled_template(
            Box::new(Expr::JSXElement(empty_div())),
            &["a".to_string(), "b".to_string(), "a".to_string(), "b".to_string()],
            &mut meta,
            &mut recorder,
        );
        // Two unique sheets → two state.sheets entries.
        assert_eq!(meta.state.sheets().len(), 2);
    }

    // ───────── build_compiled_component ─────────

    #[test]
    fn build_compiled_component_pushes_classname_when_absent() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let css_output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut meta = fresh_meta(&mut state);
        let result = build_compiled_component(empty_div(), &css_output, &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        // The user's <div /> is wrapped inside the second {jsxNode}
        // expr container. Its className got added by the splice.
        let JSXElementChild::JSXExprContainer(container) = &cc.children[1] else {
            panic!()
        };
        let JSXExpr::Expr(expr) = &container.expr else { panic!() };
        let Expr::JSXElement(div) = &**expr else { panic!() };
        // className should be present.
        assert!(div.opening.attrs.iter().any(|a| matches!(
            a,
            JSXAttrOrSpread::JSXAttr(attr) if matches!(
                &attr.name,
                JSXAttrName::Ident(i) if i.sym.as_str() == "className"
            )
        )));
    }

    #[test]
    fn build_compiled_component_pushes_style_when_variables_present() {
        let var_expr = Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: 8.0,
            raw: None,
        })));
        let css_output = CSSOutput {
            css: vec![],
            variables: vec![Variable {
                name: "--_a".to_string(),
                expression: Some(var_expr),
                prefix: None,
                suffix: None,
            }],
        };

        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_compiled_component(empty_div(), &css_output, &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        let JSXElementChild::JSXExprContainer(container) = &cc.children[1] else {
            panic!()
        };
        let JSXExpr::Expr(expr) = &container.expr else { panic!() };
        let Expr::JSXElement(div) = &**expr else { panic!() };
        // className AND style both present.
        assert!(div.opening.attrs.iter().any(|a| matches!(
            a,
            JSXAttrOrSpread::JSXAttr(attr) if matches!(
                &attr.name,
                JSXAttrName::Ident(i) if i.sym.as_str() == "style"
            )
        )));
    }

    #[test]
    fn build_compiled_component_concats_existing_classname() {
        // Pre-existing `className="user"` joins the array as the
        // trailing element.
        let user_class = JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("className"),
            value: Some(JSXAttrValue::Str(Str {
                span: DUMMY_SP,
                value: "user".into(),
                raw: None,
            })),
        });
        let mut div = empty_div();
        div.opening.attrs.push(user_class);

        let css_output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_compiled_component(div, &css_output, &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        let JSXElementChild::JSXExprContainer(container) = &cc.children[1] else {
            panic!()
        };
        let JSXExpr::Expr(expr) = &container.expr else { panic!() };
        let Expr::JSXElement(inner) = &**expr else { panic!() };
        // className value is now an ax([...]) call wrapping both.
        let class_attr = inner
            .opening
            .attrs
            .iter()
            .find_map(|a| match a {
                JSXAttrOrSpread::JSXAttr(attr) => match &attr.name {
                    JSXAttrName::Ident(i) if i.sym.as_str() == "className" => Some(attr),
                    _ => None,
                },
                _ => None,
            })
            .expect("className present");
        let Some(JSXAttrValue::JSXExprContainer(container)) = &class_attr.value else {
            panic!()
        };
        let JSXExpr::Expr(call_expr) = &container.expr else {
            panic!()
        };
        let Expr::Call(call) = &**call_expr else {
            panic!()
        };
        // ax(...) — 1 array arg containing 2 elements (our generated + user's).
        assert_eq!(call.args.len(), 1);
        let Expr::Array(arr) = &*call.args[0].expr else {
            panic!()
        };
        assert_eq!(arr.elems.len(), 2);
    }

    #[test]
    fn build_compiled_component_uses_ac_when_compression_map_present() {
        let mut state = State::default();
        let mut opts = crate::types::PluginOptions::default();
        let mut map = indexmap::IndexMap::new();
        map.insert("aaaabbbb".to_string(), "a".to_string());
        opts.class_name_compression_map = Some(map);
        state.set_opts(opts);
        let mut recorder = MutationRecorder::new();
        let css_output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut meta = fresh_meta(&mut state);
        let result = build_compiled_component(empty_div(), &css_output, &mut meta, &mut recorder);
        let cc = extract_jsx_element(&result);
        let JSXElementChild::JSXExprContainer(container) = &cc.children[1] else {
            panic!()
        };
        let JSXExpr::Expr(expr) = &container.expr else { panic!() };
        let Expr::JSXElement(div) = &**expr else { panic!() };
        let class_attr = div
            .opening
            .attrs
            .iter()
            .find_map(|a| match a {
                JSXAttrOrSpread::JSXAttr(attr) => match &attr.name {
                    JSXAttrName::Ident(i) if i.sym.as_str() == "className" => Some(attr),
                    _ => None,
                },
                _ => None,
            })
            .expect("className present");
        let Some(JSXAttrValue::JSXExprContainer(container)) = &class_attr.value else {
            panic!()
        };
        let JSXExpr::Expr(call_expr) = &container.expr else { panic!() };
        let Expr::Call(call) = &**call_expr else { panic!() };
        let Callee::Expr(callee) = &call.callee else { panic!() };
        let Expr::Ident(id) = &**callee else { panic!() };
        assert_eq!(id.sym.as_str(), "ac");
    }
}
