//! crates/babel-plugin-strip-runtime
//! Byte-for-byte port of `packages/babel-plugin-strip-runtime/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 1 §1.4 status: dispatcher visitor implemented (CC/CS removal,
//! ImportSpecifier filter, `styleSheetPath` require injection, scope
//! cleanup). The two filesystem-side outputs — `compiledRequireExclude`
//! sidecar JSON and `extractStylesToDirectory` `.compiled.css` writes
//! — are §1.5 work.

pub mod compat;
pub mod utils;

use serde::Deserialize;
use swc_core::common::comments::Comments;
use swc_core::common::{BytePos, Spanned, SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::Expr;
use swc_core::ecma::ast::{
    CallExpr, Callee, ExprOrSpread, ExprStmt, Ident, ImportSpecifier, JSXElement, JSXElementChild,
    JSXElementName, JSXExpr, Lit, ModuleDecl, ModuleExportName, ModuleItem, Program, Prop,
    PropName, PropOrSpread, Stmt, Str,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::{PluginCommentsProxy, TransformPluginProgramMetadata};

use crate::compat::scope::ModuleScope;
use crate::utils::is_automatic_runtime::{is_automatic_runtime, JsxFunc};
use crate::utils::is_cc_component::is_cc_component;
use crate::utils::is_create_element::is_create_element;
use crate::utils::remove_style_declarations::remove_style_declarations;
use crate::utils::to_uri_component::to_uri_component;

/// Plugin options. Shape matches `PluginOptions` from
/// `packages/babel-plugin-strip-runtime/src/types.ts`. Field names use
/// camelCase on the wire (Babel/JS convention).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOptions {
    #[serde(default)]
    pub style_sheet_path: Option<String>,
    #[serde(default)]
    pub compiled_require_exclude: bool,
    #[serde(default)]
    pub extract_styles_to_directory: Option<ExtractToDirOpts>,
    #[serde(default)]
    pub sort_at_rules: Option<bool>,
    #[serde(default)]
    pub sort_shorthand: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractToDirOpts {
    pub source: String,
    pub dest: String,
}

/// Visitor state. Mirrors Babel's `PluginPass`:
/// `style_rules` is the per-file accumulator the upstream `pre()`
/// hook initialises to `[]`.
struct StripRuntimeVisitor {
    scope: ModuleScope,
    style_rules: Vec<String>,
    /// SWC stores comments in a side-channel keyed by BytePos. Babel
    /// stores them on the AST node itself; the upstream plugin clears
    /// `path.node.leadingComments = null` before replacing CC-wrapped
    /// nodes so the inner-node's `/*#__PURE__*/` doesn't get stacked
    /// with the outer's. We do the analogue here: `take_leading` on
    /// the outer span before swapping in the inner expression.
    comments: PluginCommentsProxy,
}

impl StripRuntimeVisitor {
    fn drop_leading_comments_at(&self, pos: BytePos) {
        let _ = self.comments.take_leading(pos);
    }
}

impl StripRuntimeVisitor {
    /// `<CC><CS>{[...]}</CS><userland /></CC>` → `<userland />`.
    /// Children layout (Babel destructure `[, compiledStyles, , nodeToReplace]`):
    /// 0 = whitespace JSXText
    /// 1 = `<CS>{[...]}</CS>`
    /// 2 = whitespace JSXText
    /// 3 = userland JSX or expression container
    fn try_replace_cc_jsx(&mut self, jsx: &JSXElement) -> Option<Expr> {
        let JSXElementName::Ident(id) = &jsx.opening.name else {
            return None;
        };
        if id.sym != *"CC" {
            return None;
        }

        if let Some(JSXElementChild::JSXElement(cs_jsx)) = jsx.children.get(1) {
            let cs_expr = Expr::JSXElement(cs_jsx.clone());
            remove_style_declarations(&cs_expr, &mut self.scope, &mut self.style_rules);
        }

        let third = jsx.children.get(3)?;
        match third {
            JSXElementChild::JSXExprContainer(c) => match &c.expr {
                JSXExpr::Expr(e) => Some((**e).clone()),
                _ => None,
            },
            JSXElementChild::JSXElement(e) => Some(Expr::JSXElement(e.clone())),
            JSXElementChild::JSXFragment(f) => Some(Expr::JSXFragment(f.clone())),
            _ => None,
        }
    }

    /// `React.createElement(CC, ..., compiledStyles, nodeToReplace)` → `nodeToReplace`,
    /// or `_jsxs(CC, { children: [compiledStyles, nodeToReplace] })` → `nodeToReplace`.
    fn try_replace_cc_call(&mut self, call: &CallExpr) -> Option<Expr> {
        // ── classic: React.createElement(CC, ..., compiledStyles, nodeToReplace) ──
        if let Callee::Expr(callee) = &call.callee {
            if is_create_element(callee.as_ref()) {
                let component = call.args.first()?.expr.as_ref();
                if !is_cc_component(component) {
                    return None;
                }
                if let Some(s) = call.args.get(2) {
                    if s.spread.is_none() {
                        remove_style_declarations(
                            s.expr.as_ref(),
                            &mut self.scope,
                            &mut self.style_rules,
                        );
                    }
                }
                let node_to_replace = call.args.get(3)?;
                if node_to_replace.spread.is_some() {
                    return None;
                }
                return Some((*node_to_replace.expr).clone());
            }
        }

        // ── automatic: _jsxs(CC, { children: [compiledStyles, nodeToReplace] }) ──
        let outer = Expr::Call(call.clone());
        if is_automatic_runtime(&outer, JsxFunc::Jsxs) {
            let component = call.args.first()?.expr.as_ref();
            if !is_cc_component(component) {
                return None;
            }
            let props = call.args.get(1)?.expr.as_ref();
            let Expr::Object(obj) = props else {
                return None;
            };
            let children_value: Option<&Expr> = obj.props.iter().find_map(|p| {
                if let PropOrSpread::Prop(prop) = p {
                    if let Prop::KeyValue(kv) = prop.as_ref() {
                        let key_name = match &kv.key {
                            PropName::Ident(id) => Some(id.sym.as_ref()),
                            _ => None,
                        };
                        if key_name == Some("children") {
                            return Some(kv.value.as_ref());
                        }
                    }
                }
                None
            });
            let Some(Expr::Array(arr)) = children_value else {
                return None;
            };
            let compiled_styles = arr.elems.first().and_then(|e| e.as_ref())?;
            let node_to_replace = arr.elems.get(1).and_then(|e| e.as_ref())?;
            if compiled_styles.spread.is_some() || node_to_replace.spread.is_some() {
                return None;
            }
            remove_style_declarations(
                compiled_styles.expr.as_ref(),
                &mut self.scope,
                &mut self.style_rules,
            );
            return Some((*node_to_replace.expr).clone());
        }

        None
    }
}

impl VisitMut for StripRuntimeVisitor {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        // Visit children first so nested CC/CS sites are stripped
        // before the outer one needs to inspect them.
        expr.visit_mut_children_with(self);

        // Capture the outer span's start BEFORE we mutate. If we
        // recognise this as a CC-wrapped node we'll drop its leading
        // comments (mirrors Babel's `path.node.leadingComments = null`).
        let outer_pos = match expr {
            Expr::JSXElement(jsx) => Some(jsx.span.lo),
            Expr::Call(call) => Some(call.span.lo),
            _ => None,
        };

        let replacement = match expr {
            Expr::JSXElement(jsx) => self.try_replace_cc_jsx(jsx),
            Expr::Call(call) => self.try_replace_cc_call(call),
            _ => None,
        };
        if let Some(new) = replacement {
            if let Some(pos) = outer_pos {
                self.drop_leading_comments_at(pos);
            }
            *expr = new;
        }
    }

    fn visit_mut_module_decl(&mut self, decl: &mut ModuleDecl) {
        decl.visit_mut_children_with(self);

        // Drop `CC` / `CS` named-import specifiers. The parent
        // ImportDeclaration is preserved even if its specifier list
        // becomes empty — upstream's `path.remove()` on the specifier
        // does not propagate up.
        if let ModuleDecl::Import(import) = decl {
            import.specifiers.retain(|s| match s {
                ImportSpecifier::Named(n) => {
                    let imported_name = match &n.imported {
                        Some(ModuleExportName::Ident(id)) => id.sym.as_ref().to_string(),
                        Some(ModuleExportName::Str(s)) => {
                            s.value.to_atom_lossy().as_str().to_string()
                        }
                        None => n.local.sym.as_ref().to_string(),
                    };
                    !matches!(imported_name.as_str(), "CC" | "CS")
                }
                _ => true,
            });
        }
    }
}

/// Index of the first body item that ISN'T a directive prologue (a
/// bare string-literal `ExprStmt` at the start of the module). Babel's
/// `unshiftContainer` skips directives natively; SWC has no separate
/// Directive list so we mirror the behaviour here.
fn first_non_directive_index(body: &[ModuleItem]) -> usize {
    let mut i = 0;
    for item in body {
        let ModuleItem::Stmt(Stmt::Expr(es)) = item else {
            break;
        };
        let Expr::Lit(Lit::Str(_)) = es.expr.as_ref() else {
            break;
        };
        i += 1;
    }
    i
}

/// Build `require("<url>");` as a `ModuleItem`. The outer span uses
/// `attach_comments_at` (or `DUMMY_SP` if `None`) so we can route the
/// file-level leading comment to the first injected require —
/// otherwise the comment would stay anchored to the original first
/// body item and end up BELOW the new requires.
fn make_require_stmt(url: &str, attach_comments_at: Option<swc_core::common::Span>) -> ModuleItem {
    let span = attach_comments_at.unwrap_or(DUMMY_SP);
    ModuleItem::Stmt(Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(Expr::Call(CallExpr {
            span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "require".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: url.into(),
                    raw: None,
                }))),
            }],
            type_args: None,
        })),
    }))
}

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let opts: PluginOptions = meta
        .get_transform_plugin_config()
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let Program::Module(mut module) = program else {
        // Strip-runtime is module-only; scripts pass through.
        return program;
    };

    let scope = ModuleScope::from_module(&module);
    let mut visitor = StripRuntimeVisitor {
        scope,
        style_rules: Vec::new(),
        comments: meta.comments.clone().unwrap_or(PluginCommentsProxy),
    };

    module.visit_mut_with(&mut visitor);

    let StripRuntimeVisitor {
        scope,
        style_rules,
        comments: _,
    } = visitor;

    // ── Program::exit ordering ──
    //
    // Upstream:
    //   1. compiledRequireExclude → write to file.metadata, return early.
    //   2. styleSheetPath → preserveLeadingComments + unshift `require(...)` per rule.
    //   3. extractStylesToDirectory → write file + unshift `import './x.compiled.css'`.
    //
    // The contract is "never two." Per `plugins/STATUS.md` §1.4 lock.

    // Apply scope removals BEFORE any body-prepend mutation. The
    // scope's BindingLocations are indexed against `module.body`
    // pre-mutation; injecting requires up front would shift those
    // indices and `apply_removals` would clip the wrong declarators.
    scope.apply_removals(&mut module);

    if opts.compiled_require_exclude {
        // §1.5 will write style_rules to a sidecar JSON
        // (`<callScratch>/style-rules.json`). For §1.4, no AST
        // mutation is needed beyond what the visitor already did.
        let _ = style_rules;
    } else if let Some(style_sheet_path) = &opts.style_sheet_path {
        // Insert AFTER any leading directives (`'use strict';` etc).
        // Babel's `path.unshiftContainer('body', require)` knows to
        // skip directives; in SWC there's no Module.directives field,
        // so we have to find the boundary manually.
        let insert_at = first_non_directive_index(&module.body);

        // Mirror Babel's `preserveLeadingComments`: route whatever
        // leading comments sit on the body item we're about to
        // displace onto the FIRST injected require by giving it the
        // same span.lo. SWC's codegen takes leading comments at that
        // BytePos when emitting the require, so the file's banner
        // comment ends up ABOVE the require chain (matching Babel)
        // instead of being pinned to whatever statement now sits at
        // position N+1.
        let banner_span = module.body.get(insert_at).map(|item| match item {
            ModuleItem::Stmt(s) => s.span(),
            ModuleItem::ModuleDecl(m) => m.span(),
        });

        // Babel calls `unshiftContainer('body', require)` once per
        // rule, in iteration order. Each unshift goes to the FRONT,
        // so the final order is REVERSED relative to iteration.
        let mut requires: Vec<ModuleItem> = Vec::with_capacity(style_rules.len());
        for rule in &style_rules {
            let params = to_uri_component(rule);
            let url = format!("{}?style={}", style_sheet_path, params);
            requires.push(make_require_stmt(&url, None));
        }
        requires.reverse();
        if let (Some(first), Some(span)) = (requires.first_mut(), banner_span) {
            if let ModuleItem::Stmt(Stmt::Expr(ref mut es)) = first {
                es.span = span;
                if let Expr::Call(ref mut call) = *es.expr {
                    call.span = span;
                }
            }
        }
        // Splice `requires` into module.body at position `insert_at`.
        let tail: Vec<ModuleItem> = module.body.drain(insert_at..).collect();
        module.body.extend(requires);
        module.body.extend(tail);
    } else if opts.extract_styles_to_directory.is_some() && !style_rules.is_empty() {
        // §1.5 owns the actual file write. For §1.4 we leave the AST
        // alone; the harness gate for `extract_styles` fixtures stays
        // expectedToFail until §1.5 lands.
    }

    Program::Module(module)
}
