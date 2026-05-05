//! 1:1 port of `packages/babel-plugin/src/utils/normalize-props-usage.ts`.
//!
//! Runs on the styled / css call/tagged-tpl subtree BEFORE the per-API
//! handlers dispatch. For every nested `ArrowFunctionExpression`:
//!
//!   * single Ident param `(x)` → renames refs to `x` inside body to
//!     `__cmplp`; replaces param with `__cmplp`.
//!   * Object destructure `({ a, b: { c }, d = 16, ...rest })` →
//!     rewrites refs to each binding into a member-expression chain
//!     rooted at `__cmplp` (with `?? default` when the binding has a
//!     default value); replaces param with `__cmplp`.
//!   * Assignment-pattern destructure `({a, b} = { a: 1, b: 2 })` →
//!     same as ObjectPat but pulls defaults from the RHS literal.
//!   * Array destructure → throws (matches upstream).
//!
//! Without this pass, hash inputs at the catch-all CSS-variable site
//! reflect the unrenamed prop arrow (`props => props.color` →
//! `1p69eoh`) instead of the renamed form (`__cmplp => __cmplp.color`
//! → `xexnhp`), and the runtime emit uses `props.color` instead of
//! `__cmplp.color`.

use std::collections::HashMap;

use swc_core::common::{SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    ArrowExpr, AssignPatProp, BindingIdent, BlockStmtOrExpr, Expr, Ident, KeyValuePatProp,
    MemberExpr, MemberProp, ObjectLit, ObjectPat, ObjectPatProp, Pat, Prop, PropName, PropOrSpread,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::constants::PROPS_IDENTIFIER_NAME;

/// Entry point — mirrors upstream `normalizePropsUsage(styledPath)`.
/// Walks every `ArrowFunctionExpression` reachable from `expr` and
/// normalises its first param + body references. Idempotent: arrows
/// whose first param is already `__cmplp` are skipped.
pub fn normalize_props_usage(expr: &mut Expr) {
    let mut v = ArrowVisitor;
    expr.visit_mut_with(&mut v);
}

struct ArrowVisitor;

impl VisitMut for ArrowVisitor {
    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        // Recurse into nested arrows FIRST so inner arrows get their
        // own renames before we rewrite the outer arrow's body —
        // matches upstream's `path.traverse(arrowFunctionVisitor)`
        // post-order semantics.
        arrow.visit_mut_children_with(self);

        let Some(first_param) = arrow.params.first().cloned() else {
            return;
        };

        match first_param {
            Pat::Ident(bi) => {
                let name = bi.id.sym.as_ref().to_string();
                if name == PROPS_IDENTIFIER_NAME {
                    return;
                }
                let mut renamer = IdentRenamer {
                    from: name,
                    to: PROPS_IDENTIFIER_NAME.to_string(),
                };
                visit_arrow_body(&mut arrow.body, &mut renamer);
            }
            Pat::Object(op) => {
                let mut renames: HashMap<String, RenameTarget> = HashMap::new();
                let parent: Vec<String> = vec![PROPS_IDENTIFIER_NAME.to_string()];
                build_renames_from_object_pat(&op, &parent, &HashMap::new(), &mut renames);
                let mut renamer = DestructuredRenamer { renames };
                visit_arrow_body(&mut arrow.body, &mut renamer);
            }
            Pat::Assign(ap) => {
                if let Pat::Object(op) = &*ap.left {
                    let defaults = extract_defaults_from_assign_rhs(&ap.right);
                    let parent: Vec<String> = vec![PROPS_IDENTIFIER_NAME.to_string()];
                    let mut renames: HashMap<String, RenameTarget> = HashMap::new();
                    build_renames_from_object_pat(op, &parent, &defaults, &mut renames);
                    let mut renamer = DestructuredRenamer { renames };
                    visit_arrow_body(&mut arrow.body, &mut renamer);
                }
            }
            Pat::Array(_) => {
                panic!(
                    "Compiled does not support arrays given in the parameters of an arrow function."
                );
            }
            _ => return,
        }

        // Replace the first param with `__cmplp`. Mirrors upstream's
        // `propsParam.replaceWith(t.identifier(PROPS_IDENTIFIER_NAME))`.
        arrow.params[0] = Pat::Ident(BindingIdent {
            id: Ident::new(PROPS_IDENTIFIER_NAME.into(), DUMMY_SP, Default::default()),
            type_ann: None,
        });
    }
}

