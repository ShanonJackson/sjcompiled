//! 1:1 port of `packages/babel-plugin/src/utils/build-styled-component.ts`.
//!
//! Phase 6 §6.7 — second-largest helper after `build_compiled_component`.
//! Builds the `forwardRef(({ as: C = "div", style, ...props }, ref) =>
//! <CC><CS>{cssNode}</CS><C ...></CC>)` wrapper that replaces a styled
//! call/tagged-tpl. Hand-built JSX shape mirrors upstream's
//! `@babel/template`-driven body byte-for-byte.
//!
//! ### Babel → SWC divergences
//!
//! * Babel's `@babel/template(...)` parser turns a JSX string into an
//!   AST. The Rust port hand-builds the equivalent SWC tree directly
//!   — no template parser dep. Same printed bytes (modulo whitespace,
//!   which prettier flattens before the parity oracle hashes).
//! * `t.spreadElement` (Pat-side, ObjectPattern) maps to SWC
//!   `ObjectPatProp::Rest`. JSX-side spread (e.g. `{...props}`) maps
//!   to `JSXAttrOrSpread::SpreadElement` carrying a SWC `SpreadElement`.
//! * Babel's `t.assignmentPattern(left, right)` (default-arg pattern)
//!   maps to SWC `Pat::Assign(AssignPat { left, right, .. })`.
//!
//! ### invalidDomPropsVisitor scope
//!
//! Upstream walks `meta.parentPath` (the styled call itself, since
//! the dispatcher sets `meta.parentPath = path`). The visitor
//! collects `__cmplp.<name>` MemberExpression references whose
//! property name is NOT `children` and is NOT a valid DOM prop per
//! `@emotion/is-prop-valid`. Those names get destructured out of
//! `__cmplp` ahead of the spread, so React doesn't warn about
//! unknown DOM props.
//!
//! In practice the styled call's subtree carries `__cmplp.X` only
//! after CSS extraction has rewritten `props.X` → `__cmplp.X` — see
//! `css_builders::*` for where that rewrite lands. Fixtures that
//! reference `props.<invalidProp>` reach this walk and produce a
//! non-empty invalids set. Fixtures that don't end up with the
//! `props === __cmplp` rename produce an empty set; the wrapper body
//! emits the simpler shape.
//!
//! ### `findOpenSelectors` regex parity
//!
//! Upstream uses the JS RegExp `/[^;\s].+\n?{/g`. The Rust port uses
//! the `regex` crate (already in tree via cssnano deps) with the
//! identical pattern. Both are non-anchored, return all matches over
//! the input. JS regex caveats that don't bite this pattern:
//!
//! * `.` does NOT match `\n` in either — fine, the input is
//!   pre-cleaned of quoted strings carrying `{`/`}`.
//! * `\s` is `[\t\n\v\f\r ]` in JS and `\s` matches the same set in
//!   Rust's default `regex` flags.
//!
//! ### Process-env divergences
//!
//! * `process.env.NODE_ENV !== 'production'` and the `BABEL_ENV` /
//!   `NODE_ENV` checks are RUNTIME concerns in the upstream output —
//!   the emitted JS contains literal `process.env.NODE_ENV !==
//!   'production'` checks, NOT a build-time evaluation. The Rust port
//!   emits the same literal strings; bundlers downstream (Webpack,
//!   esbuild, Parcel) replace `process.env.NODE_ENV` per their
//!   `define` config.
//! * Upstream's `isDevelopmentEnv` build-time check (lines 150–153)
//!   gates EMIT of the `if (props.innerRef) throw ...` code. The
//!   gate fires at babel build time, so a production-build of the
//!   plugin omits the check entirely. The Rust port reads the host's
//!   `NODE_ENV` / `BABEL_ENV` env vars at plugin invocation; SWC
//!   runs once per file in the WASI sandbox, so the env read mirrors
//!   upstream's "called once per file" semantics.

use compiled_utils::unique;
use css::{transform_css, TransformOpts};
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    ArrayLit, ArrowExpr, AssignPat, BinExpr, BinaryOp, BindingIdent, BlockStmt, BlockStmtOrExpr,
    CallExpr, Callee, CondExpr, Expr, ExprOrSpread, ExprStmt, Function, Ident, IdentName, IfStmt,
    JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXClosingElement, JSXElement,
    JSXElementChild, JSXElementName, JSXExpr, JSXExprContainer, JSXOpeningElement, KeyValuePatProp,
    Lit, MemberExpr, MemberProp, NewExpr, ObjectLit, ObjectPat, ObjectPatProp, Pat, PropName,
    PropOrSpread, RestPat, ReturnStmt, SpreadElement, Stmt, Str, ThrowStmt, VarDecl, VarDeclKind,
    VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::compat::is_prop_valid::is_prop_valid;
