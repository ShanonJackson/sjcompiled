//! 1:1 port of `packages/babel-plugin-strip-runtime/src/utils/remove-style-declarations.ts`.
//!
//! ```ts
//! export const removeStyleDeclarations = (
//!   node: t.Node,
//!   parentPath: NodePath<any>,
//!   pass: PluginPass
//! ): void => {
//!   const processElement = (element) => {
//!     if (!t.isIdentifier(element)) return;
//!     const [binding, value] = getBindingValue(element.name, parentPath);
//!     if (binding && value && t.isStringLiteral(value)) {
//!       pass.styleRules.push(value.value);
//!       binding.path.remove();
//!     }
//!   };
//!   if (t.isCallExpression(node) && isCreateElement(node.callee)) { ... }
//!   if (isAutomaticRuntime(node, 'jsx')) { ... }
//!   if (t.isJSXElement(node) && ... node.openingElement.name.name === 'CS') { ... }
//! };
//! ```
//!
//! Babel's `pass` carries `styleRules: string[]` and `parentPath.scope`
//! holds the live binding chain. The Rust port replaces both with
//! explicit parameters: `style_rules: &mut Vec<String>` and
//! `scope: &mut ModuleScope` (see `compat/scope.rs`).
//!
//! `binding.path.remove()` is deferred: this function only **marks**
//! the binding via `scope.mark_for_removal(name)`. The dispatcher in
//! `lib.rs` calls `scope.apply_removals(&mut module)` from
//! `Program::exit` to delete every marked declarator in one pass —
//! Babel removes mid-traversal, but doing the same in SWC would invite
//! borrow-checker conflicts during visitation.

use swc_core::ecma::ast::{Callee, Expr, JSXElement, JSXElementChild, JSXElementName, JSXExpr};

use crate::compat::scope::ModuleScope;
use crate::utils::is_automatic_runtime::{is_automatic_runtime, JsxFunc};
use crate::utils::is_create_element::is_create_element;

/// Process every identifier element in a CS-rules array: look up its
/// string binding, push the literal into `style_rules`, mark the
/// binding for removal. Mirrors the JS `processElement` closure.
fn process_array_elements(
    elements: &[Option<swc_core::ecma::ast::ExprOrSpread>],
    scope: &mut ModuleScope,
    style_rules: &mut Vec<String>,
) {
    for element in elements.iter().flatten() {
        // Spreads (`...rest`) and non-identifier elements are skipped,
        // matching upstream's `if (!t.isIdentifier(element)) return;`
        // inside the per-element callback.
        if element.spread.is_some() {
            continue;
        }
        let Expr::Ident(id) = element.expr.as_ref() else {
            continue;
        };
        let name = id.sym.to_string();
        if let Some(value) = scope.get_string_binding(&name) {
            let value = value.to_string();
            style_rules.push(value);
            scope.mark_for_removal(&name);
        }
    }
}