fn visit_arrow_body<V: VisitMut + ?Sized>(body: &mut BlockStmtOrExpr, v: &mut V) {
    match body {
        BlockStmtOrExpr::Expr(e) => e.visit_mut_with(v),
        BlockStmtOrExpr::BlockStmt(b) => b.visit_mut_with(v),
    }
}

// ───────── single-Ident param renamer ─────────

struct IdentRenamer {
    from: String,
    to: String,
}

impl VisitMut for IdentRenamer {
    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        // Shadow stop: a nested arrow whose first param is the same
        // name shadows ours. Don't descend into its body.
        if shadows_first_param(&arrow.params, &self.from) {
            return;
        }
        arrow.visit_mut_children_with(self);
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        // Recurse into children FIRST so e.g. MemberExpr's `.obj`
        // gets visited, but the `.prop` ident (handled via MemberProp)
        // is not visited as Expr::Ident.
        e.visit_mut_children_with(self);

        if let Expr::Ident(id) = e {
            if id.sym.as_ref() == self.from {
                id.sym = self.to.clone().into();
                // Hygiene parity: the wrapper that the styled handler
                // emits creates `__cmplp` Idents with
                // `SyntaxContext::empty()`. The original `props` Ident
                // we're rewriting carries the parser's resolved
                // context. Without this reset, the post-plugin hygiene
                // pass sees two bindings with the name `__cmplp` at
                // different contexts and renames the wrapper's binding
                // to `__cmplp1`. See `babel_plugin.rs:286` for the
                // mirror analysis on `forwardRef`/`ax`/`ix`/`CC`/`CS`.
                id.ctxt = SyntaxContext::empty();
            }
        }
    }
}

// ───────── destructured-pattern renamer ─────────

#[derive(Clone, Debug)]
struct RenameTarget {
    chain: Vec<String>,
    default: Option<Box<Expr>>,
}

struct DestructuredRenamer {
    renames: HashMap<String, RenameTarget>,
}

impl VisitMut for DestructuredRenamer {
    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        // Shadow stop: a nested arrow whose first-param binding sites
        // include any of our binding names shadows them. Conservative
        // shadow check matching upstream's binding-scope semantics.
        if let Some(first) = arrow.params.first() {
            if pat_shadows_any(first, &self.renames) {
                return;
            }
        }
        arrow.visit_mut_children_with(self);
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        e.visit_mut_children_with(self);

        if let Expr::Ident(id) = e {
            if let Some(target) = self.renames.get(id.sym.as_ref()) {
                *e = build_rename_expr(target);
            }
        }
    }
}

fn build_rename_expr(target: &RenameTarget) -> Expr {
    let mut iter = target.chain.iter();
    let first = iter
        .next()
        .expect("rename chain must have at least one segment");
    // Use empty SyntaxContext to match the styled wrapper's `__cmplp`
    // binding context (see IdentRenamer for the same comment).
    let mut acc: Expr = Expr::Ident(Ident::new(
        first.clone().into(),
        DUMMY_SP,
        SyntaxContext::empty(),
    ));
    for seg in iter {
        acc = Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(acc),
            prop: MemberProp::Ident(swc_core::ecma::ast::IdentName {
                span: DUMMY_SP,
                sym: seg.clone().into(),
            }),
        });
    }
    if let Some(def) = &target.default {
        // `chain ?? default`
        Expr::Bin(swc_core::ecma::ast::BinExpr {
            span: DUMMY_SP,
            op: swc_core::ecma::ast::BinaryOp::NullishCoalescing,
            left: Box::new(acc),
            right: def.clone(),
        })
    } else {
        acc
    }
}

