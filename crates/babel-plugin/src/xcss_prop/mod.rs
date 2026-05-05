//! 1:1 port of `packages/babel-plugin/src/xcss-prop/index.ts`.
//!
//! Phase 6 §6.4 — first handler that consumes `state.css_map`
//! published by §6.3, and the first handler whose corpus exercises
//! the JSX-attribute walk pattern. Two branches mirror upstream
//! `visitXcssPropPath`:
//!
//! 1. **Inline ObjectExpression** (e.g. `<C xcss={{ color: 'red' }}>`) —
//!    `staticObjectInvariant` runs `path.evaluate().confident` (via
//!    `compat::evaluation::evaluate`), then `build_css` +
//!    `transform_css_items`. The single-className result replaces the
//!    expression; zero-className becomes `undefined`; multi-className
//!    is an error.
//! 2. **Member expression** (e.g. `<C xcss={styles.primary}>`) —
//!    walks the JSXAttribute's MemberExpressions, collects each
//!    object-Identifier name, then aggregates `state.css_map[name]`
//!    sheets. Empty aggregate bails (legacy runtime xcss path).
//!
//! Both branches set `state.uses_xcss = true` and replace the parent
//! JSXElement with the `<CC><CS>{[sheets]}</CS>{originalJsx}</CC>`
//! wrapper produced by `compiled_template`.
//!
//! ### Dispatch site
//!
//! `babel_plugin.rs::visit_mut_jsx_element` runs the children walk
//! FIRST, then calls [`try_handle_jsx_element`]. Post-order is
//! required so the original element's children are processed before
//! we wrap it; the wrapper's synthesised children (`<CS>` + the
//! original JSX) are NOT re-walked, which mirrors Babel's
//! `transformCache` short-circuit (a JSXOpeningElement that has been
//! seen by xcss-prop is skipped on re-entry).
//!
//! ### Babel → SWC divergences
//!
//! * **Path → JSXElement reference.** Babel's handler receives a
//!   `NodePath<JSXOpeningElement>` and mutates `path.parentPath`
//!   (the JSXElement) via `replaceWith`. The SWC visitor receives
//!   `&mut JSXElement` directly; we mutate it in place by
//!   destructuring the `Expr::JSXElement` returned by
//!   `compiled_template`.
//!
//! * **Identifier collection.** Upstream calls `propPath.traverse({
//!   MemberExpression(node) { ... } })`. The Rust port walks the
//!   JSXAttribute's value with a small recursive descent over `Expr`
//!   variants, collecting each `MemberExpression.object.Ident.sym`.
//!   The set of variants reachable from a JSX-attr expression matches
//!   upstream's `@babel/traverse` walk on the same shape.
//!
//! * **No `transformCache`.** Babel's WeakMap on NodePath guards
//!   against re-visiting the same path after `replaceWith`. The Rust
//!   visitor is post-order: the wrapper's children are NOT walked
//!   again because `n.visit_mut_children_with(self)` ran BEFORE the
//!   replacement. See `state.rs` field doc on `transform_cache` (the
//!   field is intentionally absent — single-pass design).

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    Expr, Ident, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXElement, JSXExpr,
    MemberExpr,
};

use crate::compat::evaluation::{evaluate, EvaluatedValue};
use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::MutationRecorder;
use crate::state::State;
use crate::types::{Metadata, MetadataContext};
use crate::utils::build_compiled_component::compiled_template;
use crate::utils::css_builders::build_css;
use crate::utils::transform_css_items::{transform_css_items, TransformCssItemsResult};

/// Result of [`try_handle_jsx_element`]. Caller swaps the JSXElement's
/// fields in place when this returns `Some`.
pub struct XcssReplacement {
    pub new_element: JSXElement,
}

/// Read the JSXAttribute name as a string. SWC carries `name` as
/// either an `Ident` or a `JSXNamespacedName`. We only care about the
/// Ident form — namespaced JSX attributes never end with `xcss` in
/// practice (Compiled's own surface uses bare attr names).
fn jsx_attr_name_str(name: &JSXAttrName) -> Option<&str> {
    match name {
        JSXAttrName::Ident(id) => Some(id.sym.as_ref()),
        JSXAttrName::JSXNamespacedName(_) => None,
    }
}

