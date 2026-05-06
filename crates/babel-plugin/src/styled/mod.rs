//! 1:1 port of `packages/babel-plugin/src/styled/index.ts`.
//!
//! Phase 6 §6.7 — `styled.div(...)` / `styled.div\`...\`` /
//! `styled(Component)(...)` / `styled(Component)\`...\``. Replaces
//! the call/tagged-tpl with `forwardRef(({...}) => <CC>...</CC>)`
//! built by `utils/build_styled_component.rs`. Optionally insert a
//! `displayName` assignment after the parent VarDecl (handled by
//! the dispatch site in `babel_plugin.rs`, since it has access to
//! the surrounding statement list).
//!
//! ### Babel → SWC divergences
//!
//! * **Path → expression reference.** Babel hands the handler a
//!   `NodePath<TaggedTemplateExpression | CallExpression>` and
//!   mutates via `path.replaceWith`. The Rust port works with an
//!   `&Expr` for detection + returns the replacement Expr, which the
//!   dispatch site swaps in via `*expr = replacement`.
//!
//! * **`hasInValidExpression` walks `node.quasi`.** SWC's
//!   `TaggedTpl.quasi` is `Box<Tpl>` — same shape, just behind a
//!   Box. Otherwise field-for-field equivalent.
//!
//! * **`buildCodeFrameError` → panic!()**. Phase 7 wires HANDLER for
//!   proper SWC error emission. For now the visitor abort matches
//!   §6.3 / §6.4 / §6.5 / §6.6's panic behaviour.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, MemberExpr, MemberProp, TaggedTpl,
};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::MutationRecorder;
use crate::state::State;
use crate::types::{Metadata, MetadataContext, Tag, TagKind};
use crate::utils::build_styled_component::build_styled_component;
use crate::utils::css_builders::build_css;
use crate::utils::types::CSSOutput;

/// Result of [`try_visit_styled_path`]. Caller swaps the original
/// `Expr::Call` / `Expr::TaggedTpl` for `replacement` in place.
pub struct StyledReplacement {
    pub replacement: Expr,
}

#[derive(Debug)]
struct StyledData {
    tag: Tag,
    css_node: CssNode,
}

#[derive(Debug)]
enum CssNode {
    /// Single Expr (tagged-template `quasi` form).
    Single(Expr),
    /// Multiple Exprs (call form — `styled.div({...}, {...})`).
    Multiple(Vec<Expr>),
}

/// Try to detect + handle a styled call/tagged-tpl. Returns
/// `Some(StyledReplacement)` when the expression matches one of the
/// four shapes:
///
/// 1. `styled.div\`...\`` — TaggedTpl, tag=MemberExpr(styledIdent.tag).
/// 2. `styled(C)\`...\`` — TaggedTpl, tag=CallExpr(styledIdent, [tagIdent]).
/// 3. `styled.div(...)` — CallExpr, callee=MemberExpr(styledIdent.tag).
/// 4. `styled(C)(...)` — CallExpr, callee=CallExpr(styledIdent, [tagIdent]).
///
/// Returns `None` when:
/// * `state.compiledImports.styled` is empty (no styled binding).
/// * The expression's tag/callee shape doesn't match any of the four.
pub fn try_visit_styled(
    expr: &Expr,
    state: &mut State,
    recorder: &mut MutationRecorder,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    declared_var_name: Option<&str>,
) -> Option<StyledReplacement> {
    // Upstream `babel-plugin.ts:316`: dispatch only fires for
    // expressions whose top-level shape is recognised by
    // `is_compiled_styled_*`. The matchers gate on
    // `state.compiled_imports.styled`; we re-derive the list here for
    // the data extractor.
    let styled_names: Vec<String> = state
        .compiled_imports()
        .and_then(|i| i.styled.as_ref())
        .cloned()
        .unwrap_or_default();
    if styled_names.is_empty() {
        return None;
    }

    // Resolve the styled-data shape FIRST. The invalid-expression check
    // must only run on tagged-templates that are recognised as
    // Compiled-styled — otherwise we'd panic on unrelated tagged
    // templates (e.g. `styled` imported from `styled-components` where
    // a separate `styled2` is the Compiled binding). Upstream gates the
    // entire handler dispatch through `isCompiledStyledTaggedTemplateExpression`
    // / `isCompiledStyledCallExpression` (babel-plugin.ts:316), so the
    // invalid-expression check is only reachable for Compiled tags.
    let data = extract_styled_data_from_node(expr, &styled_names)?;

    // Now safe to run invalid-expression check — `data` is `Some` only
    // when the tag/callee matches a Compiled styled name.
    if let Expr::TaggedTpl(tpl) = expr {
        if has_invalid_expression(tpl) {
            panic!("{}", invalid_expression_error_message());
        }
    }

    // Build the CSSOutput by running build_css on the node (or array
    // of nodes for the call form).
    let css_node_expr = match &data.css_node {
        CssNode::Single(e) => e.clone(),
        CssNode::Multiple(exprs) => Expr::Array(swc_core::ecma::ast::ArrayLit {
            span: DUMMY_SP,
            elems: exprs
                .iter()
                .map(|e| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(e.clone()),
                    })
                })
                .collect(),
        }),
    };

    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
            in_conditional_branch: false,
    };

    let css_output: CSSOutput = match build_css(
        &css_node_expr,
        &mut meta,
        scope_index,
        parent_scope,
        None,
        recorder,
    ) {
        Ok(o) => o,
        Err(e) => panic!("{}", e.message),
    };

    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
            in_conditional_branch: false,
    };
    // §6.8x — pass the ORIGINAL styled CallExpr / TaggedTpl
    // (the `expr` we entered with), NOT `css_node_expr`. Upstream's
    // `getInvalidDomProps(meta.parentPath)` walks the bare styled
    // call AST node and does NOT auto-resolve identifier arguments;
    // `css_node_expr` is the EXTRACTED css-arg payload (which for
    // the call form is the `(tabStyles)` argument unwrapped — same
    // shape, but the explicit invariant is "feed Babel's parentPath
    // analog, not the extracted CSS"). The two happen to be
    // structurally identical for the call form, but for
    // tagged-template / `styled(C)\`...\`` shapes the styled call
    // wraps the css node in additional AST that Babel's traversal
    // sees and we must too. See StyledTemplateOpts::original_styled_call.
    let replacement = build_styled_component(
        data.tag,
        css_output,
        Some(expr),
        declared_var_name,
        &mut meta,
        recorder,
    );
    Some(StyledReplacement { replacement })
}