use crate::constants::{
    DOM_PROPS_IDENTIFIER_NAME, PROPS_IDENTIFIER_NAME, REF_IDENTIFIER_NAME, STYLE_IDENTIFIER_NAME,
};
use crate::mutation_recorder::MutationRecorder;
use crate::types::{Metadata, PluginOptions, Tag, TagKind};
use crate::utils::ast::pick_function_body;
use crate::utils::build_css_variables::build_css_variables;
use crate::utils::compress_class_names_for_runtime::compress_class_names_for_runtime;
use crate::utils::css_builders::get_item_css;
use crate::utils::get_runtime_class_name_library::get_runtime_class_name_library;
use crate::utils::hoist_sheet::hoist_sheet;
use crate::utils::transform_css_items::{apply_selectors, transform_css_items};
use crate::utils::types::{CSSOutput, CssItem};

// ───────── Helpers ─────────

fn ident(name: &str) -> Ident {
    Ident::new(name.into(), DUMMY_SP, Default::default())
}

fn ident_expr(name: &str) -> Box<Expr> {
    Box::new(Expr::Ident(ident(name)))
}

fn ident_name(name: &str) -> IdentName {
    IdentName::new(name.into(), DUMMY_SP)
}

fn jsx_element_name_ident(name: &str) -> JSXElementName {
    JSXElementName::Ident(ident(name))
}

fn jsx_attr_name_ident(name: &str) -> JSXAttrName {
    JSXAttrName::Ident(ident_name(name))
}

fn jsx_expr_container(expr: Box<Expr>) -> JSXExprContainer {
    JSXExprContainer {
        span: DUMMY_SP,
        expr: JSXExpr::Expr(expr),
    }
}

fn str_lit(value: &str) -> Box<Expr> {
    Box::new(Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: value.into(),
        raw: None,
    })))
}

fn make_call(callee: Box<Expr>, args: Vec<ExprOrSpread>) -> Expr {
    Expr::Call(CallExpr {
        span: DUMMY_SP,
        callee: Callee::Expr(callee),
        args,
        type_args: None,
        ctxt: Default::default(),
    })
}

// ───────── styledStyleProp ─────────

/// `styledStyleProp` upstream lines 51–65. Builds `{ ...style,
/// ...buildCssVariables(...) }` where each arrow-function variable
/// expression has its body picked (so `(props) => props.color` becomes
/// `props.color` / `__cmplp.color`).
fn styled_style_prop(
    variables: &[crate::utils::types::Variable],
) -> Expr {
    let mut props: Vec<PropOrSpread> = Vec::with_capacity(variables.len() + 1);
    // `...__cmpls`
    props.push(PropOrSpread::Spread(SpreadElement {
        dot3_token: DUMMY_SP,
        expr: ident_expr(STYLE_IDENTIFIER_NAME),
    }));
    // Each variable's expression: arrow → pick function body, else
    // identity. Mirrors upstream's `(node) =>
    //   t.isArrowFunctionExpression(node) ? pickFunctionBody(node) : node`.
    let extra = build_css_variables(variables, |node| match &*node {
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                let function = Function {
                    params: vec![],
                    decorators: vec![],
                    span: DUMMY_SP,
                    body: Some(block.clone()),
                    is_generator: false,
                    is_async: false,
                    type_params: None,
                    return_type: None,
                    ctxt: Default::default(),
                };
                Box::new(pick_function_body(&function))
            }
            BlockStmtOrExpr::Expr(e) => e.clone(),
        },
        _ => node,
    });
    props.extend(extra);
    Expr::Object(ObjectLit {
        span: DUMMY_SP,
        props,
    })
}

// ───────── buildComponentTag ─────────

/// `buildComponentTag` upstream lines 75–77. Returns the `as: C =
/// <tag>` default expression. Strings for in-built tags, identifiers
/// for user-defined components.
fn build_component_tag_expr(tag: &Tag) -> Box<Expr> {
    match tag.kind {
        TagKind::InBuiltComponent => str_lit(&tag.name),
        TagKind::UserDefinedComponent => ident_expr(&tag.name),
    }
}

// ───────── invalidDomPropsVisitor ─────────

struct InvalidDomPropsVisitor {
    invalids: indexmap::IndexSet<String>,
}

impl Visit for InvalidDomPropsVisitor {
    fn visit_member_expr(&mut self, m: &MemberExpr) {
        // Mirrors `t.isIdentifier(object, { name: PROPS_IDENTIFIER_NAME })
        // && t.isIdentifier(property)`.
        if let Expr::Ident(obj) = &*m.obj {
            if obj.sym.as_ref() == PROPS_IDENTIFIER_NAME {
                if let MemberProp::Ident(prop) = &m.prop {
                    let name = prop.sym.as_ref();
                    if name != "children" && !is_prop_valid(name) {
                        self.invalids.insert(name.to_string());
                    }
                }
            }
        }
        // Continue walking — nested MemberExpressions like `__cmplp.x.y`
        // emit `x` (top-level prop name); the inner walk doesn't fire
        // because the outer object isn't `__cmplp`.
        m.visit_children_with(self);
    }
}

/// `getInvalidDomProps` upstream lines 100–107. Walks the path and
/// returns the set of `__cmplp.<name>` accesses where `<name>` is not
/// a valid DOM prop. Order preserved (IndexSet), matching upstream's
/// `Array.from(state.invalids)` over a `Set` insertion order.
fn get_invalid_dom_props(node: &Expr) -> Vec<String> {
    let mut visitor = InvalidDomPropsVisitor {
        invalids: indexmap::IndexSet::new(),
    };
    node.visit_with(&mut visitor);
    visitor.invalids.into_iter().collect()
}