/// Remove style declarations referenced from `node` and accumulate the
/// extracted rules. `node` must be one of:
///
/// * `React.createElement(CS, ..., [_1, _2])` — classic runtime
/// * `_jsx(CS, { children: [_1, _2] })` — automatic runtime
/// * `<CS>{[_1, _2]}</CS>` — pre-JSX-transform source code
///
/// Anything else is a no-op.
pub fn remove_style_declarations(
    node: &Expr,
    scope: &mut ModuleScope,
    style_rules: &mut Vec<String>,
) {
    // ── React.createElement(CS, ..., [...]) ──
    if let Expr::Call(call) = node {
        if let Callee::Expr(callee) = &call.callee {
            if is_create_element(callee.as_ref()) {
                if let Some(third_arg) = call.args.get(2) {
                    if third_arg.spread.is_none() {
                        if let Expr::Array(arr) = third_arg.expr.as_ref() {
                            process_array_elements(&arr.elems, scope, style_rules);
                        }
                    }
                }
                return;
            }
        }
    }

    // ── _jsx(CS, { children: [...] }) ──
    if is_automatic_runtime(node, JsxFunc::Jsx) {
        if let Expr::Call(call) = node {
            // `getJsxRuntimeChildren` returns the values of every
            // ObjectProperty on the second arg, in source order. The
            // first such value is the styles array.
            if let Some(props_arg) = call.args.get(1) {
                if props_arg.spread.is_none() {
                    if let Expr::Object(obj) = props_arg.expr.as_ref() {
                        let first_value: Option<&Expr> = obj.props.iter().find_map(|p| {
                            use swc_core::ecma::ast::{Prop, PropOrSpread};
                            if let PropOrSpread::Prop(prop) = p {
                                if let Prop::KeyValue(kv) = prop.as_ref() {
                                    return Some(kv.value.as_ref());
                                }
                            }
                            None
                        });
                        if let Some(Expr::Array(arr)) = first_value {
                            process_array_elements(&arr.elems, scope, style_rules);
                        }
                    }
                }
            }
            return;
        }
    }

    // ── <CS>{[...]}</CS> ──
    if let Expr::JSXElement(jsx) = node {
        if let JSXElementName::Ident(id) = &jsx.opening.name {
            if id.sym == *"CS" {
                process_jsx_cs_children(jsx, scope, style_rules);
            }
        }
    }
}