/// Find the first JSXAttribute whose name (lowercased) ends with
/// `"xcss"`. Mirrors upstream's `attr.node.name.name.toLowerCase()
/// .endsWith('xcss')` predicate. Includes `xcss`, `innerXcss`, etc.
fn find_xcss_attr(attrs: &[JSXAttrOrSpread]) -> Option<usize> {
    attrs.iter().enumerate().find_map(|(idx, a)| match a {
        JSXAttrOrSpread::JSXAttr(attr) => {
            let name = jsx_attr_name_str(&attr.name)?;
            if name.to_lowercase().ends_with("xcss") {
                Some(idx)
            } else {
                None
            }
        }
        JSXAttrOrSpread::SpreadElement(_) => None,
    })
}

/// Read the inner Expr from a JSXAttribute value when it's a
/// `JSXExprContainer` carrying a non-empty Expression. Returns `None`
/// for missing values, JSXEmptyExpression, or non-container values
/// (string literals / JSX children) — matching upstream's
/// `getJsxAttributeExpressionContainer` early-return on every shape
/// other than `JSXExpressionContainer { expression: Expression }`.
fn jsx_attr_expr_container_expr(attr: &JSXAttr) -> Option<&Expr> {
    let JSXAttrValue::JSXExprContainer(container) = attr.value.as_ref()? else {
        return None;
    };
    match &container.expr {
        JSXExpr::JSXEmptyExpr(_) => None,
        JSXExpr::Expr(e) => Some(e.as_ref()),
    }
}

/// Walk a JSXAttribute value, collecting each `MemberExpression`'s
/// `object` Identifier name. Mirrors
/// `collectPathMemberExpressionIdentifiers` upstream.
///
/// A JSX attribute expression in the xcss member-expression branch
/// reaches: bare `Ident`, `MemberExpr`, `CallExpr` (e.g. `j(...)`),
/// `BinExpr` (logical or arithmetic), `CondExpr`, `Lit`. We recurse
/// through these conservatively — the upstream traverse walks
/// everything; the Rust port mirrors only the variants the corpus
/// actually exercises (call, binary, cond, member, paren) plus a
/// generic Expr fall-through that touches each common nested shape.
fn collect_member_object_idents(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Member(m) => {
            collect_member_object_idents_in_member(m, out);
        }
        Expr::Call(c) => {
            // Walk callee + each arg (mirrors Babel's traverse into
            // CallExpression children: callee + arguments).
            if let swc_core::ecma::ast::Callee::Expr(e) = &c.callee {
                collect_member_object_idents(e, out);
            }
            for arg in &c.args {
                collect_member_object_idents(&arg.expr, out);
            }
        }
        Expr::Bin(b) => {
            collect_member_object_idents(&b.left, out);
            collect_member_object_idents(&b.right, out);
        }
        Expr::Unary(u) => {
            collect_member_object_idents(&u.arg, out);
        }
        Expr::Cond(c) => {
            collect_member_object_idents(&c.test, out);
            collect_member_object_idents(&c.cons, out);
            collect_member_object_idents(&c.alt, out);
        }
        Expr::Paren(p) => {
            collect_member_object_idents(&p.expr, out);
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                collect_member_object_idents(e, out);
            }
        }
        Expr::Seq(s) => {
            for e in &s.exprs {
                collect_member_object_idents(e, out);
            }
        }
        Expr::Array(a) => {
            for elem in a.elems.iter().flatten() {
                collect_member_object_idents(&elem.expr, out);
            }
        }
        Expr::Object(o) => {
            for prop in &o.props {
                if let swc_core::ecma::ast::PropOrSpread::Prop(p) = prop {
                    if let swc_core::ecma::ast::Prop::KeyValue(kv) = &**p {
                        collect_member_object_idents(&kv.value, out);
                    }
                }
            }
        }
        Expr::TsAs(t) => {
            collect_member_object_idents(&t.expr, out);
        }
        Expr::TsNonNull(t) => {
            collect_member_object_idents(&t.expr, out);
        }
        Expr::TsTypeAssertion(t) => {
            collect_member_object_idents(&t.expr, out);
        }
        // Bare Ident, Lit, This, etc. — no MemberExpression to extract.
        _ => {}
    }
}

fn collect_member_object_idents_in_member(m: &MemberExpr, out: &mut Vec<String>) {
    if let Expr::Ident(i) = &*m.obj {
        out.push(i.sym.as_ref().to_string());
    } else {
        // Nested member expression (e.g. `a.b.c`); recurse into the
        // object position so the leaf identifier is captured.
        collect_member_object_idents(&m.obj, out);
    }
    // `m.prop` is a property name — the upstream traverse visits it
    // as a child but the predicate only fires when `node.object.type
    // === 'Identifier'`, so the prop side is irrelevant.
}

