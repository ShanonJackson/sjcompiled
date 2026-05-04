//! 1:1 port of `packages/babel-plugin/src/utils/is-compiled.ts`.
//!
//! Predicates for "is this AST node a Compiled API call?" — drives
//! every handler dispatch in `babel_plugin.rs` (Phase 2 §2.3 stubs;
//! Phase 6 fills in handlers).
//!
//! ### State shape divergence
//!
//! Babel reads `state.compiledImports?.css || []` (Vec) AND
//! `state.importedCompiledImports?.css` (single string) and merges
//! both for the css call-expression check. The Rust port does the
//! same merge inline — `imported_compiled_imports.css` stays
//! `Option<String>` as upstream.

use swc_core::ecma::ast::{Callee, CallExpr, Expr, MemberExpr, MemberProp, TaggedTpl};

use crate::state::{CompiledImports, State};

/// Helper: get the bound names for a given API kind on
/// `state.compiledImports`. Returns an empty slice when the import
/// bucket is missing or empty.
fn get_compiled_names<'a>(
    imports: Option<&'a CompiledImports>,
    pick: impl FnOnce(&'a CompiledImports) -> Option<&'a Vec<String>>,
) -> &'a [String] {
    imports
        .and_then(pick)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

/// Returns `true` if the node is using `css` from `@compiled/react` as
/// a call expression.
///
/// Mirrors `isCompiledCSSCallExpression` upstream lines 12–20:
/// merges `state.compiledImports.css` (Vec) with
/// `state.importedCompiledImports.css` (single string) and checks
/// membership against the callee identifier name.
pub fn is_compiled_css_call_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Some(callee_name) = call_callee_ident_name(call) else {
        return false;
    };

    let from_imports = get_compiled_names(state.compiled_imports(), |i| i.css.as_ref());
    if from_imports.iter().any(|n| n == callee_name) {
        return true;
    }
    if let Some(imported) = state
        .imported_compiled_imports()
        .and_then(|i| i.css.as_deref())
    {
        if imported == callee_name {
            return true;
        }
    }
    false
}

/// Returns `true` if the node is using `css` from `@compiled/react` as
/// a tagged template expression.
///
/// Mirrors `isCompiledCSSTaggedTemplateExpression` upstream lines
/// 29–35. Note: this one does NOT merge in
/// `importedCompiledImports.css` (matches upstream — only the call
/// variant does the merge).
pub fn is_compiled_css_tagged_template_expression(expr: &Expr, state: &State) -> bool {
    let Expr::TaggedTpl(tpl) = expr else {
        return false;
    };
    is_compiled_tag(tpl, state, |i| i.css.as_ref())
}

/// Returns `true` if the node is using `keyframes` from
/// `@compiled/react` as a call expression.
pub fn is_compiled_keyframes_call_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Some(callee_name) = call_callee_ident_name(call) else {
        return false;
    };
    get_compiled_names(state.compiled_imports(), |i| i.keyframes.as_ref())
        .iter()
        .any(|n| n == callee_name)
}

/// Returns `true` if the node is using `cssMap` from `@compiled/react`
/// as a call expression.
pub fn is_compiled_css_map_call_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Some(callee_name) = call_callee_ident_name(call) else {
        return false;
    };
    get_compiled_names(state.compiled_imports(), |i| i.css_map.as_ref())
        .iter()
        .any(|n| n == callee_name)
}

/// Returns `true` if the node is using `keyframes` from
/// `@compiled/react` as a tagged template expression.
pub fn is_compiled_keyframes_tagged_template_expression(expr: &Expr, state: &State) -> bool {
    let Expr::TaggedTpl(tpl) = expr else {
        return false;
    };
    is_compiled_tag(tpl, state, |i| i.keyframes.as_ref())
}

/// Returns `true` if the node is `styled.tag` member expression — used
/// internally by `isCompiledStyledCallExpression` /
/// `isCompiledStyledTaggedTemplateExpression`. Mirrors the private
/// upstream helper at lines 89–92.
fn is_compiled_styled_member_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Member(MemberExpr { obj, .. }) = expr else {
        return false;
    };
    let Expr::Ident(obj_ident) = &**obj else {
        return false;
    };
    get_compiled_names(state.compiled_imports(), |i| i.styled.as_ref())
        .iter()
        .any(|n| n == &*obj_ident.sym)
}

/// Returns `true` if the node is `styled(Component)` call expression —
/// the composition variant. Mirrors upstream lines 101–107.
fn is_compiled_styled_composition_call_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Some(callee_name) = call_callee_ident_name(call) else {
        return false;
    };
    get_compiled_names(state.compiled_imports(), |i| i.styled.as_ref())
        .iter()
        .any(|n| n == callee_name)
}

/// Returns `true` if the node is using `styled` from `@compiled/react`
/// as a call expression — covers both `styled.div(...)` and
/// `styled(Component)(...)` shapes.
pub fn is_compiled_styled_call_expression(expr: &Expr, state: &State) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Callee::Expr(callee_expr) = &call.callee else {
        return false;
    };
    is_compiled_styled_member_expression(callee_expr, state)
        || is_compiled_styled_composition_call_expression(callee_expr, state)
}

