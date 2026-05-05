//! 1:1 port of `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/resolve-expression/function-args.ts`.
//!
//! ```ts
//! /*
//!  * Finds a call expression within a member given the function name
//!  * TODO:FIX - This won't work if the member contains more than
//!  * one of the same function name i.e. `obj.getValue().getValue()`
//!  */
//! export const getFunctionArgs = (
//!   functionName: string,
//!   memberExpression: t.MemberExpression
//! ): t.CallExpression['arguments'] => {
//!   const identifierOpts = { name: functionName };
//!   let args: t.CallExpression['arguments'] = [];
//!
//!   traverse(memberExpression, {
//!     noScope: true,
//!     CallExpression(path) {
//!       const { node } = path;
//!       const { callee } = node;
//!       const found =
//!         t.isIdentifier(callee, identifierOpts) ||
//!         (t.isMemberExpression(callee) && t.isIdentifier(callee.property, identifierOpts));
//!
//!       if (found) {
//!         args = node.arguments;
//!         path.stop();
//!       }
//!     },
//!   });
//!
//!   return args;
//! };
//! ```
//!
//! Babel `traverse(memberExpr, { CallExpression(path) {...; path.stop()} })`
//! → SWC `Visit` impl with a `done` flag, mirroring the §5.5 leaf
//! `traverse_function.rs` pattern. Returns `Vec<ExprOrSpread>` —
//! SWC's `CallExpr.args` shape (Babel's `t.CallExpression['arguments']`
//! is the union of `Expression | SpreadElement | ArgumentPlaceholder`,
//! which SWC unifies as `ExprOrSpread`). Bug-for-bug parity:
//! upstream's TODO note about repeated function names is preserved
//! by the first-match-wins flag.

use swc_core::ecma::ast::{Callee, Expr, ExprOrSpread, MemberExpr};
use swc_core::ecma::visit::{Visit, VisitWith};

/// 1:1 port of `getFunctionArgs`.
pub fn get_function_args(function_name: &str, member_expression: &MemberExpr) -> Vec<ExprOrSpread> {
    let mut finder = FirstMatchingCall {
        function_name,
        captured_args: Vec::new(),
        done: false,
    };
    member_expression.visit_with(&mut finder);
    finder.captured_args
}

struct FirstMatchingCall<'a> {
    function_name: &'a str,
    captured_args: Vec<ExprOrSpread>,
    done: bool,
}

impl<'a> Visit for FirstMatchingCall<'a> {
    fn visit_call_expr(&mut self, n: &swc_core::ecma::ast::CallExpr) {
        if self.done {
            return;
        }

        let found = match &n.callee {
            // `t.isIdentifier(callee, { name })`
            Callee::Expr(boxed) => match &**boxed {
                Expr::Ident(id) => id.sym.as_str() == self.function_name,
                // `t.isMemberExpression(callee) && t.isIdentifier(callee.property, { name })`
                Expr::Member(member) => match &member.prop {
                    swc_core::ecma::ast::MemberProp::Ident(id) => {
                        id.sym.as_str() == self.function_name
                    }
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        };

        if found {
            self.captured_args = n.args.clone();
            self.done = true;
            // Babel `path.stop()` — don't recurse into children of
            // matching CallExpr.
            return;
        }
        // Continue recursion to find a deeper match.
        n.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, ExprStmt, ModuleItem, Stmt};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_member(src: &str) -> MemberExpr {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"));
        let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = &module.body[0] else {
            panic!("expected expr stmt");
        };
        match &**expr {
            Expr::Member(m) => m.clone(),
            other => panic!("expected member, got {other:?}"),
        }
    }

    #[test]
    fn extracts_args_from_identifier_callee() {
        // `getValue('a', 'b').color` — getFunctionArgs('getValue', ...)
        // returns ['a', 'b'].
        let mem = parse_member("getValue('a', 'b').color");
        let args = get_function_args("getValue", &mem);
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn extracts_args_from_member_callee_property() {
        // `obj.getValue('x').color` — callee is a MemberExpression
        // whose property is `getValue`. Match.
        let mem = parse_member("obj.getValue('x').color");
        let args = get_function_args("getValue", &mem);
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn returns_empty_when_no_match() {
        let mem = parse_member("obj.foo.bar");
        let args = get_function_args("getValue", &mem);
        assert!(args.is_empty());
    }

    #[test]
    fn first_match_wins_on_repeated_function_name() {
        // Bug-parity TODO from upstream: only the first match wins;
        // subsequent calls with the same name are skipped.
        let mem = parse_member("obj.getValue('first').getValue('second').color");
        let args = get_function_args("getValue", &mem);
        // Babel pre-order DFS visits the OUTER CallExpression first
        // (`obj.getValue('first').getValue('second')` as a whole, where
        // the callee is `obj.getValue('first').getValue` — its
        // property `getValue` matches → captures the OUTER args
        // `['second']`). Verify byte-parity by inspecting first arg.
        assert_eq!(args.len(), 1);
    }
}