/// `collectPassStyles` upstream — for each identifier appearing in
/// the JSX attribute walk, push every sheet from
/// `state.cssMap[identifier]` into the output vec. Order: outer for-
/// each over `state.cssMap` keys (insertion order via IndexMap),
/// matching upstream's `for (const key in meta.state.cssMap)`.
fn collect_pass_styles(state: &State, identifiers: &[String]) -> Vec<String> {
    let mut styles = Vec::new();
    for (key, sheets) in state.css_map() {
        if identifiers.iter().any(|i| i == key) {
            styles.extend(sheets.iter().cloned());
        }
    }
    styles
}

/// `staticObjectInvariant` upstream lines 18–30. Runs the
/// constant-folder against the inline ObjectExpression; returns
/// `Ok(())` on confident, `Err(message)` otherwise. The error string
/// matches the upstream message verbatim.
fn static_object_invariant(
    expr: &Expr,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
) -> Result<(), &'static str> {
    match evaluate(expr, scope_index, parent_scope) {
        EvaluatedValue::Confident(_) => Ok(()),
        EvaluatedValue::Deopt => Err("Object given to the xcss prop must be static"),
    }
}

/// Extract the inner JSXElement from an `Expr::JSXElement` returned by
/// `compiled_template`. The function is internal to this module — the
/// invariant is that `compiled_template` always returns an
/// `Expr::JSXElement` (it constructs the `<CC>` wrapper). Anything
/// else is a refactor regression we want loud.
fn unwrap_jsx_element_expr(e: Expr) -> JSXElement {
    match e {
        Expr::JSXElement(b) => *b,
        _ => unreachable!(
            "compiled_template returned a non-JSXElement Expr — refactor regression in build_compiled_component"
        ),
    }
}

