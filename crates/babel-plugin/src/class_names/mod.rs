//! 1:1 port of `packages/babel-plugin/src/class-names/index.ts`.
//!
//! Phase 6 §6.6 — `<ClassNames>{ ({css, style}) => ... }</ClassNames>`
//! render-prop pattern. Two-pass sub-traversal over the JSX element's
//! subtree:
//!
//! 1. **css() pass** — replace every `css({...})` /
//!    `<rename>({...})` / `props.css({...})` / tagged-template
//!    inside the children-as-function with `ax([classNames])`,
//!    accumulating sheets and variables.
//! 2. **style pass** — replace identifier `style` and member-expr
//!    `<X>.style` references with the variables-built ObjectExpression
//!    (or `undefined` when no variables collected).
//!
//! Final step: pick the function body and replace the entire
//! `<ClassNames>...</ClassNames>` JSXElement with
//! `compiled_template(body, sheets, meta)`.
//!
//! ### Babel → SWC divergences
//!
//! * **Path → JSXElement reference.** Babel's handler receives a
//!   `NodePath<JSXElement>` and mutates via `path.replaceWith`. The
//!   SWC visitor receives `&mut JSXElement` directly.
//! * **Sub-traversal model.** Babel's `path.traverse({ Expression(p)
//!   { ... } })` translates to a dedicated `VisitMut` impl that runs
//!   over `el.opening` + `el.children` + `el.closing`.
//! * **Scope binding lookups.** Upstream calls
//!   `path.scope.hasOwnBinding(...)` / `path.scope.getBinding(...)`
//!   to disambiguate `style`-the-prop from `style`-the-css-prop. The
//!   Rust port relies on `state.compiled_imports.class_names` (the
//!   list of local names bound to the Compiled `ClassNames`
//!   component) for the dispatch gate; the inner-scope rename
//!   detection (`<inner-scope> { (style) => ... }`) is recognised by
//!   walking the function's parameters for `style` / a destructuring
//!   key matching `style`. This mirrors upstream's
//!   `resolveIdentifierComingFromDestructuring` predicate against the
//!   binding's path.
//!
//! ### Drift watch points
//!
//! * Upstream's first pass uses `getBinding` to find renamed
//!   `c({...})` calls (`c` is destructured from `({css: c}) => ...`).
//!   The Rust port reads the function's parameters once at dispatch
//!   entry to build a (rename → original-API) map covering both
//!   `css`/`c` and `style`/renamed-style. Comparison is by name
//!   only — not full scope-walk — which matches §5.0a's pattern-skip
//!   coverage in practice (ClassNames children are always
//!   immediately-nested arrow functions; outer-scope shadowing is
//!   not reachable from the corpus).
//! * Upstream throws when `getJsxChildrenAsFunction` finds no
//!   function child. The Rust port mirrors via `panic!()`.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    ArrayLit, ArrowExpr, BindingIdent, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread,
    Function, Ident, JSXElement, JSXElementChild, JSXElementName, JSXExpr, KeyValueProp,
    MemberExpr, MemberProp, ObjectLit, ObjectPatProp, Pat, Prop, PropName, PropOrSpread,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::MutationRecorder;
use crate::state::State;
use crate::types::{Metadata, MetadataContext};
use crate::utils::ast::pick_function_body;
use crate::utils::build_compiled_component::compiled_template;
use crate::utils::build_css_variables::build_css_variables;
use crate::utils::css_builders::build_css;
use crate::utils::get_runtime_class_name_library::get_runtime_class_name_library;
use crate::utils::transform_css_items::{transform_css_items, TransformCssItemsResult};
use crate::utils::types::Variable;

/// Result for the dispatch site. The new JSXElement is the
/// `<CC>...</CC>` wrapper produced by `compiled_template`.
pub struct ClassNamesReplacement {
    pub new_element: JSXElement,
}