/// `extractStyledDataFromNode` upstream lines 99–112. Returns
/// `Some(StyledData)` when the node matches one of the four
/// recognised shapes; otherwise `None`.
fn extract_styled_data_from_node(node: &Expr, styled_names: &[String]) -> Option<StyledData> {
    match node {
        Expr::TaggedTpl(tpl) => extract_styled_data_from_template_literal(tpl, styled_names),
        Expr::Call(call) => extract_styled_data_from_object_literal(call, styled_names),
        _ => None,
    }
}

/// `extractStyledDataFromTemplateLiteral` upstream lines 31–60.
fn extract_styled_data_from_template_literal(
    node: &TaggedTpl,
    styled_names: &[String],
) -> Option<StyledData> {
    // Form 1: `styled.div\`...\`` — tag is MemberExpr(Ident, Ident).
    if let Expr::Member(MemberExpr { obj, prop, .. }) = &*node.tag {
        if let (Expr::Ident(obj_ident), MemberProp::Ident(prop_ident)) = (&**obj, prop) {
            if styled_names.iter().any(|n| n == obj_ident.sym.as_ref()) {
                return Some(StyledData {
                    tag: Tag {
                        name: prop_ident.sym.as_ref().to_string(),
                        kind: TagKind::InBuiltComponent,
                    },
                    css_node: CssNode::Single(Expr::Tpl((*node.tpl).clone())),
                });
            }
        }
    }

    // Form 2: `styled(Component)\`...\`` — tag is CallExpr with
    // ident callee + ident first arg.
    if let Expr::Call(call) = &*node.tag {
        if let Some(name) = styled_call_user_component(call, styled_names) {
            return Some(StyledData {
                tag: Tag {
                    name,
                    kind: TagKind::UserDefinedComponent,
                },
                css_node: CssNode::Single(Expr::Tpl((*node.tpl).clone())),
            });
        }
    }

    None
}