fn process_jsx_cs_children(
    jsx: &JSXElement,
    scope: &mut ModuleScope,
    style_rules: &mut Vec<String>,
) {
    // Upstream destructures: `const [styles] = node.children;` then
    // checks `t.isJSXExpressionContainer(styles)`. So only the FIRST
    // child matters; ignore everything after it.
    let Some(first) = jsx.children.first() else {
        return;
    };
    let JSXElementChild::JSXExprContainer(container) = first else {
        return;
    };
    let JSXExpr::Expr(expr) = &container.expr else {
        return;
    };
    let Expr::Array(arr) = expr.as_ref() else {
        return;
    };
    process_array_elements(&arr.elems, scope, style_rules);
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        ArrayLit, BindingIdent, CallExpr, Decl, ExprOrSpread, Ident, IdentName, KeyValueProp, Lit,
        MemberExpr, MemberProp, Module, ModuleItem, ObjectLit, Pat, Prop, PropName, PropOrSpread,
        Stmt, Str, VarDecl, VarDeclKind, VarDeclarator,
    };

    fn const_str(name: &str, value: &str) -> ModuleItem {
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()),
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: value.into(),
                    raw: None,
                })))),
                definite: false,
            }],
        }))))
    }

    fn ident_arg(name: &str) -> Option<ExprOrSpread> {
        Some(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident::new(
                name.into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
        })
    }

    fn array_of_idents(names: &[&str]) -> ArrayLit {
        ArrayLit {
            span: DUMMY_SP,
            elems: names.iter().map(|n| ident_arg(n)).collect(),
        }
    }

    /// `React.createElement(CS, null, [<idents>])`
    fn create_element_call(idents: &[&str]) -> Expr {
        let react_create_element = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(Ident::new(
                "React".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
            prop: MemberProp::Ident(IdentName::new("createElement".into(), DUMMY_SP)),
        });
        Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(react_create_element)),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(Ident::new(
                        "CS".into(),
                        DUMMY_SP,
                        SyntaxContext::empty(),
                    ))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Null(swc_core::ecma::ast::Null {
                        span: DUMMY_SP,
                    }))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Array(array_of_idents(idents))),
                },
            ],
            type_args: None,
        })
    }

    /// `_jsx(CS, { children: [<idents>] })`
    fn automatic_jsx_call(idents: &[&str]) -> Expr {
        let props = Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Ident(IdentName::new("children".into(), DUMMY_SP)),
                    value: Box::new(Expr::Array(array_of_idents(idents))),
                },
            )))],
        });
        Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "_jsx".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(Ident::new(
                        "CS".into(),
                        DUMMY_SP,
                        SyntaxContext::empty(),
                    ))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(props),
                },
            ],
            type_args: None,
        })
    }

    fn module_with(decls: &[(&str, &str)]) -> Module {
        Module {
            span: DUMMY_SP,
            body: decls
                .iter()
                .map(|(n, v)| const_str(n, v))
                .collect(),
            shebang: None,
        }
    }

    #[test]
    fn extracts_from_create_element() {
        let m = module_with(&[
            ("_1", "._a{color:red}"),
            ("_2", "._b{color:blue}"),
        ]);
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        let node = create_element_call(&["_1", "_2"]);
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        assert_eq!(
            style_rules,
            vec!["._a{color:red}".to_string(), "._b{color:blue}".to_string()]
        );
    }

    #[test]
    fn extracts_from_automatic_runtime() {
        let m = module_with(&[
            ("_1", "._a{color:red}"),
            ("_2", "._b{color:blue}"),
        ]);
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        let node = automatic_jsx_call(&["_1", "_2"]);
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        assert_eq!(
            style_rules,
            vec!["._a{color:red}".to_string(), "._b{color:blue}".to_string()]
        );
    }

    #[test]
    fn marks_bindings_for_removal_after_extraction() {
        let mut m = module_with(&[
            ("_1", "._a{color:red}"),
            ("_2", "._b{color:blue}"),
            ("Component", "kept"),
        ]);
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        let node = create_element_call(&["_1", "_2"]);
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        scope.apply_removals(&mut m);
        let scope2 = ModuleScope::from_module(&m);
        assert!(scope2.get_string_binding("_1").is_none());
        assert!(scope2.get_string_binding("_2").is_none());
        assert_eq!(scope2.get_string_binding("Component"), Some("kept"));
    }

    #[test]
    fn skips_non_identifier_array_elements() {
        // `React.createElement(CS, null, ["literal", _2])` — the
        // string-literal element is skipped (not an Identifier);
        // the identifier element is processed normally.
        let m = module_with(&[("_2", "._b{color:blue}")]);
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        let arr = ArrayLit {
            span: DUMMY_SP,
            elems: vec![
                Some(ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: "literal".into(),
                        raw: None,
                    }))),
                }),
                ident_arg("_2"),
            ],
        };
        let react_create_element = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(Ident::new(
                "React".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
            prop: MemberProp::Ident(IdentName::new("createElement".into(), DUMMY_SP)),
        });
        let node = Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(react_create_element)),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(Ident::new(
                        "CS".into(),
                        DUMMY_SP,
                        SyntaxContext::empty(),
                    ))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Null(swc_core::ecma::ast::Null {
                        span: DUMMY_SP,
                    }))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Array(arr)),
                },
            ],
            type_args: None,
        });
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        assert_eq!(style_rules, vec!["._b{color:blue}".to_string()]);
    }

    #[test]
    fn ignores_non_string_bindings() {
        // `_1 = 42` — not a string literal; upstream's
        // `t.isStringLiteral(value)` check rejects, leaving the
        // binding untouched.
        let m = Module {
            span: DUMMY_SP,
            shebang: None,
            body: vec![ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                span: DUMMY_SP,
                ctxt: SyntaxContext::empty(),
                kind: VarDeclKind::Const,
                declare: false,
                decls: vec![VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: Ident::new("_1".into(), DUMMY_SP, SyntaxContext::empty()),
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Lit(Lit::Num(
                        swc_core::ecma::ast::Number {
                            span: DUMMY_SP,
                            value: 42.0,
                            raw: None,
                        },
                    )))),
                    definite: false,
                }],
            }))))],
        };
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        let node = create_element_call(&["_1"]);
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        assert!(style_rules.is_empty());
    }

    #[test]
    fn unmatched_node_is_noop() {
        let m = module_with(&[("_1", "x")]);
        let mut scope = ModuleScope::from_module(&m);
        let mut style_rules = Vec::new();
        // `foo()` — a call expression whose callee is neither
        // React.createElement nor _jsx.
        let node = Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "foo".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![],
            type_args: None,
        });
        let _ = &m; // module no longer needed at call-site; scope cached.
        remove_style_declarations(&node, &mut scope, &mut style_rules);
        assert!(style_rules.is_empty());
    }
}