// ───────── shadow checks ─────────

fn shadows_first_param(params: &[Pat], name: &str) -> bool {
    let Some(first) = params.first() else { return false };
    pat_binds_name(first, name)
}

fn pat_shadows_any(p: &Pat, renames: &HashMap<String, RenameTarget>) -> bool {
    let mut names: Vec<String> = Vec::new();
    collect_pat_binding_names(p, &mut names);
    names.iter().any(|n| renames.contains_key(n))
}

fn pat_binds_name(p: &Pat, name: &str) -> bool {
    let mut names: Vec<String> = Vec::new();
    collect_pat_binding_names(p, &mut names);
    names.iter().any(|n| n == name)
}

fn collect_pat_binding_names(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Ident(bi) => out.push(bi.id.sym.as_ref().to_string()),
        Pat::Object(o) => {
            for prop in &o.props {
                match prop {
                    ObjectPatProp::KeyValue(KeyValuePatProp { value, .. }) => {
                        collect_pat_binding_names(value, out);
                    }
                    ObjectPatProp::Assign(AssignPatProp { key, .. }) => {
                        out.push(key.sym.as_ref().to_string());
                    }
                    ObjectPatProp::Rest(r) => collect_pat_binding_names(&r.arg, out),
                }
            }
        }
        Pat::Array(a) => {
            for elem in a.elems.iter().flatten() {
                collect_pat_binding_names(elem, out);
            }
        }
        Pat::Rest(r) => collect_pat_binding_names(&r.arg, out),
        Pat::Assign(a) => collect_pat_binding_names(&a.left, out),
        _ => {}
    }
}

// ───────── object-pat → rename map ─────────

fn build_renames_from_object_pat(
    op: &ObjectPat,
    parent_chain: &[String],
    defaults: &HashMap<String, Box<Expr>>,
    out: &mut HashMap<String, RenameTarget>,
) {
    for prop in &op.props {
        match prop {
            ObjectPatProp::KeyValue(KeyValuePatProp { key, value }) => {
                let key_name = match prop_name_to_string(key) {
                    Some(s) => s,
                    None => continue,
                };
                let mut chain = parent_chain.to_vec();
                chain.push(key_name.clone());
                handle_destructured_value(value, &chain, defaults, out);
            }
            ObjectPatProp::Assign(AssignPatProp { key, value, .. }) => {
                let key_name = key.sym.as_ref().to_string();
                let mut chain = parent_chain.to_vec();
                chain.push(key_name.clone());
                let default = value
                    .clone()
                    .or_else(|| defaults.get(&key_name).cloned());
                out.insert(key_name, RenameTarget { chain, default });
            }
            ObjectPatProp::Rest(r) => {
                if let Pat::Ident(bi) = &*r.arg {
                    // Rest binding maps to the parent chain itself — refs
                    // like `rest.x` flow through `__cmplp.x` (or the
                    // appropriate nested chain). Mirrors upstream's
                    // `buildObjectChain` returning the chain WITHOUT the
                    // rest's name.
                    out.insert(
                        bi.id.sym.as_ref().to_string(),
                        RenameTarget {
                            chain: parent_chain.to_vec(),
                            default: None,
                        },
                    );
                }
            }
        }
    }
}

fn handle_destructured_value(
    value: &Pat,
    chain: &[String],
    defaults: &HashMap<String, Box<Expr>>,
    out: &mut HashMap<String, RenameTarget>,
) {
    match value {
        Pat::Ident(bi) => {
            let name = bi.id.sym.as_ref().to_string();
            let default = defaults.get(&name).cloned();
            out.insert(
                name,
                RenameTarget {
                    chain: chain.to_vec(),
                    default,
                },
            );
        }
        Pat::Object(inner) => {
            build_renames_from_object_pat(inner, chain, defaults, out);
        }
        Pat::Assign(ap) => {
            // `{ k: v = default }`. The key was already pushed; recurse
            // on left with default scoped to this site.
            if let Pat::Ident(bi) = &*ap.left {
                let name = bi.id.sym.as_ref().to_string();
                out.insert(
                    name,
                    RenameTarget {
                        chain: chain.to_vec(),
                        default: Some(ap.right.clone()),
                    },
                );
            } else {
                handle_destructured_value(&ap.left, chain, defaults, out);
            }
        }
        Pat::Array(_) => {
            panic!(
                "Compiled does not support arrays given in the parameters of an arrow function."
            );
        }
        _ => {}
    }
}