// ───────── findOpenSelectors ─────────

/// `findOpenSelectors` upstream lines 207–218. Returns the open
/// selector strings (e.g. `:hover {`) inside an unconditional CSS
/// run.
fn find_open_selectors(css: &str) -> Vec<String> {
    // Mirror JS `css.replace(/['|"].*[{|}].*['|"]/g, '')` — strip any
    // `'...'` / `"..."` chunks that contain `{` or `}` so they don't
    // interfere with closure matches. Note the JS regex's character
    // class `['|"]` matches ANY of `'`, `|`, `"` (the `|` inside `[]`
    // is literal pipe), and similarly `[{|}]` matches `{`, `|`, `}`.
    // Bug-parity rule: replicate this verbatim.
    static STRIP: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r#"['|"].*[{|}].*['|"]"#).expect("static regex")
    });
    static OPEN: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"[^;\s].+\n?\{").expect("static regex"));

    let cleaned = STRIP.replace_all(css, "").to_string();

    // Substring after the LAST `}`.
    let search_area = match cleaned.rfind('}') {
        Some(idx) => &cleaned[idx + 1..],
        None => cleaned.as_str(),
    };

    OPEN.find_iter(search_area)
        .map(|m| m.as_str().to_string())
        .collect()
}

// ───────── styledTemplate ─────────

#[derive(Debug)]
pub struct StyledTemplateOpts {
    pub class_names: Vec<Box<Expr>>,
    pub tag: Tag,
    pub variables: Vec<crate::utils::types::Variable>,
    pub sheets: Vec<String>,
    /// §6.8x — the ORIGINAL styled CallExpr / TaggedTpl AST node, as
    /// it lived in source BEFORE CSS extraction. Used as the SOLE
    /// input to the invalid-DOM-prop walk inside `styled_template`,
    /// mirroring upstream's
    /// `getInvalidDomProps(meta.parentPath)` at
    /// `packages/babel-plugin/src/utils/build-styled-component.ts:123`.
    ///
    /// **Why this and only this:** upstream's `path.traverse` walks
    /// the static AST subtree of the styled call. It does NOT
    /// auto-resolve identifier arguments; for
    /// `styled.div(tabStyles)` it sees only the literal
    /// `styled.div(tabStyles)` text — no `__cmplp.<name>`
    /// MemberExprs reachable through the `tabStyles` Identifier.
    /// To match byte-for-byte, the Rust port must walk this same
    /// node and ONLY this node.
    ///
    /// Earlier ports (§6.8g/§6.8h/§6.8p) walked `class_names` /
    /// `variables[].expression` / a post-extraction CSS node. Those
    /// walks see resolved-init expansions that upstream's
    /// `parentPath.traverse` cannot reach, so they over-report
    /// invalid DOM props and emit a spurious
    /// `const { X, Y, ...__cmpldp } = __cmplp` destructure
    /// (drift exposed by the `ct-hover-display` snapshot fix
    /// 2026-05-07; see FIXTURES_STATUS.md).
    pub original_styled_call: Option<Expr>,
    /// §6.8p — the binding name of the surrounding `const X = styled...`
    /// VarDecl when its first declarator is a `Pat::Ident`. Powers
    /// the `addComponentName: true` `c_<name>` className emit
    /// upstream wires from `meta.parentPath.findParent(VariableDeclaration)`.
    /// `None` when the styled expression is not directly assigned to
    /// a `Pat::Ident` declarator (e.g. `[X] = [styled.div...]` or
    /// returned from an arrow body) — same scope-of-pre-detect
    /// caveat as the displayName queue.
    pub declared_var_name: Option<String>,
}

