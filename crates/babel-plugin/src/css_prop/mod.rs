//! 1:1 port of `packages/babel-plugin/src/css-prop/index.ts`.
//!
//! Phase 6 §6.5 — first handler whose corpus reaches
//! `extract_member_expression`'s late-resolve path through `build_css`
//! (members like `<div css={styles.primary}/>` resolve through the
//! `generate_cache_for_css_map` recursion that §6.5 lit up).
//!
//! Two-step transform per upstream `visitCssPropPath`:
//!
//! 1. Find the JSXAttribute named `css` (exact match — no
//!    case-insensitive endsWith like xcss). Bail when absent or value
//!    is None.
//! 2. Check disable directives via `getNodeComments` (port stubbed —
//!    see §6.5 drift note below). Bail when disabled.
//! 3. Run `build_css(getJsxAttributeExpression(cssProp.node), meta)`.
//! 4. Splice the css attribute out of `attributes`.
//! 5. If `cssOutput.css` empty, return without wrapping.
//! 6. Otherwise replace the parent JSXElement with
//!    `build_compiled_component(jsxElementNode, cssOutput, meta)`.
//!
//! ### Babel → SWC divergences
//!
//! * **Path → JSXElement reference.** Babel's handler receives a
//!   `NodePath<JSXOpeningElement>` and mutates `path.parentPath`
//!   (the JSXElement) via `replaceWith`. The SWC visitor receives
//!   `&mut JSXElement` directly (matching the §6.4 xcss-prop pattern);
//!   we mutate it in place by destructuring the `Expr::JSXElement`
//!   returned by `build_compiled_component`.
//!
//! * **No `transformCache`.** Babel's WeakMap on NodePath guards
//!   against re-visiting the same path after `replaceWith`. The Rust
//!   visitor is post-order: the wrapper's children are NOT walked
//!   again because `n.visit_mut_children_with(self)` ran BEFORE the
//!   replacement.
//!
//! ### Disable-directive divergence — §6.5 incomplete branch
//!
//! `is_css_prop_disabled` upstream walks
//! `meta.state.file.ast.comments`, filtering by line number against
//! `path.node.loc.start.line` / `lineNumber - 1`. SWC's plugin runtime
//! doesn't expose source-position-to-line conversion to the visitor
//! today — converting `BytePos` to `Loc` requires a `SourceMap` proxy
//! we haven't threaded into the visitor yet.
//!
//! The Rust port returns `false` (transform always runs) when no
//! `@compiled-disable*` directives are present in the file's comment
//! store; if any directive IS present, we bail conservatively (no
//! transform) until the SourceMap-based per-line filtering lands. This
//! matches "BUGS in OLD = BUGS in NEW" by erring TOWARD upstream's
//! disable behaviour: directives in the file disable BROADLY rather
//! than per-line, but no false-positive transforms.
//!
//! See `comments.rs` module doc for the SourceMap-thread followup.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    Expr, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXElement, JSXExpr, Lit,
};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::MutationRecorder;
use crate::state::State;
use crate::types::{Metadata, MetadataContext};
use crate::utils::build_compiled_component::build_compiled_component;
use crate::utils::comments::is_css_prop_disabled_via_comment_store;
use crate::utils::css_builders::build_css;
use crate::utils::types::CSSOutput;

/// Result of [`try_handle_jsx_element`]. Caller swaps the JSXElement's
/// fields in place when this returns `Some(XcssReplacement)`.
pub struct CssPropReplacement {
    pub new_element: JSXElement,
}

/// Read the JSXAttribute name as a string. SWC carries `name` as
/// either an `Ident` or a `JSXNamespacedName`. Upstream operates on
/// `name.name` which strips the namespace prefix; our port skips
/// namespaced JSX attribute names because the Compiled `css` prop is
/// always a bare identifier.
fn jsx_attr_name_str(name: &JSXAttrName) -> Option<&str> {
    match name {
        JSXAttrName::Ident(id) => Some(id.sym.as_ref()),
        JSXAttrName::JSXNamespacedName(_) => None,
    }
}