/// `extractStyledDataFromObjectLiteral` upstream lines 62–93.
fn extract_styled_data_from_object_literal(
    node: &CallExpr,
    styled_names: &[String],
) -> Option<StyledData> {
    let Callee::Expr(callee_box) = &node.callee else {
        return None;
    };
    let callee = &**callee_box;

    // Form 3: `styled.div(...)` — callee is MemberExpr(Ident, Ident).
    if let Expr::Member(MemberExpr { obj, prop, .. }) = callee {
        if let (Expr::Ident(obj_ident), MemberProp::Ident(prop_ident)) = (&**obj, prop) {
            if styled_names.iter().any(|n| n == obj_ident.sym.as_ref()) {
                // Upstream gates the FIRST argument with
                // `t.isExpression(node.arguments[0])`. SWC's
                // `args[0].expr` is always an `Expr`; we just check
                // it's present.
                if !node.args.is_empty() && node.args[0].spread.is_none() {
                    let exprs: Vec<Expr> = node
                        .args
                        .iter()
                        .filter_map(|a| {
                            if a.spread.is_some() {
                                None
                            } else {
                                Some((*a.expr).clone())
                            }
                        })
                        .collect();
                    return Some(StyledData {
                        tag: Tag {
                            name: prop_ident.sym.as_ref().to_string(),
                            kind: TagKind::InBuiltComponent,
                        },
                        css_node: CssNode::Multiple(exprs),
                    });
                }
            }
        }
    }

    // Form 4: `styled(Component)(...)` — callee is CallExpr.
    if let Expr::Call(inner_call) = callee {
        if let Some(name) = styled_call_user_component(inner_call, styled_names) {
            if !node.args.is_empty() && node.args[0].spread.is_none() {
                let exprs: Vec<Expr> = node
                    .args
                    .iter()
                    .filter_map(|a| {
                        if a.spread.is_some() {
                            None
                        } else {
                            Some((*a.expr).clone())
                        }
                    })
                    .collect();
                return Some(StyledData {
                    tag: Tag {
                        name,
                        kind: TagKind::UserDefinedComponent,
                    },
                    css_node: CssNode::Multiple(exprs),
                });
            }
        }
    }

    None
}

/// Match the `styled(Component)` shape and pull out the user-defined
/// component name. Mirrors the inner predicate of upstream's
/// composition path.
fn styled_call_user_component(call: &CallExpr, styled_names: &[String]) -> Option<String> {
    let Callee::Expr(callee_box) = &call.callee else {
        return None;
    };
    let Expr::Ident(callee_ident) = &**callee_box else {
        return None;
    };
    if !styled_names.iter().any(|n| n == callee_ident.sym.as_ref()) {
        return None;
    }
    let first_arg = call.args.first()?;
    if first_arg.spread.is_some() {
        return None;
    }
    let Expr::Ident(arg_ident) = &*first_arg.expr else {
        return None;
    };
    Some(arg_ident.sym.as_ref().to_string())
}

// ───────── hasInValidExpression ─────────

/// `hasInValidExpression` upstream lines 124–155. Detects malformed
/// CSS declarations of the shape:
///   `font-weight: ${(props) => (props.x && props.y) && 'bold'};`
/// (a logical-expression interpolant ending an empty CSS declaration).
fn has_invalid_expression(node: &TaggedTpl) -> bool {
    use swc_core::ecma::ast::ArrowExpr;

    // Filter expressions to ArrowExpr → LogicalExpression body.
    let logical_arrows: Vec<()> = node
        .tpl
        .exprs
        .iter()
        .filter(|expr| match &***expr {
            Expr::Arrow(ArrowExpr { body, .. }) => match &**body {
                BlockStmtOrExpr::Expr(e) => is_logical(e),
                _ => false,
            },
            _ => false,
        })
        .map(|_| ())
        .collect();

    if logical_arrows.is_empty() {
        return false;
    }

    // Walk quasis, looking for declarations whose ":<value>" trims to empty.
    for item in &node.tpl.quasis {
        let raw = item.raw.as_ref();
        for d in raw.split(';') {
            if let Some(colon_idx) = d.find(':') {
                let after_colon = &d[colon_idx + 1..];
                if after_colon.trim().is_empty() {
                    return true;
                }
            }
        }
    }

    false
}

fn is_logical(expr: &Expr) -> bool {
    if let Expr::Bin(bin) = expr {
        matches!(
            bin.op,
            swc_core::ecma::ast::BinaryOp::LogicalAnd
                | swc_core::ecma::ast::BinaryOp::LogicalOr
                | swc_core::ecma::ast::BinaryOp::NullishCoalescing
        )
    } else {
        false
    }
}