fn ident_expr(name: &str) -> Box<Expr> {
    Box::new(Expr::Ident(Ident::new(
        name.into(),
        DUMMY_SP,
        Default::default(),
    )))
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

/// Rename map built from the children-as-function's parameter list.
/// Keys are local names (the user's destructured / renamed identifier);
/// values are the original Compiled API names (`"css"` / `"style"`).
#[derive(Debug, Default, Clone)]
struct RenameMap {
    /// `local_name → original_api`. Includes the trivial identity
    /// entries `("css" → "css")`, `("style" → "style")` so the lookup
    /// is uniform.
    inner: indexmap::IndexMap<String, String>,
}

impl RenameMap {
    fn original(&self, local: &str) -> Option<&str> {
        self.inner.get(local).map(String::as_str)
    }
}

/// Walk the function's parameters and pull out the (local, original)
/// pairs for `css` / `style`. Mirrors upstream's
/// `resolveIdentifierComingFromDestructuring` shape for the common
/// `({ css, style })` / `({ css: c, style: s })` patterns.
fn rename_map_from_arrow(arrow: &ArrowExpr) -> RenameMap {
    let mut out = RenameMap::default();
    out.inner.insert("css".into(), "css".into());
    out.inner.insert("style".into(), "style".into());
    if let Some(first) = arrow.params.first() {
        record_rename_from_pat(first, &mut out);
    }
    out
}

fn rename_map_from_function(fun: &Function) -> RenameMap {
    let mut out = RenameMap::default();
    out.inner.insert("css".into(), "css".into());
    out.inner.insert("style".into(), "style".into());
    if let Some(first) = fun.params.first() {
        record_rename_from_pat(&first.pat, &mut out);
    }
    out
}

fn record_rename_from_pat(pat: &Pat, out: &mut RenameMap) {
    let Pat::Object(obj) = pat else { return };
    for prop in &obj.props {
        match prop {
            ObjectPatProp::KeyValue(kv) => {
                let key = match &kv.key {
                    PropName::Ident(id) => id.sym.as_ref().to_string(),
                    PropName::Str(s) => s.value.to_atom_lossy().as_str().to_string(),
                    _ => continue,
                };
                if key == "css" || key == "style" {
                    if let Pat::Ident(BindingIdent { id, .. }) = &*kv.value {
                        out.inner.insert(id.sym.as_ref().to_string(), key);
                    }
                }
            }
            ObjectPatProp::Assign(a) => {
                let local = a.key.id.sym.as_ref().to_string();
                if local == "css" || local == "style" {
                    out.inner.insert(local.clone(), local);
                }
            }
            ObjectPatProp::Rest(_) => {}
        }
    }
}

/// Test whether the dispatch element is a `<ClassNames>` element bound
/// to a Compiled `class_names` import. Mirrors upstream's
/// `meta.state.compiledImports?.ClassNames?.includes(name)` check.
fn is_class_names_element(el: &JSXElement, state: &State) -> bool {
    let JSXElementName::Ident(id) = &el.opening.name else {
        return false;
    };
    state
        .compiled_imports()
        .and_then(|imp| imp.class_names.as_ref())
        .map(|names| names.iter().any(|n| n == id.sym.as_ref()))
        .unwrap_or(false)
}

/// Find the children-as-function expression. Returns the inner Function
/// or ArrowExpr Expr. Panics on absence (matches upstream's throw).
fn get_jsx_children_function(el: &JSXElement) -> Expr {
    for child in &el.children {
        if let JSXElementChild::JSXExprContainer(c) = child {
            if let JSXExpr::Expr(e) = &c.expr {
                if matches!(&**e, Expr::Arrow(_) | Expr::Fn(_)) {
                    return (**e).clone();
                }
            }
        }
    }
    panic!(
        "ClassNames children should be a function\nE.g: <ClassNames>{{props => <div />}}</ClassNames>"
    );
}

/// `extractStyles` upstream lines 34–81. Returns `Some(args)` when the
/// expression is a recognised css()-shape; otherwise `None`.
fn extract_styles(expr: &Expr, rename: &RenameMap) -> Option<Vec<Expr>> {
    match expr {
        Expr::Call(call) => {
            // Form 1: `css({...})` or rename `c({...})`.
            if let Callee::Expr(callee) = &call.callee {
                if let Expr::Ident(id) = &**callee {
                    if rename.original(id.sym.as_ref()) == Some("css") {
                        return Some(args_to_exprs(&call.args));
                    }
                }
                // Form 2: `props.css({...})`.
                if let Expr::Member(MemberExpr {
                    prop: MemberProp::Ident(prop),
                    ..
                }) = &**callee
                {
                    if prop.sym.as_ref() == "css" && !call.args.is_empty() {
                        return Some(args_to_exprs(&call.args));
                    }
                }
            }
            None
        }
        Expr::TaggedTpl(tpl) => {
            // Form 3: `css\`...\`` — args is the template literal as
            // a single Expr.
            // Upstream returns `path.node.quasi` directly; the
            // build_css dispatcher handles `Expr::Tpl`.
            Some(vec![Expr::Tpl((*tpl.tpl).clone())])
        }
        _ => None,
    }
}

fn args_to_exprs(args: &[ExprOrSpread]) -> Vec<Expr> {
    args.iter()
        .filter_map(|a| {
            if a.spread.is_some() {
                None
            } else {
                Some((*a.expr).clone())
            }
        })
        .collect()
}

/// First-pass `VisitMut` — replaces every `css(...)` shape with
/// `ax([classNames])` and accumulates sheets + variables.
struct CssCallReplacer<'a> {
    state: &'a mut State,
    recorder: &'a mut MutationRecorder,
    scope_index: &'a mut ScopeIndex,
    parent_scope: ScopeId,
    rename: RenameMap,
    runtime_lib: &'static str,
    sheets: Vec<String>,
    variables: Vec<Variable>,
}