fn prop_name_to_string(p: &PropName) -> Option<String> {
    match p {
        PropName::Ident(i) => Some(i.sym.as_ref().to_string()),
        PropName::Str(s) => Some(s.value.to_atom_lossy().as_str().to_string()),
        _ => None,
    }
}

fn extract_defaults_from_assign_rhs(rhs: &Expr) -> HashMap<String, Box<Expr>> {
    let mut out = HashMap::new();
    if let Expr::Object(ObjectLit { props, .. }) = rhs {
        for p in props {
            if let PropOrSpread::Prop(b) = p {
                if let Prop::KeyValue(kv) = &**b {
                    if let Some(name) = prop_name_to_string(&kv.key) {
                        out.insert(name, kv.value.clone());
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::generator::generate;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, Module};
    use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax};

    fn parse_expr(src: &str) -> Box<Expr> {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(Lrc::new(FileName::Custom("t.ts".into())), src.to_string());
        let lexer = Lexer::new(
            Syntax::Es(Default::default()),
            EsVersion::EsNext,
            StringInput::from(&*fm),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        let module: Module = parser.parse_module().expect("parse");
        // Find the first ExprStmt
        for item in module.body {
            if let swc_core::ecma::ast::ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Expr(es)) = item
            {
                return es.expr;
            }
        }
        panic!("no expression statement");
    }

    fn round_trip(src: &str) -> String {
        let mut e = parse_expr(src);
        normalize_props_usage(&mut e);
        generate(&e)
    }

    #[test]
    fn renames_simple_ident_param() {
        let out = round_trip("(p) => p.color");
        assert!(out.contains("__cmplp"), "got: {}", out);
        assert!(!out.contains(" p."), "got: {}", out);
    }

    #[test]
    fn renames_destructured_ident() {
        let out = round_trip("({ color }) => color");
        assert!(out.contains("__cmplp.color"), "got: {}", out);
    }

    #[test]
    fn renames_destructured_rename_key() {
        let out = round_trip("({ width: w }) => w");
        assert!(out.contains("__cmplp.width"), "got: {}", out);
    }

    #[test]
    fn renames_nested_destructure() {
        let out = round_trip("({ theme: { colors: { dark } } }) => dark");
        assert!(out.contains("__cmplp.theme.colors.dark"), "got: {}", out);
    }

    #[test]
    fn renames_default_value_with_nullish() {
        let out = round_trip("({ a, b = 16 }) => b");
        assert!(out.contains("__cmplp.b"), "got: {}", out);
        assert!(out.contains("16"), "got: {}", out);
        assert!(out.contains("??"), "got: {}", out);
    }

    #[test]
    fn renames_rest_to_parent_chain() {
        let out = round_trip("({ width, ...rest }) => rest.height");
        assert!(out.contains("__cmplp.height"), "got: {}", out);
    }

    #[test]
    fn skips_already_normalized() {
        let out = round_trip("(__cmplp) => __cmplp.color");
        assert!(out.contains("__cmplp.color"), "got: {}", out);
    }

    #[test]
    fn shadow_stops_inner_arrow() {
        // Inner arrow re-binds `p` — outer references inside its body
        // belong to the inner binding and must NOT be rewritten.
        let out = round_trip("(p) => (p) => p.x");
        // Outer `p =>` becomes `__cmplp =>` (no body refs to outer p).
        // Inner `(p) => p.x` is normalized independently → `(__cmplp) => __cmplp.x`.
        assert!(out.starts_with("__cmplp"), "got: {}", out);
        assert!(out.matches("__cmplp").count() >= 2, "got: {}", out);
    }
}