/// `styledTemplate` upstream lines 115–199. Returns the
/// `forwardRef(({...}) => <CC>...</CC>)` Expr.
fn styled_template(
    opts: StyledTemplateOpts,
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
) -> Expr {
    // ───── styleProp ─────
    let style_prop_expr: Box<Expr> = if opts.variables.is_empty() {
        ident_expr(STYLE_IDENTIFIER_NAME)
    } else {
        Box::new(styled_style_prop(&opts.variables))
    };

    let is_in_built_component = opts.tag.kind == TagKind::InBuiltComponent;

    // ───── invalidDomProps ─────
    //
    // Upstream `build-styled-component.ts:123`:
    //   const invalidDomProps = isInBuiltComponent
    //     ? getInvalidDomProps(meta.parentPath)
    //     : [];
    //
    // `meta.parentPath` is the styled CallExpr / TaggedTpl path.
    // `path.traverse(invalidDomPropsVisitor, state)` walks the
    // STATIC AST subtree of that node — it does NOT auto-resolve
    // identifier arguments. For `styled.div(tabStyles)` the only
    // nodes visited are `styled`, `div`, `tabStyles` — no
    // `__cmplp.<name>` MemberExprs reachable through the
    // `tabStyles` Identifier (the resolved init lives in a
    // separate VarDecl that the traversal cannot enter).
    //
    // The Rust port mirrors this by threading the original styled
    // call expression through `opts.original_styled_call` and
    // walking ONLY that node. The previous shape (walk
    // `class_names` + `variables[].expression` + a post-extraction
    // CSS node) saw resolved-init expansions that upstream's
    // `parentPath.traverse` cannot reach, over-reporting invalid
    // DOM props for any fixture that resolves a binding into a
    // styled call (e.g. `ct-hover-display`).
    //
    // For the `styled.div(tabStyles)` case both inputs are bare
    // Identifiers — `get_invalid_dom_props` produces `[]` —
    // exactly matching Babel's parentPath walk.
    let invalid_dom_props: Vec<String> = if is_in_built_component {
        match &opts.original_styled_call {
            Some(node) => get_invalid_dom_props(node),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let has_invalid_dom_props = !invalid_dom_props.is_empty();

    // ───── classNames assembly ─────
    //
    // Upstream lines 126–149 build a single string template:
    //   `${componentClassName}"${unconditionalClassNames.trim()}", ${conditionalClassNames}`
    //
    // We hand-build the equivalent ArrayLit elements:
    //
    //   * componentClassName: optional Str("c_<name>") when
    //     addComponentName + non-prod + variableName present.
    //   * unconditionalClassNames: a single Str holding the
    //     space-joined StringLiteral classNames from
    //     `opts.class_names`.
    //   * conditionalClassNames: each Logical/Conditional Expr from
    //     `opts.class_names` is appended directly.
    //   * trailing __cmplp.className.
    let component_name =
        derive_component_name_from_opts(&opts).or_else(|| derive_component_name(meta));
    let component_class_name: Option<String> = component_class_name_for(meta, component_name.as_deref());

    let mut unconditional_buf = String::new();
    let mut conditional_exprs: Vec<Box<Expr>> = Vec::new();

    for item in &opts.class_names {
        match &**item {
            Expr::Lit(Lit::Str(s)) => {
                unconditional_buf.push_str(s.value.to_atom_lossy().as_str());
                unconditional_buf.push(' ');
            }
            Expr::Bin(b) if matches!(
                b.op,
                BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
            ) => {
                conditional_exprs.push(item.clone());
            }
            Expr::Cond(_) => {
                conditional_exprs.push(item.clone());
            }
            _ => {
                // Anything else (defensive): treat as literal-position
                // pass-through. Upstream's branches don't catch this
                // either.
                conditional_exprs.push(item.clone());
            }
        }
    }

    let unconditional_trimmed = unconditional_buf.trim().to_string();

    let mut classnames_array: Vec<Option<ExprOrSpread>> = Vec::new();
    if let Some(cc_name) = &component_class_name {
        classnames_array.push(Some(ExprOrSpread {
            spread: None,
            expr: str_lit(cc_name),
        }));
    }
    classnames_array.push(Some(ExprOrSpread {
        spread: None,
        expr: str_lit(&unconditional_trimmed),
    }));
    for expr in conditional_exprs {
        classnames_array.push(Some(ExprOrSpread {
            spread: None,
            expr,
        }));
    }
    // Trailing `__cmplp.className`.
    classnames_array.push(Some(ExprOrSpread {
        spread: None,
        expr: Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr(PROPS_IDENTIFIER_NAME),
            prop: MemberProp::Ident(ident_name("className")),
        })),
    }));

    let class_name_lib = get_runtime_class_name_library(meta);
    let class_name_call = make_call(
        ident_expr(class_name_lib),
        vec![ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: classnames_array,
            })),
        }],
    );

    // ───── cssNode array (hoisted sheets) ─────
    let unique_sheets = unique(&opts.sheets);
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

    // ───── arrow function body ─────
    let mut body_stmts: Vec<Stmt> = Vec::new();

    // `if (__cmplp.innerRef) { throw new Error("Please use 'ref' instead of 'innerRef'."); }`
    // — emitted bare (no NODE_ENV runtime wrapper); gated at build time by
    // `is_development_env()` per upstream build-styled-component.ts:150-168.
    if is_development_env() {
        body_stmts.push(build_inner_ref_guard());
    }

    // `const { invalidProp1, invalidProp2, ...__cmpldp } = __cmplp;`
    if has_invalid_dom_props {
        body_stmts.push(build_invalid_dom_props_destructure(&invalid_dom_props));
    }

    // `return (<CC><CS [nonce={...}]>{cssArray}</CS><C ... /></CC>);`
    let nonce_attr_expr: Option<Box<Expr>> = meta
        .state
        .opts()
        .nonce
        .as_ref()
        .map(|name| ident_expr(name));

    let mut cs_attrs: Vec<JSXAttrOrSpread> = Vec::new();
    if let Some(nonce) = nonce_attr_expr {
        cs_attrs.push(JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("nonce"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(nonce))),
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

    // Inner `<C {...__cmpldp|__cmplp} style={...} ref={...} className={...} />`
    let spread_target = if has_invalid_dom_props {
        DOM_PROPS_IDENTIFIER_NAME
    } else {
        PROPS_IDENTIFIER_NAME
    };
    let c_attrs: Vec<JSXAttrOrSpread> = vec![
        JSXAttrOrSpread::SpreadElement(SpreadElement {
            dot3_token: DUMMY_SP,
            expr: ident_expr(spread_target),
        }),
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("style"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                style_prop_expr,
            ))),
        }),
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("ref"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                ident_expr(REF_IDENTIFIER_NAME),
            ))),
        }),
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: jsx_attr_name_ident("className"),
            value: Some(JSXAttrValue::JSXExprContainer(jsx_expr_container(
                Box::new(class_name_call),
            ))),
        }),
    ];
    let c_element = JSXElement {
        span: DUMMY_SP,
        opening: JSXOpeningElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("C"),
            attrs: c_attrs,
            self_closing: true,
            type_args: None,
        },
        children: vec![],
        closing: None,
    };

    let cc_element = JSXElement {
        span: DUMMY_SP,
        opening: JSXOpeningElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CC"),
            attrs: vec![],
            self_closing: false,
            type_args: None,
        },
        children: vec![
            JSXElementChild::JSXElement(Box::new(cs_element)),
            JSXElementChild::JSXElement(Box::new(c_element)),
        ],
        closing: Some(JSXClosingElement {
            span: DUMMY_SP,
            name: jsx_element_name_ident("CC"),
        }),
    };

    body_stmts.push(Stmt::Return(ReturnStmt {
        span: DUMMY_SP,
        arg: Some(Box::new(Expr::JSXElement(Box::new(cc_element)))),
    }));

    // ───── arrow params ─────
    //
    // `({ as: C = <tagExpr>, style: __cmpls, ...__cmplp }, __cmplr)`
    let object_pat = Pat::Object(ObjectPat {
        span: DUMMY_SP,
        props: vec![
            ObjectPatProp::KeyValue(KeyValuePatProp {
                key: PropName::Ident(ident_name("as")),
                value: Box::new(Pat::Assign(AssignPat {
                    span: DUMMY_SP,
                    left: Box::new(Pat::Ident(BindingIdent {
                        id: ident("C"),
                        type_ann: None,
                    })),
                    right: build_component_tag_expr(&opts.tag),
                })),
            }),
            ObjectPatProp::KeyValue(KeyValuePatProp {
                key: PropName::Ident(ident_name("style")),
                value: Box::new(Pat::Ident(BindingIdent {
                    id: ident(STYLE_IDENTIFIER_NAME),
                    type_ann: None,
                })),
            }),
            ObjectPatProp::Rest(RestPat {
                span: DUMMY_SP,
                dot3_token: DUMMY_SP,
                arg: Box::new(Pat::Ident(BindingIdent {
                    id: ident(PROPS_IDENTIFIER_NAME),
                    type_ann: None,
                })),
                type_ann: None,
            }),
        ],
        optional: false,
        type_ann: None,
    });

    let ref_param = Pat::Ident(BindingIdent {
        id: ident(REF_IDENTIFIER_NAME),
        type_ann: None,
    });

    let arrow = ArrowExpr {
        span: DUMMY_SP,
        params: vec![object_pat, ref_param],
        body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
            span: DUMMY_SP,
            stmts: body_stmts,
            ctxt: Default::default(),
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
        ctxt: Default::default(),
    };

    // ───── forwardRef wrap ─────
    make_call(
        ident_expr("forwardRef"),
        vec![ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Arrow(arrow)),
        }],
    )
}