/// Returns `true` if the node is using `styled` from `@compiled/react`
/// as a tagged template expression — covers both shapes.
pub fn is_compiled_styled_tagged_template_expression(expr: &Expr, state: &State) -> bool {
    let Expr::TaggedTpl(tpl) = expr else {
        return false;
    };
    is_compiled_styled_member_expression(&tpl.tag, state)
        || is_compiled_styled_composition_call_expression(&tpl.tag, state)
}

// ───────── Internal helpers ─────────

/// Get the callee identifier name from a `CallExpr`, returning None
/// when the callee isn't a bare identifier.
fn call_callee_ident_name(call: &CallExpr) -> Option<&str> {
    let Callee::Expr(callee_expr) = &call.callee else {
        return None;
    };
    let Expr::Ident(ident) = &**callee_expr else {
        return None;
    };
    Some(&ident.sym)
}

/// Check that a tagged-template's tag is an identifier whose name
/// appears in the named import bucket.
fn is_compiled_tag<'a>(
    tpl: &TaggedTpl,
    state: &'a State,
    pick: impl FnOnce(&'a CompiledImports) -> Option<&'a Vec<String>>,
) -> bool {
    let Expr::Ident(tag_ident) = &*tpl.tag else {
        return false;
    };
    get_compiled_names(state.compiled_imports(), pick)
        .iter()
        .any(|n| n == &*tag_ident.sym)
}

// Suppress an unused import in non-styled call paths. MemberProp is
// re-exported indirectly when the styled member-expr arm matches.
const _: Option<MemberProp> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{
        Callee, CallExpr, Expr, ExprOrSpread, Ident, IdentName, MemberExpr, MemberProp, TaggedTpl,
        Tpl,
    };

    fn state_with_imports() -> State {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        for (api, name) in [
            (ApiKind::Css, "css"),
            (ApiKind::Keyframes, "keyframes"),
            (ApiKind::CssMap, "cssMap"),
            (ApiKind::Styled, "styled"),
        ] {
            recorder.apply(
                StateDiff::CompiledImportsAppend {
                    api,
                    local_name: name.into(),
                },
                &mut state,
            );
        }
        state
    }

    fn ident_call(name: &str) -> Expr {
        Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                name.into(),
                DUMMY_SP,
                Default::default(),
            )))),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        })
    }

    fn ident_tagged_tpl(name: &str) -> Expr {
        Expr::TaggedTpl(TaggedTpl {
            span: DUMMY_SP,
            tag: Box::new(Expr::Ident(Ident::new(
                name.into(),
                DUMMY_SP,
                Default::default(),
            ))),
            type_params: None,
            tpl: Box::new(Tpl {
                span: DUMMY_SP,
                exprs: vec![],
                quasis: vec![],
            }),
            ctxt: Default::default(),
        })
    }

    #[test]
    fn css_call_expression_matches_named_import() {
        let s = state_with_imports();
        assert!(is_compiled_css_call_expression(&ident_call("css"), &s));
        assert!(!is_compiled_css_call_expression(&ident_call("notCss"), &s));
    }

    #[test]
    fn css_call_expression_matches_imported_compiled_imports() {
        let mut s = State::default();
        s.imported_compiled_imports = Some(crate::state::ImportedCompiledImports {
            css: Some("aliasedCss".into()),
        });
        assert!(is_compiled_css_call_expression(
            &ident_call("aliasedCss"),
            &s
        ));
    }

    #[test]
    fn css_tagged_template_does_not_consult_imported_compiled_imports() {
        // Mirrors the asymmetry in upstream — the tagged-template
        // variant only checks `compiledImports.css`, not
        // `importedCompiledImports.css`.
        let mut s = State::default();
        s.imported_compiled_imports = Some(crate::state::ImportedCompiledImports {
            css: Some("aliasedCss".into()),
        });
        assert!(!is_compiled_css_tagged_template_expression(
            &ident_tagged_tpl("aliasedCss"),
            &s
        ));
    }

    #[test]
    fn keyframes_call_and_tagged_template_both_match() {
        let s = state_with_imports();
        assert!(is_compiled_keyframes_call_expression(
            &ident_call("keyframes"),
            &s
        ));
        assert!(is_compiled_keyframes_tagged_template_expression(
            &ident_tagged_tpl("keyframes"),
            &s
        ));
    }

    #[test]
    fn css_map_call_matches() {
        let s = state_with_imports();
        assert!(is_compiled_css_map_call_expression(
            &ident_call("cssMap"),
            &s
        ));
        assert!(!is_compiled_css_map_call_expression(
            &ident_call("notCssMap"),
            &s
        ));
    }

    #[test]
    fn styled_member_call_expression_matches() {
        // styled.div(...)
        let s = state_with_imports();
        let inner = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(Ident::new(
                "styled".into(),
                DUMMY_SP,
                Default::default(),
            ))),
            prop: MemberProp::Ident(IdentName::new("div".into(), DUMMY_SP)),
        });
        let call = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(inner)),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        });
        assert!(is_compiled_styled_call_expression(&call, &s));
    }

    #[test]
    fn styled_composition_call_matches() {
        // styled(Component)(...)
        let s = state_with_imports();
        let inner_call = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "styled".into(),
                DUMMY_SP,
                Default::default(),
            )))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(Ident::new(
                    "Component".into(),
                    DUMMY_SP,
                    Default::default(),
                ))),
            }],
            type_args: None,
            ctxt: Default::default(),
        });
        let outer = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(inner_call)),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        });
        assert!(is_compiled_styled_call_expression(&outer, &s));
    }
}