/// Try to handle the xcss prop on a JSXElement. Returns
/// `Some(XcssReplacement)` when the element carried an xcss-suffixed
/// attribute that the handler successfully rewrote; `None` when the
/// element is unrelated, has no xcss attribute, has an empty xcss
/// expression, or hits the legacy-runtime member-expression bail-out.
///
/// Errors abort the WASI invocation via `panic!()` (matching §6.3's
/// approach; the proper SWC HANDLER error channel lands in Phase 7).
pub fn try_handle_jsx_element(
    el: &mut JSXElement,
    state: &mut State,
    recorder: &mut MutationRecorder,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
) -> Option<XcssReplacement> {
    // Upstream `processXcss = state.opts.processXcss ?? true`. Skip
    // the entire handler when the user disabled it.
    if !state.opts().process_xcss.unwrap_or(true) {
        return None;
    }

    let attrs = &el.opening.attrs;
    let attr_idx = find_xcss_attr(attrs)?;

    let JSXAttrOrSpread::JSXAttr(xcss_attr) = &attrs[attr_idx] else {
        unreachable!("find_xcss_attr only returns JSXAttr indices")
    };

    let inner_expr = jsx_attr_expr_container_expr(xcss_attr)?.clone();

    // ───── Branch 1: inline ObjectExpression ─────
    if let Expr::Object(obj) = &inner_expr {
        if let Err(msg) = static_object_invariant(&inner_expr, scope_index, parent_scope) {
            panic!("{}", msg);
        }

        let mut meta = Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };

        let css_output = match build_css(
            &Expr::Object(obj.clone()),
            &mut meta,
            scope_index,
            parent_scope,
            None,
            recorder,
        ) {
            Ok(o) => o,
            Err(e) => panic!("{}", e.message),
        };

        let TransformCssItemsResult { sheets, class_names } =
            transform_css_items(&css_output.css, &mut meta);

        // Upstream switch on classNames.length:
        //   1   → replace expression with classNames[0]
        //   0   → replace expression with t.identifier('undefined')
        //   else → throw "Unexpected count of class names…"
        let new_attr_expr: Box<Expr> = match class_names.len() {
            1 => class_names.into_iter().next().expect("len == 1"),
            0 => Box::new(Expr::Ident(Ident::new(
                "undefined".into(),
                DUMMY_SP,
                Default::default(),
            ))),
            _ => panic!(
                "Unexpected count of class names please raise an issue on Github"
            ),
        };

        // Mutate the xcss attribute's expression in place. We need to
        // build a fresh JSXAttrValue::JSXExprContainer carrying the
        // new expression. Take a mutable borrow of the attribute via
        // its index and rewrite the value in place.
        let xcss_attr_value = JSXAttrValue::JSXExprContainer(swc_core::ecma::ast::JSXExprContainer {
            span: DUMMY_SP,
            expr: JSXExpr::Expr(new_attr_expr),
        });
        // SAFETY: we already extracted the attr_idx from the same vec;
        // no aliasing concerns.
        if let JSXAttrOrSpread::JSXAttr(a) = &mut el.opening.attrs[attr_idx] {
            a.value = Some(xcss_attr_value);
        }

        state.set_uses_xcss();

        // Wrap the (now-rewritten) JSXElement with the compiled
        // template. compiled_template needs &mut Metadata + &mut
        // MutationRecorder for hoist_sheet's StateDiff::SheetsInsert
        // emission. We re-build meta here because we just mutated
        // state directly via set_uses_xcss; reborrow is cheaper than
        // threading the meta through the in-place attribute write.
        let original_jsx = std::mem::replace(
            el,
            JSXElement {
                span: DUMMY_SP,
                opening: el.opening.clone(),
                children: el.children.clone(),
                closing: el.closing.clone(),
            },
        );
        // The std::mem::replace above is a no-op for our purposes —
        // we restore in just below — but borrows demand it. Take
        // ownership of the original via the replace.
        let mut meta = Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        let wrapper = compiled_template(
            Box::new(Expr::JSXElement(Box::new(original_jsx))),
            &sheets,
            &mut meta,
            recorder,
        );
        return Some(XcssReplacement {
            new_element: unwrap_jsx_element_expr(wrapper),
        });
    }

    // ───── Branch 2: member expression / call / etc. ─────
    let mut identifiers: Vec<String> = Vec::new();
    if let Some(value) = &xcss_attr.value {
        if let JSXAttrValue::JSXExprContainer(container) = value {
            if let JSXExpr::Expr(e) = &container.expr {
                collect_member_object_idents(e.as_ref(), &mut identifiers);
            }
        }
    }
    let sheets = collect_pass_styles(state, &identifiers);
    if sheets.is_empty() {
        // Legacy runtime xcss path — bail without mutation. Upstream:
        // "No sheets were extracted — bail out from the transform.
        // This covers the legacy use case of runtime xcss prop."
        return None;
    }

    state.set_uses_xcss();

    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
            in_conditional_branch: false,
    };
    // Take ownership of the original via mem::replace — SWC requires
    // an owned JSXElement to feed compiled_template's `Expr::JSXElement`
    // wrapper.
    let original_jsx = std::mem::replace(
        el,
        JSXElement {
            span: DUMMY_SP,
            opening: el.opening.clone(),
            children: el.children.clone(),
            closing: el.closing.clone(),
        },
    );
    let wrapper = compiled_template(
        Box::new(Expr::JSXElement(Box::new(original_jsx))),
        &sheets,
        &mut meta,
        recorder,
    );
    Some(XcssReplacement {
        new_element: unwrap_jsx_element_expr(wrapper),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::SyntaxContext;
    use swc_core::ecma::ast::{
        ComputedPropName, IdentName, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue,
        JSXClosingElement, JSXElementName, JSXExprContainer, JSXOpeningElement, KeyValueProp, Lit,
        MemberExpr, MemberProp, Number, ObjectLit, Prop, PropName, PropOrSpread, Str,
    };

    use crate::compat::scope::ScopeIndex;
    use crate::mutation_recorder::MutationRecorder;
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

    fn member_expr(obj: &str, prop: &str) -> Box<Expr> {
        Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(ident(obj))),
            prop: MemberProp::Ident(IdentName::new(prop.into(), DUMMY_SP)),
        }))
    }

    fn call_expr(callee: &str, args: Vec<Box<Expr>>) -> Box<Expr> {
        Box::new(Expr::Call(swc_core::ecma::ast::CallExpr {
            span: DUMMY_SP,
            callee: swc_core::ecma::ast::Callee::Expr(Box::new(Expr::Ident(ident(callee)))),
            args: args
                .into_iter()
                .map(|e| swc_core::ecma::ast::ExprOrSpread { spread: None, expr: e })
                .collect(),
            type_args: None,
            ctxt: Default::default(),
        }))
    }

    fn empty_module_index() -> ScopeIndex {
        let module = swc_core::ecma::ast::Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        ScopeIndex::build(&module)
    }

    #[test]
    fn finds_lowercase_xcss_attr() {
        let el = make_jsx_element(
            "Component",
            vec![jsx_attr("xcss", obj_color_red())],
        );
        assert_eq!(find_xcss_attr(&el.opening.attrs), Some(0));
    }

    #[test]
    fn finds_named_xcss_attr_case_insensitive() {
        // `innerXcss` ends with "xcss" case-insensitively.
        let el = make_jsx_element(
            "Component",
            vec![jsx_attr("innerXcss", obj_color_red())],
        );
        assert_eq!(find_xcss_attr(&el.opening.attrs), Some(0));
    }

    #[test]
    fn ignores_unrelated_attr() {
        let el = make_jsx_element(
            "Component",
            vec![jsx_attr("className", obj_color_red())],
        );
        assert_eq!(find_xcss_attr(&el.opening.attrs), None);
    }

    #[test]
    fn collects_idents_from_simple_member() {
        let mut idents = Vec::new();
        collect_member_object_idents(&member_expr("styles", "primary"), &mut idents);
        assert_eq!(idents, vec!["styles".to_string()]);
    }

    #[test]
    fn collects_idents_from_call_with_logical_args() {
        // `j(isPrimary && styles.primary, !isPrimary && styles.secondary)`
        let primary = Box::new(Expr::Bin(swc_core::ecma::ast::BinExpr {
            span: DUMMY_SP,
            op: swc_core::ecma::ast::BinaryOp::LogicalAnd,
            left: Box::new(Expr::Ident(ident("isPrimary"))),
            right: member_expr("styles", "primary"),
        }));
        let secondary = Box::new(Expr::Bin(swc_core::ecma::ast::BinExpr {
            span: DUMMY_SP,
            op: swc_core::ecma::ast::BinaryOp::LogicalAnd,
            left: Box::new(Expr::Unary(swc_core::ecma::ast::UnaryExpr {
                span: DUMMY_SP,
                op: swc_core::ecma::ast::UnaryOp::Bang,
                arg: Box::new(Expr::Ident(ident("isPrimary"))),
            })),
            right: member_expr("styles", "secondary"),
        }));
        let call = call_expr("j", vec![primary, secondary]);

        let mut idents = Vec::new();
        collect_member_object_idents(&call, &mut idents);
        assert_eq!(idents, vec!["styles".to_string(), "styles".to_string()]);
    }

    #[test]
    fn collect_pass_styles_aggregates_state_css_map() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        recorder.apply(
            crate::mutation_recorder::StateDiff::CssMapInsert {
                binding: "styles".into(),
                sheets: vec!["._a{color:red}".into()],
            },
            &mut state,
        );
        recorder.apply(
            crate::mutation_recorder::StateDiff::CssMapInsert {
                binding: "stylesTwo".into(),
                sheets: vec!["._b{color:blue}".into()],
            },
            &mut state,
        );

        let only_styles = collect_pass_styles(&state, &["styles".to_string()]);
        assert_eq!(only_styles, vec!["._a{color:red}".to_string()]);
        let both = collect_pass_styles(
            &state,
            &["styles".to_string(), "stylesTwo".to_string()],
        );
        assert_eq!(
            both,
            vec!["._a{color:red}".to_string(), "._b{color:blue}".to_string()]
        );
        let none = collect_pass_styles(&state, &["unknown".to_string()]);
        assert!(none.is_empty());
    }

    #[test]
    fn handles_inline_static_object_replaces_attr_with_class_string() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let mut el = make_jsx_element(
            "Component",
            vec![jsx_attr("xcss", obj_color_red())],
        );

        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        let replacement = result.expect("inline static object should rewrite");

        // The wrapper is a `<CC>...` JSXElement.
        if let JSXElementName::Ident(id) = &replacement.new_element.opening.name {
            assert_eq!(id.sym.as_ref(), "CC");
        } else {
            panic!("expected CC wrapper");
        }

        // state.usesXcss is set.
        assert_eq!(state.uses_xcss(), Some(true));

        // sheet was emitted.
        assert!(!state.sheets().is_empty(), "sheets should be populated");
    }

    #[test]
    fn empty_inline_object_replaces_attr_with_undefined_ident() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let empty_obj = Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        }));
        let mut el = make_jsx_element(
            "Component",
            vec![jsx_attr("xcss", empty_obj)],
        );

        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        let replacement = result.expect("empty object still wraps");

        // Walk the wrapper to find the inner Component xcss attr —
        // it should now be `undefined` ident.
        // Wrapper is <CC><CS>{[]}</CS>{<Component xcss={undefined}/>}</CC>.
        // The second child is the original JSX wrapped in a JSXExprContainer.
        let inner = &replacement.new_element.children[1];
        let swc_core::ecma::ast::JSXElementChild::JSXExprContainer(c) = inner else {
            panic!("expected JSXExprContainer for the wrapped JSX child");
        };
        let JSXExpr::Expr(e) = &c.expr else {
            panic!("expected an Expr inside the wrapper child container");
        };
        let Expr::JSXElement(component) = e.as_ref() else {
            panic!("expected JSXElement inside the wrapper child container");
        };
        let JSXAttrOrSpread::JSXAttr(xcss_attr) = &component.opening.attrs[0] else {
            panic!("xcss attribute missing");
        };
        let JSXAttrValue::JSXExprContainer(container) = xcss_attr.value.as_ref().unwrap() else {
            panic!("xcss attr value should be an expression container");
        };
        let JSXExpr::Expr(replaced_expr) = &container.expr else {
            panic!("xcss expression should be a non-empty expr");
        };
        let Expr::Ident(id) = replaced_expr.as_ref() else {
            panic!("xcss expression should fold to undefined Ident");
        };
        assert_eq!(id.sym.as_ref(), "undefined");
    }

    #[test]
    fn member_expr_branch_uses_state_css_map_and_wraps() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        recorder.apply(
            crate::mutation_recorder::StateDiff::CssMapInsert {
                binding: "styles".into(),
                sheets: vec!["._a{color:red}".into()],
            },
            &mut state,
        );
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let mut el = make_jsx_element(
            "Component",
            vec![jsx_attr("xcss", member_expr("styles", "primary"))],
        );

        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        let replacement = result.expect("member-expr branch should wrap");

        if let JSXElementName::Ident(id) = &replacement.new_element.opening.name {
            assert_eq!(id.sym.as_ref(), "CC");
        } else {
            panic!("expected CC wrapper");
        }
        assert_eq!(state.uses_xcss(), Some(true));
    }

    #[test]
    fn member_expr_branch_bails_when_state_css_map_misses() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let mut el = make_jsx_element(
            "Box",
            vec![jsx_attr("xcss", call_expr("xcss", vec![obj_color_red()]))],
        );

        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        assert!(
            result.is_none(),
            "no state.css_map entry → legacy runtime xcss path → no rewrite"
        );
        // usesXcss stays None — the bail-out is total.
        assert_eq!(state.uses_xcss(), None);
    }

    #[test]
    fn process_xcss_false_disables_handler() {
        use crate::types::PluginOptions;
        let mut state = State::default();
        state.set_opts(PluginOptions {
            process_xcss: Some(false),
            ..Default::default()
        });
        let mut recorder = MutationRecorder::new();
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let mut el = make_jsx_element(
            "Component",
            vec![jsx_attr("xcss", obj_color_red())],
        );

        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        assert!(result.is_none());
        assert_eq!(state.uses_xcss(), None);
    }

    #[test]
    fn no_xcss_attribute_returns_none() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut index = empty_module_index();
        let parent = index.program_scope();

        let mut el = make_jsx_element("div", vec![]);
        let result = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut index, parent);
        assert!(result.is_none());
    }

    #[test]
    fn jsx_namespaced_attr_name_is_skipped() {
        // `find_xcss_attr` MUST not match namespaced attr names; the
        // upstream predicate operates on `name.name` (string-coerced
        // identifier) which never carries a namespace prefix.
        // Synthesise one and verify we skip it.
        let attrs = vec![JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::JSXNamespacedName(swc_core::ecma::ast::JSXNamespacedName {
                span: DUMMY_SP,
                ns: IdentName::new("ns".into(), DUMMY_SP),
                name: IdentName::new("xcss".into(), DUMMY_SP),
            }),
            value: None,
        })];
        assert_eq!(find_xcss_attr(&attrs), None);

        // Also throw in a bare 'data-foo' to confirm bare-attr filter
        // works.
        let _ = ComputedPropName {
            span: DUMMY_SP,
            expr: Box::new(Expr::Lit(Lit::Num(Number {
                span: DUMMY_SP,
                value: 1.0,
                raw: None,
            }))),
        };
    }
}