/// Mirrors upstream lines 138–140:
/// `(meta.parentPath.parent as VariableDeclarator)?.id as Identifier?.name`.
///
/// The Rust port's caller (visit_mut_var_declarator hook in
/// `babel_plugin.rs`) sets the binding name onto the visitor's
/// pending-display-name queue when it detects a styled call init.
/// At this layer we don't have a direct handle to the queue; we
/// re-derive from `meta.parent_id` … which today is unset for the
/// styled handler. Returns `None` for now — the displayName insert
/// happens at a different code site (post-styled, in
/// `babel_plugin.rs`).
fn derive_component_name(_meta: &Metadata<'_>) -> Option<String> {
    None
}

/// §6.8p variant — derive the component name from the styled
/// VarDecl's binding (captured via `current_styled_var_name` on the
/// dispatcher visitor and threaded through `StyledTemplateOpts`).
/// Mirrors upstream's `findParent(VariableDeclaration)` walk for the
/// `addComponentName: true` className emit.
fn derive_component_name_from_opts(opts: &StyledTemplateOpts) -> Option<String> {
    opts.declared_var_name.clone()
}

/// Mirrors upstream lines 142–147. `c_<componentName>` when
/// `addComponentName=true` AND non-prod AND we have a name.
fn component_class_name_for(meta: &Metadata<'_>, component_name: Option<&str>) -> Option<String> {
    // JS `toBoolean(opts.addComponentName)`: `Boolean(undefined)` is
    // false, `Boolean(false)` is false, `Boolean(true)` is true. The
    // Rust port's `compiled_utils::to_boolean` returns `is_some()`,
    // which mishandles `Some(false)` → true; inline the proper check.
    let add = meta.state.opts().add_component_name.unwrap_or(false);
    if !add {
        return None;
    }
    if is_production_env() {
        return None;
    }
    component_name.map(|n| format!("c_{}", n))
}

// ───────── Env-var checks (build-time, mirrors upstream) ─────────