fn invalid_expression_error_message() -> String {
    // Verbatim from upstream lines 171–175.
    "A logical expression contains an invalid CSS declaration.\n      Compiled doesn't support CSS properties that are defined with a conditional rule that doesn't specify a default value.\n      Eg. font-weight: ${(props) => (props.isPrimary && props.isMaybe) && 'bold'}; is invalid.\n      Use ${(props) => props.isPrimary && props.isMaybe && ({ 'font-weight': 'bold' })}; instead".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
    use crate::state::State;
    use swc_core::common::SyntaxContext;
    use swc_core::ecma::ast::{
        ExprOrSpread, Ident, IdentName, Lit, Module, ObjectLit, Tpl, TplElement,
    };

    fn ident(name: &str) -> Ident {
        Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty())
    }

    fn empty_tpl() -> Tpl {
        Tpl {
            span: DUMMY_SP,
            exprs: vec![],
            quasis: vec![TplElement {
                span: DUMMY_SP,
                tail: true,
                cooked: Some("color: red;".into()),
                raw: "color: red;".into(),
            }],
        }
    }

    fn empty_module_index() -> ScopeIndex {
        let module = Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        ScopeIndex::build(&module)
    }

    fn state_with_styled_import() -> State {
        let mut s = State::default();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CompiledImportsAppend {
                api: ApiKind::Styled,
                local_name: "styled".into(),
            },
            &mut s,
        );
        s
    }

    fn member_call(obj: &str, prop: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(Expr::Ident(ident(obj))),
                prop: MemberProp::Ident(IdentName::new(prop.into(), DUMMY_SP)),
            }))),
            args: args
                .into_iter()
                .map(|e| ExprOrSpread {
                    spread: None,
                    expr: Box::new(e),
                })
                .collect(),
            type_args: None,
            ctxt: Default::default(),
        })
    }

    fn obj_color_red() -> Expr {
        Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(swc_core::ecma::ast::Prop::KeyValue(
                swc_core::ecma::ast::KeyValueProp {
                    key: swc_core::ecma::ast::PropName::Ident(IdentName::new(
                        "color".into(),
                        DUMMY_SP,
                    )),
                    value: Box::new(Expr::Lit(Lit::Str(swc_core::ecma::ast::Str {
                        span: DUMMY_SP,
                        value: "red".into(),
                        raw: None,
                    }))),
                },
            )))],
        })
    }

    use swc_core::ecma::ast::PropOrSpread;

    #[test]
    fn extract_recognises_member_call_inbuilt() {
        // `styled.div({ color: 'red' })`
        let call = match member_call("styled", "div", vec![obj_color_red()]) {
            Expr::Call(c) => c,
            _ => panic!(),
        };
        let data = extract_styled_data_from_object_literal(&call, &["styled".into()])
            .expect("recognised");
        assert_eq!(data.tag.name, "div");
        assert_eq!(data.tag.kind, TagKind::InBuiltComponent);
    }

    #[test]
    fn extract_recognises_tagged_tpl_inbuilt() {
        // `styled.div\`color: red\``
        let tagged = TaggedTpl {
            span: DUMMY_SP,
            tag: Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(Expr::Ident(ident("styled"))),
                prop: MemberProp::Ident(IdentName::new("div".into(), DUMMY_SP)),
            })),
            type_params: None,
            tpl: Box::new(empty_tpl()),
            ctxt: Default::default(),
        };
        let data = extract_styled_data_from_template_literal(&tagged, &["styled".into()])
            .expect("recognised");
        assert_eq!(data.tag.name, "div");
        assert_eq!(data.tag.kind, TagKind::InBuiltComponent);
    }

    #[test]
    fn extract_recognises_user_component_call() {
        // `styled(MyButton)({color:'red'})`
        let inner = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(ident("styled")))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(ident("MyButton"))),
            }],
            type_args: None,
            ctxt: Default::default(),
        });
        let outer = CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(inner)),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(obj_color_red()),
            }],
            type_args: None,
            ctxt: Default::default(),
        };
        let data = extract_styled_data_from_object_literal(&outer, &["styled".into()])
            .expect("recognised");
        assert_eq!(data.tag.name, "MyButton");
        assert_eq!(data.tag.kind, TagKind::UserDefinedComponent);
    }

    #[test]
    fn extract_returns_none_when_not_styled_binding() {
        // `notStyled.div({})`
        let call = match member_call("notStyled", "div", vec![obj_color_red()]) {
            Expr::Call(c) => c,
            _ => panic!(),
        };
        assert!(extract_styled_data_from_object_literal(&call, &["styled".into()]).is_none());
    }

    #[test]
    fn try_visit_styled_returns_none_without_styled_import() {
        let mut state = State::default(); // no styled import
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();
        let call = member_call("styled", "div", vec![obj_color_red()]);
        let res = try_visit_styled(&call, &mut state, &mut recorder, &mut idx, parent, None);
        assert!(res.is_none());
    }

    #[test]
    fn try_visit_styled_wraps_member_call_in_forward_ref() {
        let mut state = state_with_styled_import();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let call = member_call("styled", "div", vec![obj_color_red()]);
        let result = try_visit_styled(&call, &mut state, &mut recorder, &mut idx, parent, None)
            .expect("should wrap");
        let Expr::Call(c) = &result.replacement else {
            panic!("not a CallExpr")
        };
        let Callee::Expr(callee) = &c.callee else {
            panic!("not Expr callee")
        };
        let Expr::Ident(id) = &**callee else {
            panic!("not Ident callee")
        };
        assert_eq!(id.sym.as_ref(), "forwardRef");
    }
}