impl<'a> VisitMut for CssCallReplacer<'a> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);

        let Some(styles) = extract_styles(n, &self.rename) else {
            return;
        };
        // Upstream: `buildCss(styles, meta)` where `styles` may be an
        // array (multi-arg css) OR a single Expr. The Rust port's
        // build_css takes a single Expr; the array path goes through
        // extract_array via the dispatcher's Array branch. For
        // ClassNames the corpus reach is single-arg css(); we honour
        // multi-arg by wrapping into an ArrayLit.
        let styles_node = if styles.len() == 1 {
            styles.into_iter().next().expect("len == 1")
        } else {
            Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: styles
                    .into_iter()
                    .map(|e| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(e),
                        })
                    })
                    .collect(),
            })
        };

        let mut meta = Metadata {
            state: self.state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };

        let css_output = match build_css(
            &styles_node,
            &mut meta,
            self.scope_index,
            self.parent_scope,
            None,
            self.recorder,
        ) {
            Ok(o) => o,
            Err(e) => panic!("{}", e.message),
        };

        let TransformCssItemsResult { sheets, class_names } =
            transform_css_items(&css_output.css, &mut meta);

        self.sheets.extend(sheets);
        self.variables.extend(css_output.variables);

        // Replace `css(...)` with `ax([...classNames])`.
        let replacement = make_call(
            self.runtime_lib,
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
        *n = replacement;
    }
}

/// Second-pass `VisitMut` — replaces `style` Identifier and `<x>.style`
/// MemberExpression references with the variables-built ObjectExpression
/// (or `undefined` when no variables collected).
struct StyleRefReplacer<'a> {
    rename: RenameMap,
    variables: &'a [Variable],
}

impl<'a> StyleRefReplacer<'a> {
    fn build_style_value(&self) -> Expr {
        if self.variables.is_empty() {
            Expr::Ident(Ident::new(
                "undefined".into(),
                DUMMY_SP,
                Default::default(),
            ))
        } else {
            // build_css_variables returns Vec<PropOrSpread>.
            let props = build_css_variables(self.variables, |e| e);
            Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props,
            })
        }
    }
}

impl<'a> VisitMut for StyleRefReplacer<'a> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);

        match n {
            Expr::Ident(id) => {
                // `style={style}` or rename `style={s}`.
                if self.rename.original(id.sym.as_ref()) == Some("style") {
                    *n = self.build_style_value();
                }
            }
            Expr::Member(m) => {
                if let MemberProp::Ident(prop) = &m.prop {
                    if prop.sym.as_ref() == "style" {
                        // `style={props.style}`.
                        *n = self.build_style_value();
                    }
                }
            }
            _ => {}
        }
    }

    /// Don't descend into ObjectExpression KeyValue keys — upstream's
    /// `path.parentPath.isProperty()` skip avoids replacing the `style`
    /// inside `{ style: value }` keys.
    fn visit_mut_key_value_prop(&mut self, n: &mut KeyValueProp) {
        // Skip the key entirely; recurse into the value only.
        n.value.visit_mut_with(self);
    }
}