/// Find the index of the `css` attribute. Exact name match — `cssMap`,
/// `xcss`, `cssText` etc. are SKIPPED.
fn find_css_attr_index(attrs: &[JSXAttrOrSpread]) -> Option<usize> {
    attrs.iter().enumerate().find_map(|(idx, a)| match a {
        JSXAttrOrSpread::JSXAttr(attr) => {
            let name = jsx_attr_name_str(&attr.name)?;
            if name == "css" {
                Some(idx)
            } else {
                None
            }
        }
        JSXAttrOrSpread::SpreadElement(_) => None,
    })
}

/// `getJsxAttributeExpression` upstream lines 14–24. Returns the
/// expression we run `build_css` against. Upstream:
/// * `StringLiteral value` → return the string literal.
/// * `JSXExpressionContainer { expression }` → return expression.
/// * Otherwise → throw.
///
/// The Rust port returns `Result<Box<Expr>, &'static str>` matching
/// the upstream throw shape. Callers convert the error to a
/// CssBuildError or panic per the dispatch contract.
fn get_jsx_attribute_expression(attr: &JSXAttr) -> Result<Expr, &'static str> {
    match attr.value.as_ref() {
        // SWC's JSXAttrValue::Str holds Str directly (not Lit-wrapped) —
        // mirrors `t.isStringLiteral(node.value)` from upstream.
        Some(JSXAttrValue::Str(s)) => Ok(Expr::Lit(Lit::Str(s.clone()))),
        Some(JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
            JSXExpr::JSXEmptyExpr(_) => Err("Value of JSX attribute was unexpected."),
            JSXExpr::Expr(e) => Ok((**e).clone()),
        },
        _ => Err("Value of JSX attribute was unexpected."),
    }
}

