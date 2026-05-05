//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/index.ts`.
//!
//! Two exports mirror upstream:
//!
//! - `get_member_expression_meta` (private) — pre-order DFS over a
//!   `MemberExpression` to extract the bottom binding identifier
//!   (`foo.bar.baz` → `foo`) and the access path
//!   (`foo.bar.baz` → `[bar, baz]`).
//! - `traverse_member_expression` (public) — top-level dispatcher
//!   that routes through `traverse_member_access_path` once the
//!   access-path / binding-identifier are known.
//!
//! ## SWC mapping notes
//!
//! - Babel `traverse(t.expressionStatement(expression), { MemberExpression(path) ... })`
//!   wraps the input in an `ExpressionStatement` so the visitor has
//!   a stable parent. The Rust analog walks the input directly via
//!   `Visit` since SWC's traverse doesn't require a wrapping
//!   statement — but we replicate the visit shape so the early
//!   `path.listKey === 'arguments'` guard still works (we check
//!   the same shape via a context tag in the visitor).
//! - Babel `path.listKey === 'arguments'` is "are we visiting a
//!   member that was a CallExpr argument?". The SWC analog: we
//!   guard each visit by tracking whether we're under a CallExpr's
//!   args list; the `arg_depth` counter increments while visiting
//!   `CallExpr.args` and decrements after. A nonzero counter
//!   means "skip this MemberExpr".

pub mod traverse_access_path;

pub use traverse_access_path::traverse_member_access_path;

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, Ident, MemberExpr, MemberProp,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::types::Metadata;
use crate::utils::create_result_pair::{create_result_pair, ResultPair};

/// `getMemberExpressionMeta` — returns the binding identifier and
/// reversed access path for a member expression.
fn get_member_expression_meta(expression: &MemberExpr) -> MemberExpressionMeta {
    let mut visitor = MemberMetaVisitor {
        access_path: Vec::new(),
        binding_identifier: None,
        arg_depth: 0,
    };
    expression.visit_with(&mut visitor);
    // Babel's order is bottom-up via DFS; the JS code reverses at the
    // end. The visitor pushes property identifiers in the order the
    // visitor encounters MemberExpressions (outer first) — the JS
    // also walks outer-first because Babel's traverse is pre-order.
    // After reversal the path matches "from binding outward":
    // `foo.bar.baz` → bindingIdentifier = foo, accessPath = [bar, baz].
    visitor.access_path.reverse();
    MemberExpressionMeta {
        access_path: visitor.access_path,
        binding_identifier: visitor.binding_identifier,
    }
}

#[derive(Debug)]
struct MemberExpressionMeta {
    access_path: Vec<Ident>,
    binding_identifier: Option<Ident>,
}

struct MemberMetaVisitor {
    access_path: Vec<Ident>,
    binding_identifier: Option<Ident>,
    arg_depth: u32,
}

impl Visit for MemberMetaVisitor {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        // The callee is NOT in `arguments`, walk normally.
        n.callee.visit_with(self);
        // Args ARE — bump depth so any MemberExpr inside is skipped.
        self.arg_depth += 1;
        for arg in &n.args {
            arg.visit_with(self);
        }
        self.arg_depth -= 1;
    }

    fn visit_member_expr(&mut self, n: &MemberExpr) {
        if self.arg_depth > 0 {
            // path.listKey === 'arguments' → skip.
            return;
        }

        // Binding-identifier extraction:
        // - object is Identifier → that's the binding
        // - object is CallExpression with Identifier callee →
        //   the callee is the binding (originalBindingType =
        //   'CallExpression' in JS, but we just track the Ident)
        match &*n.obj {
            Expr::Ident(id) => {
                self.binding_identifier = Some(id.clone());
            }
            Expr::Call(call) => {
                if let Callee::Expr(boxed) = &call.callee {
                    if let Expr::Ident(id) = &**boxed {
                        self.binding_identifier = Some(id.clone());
                    }
                }
            }
            _ => {}
        }

        // Property collection:
        // - property is Identifier → push property
        // - property is CallExpression with Identifier callee →
        //   push the callee (trailing call expression name)
        match &n.prop {
            MemberProp::Ident(id) => {
                self.access_path
                    .push(Ident::new(id.sym.clone(), DUMMY_SP, Default::default()));
            }
            MemberProp::Computed(c) => {
                if let Expr::Call(call) = &*c.expr {
                    if let Callee::Expr(boxed) = &call.callee {
                        if let Expr::Ident(id) = &**boxed {
                            self.access_path.push(id.clone());
                        }
                    }
                }
            }
            _ => {}
        }

        // Default recursion handles inner Member nodes (e.g.,
        // `foo.bar.baz` is `Member { obj: Member { obj: foo, prop: bar }, prop: baz }`).
        n.obj.visit_with(self);
        n.prop.visit_with(self);
    }
}

/// 1:1 port of `traverseMemberExpression`.
pub fn traverse_member_expression<'a, F>(
    expression: &MemberExpr,
    meta: &mut Metadata<'a>,
    scope_index: &ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    evaluate_expression: &mut F,
) -> ResultPair
where
    F: FnMut(&Expr, &mut Metadata<'a>) -> ResultPair,
{
    let meta_info = get_member_expression_meta(expression);

    if let Some(binding_identifier) = meta_info.binding_identifier {
        let binding_name = binding_identifier.sym.as_str().to_string();
        let identifier_expr = Expr::Ident(Ident::new(
            binding_identifier.sym.clone(),
            DUMMY_SP,
            Default::default(),
        ));
        return traverse_member_access_path(
            &identifier_expr,
            meta,
            &binding_name,
            &meta_info.access_path,
            expression,
            scope_index,
            parent_scope,
            own_scope,
            evaluate_expression,
        );
    }

    // Fall-through: returns the input member expression unchanged.
    create_result_pair(Some(Box::new(Expr::Member(expression.clone()))), meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(name: &str) -> Ident {
        Ident::new(name.into(), DUMMY_SP, Default::default())
    }

    fn member_chain(base: &str, props: &[&str]) -> MemberExpr {
        // Build `base.p1.p2.p3` left-to-right.
        let mut cur: Expr = Expr::Ident(ident(base));
        let mut last: Option<MemberExpr> = None;
        for p in props {
            let m = MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(cur),
                prop: MemberProp::Ident(swc_core::ecma::ast::IdentName::new(
                    (*p).into(),
                    DUMMY_SP,
                )),
            };
            cur = Expr::Member(m.clone());
            last = Some(m);
        }
        last.expect("at least one prop")
    }

    #[test]
    fn extracts_binding_identifier_and_access_path() {
        // `foo.bar.baz` → binding=foo, accessPath=[bar, baz].
        let m = member_chain("foo", &["bar", "baz"]);
        let info = get_member_expression_meta(&m);
        assert_eq!(
            info.binding_identifier
                .as_ref()
                .map(|i| i.sym.as_str().to_string()),
            Some("foo".to_string())
        );
        let path: Vec<String> = info
            .access_path
            .iter()
            .map(|i| i.sym.as_str().to_string())
            .collect();
        assert_eq!(path, vec!["bar".to_string(), "baz".to_string()]);
    }
}