/// Try to handle a `<ClassNames>...</ClassNames>` element. Returns
/// `Some(ClassNamesReplacement)` on success.
pub fn try_handle_jsx_element(
    el: &mut JSXElement,
    state: &mut State,
    recorder: &mut MutationRecorder,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
) -> Option<ClassNamesReplacement> {
    if !is_class_names_element(el, state) {
        return None;
    }

    // Build the rename map from the children-as-function's parameters.
    let children_fn = get_jsx_children_function(el);
    let rename = match &children_fn {
        Expr::Arrow(arrow) => rename_map_from_arrow(arrow),
        Expr::Fn(f) => rename_map_from_function(&f.function),
        _ => unreachable!("get_jsx_children_function only returns Arrow or Fn"),
    };

    let runtime_lib = {
        let meta = Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        };
        get_runtime_class_name_library(&meta)
    };

    // First pass — replace css() shapes with ax([...]).
    let mut css_pass = CssCallReplacer {
        state,
        recorder,
        scope_index,
        parent_scope,
        rename: rename.clone(),
        runtime_lib,
        sheets: Vec::new(),
        variables: Vec::new(),
    };
    el.visit_mut_with(&mut css_pass);
    let CssCallReplacer {
        sheets, variables, ..
    } = css_pass;

    // Second pass — replace style references.
    let mut style_pass = StyleRefReplacer {
        rename,
        variables: &variables,
    };
    el.visit_mut_with(&mut style_pass);

    // Pick the function body. Upstream calls `pickFunctionBody(children)`
    // where children is the function (BlockStmt → IIFE wrap, expression
    // body → return as-is).
    let body_expr = match get_jsx_children_function(el) {
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(_) => {
                // Wrap block via the existing pick_function_body helper —
                // it takes a Function shape, not an ArrowExpr. Convert
                // by hand.
                use crate::utils::ast::wrap_node_in_iife;
                Expr::Call(wrap_node_in_iife(BlockStmtOrExpr::BlockStmt(
                    match &*arrow.body {
                        BlockStmtOrExpr::BlockStmt(b) => b.clone(),
                        _ => unreachable!(),
                    },
                )))
            }
            BlockStmtOrExpr::Expr(e) => (**e).clone(),
        },
        Expr::Fn(f) => pick_function_body(&f.function),
        _ => unreachable!(),
    };

    let mut meta = Metadata {
        state,
        parent_id: 0,
        own_id: None,
        context: MetadataContext::Root,
        own_scope_override: None,
            in_conditional_branch: false,
    };
    let wrapper = compiled_template(
        Box::new(body_expr),
        &sheets,
        &mut meta,
        recorder,
    );
    let new_element = match wrapper {
        Expr::JSXElement(b) => *b,
        _ => unreachable!("compiled_template returns Expr::JSXElement"),
    };
    Some(ClassNamesReplacement { new_element })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::SyntaxContext;
    use swc_core::ecma::ast::{
        ArrowExpr, BindingIdent, BlockStmt, IdentName, JSXAttrOrSpread, JSXClosingElement,
        JSXElementName, JSXExprContainer, JSXOpeningElement, KeyValueProp, Lit, Module, ObjectLit,
        ObjectPat, ObjectPatProp, Param, Pat, Prop, PropName, PropOrSpread, Str,
    };

    use crate::mutation_recorder::{ApiKind, MutationRecorder, StateDiff};
    use crate::state::State;

    fn ident(name: &str) -> Ident {
        Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty())
    }

    fn jsx_name(name: &str) -> JSXElementName {
        JSXElementName::Ident(ident(name))
    }

    fn empty_module_index() -> ScopeIndex {
        let module = Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        ScopeIndex::build(&module)
    }

    fn arrow_with_destructured_param(props: Vec<&str>) -> ArrowExpr {
        ArrowExpr {
            span: DUMMY_SP,
            params: vec![Pat::Object(ObjectPat {
                span: DUMMY_SP,
                props: props
                    .into_iter()
                    .map(|p| {
                        ObjectPatProp::Assign(swc_core::ecma::ast::AssignPatProp {
                            span: DUMMY_SP,
                            key: BindingIdent {
                                id: ident(p),
                                type_ann: None,
                            },
                            value: None,
                        })
                    })
                    .collect(),
                optional: false,
                type_ann: None,
            })],
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: "".into(),
                raw: None,
            }))))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        }
    }

    fn make_class_names_element(children_expr: Box<Expr>) -> JSXElement {
        JSXElement {
            span: DUMMY_SP,
            opening: JSXOpeningElement {
                span: DUMMY_SP,
                name: jsx_name("ClassNames"),
                attrs: vec![],
                self_closing: false,
                type_args: None,
            },
            children: vec![JSXElementChild::JSXExprContainer(JSXExprContainer {
                span: DUMMY_SP,
                expr: JSXExpr::Expr(children_expr),
            })],
            closing: Some(JSXClosingElement {
                span: DUMMY_SP,
                name: jsx_name("ClassNames"),
            }),
        }
    }

    fn state_with_class_names_import() -> State {
        let mut s = State::default();
        let mut r = MutationRecorder::new();
        r.apply(
            StateDiff::CompiledImportsAppend {
                api: ApiKind::ClassNames,
                local_name: "ClassNames".into(),
            },
            &mut s,
        );
        s
    }

    #[test]
    fn rename_map_picks_up_simple_destructuring() {
        let arrow = arrow_with_destructured_param(vec!["css", "style"]);
        let map = rename_map_from_arrow(&arrow);
        assert_eq!(map.original("css"), Some("css"));
        assert_eq!(map.original("style"), Some("style"));
    }

    #[test]
    fn rename_map_picks_up_keyvalue_renames() {
        // ({ css: c, style: s }) => ...
        let arrow = ArrowExpr {
            span: DUMMY_SP,
            params: vec![Pat::Object(ObjectPat {
                span: DUMMY_SP,
                props: vec![
                    ObjectPatProp::KeyValue(swc_core::ecma::ast::KeyValuePatProp {
                        key: PropName::Ident(IdentName::new("css".into(), DUMMY_SP)),
                        value: Box::new(Pat::Ident(BindingIdent {
                            id: ident("c"),
                            type_ann: None,
                        })),
                    }),
                    ObjectPatProp::KeyValue(swc_core::ecma::ast::KeyValuePatProp {
                        key: PropName::Ident(IdentName::new("style".into(), DUMMY_SP)),
                        value: Box::new(Pat::Ident(BindingIdent {
                            id: ident("s"),
                            type_ann: None,
                        })),
                    }),
                ],
                optional: false,
                type_ann: None,
            })],
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: "".into(),
                raw: None,
            }))))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        };
        let map = rename_map_from_arrow(&arrow);
        assert_eq!(map.original("c"), Some("css"));
        assert_eq!(map.original("s"), Some("style"));
    }

    #[test]
    fn extract_styles_recognises_bare_css_call() {
        let map = RenameMap {
            inner: indexmap::IndexMap::from([
                ("css".to_string(), "css".to_string()),
                ("style".to_string(), "style".to_string()),
            ]),
        };
        let call = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(ident("css")))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props: vec![],
                })),
            }],
            type_args: None,
            ctxt: Default::default(),
        });
        let extracted = extract_styles(&call, &map).expect("recognised");
        assert_eq!(extracted.len(), 1);
    }

    #[test]
    fn extract_styles_recognises_props_css_call() {
        let map = RenameMap::default();
        let call = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(Expr::Ident(ident("props"))),
                prop: MemberProp::Ident(IdentName::new("css".into(), DUMMY_SP)),
            }))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props: vec![],
                })),
            }],
            type_args: None,
            ctxt: Default::default(),
        });
        assert!(extract_styles(&call, &map).is_some());
    }

    #[test]
    fn extract_styles_returns_none_on_unrelated_call() {
        let map = RenameMap::default();
        let call = Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(ident("foo")))),
            args: vec![],
            type_args: None,
            ctxt: Default::default(),
        });
        assert!(extract_styles(&call, &map).is_none());
    }

    #[test]
    fn dispatch_skips_non_class_names_jsx() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();
        let arrow = Expr::Arrow(arrow_with_destructured_param(vec!["css", "style"]));
        let mut el = make_class_names_element(Box::new(arrow));
        // tag = ClassNames but state has no class_names import.
        let res = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        assert!(res.is_none());
    }

    #[test]
    fn dispatch_handles_simple_class_names_with_no_inner_css_calls() {
        // <ClassNames>{({css, style}) => "noop"}</ClassNames>
        let mut state = state_with_class_names_import();
        let mut recorder = MutationRecorder::new();
        let mut idx = empty_module_index();
        let parent = idx.program_scope();

        let arrow = Expr::Arrow(arrow_with_destructured_param(vec!["css", "style"]));
        let mut el = make_class_names_element(Box::new(arrow));
        let res = try_handle_jsx_element(&mut el, &mut state, &mut recorder, &mut idx, parent);
        let replacement = res.expect("should wrap");

        if let JSXElementName::Ident(id) = &replacement.new_element.opening.name {
            assert_eq!(id.sym.as_ref(), "CC");
        } else {
            panic!("expected CC wrapper");
        }
    }
}