/// `process.env.NODE_ENV === 'production'` at build time.
fn is_production_env() -> bool {
    std::env::var("NODE_ENV")
        .map(|v| v == "production")
        .unwrap_or(false)
}

/// `isDevelopmentEnv` upstream lines 150–153. True when neither
/// `BABEL_ENV` nor `NODE_ENV` is set, OR either is `'development'` /
/// `'test'`.
fn is_development_env() -> bool {
    let babel_env = std::env::var("BABEL_ENV").ok();
    let node_env = std::env::var("NODE_ENV").ok();
    let neither_set = babel_env.is_none() && node_env.is_none();
    if neither_set {
        return true;
    }
    let is_dev_or_test = |v: Option<&String>| {
        v.map(|s| s.as_str() == "development" || s.as_str() == "test")
            .unwrap_or(false)
    };
    is_dev_or_test(babel_env.as_ref()) || is_dev_or_test(node_env.as_ref())
}

// ───────── Hand-built statements ─────────

/// `if (__cmplp.innerRef) { throw new Error("Please use 'ref' instead of 'innerRef'."); }`
///
/// Upstream `build-styled-component.ts:162-168` emits this bare `if` with no
/// outer `process.env.NODE_ENV !== 'production'` wrapper — the dev/test gate
/// is applied at BUILD TIME via `isDevelopmentEnv` (mirrored here by
/// `is_development_env()` at the call site), and the emitted statement is
/// the bare inner check. Earlier port wrapped this in an extra
/// NODE_ENV-runtime check, double-gating; corrected per §6.8 drift detection.
fn build_inner_ref_guard() -> Stmt {
    let inner_test = Expr::Member(MemberExpr {
        span: DUMMY_SP,
        obj: ident_expr(PROPS_IDENTIFIER_NAME),
        prop: MemberProp::Ident(ident_name("innerRef")),
    });
    let throw_stmt = Stmt::Throw(ThrowStmt {
        span: DUMMY_SP,
        arg: Box::new(Expr::New(NewExpr {
            span: DUMMY_SP,
            callee: ident_expr("Error"),
            args: Some(vec![ExprOrSpread {
                spread: None,
                expr: str_lit("Please use 'ref' instead of 'innerRef'."),
            }]),
            type_args: None,
            ctxt: Default::default(),
        })),
    });
    Stmt::If(IfStmt {
        span: DUMMY_SP,
        test: Box::new(inner_test),
        cons: Box::new(Stmt::Block(BlockStmt {
            span: DUMMY_SP,
            stmts: vec![throw_stmt],
            ctxt: Default::default(),
        })),
        alt: None,
    })
}

/// `const { <invalid1>, <invalid2>, ...__cmpldp } = __cmplp;`
fn build_invalid_dom_props_destructure(invalid: &[String]) -> Stmt {
    let mut props: Vec<ObjectPatProp> = invalid
        .iter()
        .map(|name| {
            ObjectPatProp::Assign(swc_core::ecma::ast::AssignPatProp {
                span: DUMMY_SP,
                key: BindingIdent {
                    id: ident(name),
                    type_ann: None,
                },
                value: None,
            })
        })
        .collect();
    props.push(ObjectPatProp::Rest(RestPat {
        span: DUMMY_SP,
        dot3_token: DUMMY_SP,
        arg: Box::new(Pat::Ident(BindingIdent {
            id: ident(DOM_PROPS_IDENTIFIER_NAME),
            type_ann: None,
        })),
        type_ann: None,
    }));

    Stmt::Decl(swc_core::ecma::ast::Decl::Var(Box::new(VarDecl {
        span: DUMMY_SP,
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Object(ObjectPat {
                span: DUMMY_SP,
                props,
                optional: false,
                type_ann: None,
            }),
            init: Some(ident_expr(PROPS_IDENTIFIER_NAME)),
            definite: false,
        }],
        ctxt: Default::default(),
    })))
}

// ───────── buildStyledComponent (public entry) ─────────