/// Try to handle the css prop on a JSXElement. Returns
/// `Some(CssPropReplacement)` when the element carried a css attribute
/// that the handler successfully transformed and the cssOutput emitted
/// at least one CSS item; `None` when the element is unrelated, has no
/// css attribute, has an empty value, has the disable directive, or
/// produced an empty cssOutput (in the empty-output case the css
/// attribute IS spliced out — caller handles via the `attr_removed`
/// flag).
///
/// Errors abort the WASI invocation via `panic!()` (matching §6.3 /
/// §6.4 approach; the proper SWC HANDLER error channel lands in
/// Phase 7).
pub fn try_handle_jsx_element(
    el: &mut JSXElement,
    state: &mut State,
    recorder: &mut MutationRecorder,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
) -> Option<CssPropReplacement> {
    // Upstream gate (`babel-plugin.ts:364`): `if (state.compiledImports)`.
    // Without a Compiled import in scope, no css transform.
    if state.compiled_imports().is_none() {
        return None;
    }

    let attr_idx = find_css_attr_index(&el.opening.attrs)?;

    let JSXAttrOrSpread::JSXAttr(css_attr) = &el.opening.attrs[attr_idx] else {
        unreachable!("find_css_attr_index only returns JSXAttr indices")
    };

    // Upstream: `if (!cssProp || !cssProp.node.value) return;`
    if css_attr.value.is_none() {
        return None;
    }

    // Upstream: `if (isCssPropDisabled(path, meta) ||
    //              isCssPropDisabled(cssProp, meta)) return;`
    // The two checks share the same comment-store walk; in the Rust
    // port we collapse to a single check on the file-level store. See
    // module doc for the divergence rationale.
    if is_css_prop_disabled_via_comment_store(state) {
        return None;
    }

    let css_value_expr = match get_jsx_attribute_expression(css_attr) {
        Ok(e) => e,
        Err(msg) => panic!("{}", msg),
    };

    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
    };

    let css_output: CSSOutput = match build_css(
        &css_value_expr,
        &mut meta,
        scope_index,
        parent_scope,
        None,
        recorder,
    ) {
        Ok(o) => o,
        Err(e) => panic!("{}", e.message),
    };

    // Upstream: `path.node.attributes.splice(cssPropIndex, 1)`.
    // Always remove the css attribute, even when cssOutput is empty.
    el.opening.attrs.remove(attr_idx);

    // Upstream: `if (!cssOutput.css.length) return;` — splice happened,
    // no wrap. Mirror with early return; caller observes the modified
    // `el` (css attribute is gone) and proceeds without wrapping.
    if css_output.css.is_empty() {
        return None;
    }

    // Mutate the (now-cleaned) JSXElement and pass to
    // build_compiled_component. The function takes Box<JSXElement>; we
    // mem::replace to take ownership.
    let original_jsx = std::mem::replace(
        el,
        JSXElement {
            span: DUMMY_SP,
            opening: el.opening.clone(),
            children: el.children.clone(),
            closing: el.closing.clone(),
        },
    );
    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
    };
    let wrapper = build_compiled_component(
        Box::new(original_jsx),
        &css_output,
        &mut meta,
        recorder,
    );
    let new_element = match wrapper {
        Expr::JSXElement(b) => *b,
        _ => unreachable!(
            "build_compiled_component returned a non-JSXElement Expr — refactor regression"
        ),
    };
    Some(CssPropReplacement { new_element })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::SyntaxContext;
    use swc_core::ecma::ast::{
        Ident, IdentName, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXClosingElement,
        JSXElementName, JSXExprContainer, JSXOpeningElement, KeyValueProp, Module, ObjectLit, Prop,
        PropName, PropOrSpread, Str,
    };

    use crate::compat::scope::ScopeIndex;
    use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
    use crate::state::State;

    fn ident(name: &str) -> Ident {
        Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty())
    }

    fn jsx_name(name: &str) -> JSXElementName {
        JSXElementName::Ident(ident(name))
    }

    fn jsx_attr(name: &str, value_expr: Box<Expr>) -> JSXAttrOrSpread {
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::Ident(IdentName::new(name.into(), DUMMY_SP)),
            value: Some(JSXAttrValue::JSXExprContainer(JSXExprContainer {
                span: DUMMY_SP,
                expr: JSXExpr::Expr(value_expr),
            })),
        })
    }

    fn make_jsx_element(tag: &str, attrs: Vec<JSXAttrOrSpread>) -> JSXElement {
        JSXElement {
            span: DUMMY_SP,
            opening: JSXOpeningElement {
                span: DUMMY_SP,
                name: jsx_name(tag),
                attrs,
                self_closing: true,
                type_args: None,
            },
            children: vec![],
            closing: Some(JSXClosingElement {
                span: DUMMY_SP,
                name: jsx_name(tag),
            }),
        }
    }

    fn obj_color_red() -> Box<Expr> {
        Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName::new("color".into(), DUMMY_SP)),
                value: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: "red".into(),
                    raw: None,
                }))),
            })))],
        }))
    }

    fn empty_module_index() -> ScopeIndex {
        let module = Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        ScopeIndex::build(&module)
    }

    fn state_with_compiled_css_import() -> State {
        let mut s = State::default();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Css,
                local_name: "css".into(),
            },
            &mut s,
        );
        s
    }

    #[test]
    fn finds_css_attr_exact_match() {
        let el = make_jsx_element("div", vec![jsx_attr("css", obj_color_red())]);
        assert_eq!(find_css_attr_index(&el.opening.attrs), Some(0));
    }

    #[test]
    fn does_not_match_xcss_or_cssmap_or_csstext() {
        for name in ["xcss", "cssMap", "cssText", "innerCss", "Css"] {
            let el = make_jsx_element("div", vec![jsx_attr(name, obj_color_red())]);
            assert_eq!(find_css_attr_index(&el.opening.attrs), None, "{}", name);
        }
    }

    #[test]
    fn does_not_match_namespaced_css() {
        let attrs = vec![JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::JSXNamespacedName(swc_core::ecma::ast::JSXNamespacedName {
                span: DUMMY_SP,
                ns: IdentName::new("ns".into(), DUMMY_SP),
                name: IdentName::new("css".into(), DUMMY_SP),
            }),
            value: None,
        })];
        assert_eq!(find_css_attr_index(&attrs), None);
    }

    #[test]
    fn no_compiled_imports_returns_none() {
        let mut state = State::default(); // no imports
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();
        let mut el = make_jsx_element("div", vec![jsx_attr("css", obj_color_red())]);
        let res = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        assert!(res.is_none());
        // Css attribute NOT spliced — gate is upstream of the splice.
        assert_eq!(el.opening.attrs.len(), 1);
    }

    #[test]
    fn handles_inline_object_wraps_jsx_with_cc() {
        let mut state = state_with_compiled_css_import();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let mut el = make_jsx_element("div", vec![jsx_attr("css", obj_color_red())]);
        let result =
            try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        let replacement = result.expect("inline object should wrap");

        // Wrapper is `<CC>...`.
        if let JSXElementName::Ident(id) = &replacement.new_element.opening.name {
            assert_eq!(id.sym.as_ref(), "CC");
        } else {
            panic!("expected CC wrapper");
        }

        // sheet was emitted via build_compiled_component → hoist_sheet.
        assert!(!state.sheets().is_empty());
    }

    #[test]
    fn empty_object_splices_attr_but_returns_none() {
        let mut state = state_with_compiled_css_import();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let empty_obj = Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        }));
        let mut el = make_jsx_element("div", vec![jsx_attr("css", empty_obj)]);
        let result =
            try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);

        // Empty cssOutput → no wrap, but the css attribute IS spliced.
        assert!(result.is_none());
        assert!(el.opening.attrs.is_empty());
    }

    #[test]
    fn missing_value_returns_none_without_splice() {
        let mut state = state_with_compiled_css_import();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let attrs = vec![JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::Ident(IdentName::new("css".into(), DUMMY_SP)),
            value: None,
        })];
        let mut el = make_jsx_element("div", attrs);
        let result =
            try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        assert!(result.is_none());
        // Attribute NOT spliced — value-less attribute is ignored.
        assert_eq!(el.opening.attrs.len(), 1);
    }

    #[test]
    fn member_expr_value_routes_through_late_resolve_and_emits_map_branch() {
        // §6.5 reachability gate: `<div css={styles.primary} />` with
        // `styles` already bound in `state.cssMap` (source-order
        // case). The build_css → extract_member_expression path emits
        // a CssItem::Map, which transform_css_items resolves against
        // state.cssMap to lift the cached sheets.
        let mut state = state_with_compiled_css_import();
        let mut recorder = MutationRecorder::new();
        // Pre-populate state.cssMap as if the §6.3 cssMap dispatch
        // had already run.
        recorder.apply(
            StateDiff::CssMapInsert {
                binding: "styles".into(),
                sheets: vec!["._a{color:red}".into()],
            },
            &mut state,
        );
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let member = Box::new(Expr::Member(swc_core::ecma::ast::MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(ident("styles"))),
            prop: swc_core::ecma::ast::MemberProp::Ident(IdentName::new(
                "primary".into(),
                DUMMY_SP,
            )),
        }));
        let mut el = make_jsx_element("div", vec![jsx_attr("css", member)]);
        let result =
            try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        let replacement = result.expect("member-expr path should wrap");

        if let JSXElementName::Ident(id) = &replacement.new_element.opening.name {
            assert_eq!(id.sym.as_ref(), "CC");
        } else {
            panic!("expected CC wrapper");
        }
    }
}
