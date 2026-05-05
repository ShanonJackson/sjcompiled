//! Phase 6 §6.1 — `keyframes` cleanup-only handler.
//!
//! Upstream source: `packages/babel-plugin/src/babel-plugin.ts`
//! lines 331–340 (the `isCompiledUtil` branch — keyframes half) and
//! lines 222–238 (the `Program::exit` `pathsToCleanup.forEach` drain).
//!
//! The upstream `keyframes/` directory ships only fixtures and tests
//! (`packages/babel-plugin/src/keyframes/`) — there is NO `index.ts`
//! source file. The runtime work for keyframes is split between
//! `babel-plugin.ts` (cleanup queueing, replace-with-null at exit) and
//! `utils/css-builders.ts::extractKeyframes` (the inner extraction
//! that fires when a keyframes binding is referenced from a styled /
//! css call). The Rust port already shipped `extract_keyframes` in
//! Phase 4 (`crates/babel-plugin/src/utils/css_builders.rs::extract_keyframes`).
//! This module owns the OTHER half: detect a free-standing
//! `keyframes(...)` / `` keyframes`...` `` site at the top-level
//! visitor, queue it for cleanup, and replace it with `null` at
//! `Program::exit`.
//!
//! Cleanup-only means: the keyframes call's effect on the surrounding
//! styled / css call has ALREADY happened by the time the cleanup
//! pass runs (the inner walk via `extract_keyframes` reaches the
//! binding's init independently). All this pass does is replace the
//! standalone reference with `null` so the runtime no longer ships
//! the call. Mirrors `t.nullLiteral()` in upstream's
//! `clean.path.replaceWith(t.nullLiteral())`.
//!
//! Why a deferred queue instead of inline replace at the visit site:
//! 1:1 port of upstream's `pathsToCleanup` architecture (PLAN.md
//! §3.4 and `babel-plugin.ts:222-238`). Phase 6 §6.2 (`css` cleanup-
//! only handler) reuses the SAME queue + drain, so building the
//! infrastructure once at §6.1 is the right shape. Phase 6 §6.3
//! (cssMap) ALSO uses `pathsToCleanup` for its post-visit replace
//! step (see `cssMap/index.ts` upstream).
//!
//! Drift watch points:
//! - The `id` field of `CleanupAction` is the matched node's
//!   `span.lo.0` (`BytePos` as `u32`). Replace-pass identification
//!   walks the module looking for `Expr::Call` / `Expr::TaggedTpl`
//!   whose `span.lo` matches a queued id. Synthetic nodes generated
//!   mid-pass with `DUMMY_SP` (`BytePos(0)`) would collide; today's
//!   keyframes path never synthesises so this is sound. If §6.3
//!   (cssMap) ships expansion that emits synthetic CallExprs, the
//!   id encoding needs to change (e.g. monotonically-allocated
//!   handle owned by `MutationRecorder`).
//! - This module assumes the `Replace` action means "replace with
//!   `null`". Upstream's `Remove` action is reserved for §2.3(b)
//!   ImportSpecifier removal. Mixing the two queues today is fine
//!   because §2.3(b) hasn't wired `Remove` yet — when it does, the
//!   replace-pass MUST filter for `CleanupKind::Replace` only
//!   (already done in `paths_to_cleanup_replace_ids` below).
//! - The replace-pass walks `Module` post-order via
//!   `VisitMut::visit_mut_expr`; nested matches (a keyframes call
//!   inside another keyframes call's args — pathological but
//!   reachable) are replaced inner-first then outer-second, both
//!   ending up as `null`. Matches Babel's stale-path no-op behaviour.

use std::collections::HashSet;

use swc_core::ecma::ast::{Expr, Lit, Module, Null};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::state::{CleanupAction, CleanupKind, State};
use crate::utils::is_compiled::{
    is_compiled_keyframes_call_expression, is_compiled_keyframes_tagged_template_expression,
};

/// Inspect `expr` (post-children-walk) and queue a cleanup-replace
/// entry if it is a free-standing Compiled-`keyframes` call or
/// tagged-template expression.
///
/// Returns `true` iff the node was matched and queued. The caller
/// uses this to short-circuit subsequent dispatch on the same Expr
/// — mirrors upstream's `return` after
/// `state.pathsToCleanup.push(...)` (line 339).
///
/// Mirrors `babel-plugin.ts:331-340` (keyframes half of
/// `isCompiledUtil`).
pub fn try_queue_cleanup(expr: &Expr, state: &mut State) -> bool {
    let span = if is_compiled_keyframes_call_expression(expr, state) {
        match expr {
            Expr::Call(c) => c.span,
            _ => return false,
        }
    } else if is_compiled_keyframes_tagged_template_expression(expr, state) {
        match expr {
            Expr::TaggedTpl(t) => t.span,
            _ => return false,
        }
    } else {
        return false;
    };
    state.queue_cleanup(CleanupAction {
        action: CleanupKind::Replace,
        id: span.lo.0,
    });
    true
}