/// `buildStyledComponent` upstream lines 227–275. Sorts the
/// CssOutput into unconditional vs conditional/logical, runs
/// `transform_css` on the unconditional run for atomic dedup, runs
/// `transform_css_items` on the conditional run, builds the final
/// classNames + sheets, then delegates to `styled_template`.
///
/// `original_styled_call` (§6.8x) is the ORIGINAL styled CallExpr /
/// TaggedTpl AST node, as it lived in source BEFORE any CSS
/// extraction or identifier resolution. Threaded through
/// `StyledTemplateOpts.original_styled_call` and walked as the
/// SOLE input to the invalid-DOM-prop derivation, matching
/// upstream's `getInvalidDomProps(meta.parentPath)`. See the
/// field doc on `StyledTemplateOpts.original_styled_call` for
/// the rationale (the §6.8g/§6.8h/§6.8p drift retraction).
pub fn build_styled_component(
    tag: Tag,
    css_output: CSSOutput,
    original_styled_call: Option<&Expr>,
    declared_var_name: Option<&str>,
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
) -> Expr {
    let mut unconditional_css = String::new();
    let mut conditional_css_items: Vec<CssItem> = Vec::new();

    for item in css_output.css.iter() {
        match item {
            CssItem::Logical(_) | CssItem::Conditional(_) => {
                let mut item_clone = item.clone();
                let selectors = find_open_selectors(&unconditional_css);
                if !selectors.is_empty() {
                    apply_selectors(&mut item_clone, &selectors);
                }
                conditional_css_items.push(item_clone);
            }
            _ => {
                unconditional_css.push_str(&get_item_css(item));
            }
        }
    }

    let opts = plugin_opts_to_transform_opts(meta.state.opts());
    let unique_unconditional = transform_css(&unconditional_css, &opts)
        .unwrap_or_else(|e| panic!("transform_css failed in build_styled_component: {e}"));

    let conditional_output = transform_css_items(&conditional_css_items, meta);

    let mut sheets = unique_unconditional.sheets.clone();
    sheets.extend(conditional_output.sheets);

    let compressed_unconditional = compress_class_names_for_runtime(
        unique_unconditional.class_names,
        meta.state.opts().class_name_compression_map.as_ref(),
    )
    .join(" ");

    let mut class_names: Vec<Box<Expr>> =
        vec![str_lit(&compressed_unconditional)];
    class_names.extend(conditional_output.class_names);

    styled_template(
        StyledTemplateOpts {
            class_names,
            tag,
            sheets,
            variables: css_output.variables,
            original_styled_call: original_styled_call.cloned(),
            declared_var_name: declared_var_name.map(|s| s.to_string()),
        },
        meta,
        recorder,
    )
}

/// Mirror of the plugin-opts → transform-opts conversion in
/// `transform_css_items.rs`. Repeated here because `transform_css_items`
/// keeps it private.
fn plugin_opts_to_transform_opts(opts: &PluginOptions) -> TransformOpts {
    TransformOpts {
        optimize_css: opts.optimize_css,
        class_name_compression_map: opts.class_name_compression_map.clone(),
        increase_specificity: opts.increase_specificity,
        sort_at_rules: opts.sort_at_rules,
        sort_shorthand: None,
        class_hash_prefix: opts.class_hash_prefix.clone(),
        precomputed_prefixes: None,
        precomputed_prefixes_path: None,
    }
}

// Suppress unused-import warnings on Expr/ExprStmt placeholder paths
// kept for future Phase 7 stmt-level edits.
const _: Option<ExprStmt> = None;
const _: Option<CondExpr> = None;
const _: Option<JSXAttr> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation_recorder::MutationRecorder;
    use crate::state::State;
    use crate::types::MetadataContext;
    use crate::utils::types::{UnconditionalCssItem, Variable};
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::Number;

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

    fn unconditional(css: &str) -> CssItem {
        CssItem::Unconditional(UnconditionalCssItem {
            css: css.to_string(),
        })
    }

    fn extract_call(expr: &Expr) -> &CallExpr {
        let Expr::Call(c) = expr else {
            panic!("not a CallExpr")
        };
        c
    }

    fn extract_arrow(expr: &Expr) -> &ArrowExpr {
        let Expr::Arrow(a) = expr else {
            panic!("not Arrow")
        };
        a
    }

    fn callee_name<'a>(call: &'a CallExpr) -> &'a str {
        let Callee::Expr(c) = &call.callee else {
            panic!("non-expr callee")
        };
        let Expr::Ident(id) = &**c else {
            panic!("callee not Ident")
        };
        id.sym.as_ref()
    }

    fn extract_block(arrow: &ArrowExpr) -> &BlockStmt {
        let BlockStmtOrExpr::BlockStmt(b) = &*arrow.body else {
            panic!("body not BlockStmt")
        };
        b
    }

    // ───────── is_prop_valid integration ─────────

    #[test]
    fn invalid_dom_props_visitor_picks_up_unknown_prop() {
        let expr = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr(PROPS_IDENTIFIER_NAME),
            prop: MemberProp::Ident(ident_name("isPrimary")),
        });
        let invalids = get_invalid_dom_props(&expr);
        assert_eq!(invalids, vec!["isPrimary".to_string()]);
    }

    #[test]
    fn invalid_dom_props_visitor_skips_valid_dom_prop() {
        let expr = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr(PROPS_IDENTIFIER_NAME),
            prop: MemberProp::Ident(ident_name("href")),
        });
        let invalids = get_invalid_dom_props(&expr);
        assert!(invalids.is_empty());
    }

    #[test]
    fn invalid_dom_props_visitor_skips_children() {
        let expr = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: ident_expr(PROPS_IDENTIFIER_NAME),
            prop: MemberProp::Ident(ident_name("children")),
        });
        let invalids = get_invalid_dom_props(&expr);
        assert!(invalids.is_empty());
    }

    // ───────── findOpenSelectors ─────────

    #[test]
    fn find_open_selectors_finds_hover_brace() {
        let css = "color: red; :hover {";
        let out = find_open_selectors(css);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains(":hover"));
    }

    #[test]
    fn find_open_selectors_returns_empty_when_balanced() {
        let css = ":hover { color: red; }";
        let out = find_open_selectors(css);
        assert!(out.is_empty());
    }

    // ───────── styled_style_prop ─────────

    #[test]
    fn styled_style_prop_starts_with_spread_of_style_ident() {
        let v = Variable {
            name: "--_a".into(),
            expression: Some(Box::new(Expr::Lit(Lit::Num(Number {
                span: DUMMY_SP,
                value: 8.0,
                raw: None,
            })))),
            prefix: None,
            suffix: None,
        };
        let expr = styled_style_prop(&[v]);
        let Expr::Object(obj) = expr else {
            panic!("not an object")
        };
        let PropOrSpread::Spread(s) = &obj.props[0] else {
            panic!("first not spread")
        };
        let Expr::Ident(id) = &*s.expr else {
            panic!("not Ident")
        };
        assert_eq!(id.sym.as_ref(), STYLE_IDENTIFIER_NAME);
    }

    // ───────── build_styled_component ─────────

    #[test]
    fn build_styled_component_returns_forward_ref_call() {
        let tag = Tag {
            name: "div".into(),
            kind: TagKind::InBuiltComponent,
        };
        let output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_styled_component(tag, output, None, None, &mut meta, &mut recorder);
        let call = extract_call(&result);
        assert_eq!(callee_name(call), "forwardRef");
        assert_eq!(call.args.len(), 1);
    }

    #[test]
    fn build_styled_component_arrow_has_two_params() {
        let tag = Tag {
            name: "div".into(),
            kind: TagKind::InBuiltComponent,
        };
        let output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_styled_component(tag, output, None, None, &mut meta, &mut recorder);
        let call = extract_call(&result);
        let arrow = extract_arrow(&call.args[0].expr);
        assert_eq!(arrow.params.len(), 2);
        // First param is an ObjectPat with `as` / `style` / rest.
        let Pat::Object(obj) = &arrow.params[0] else {
            panic!("first param not ObjectPat")
        };
        assert_eq!(obj.props.len(), 3);
        // Rest pattern targets PROPS_IDENTIFIER_NAME.
        let ObjectPatProp::Rest(rest) = &obj.props[2] else {
            panic!("third prop not Rest")
        };
        let Pat::Ident(BindingIdent { id, .. }) = &*rest.arg else {
            panic!("rest arg not Ident")
        };
        assert_eq!(id.sym.as_ref(), PROPS_IDENTIFIER_NAME);
        // Second arrow param is REF_IDENTIFIER_NAME.
        let Pat::Ident(BindingIdent { id: ref_id, .. }) = &arrow.params[1] else {
            panic!("ref param not Ident")
        };
        assert_eq!(ref_id.sym.as_ref(), REF_IDENTIFIER_NAME);
    }

    #[test]
    fn build_styled_component_inbuilt_emits_string_default_for_as() {
        let tag = Tag {
            name: "div".into(),
            kind: TagKind::InBuiltComponent,
        };
        let output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_styled_component(tag, output, None, None, &mut meta, &mut recorder);
        let call = extract_call(&result);
        let arrow = extract_arrow(&call.args[0].expr);
        let Pat::Object(obj) = &arrow.params[0] else {
            panic!("not ObjectPat")
        };
        let ObjectPatProp::KeyValue(kv) = &obj.props[0] else {
            panic!("not KeyValue")
        };
        let Pat::Assign(assign) = &*kv.value else {
            panic!("not Assign")
        };
        let Expr::Lit(Lit::Str(s)) = &*assign.right else {
            panic!("default not Str")
        };
        assert_eq!(s.value.to_atom_lossy().as_str(), "div");
    }

    #[test]
    fn build_styled_component_user_defined_emits_ident_default_for_as() {
        let tag = Tag {
            name: "MyButton".into(),
            kind: TagKind::UserDefinedComponent,
        };
        let output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_styled_component(tag, output, None, None, &mut meta, &mut recorder);
        let call = extract_call(&result);
        let arrow = extract_arrow(&call.args[0].expr);
        let Pat::Object(obj) = &arrow.params[0] else {
            panic!("not ObjectPat")
        };
        let ObjectPatProp::KeyValue(kv) = &obj.props[0] else {
            panic!("not KeyValue")
        };
        let Pat::Assign(assign) = &*kv.value else {
            panic!("not Assign")
        };
        let Expr::Ident(id) = &*assign.right else {
            panic!("default not Ident")
        };
        assert_eq!(id.sym.as_ref(), "MyButton");
    }

    #[test]
    fn build_styled_component_returns_cc_wrapped_jsx_in_body() {
        let tag = Tag {
            name: "div".into(),
            kind: TagKind::InBuiltComponent,
        };
        let output = CSSOutput {
            css: vec![unconditional("color: red;")],
            variables: vec![],
        };
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let result = build_styled_component(tag, output, None, None, &mut meta, &mut recorder);
        let call = extract_call(&result);
        let arrow = extract_arrow(&call.args[0].expr);
        let block = extract_block(arrow);
        // Last stmt is `return <CC>...</CC>;`
        let Stmt::Return(ret) = block.stmts.last().expect("non-empty body") else {
            panic!("last stmt not Return")
        };
        let Expr::JSXElement(jsx) = &**ret.arg.as_ref().expect("returns something") else {
            panic!("Return arg not JSX")
        };
        let JSXElementName::Ident(id) = &jsx.opening.name else {
            panic!("opening not Ident")
        };
        assert_eq!(id.sym.as_ref(), "CC");
    }
}