/// Drain the `Replace` ids out of `state.paths_to_cleanup` into a
/// HashSet for fast contains-checks during the replace pass.
/// `Remove` actions are passed through (today they're not produced by
/// any visitor — §2.3(b) work).
pub fn paths_to_cleanup_replace_ids(state: &State) -> HashSet<u32> {
    state
        .paths_to_cleanup()
        .iter()
        .filter(|a| a.action == CleanupKind::Replace)
        .map(|a| a.id)
        .collect()
}

/// Walk `module` and replace every `Expr::Call` / `Expr::TaggedTpl`
/// whose `span.lo.0` is in `ids` with `Expr::Lit(Lit::Null(_))`,
/// preserving the original span (so codegen + comment attachment
/// stay anchored at the same source position).
///
/// Mirrors upstream `babel-plugin.ts:222-238`'s `pathsToCleanup.forEach`
/// `case 'replace': clean.path.replaceWith(t.nullLiteral())`.
pub fn run_cleanup_replace(module: &mut Module, ids: &HashSet<u32>) {
    if ids.is_empty() {
        return;
    }
    let mut pass = ReplacePass { ids };
    module.visit_mut_with(&mut pass);
}

struct ReplacePass<'a> {
    ids: &'a HashSet<u32>,
}

impl<'a> VisitMut for ReplacePass<'a> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        // Walk children first — handles nested matches (a keyframes
        // call literally inside another keyframes call's args).
        n.visit_mut_children_with(self);

        let span = match n {
            Expr::Call(c) => c.span,
            Expr::TaggedTpl(t) => t.span,
            _ => return,
        };
        if self.ids.contains(&span.lo.0) {
            *n = Expr::Lit(Lit::Null(Null { span }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{BytePos, Span, SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        CallExpr, Callee, ExprStmt, Ident, ModuleItem, Stmt, TaggedTpl, Tpl,
    };

    use crate::mutation_recorder::ApiKind;
    use crate::state::State;

    fn span(lo: u32, hi: u32) -> Span {
        Span::new(BytePos(lo), BytePos(hi))
    }

    fn ident_expr(name: &str) -> Expr {
        Expr::Ident(Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()))
    }

    fn keyframes_call(callee_name: &str, lo: u32) -> Expr {
        Expr::Call(CallExpr {
            span: span(lo, lo + 10),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(ident_expr(callee_name))),
            args: vec![],
            type_args: None,
        })
    }

    fn keyframes_tagged_tpl(tag_name: &str, lo: u32) -> Expr {
        Expr::TaggedTpl(TaggedTpl {
            span: span(lo, lo + 10),
            ctxt: SyntaxContext::empty(),
            tag: Box::new(ident_expr(tag_name)),
            type_params: None,
            tpl: Box::new(Tpl {
                span: DUMMY_SP,
                exprs: vec![],
                quasis: vec![],
            }),
        })
    }

    /// Build a State with `keyframes` registered as a Compiled API
    /// under the given local-binding name. Routes through the
    /// MutationRecorder so the test fixture exercises the same write
    /// channel the production dispatcher uses.
    fn state_with_keyframes_local(local: &str) -> State {
        let mut s = State::default();
        s.ensure_compiled_imports();
        let mut rec = crate::mutation_recorder::MutationRecorder::new();
        rec.apply(
            crate::mutation_recorder::StateDiff::CompiledImportsAppend {
                api: ApiKind::Keyframes,
                local_name: local.to_string(),
            },
            &mut s,
        );
        s
    }

    #[test]
    fn try_queue_skips_non_keyframes_call() {
        let mut state = state_with_keyframes_local("keyframes");
        let expr = keyframes_call("css", 100);
        assert!(!try_queue_cleanup(&expr, &mut state));
        assert!(state.paths_to_cleanup().is_empty());
    }

    #[test]
    fn try_queue_matches_keyframes_call_and_records_span() {
        let mut state = state_with_keyframes_local("keyframes");
        let expr = keyframes_call("keyframes", 200);
        assert!(try_queue_cleanup(&expr, &mut state));
        let actions = state.paths_to_cleanup();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, CleanupKind::Replace);
        assert_eq!(actions[0].id, 200);
    }

    #[test]
    fn try_queue_matches_renamed_keyframes_call() {
        // `import { keyframes as kf } from '@compiled/react'` →
        // local name "kf". A `kf(...)` call must match.
        let mut state = state_with_keyframes_local("kf");
        let expr = keyframes_call("kf", 300);
        assert!(try_queue_cleanup(&expr, &mut state));
        assert_eq!(state.paths_to_cleanup()[0].id, 300);
    }

    #[test]
    fn try_queue_matches_keyframes_tagged_tpl() {
        let mut state = state_with_keyframes_local("keyframes");
        let expr = keyframes_tagged_tpl("keyframes", 400);
        assert!(try_queue_cleanup(&expr, &mut state));
        let actions = state.paths_to_cleanup();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, 400);
    }

    #[test]
    fn try_queue_skips_unrelated_tagged_tpl() {
        let mut state = state_with_keyframes_local("keyframes");
        let expr = keyframes_tagged_tpl("css", 500);
        assert!(!try_queue_cleanup(&expr, &mut state));
        assert!(state.paths_to_cleanup().is_empty());
    }

    #[test]
    fn try_queue_skips_when_compiled_imports_empty() {
        // No `import { keyframes } ...` was recorded — even a call
        // named `keyframes` must NOT match (parity with upstream's
        // `getCompiledNames` returning empty).
        let mut state = State::default();
        let expr = keyframes_call("keyframes", 600);
        assert!(!try_queue_cleanup(&expr, &mut state));
    }

    fn module_with_stmts(stmts: Vec<Stmt>) -> Module {
        Module {
            span: DUMMY_SP,
            body: stmts.into_iter().map(ModuleItem::Stmt).collect(),
            shebang: None,
        }
    }

    #[test]
    fn replace_pass_swaps_matched_call_with_null_lit() {
        // Module: `keyframes();` (single ExprStmt).
        let call = keyframes_call("keyframes", 700);
        let mut module = module_with_stmts(vec![Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(call),
        })]);

        let mut ids = HashSet::new();
        ids.insert(700u32);
        run_cleanup_replace(&mut module, &ids);

        let stmt = match &module.body[0] {
            ModuleItem::Stmt(s) => s,
            _ => panic!("expected Stmt"),
        };
        let Stmt::Expr(es) = stmt else {
            panic!("expected ExprStmt");
        };
        match &*es.expr {
            Expr::Lit(Lit::Null(n)) => {
                // Span preserved from the original call.
                assert_eq!(n.span.lo.0, 700);
            }
            other => panic!("expected null literal, got {:?}", other),
        }
    }

    #[test]
    fn replace_pass_preserves_unmatched_nodes() {
        // Module: `css();` (NOT a keyframes call). With a queue of one
        // unrelated id, the replace pass must leave the css call alone.
        let call = keyframes_call("css", 800);
        let mut module = module_with_stmts(vec![Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(call),
        })]);

        let mut ids = HashSet::new();
        ids.insert(999u32); // arbitrary unrelated id
        run_cleanup_replace(&mut module, &ids);

        let stmt = match &module.body[0] {
            ModuleItem::Stmt(s) => s,
            _ => panic!("expected Stmt"),
        };
        let Stmt::Expr(es) = stmt else {
            panic!("expected ExprStmt");
        };
        // Still a call expression — unchanged.
        assert!(matches!(&*es.expr, Expr::Call(_)));
    }

    #[test]
    fn replace_pass_handles_empty_id_set_no_op() {
        let call = keyframes_call("keyframes", 900);
        let mut module = module_with_stmts(vec![Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(call),
        })]);

        let ids = HashSet::new();
        run_cleanup_replace(&mut module, &ids);

        // Untouched.
        let stmt = match &module.body[0] {
            ModuleItem::Stmt(s) => s,
            _ => panic!("expected Stmt"),
        };
        let Stmt::Expr(es) = stmt else {
            panic!("expected ExprStmt");
        };
        assert!(matches!(&*es.expr, Expr::Call(_)));
    }

    #[test]
    fn paths_to_cleanup_replace_ids_filters_replace_only() {
        let mut state = State::default();
        state.queue_cleanup(CleanupAction {
            action: CleanupKind::Replace,
            id: 1,
        });
        state.queue_cleanup(CleanupAction {
            action: CleanupKind::Remove,
            id: 2,
        });
        state.queue_cleanup(CleanupAction {
            action: CleanupKind::Replace,
            id: 3,
        });

        let ids = paths_to_cleanup_replace_ids(&state);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&2));
    }

}
