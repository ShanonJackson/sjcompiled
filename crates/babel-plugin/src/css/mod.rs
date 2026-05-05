//! Phase 6 §6.2 — `css` (utility) cleanup-only handler.
//!
//! Upstream source: `packages/babel-plugin/src/babel-plugin.ts`
//! lines 331–340 (the `isCompiledUtil` branch — css half) and
//! lines 222–238 (the `Program::exit` `pathsToCleanup` drain — shared
//! with §6.1 keyframes).
//!
//! The upstream `css/` directory under `packages/babel-plugin/src/`
//! ships only fixtures and tests (no `index.ts` source file). The
//! runtime work for the `css(...)` utility splits between
//! `babel-plugin.ts` (cleanup queueing, replace-with-null at exit)
//! and `utils/css-builders.ts::buildCss` (the inner extraction that
//! fires when a css binding is referenced from a styled / css call).
//! The Rust port already shipped `build_css` in Phase 4
//! (`crates/babel-plugin/src/utils/css_builders.rs`). This module
//! owns the OTHER half: detect a free-standing `css(...)` /
//! `` css`...` `` site at the top-level visitor, queue it for
//! cleanup, and let the shared drain (`keyframes::run_cleanup_replace`)
//! replace it with `null` at `Program::exit`.
//!
//! Cleanup-only means: the css call's effect on the surrounding
//! styled / css call has ALREADY happened by the time the cleanup
//! pass runs. All this pass does is replace the standalone reference
//! with `null` so the runtime no longer ships the call. Mirrors
//! `t.nullLiteral()` in upstream's
//! `clean.path.replaceWith(t.nullLiteral())`.
//!
//! Architecture parity with §6.1: the deferred queue (vs. inline
//! replace at the visit site) is the SAME `state.paths_to_cleanup`
//! channel keyframes uses. The drain pass
//! (`keyframes::paths_to_cleanup_replace_ids` +
//! `keyframes::run_cleanup_replace`) is generic over the queued ids
//! and is reused verbatim — §6.2 only contributes a new matcher into
//! the `visit_mut_expr` dispatch chain. The drain module's name
//! (`keyframes`) is a historical artifact of §6.1 owning the
//! infrastructure first; functionally the drain is shared.
//!
//! Drift watch points (mirroring §6.1):
//! - The `id` field of `CleanupAction` is the matched node's
//!   `span.lo.0` (`BytePos` as `u32`). Today no §6.2 path emits
//!   synthetic `DUMMY_SP` css calls so the encoding is sound. If a
//!   future path emits synthetic CallExprs the id encoding migrates
//!   to a monotonically-allocated handle owned by `MutationRecorder`.
//! - `Replace` and `Remove` actions share `paths_to_cleanup`; the
//!   drain pass filters for `Replace` only — both §6.1 and §6.2
//!   queue `Replace` exclusively.
//! - Dispatch order in `visit_mut_expr` MUST match upstream's
//!   `isCompiledUtil` short-circuit: keyframes and css both queue
//!   into the SAME bucket and either match returns early. There is
//!   no observable ordering difference between matching keyframes-
//!   first vs css-first because the matchers are mutually exclusive
//!   on a given node (a `css(...)` call cannot also be a
//!   `keyframes(...)` call).

use crate::state::{CleanupAction, CleanupKind, State};
use crate::utils::is_compiled::{
    is_compiled_css_call_expression, is_compiled_css_tagged_template_expression,
};
use swc_core::ecma::ast::Expr;

/// Inspect `expr` (post-children-walk) and queue a cleanup-replace
/// entry if it is a free-standing Compiled-`css` call or
/// tagged-template expression.
///
/// Returns `true` iff the node was matched and queued. The caller
/// uses this to short-circuit subsequent dispatch on the same Expr
/// — mirrors upstream's `return` after
/// `state.pathsToCleanup.push(...)` (line 339).
///
/// Mirrors `babel-plugin.ts:331-340` (css half of `isCompiledUtil`).
pub fn try_queue_cleanup(expr: &Expr, state: &mut State) -> bool {
    let span = if is_compiled_css_call_expression(expr, state) {
        match expr {
            Expr::Call(c) => c.span,
            _ => return false,
        }
    } else if is_compiled_css_tagged_template_expression(expr, state) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{BytePos, Span, SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{CallExpr, Callee, Ident, TaggedTpl, Tpl};

    use crate::mutation_recorder::ApiKind;
    use crate::state::State;

    fn span(lo: u32, hi: u32) -> Span {
        Span::new(BytePos(lo), BytePos(hi))
    }

    fn ident_expr(name: &str) -> Expr {
        Expr::Ident(Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()))
    }

    fn css_call(callee_name: &str, lo: u32) -> Expr {
        Expr::Call(CallExpr {
            span: span(lo, lo + 10),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(ident_expr(callee_name))),
            args: vec![],
            type_args: None,
        })
    }

    fn css_tagged_tpl(tag_name: &str, lo: u32) -> Expr {
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

    /// Build a State with `css` registered as a Compiled API
    /// under the given local-binding name.
    fn state_with_css_local(local: &str) -> State {
        let mut s = State::default();
        s.ensure_compiled_imports();
        let mut rec = crate::mutation_recorder::MutationRecorder::new();
        rec.apply(
            crate::mutation_recorder::StateDiff::CompiledImportsAppend {
                api: ApiKind::Css,
                local_name: local.to_string(),
            },
            &mut s,
        );
        s
    }

    #[test]
    fn try_queue_skips_non_css_call() {
        let mut state = state_with_css_local("css");
        let expr = css_call("keyframes", 100);
        assert!(!try_queue_cleanup(&expr, &mut state));
        assert!(state.paths_to_cleanup().is_empty());
    }

    #[test]
    fn try_queue_matches_css_call_and_records_span() {
        let mut state = state_with_css_local("css");
        let expr = css_call("css", 200);
        assert!(try_queue_cleanup(&expr, &mut state));
        let actions = state.paths_to_cleanup();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, CleanupKind::Replace);
        assert_eq!(actions[0].id, 200);
    }

    #[test]
    fn try_queue_matches_renamed_css_call() {
        // `import { css as c } from '@compiled/react'` → local
        // name "c". A `c(...)` call must match.
        let mut state = state_with_css_local("c");
        let expr = css_call("c", 300);
        assert!(try_queue_cleanup(&expr, &mut state));
        assert_eq!(state.paths_to_cleanup()[0].id, 300);
    }

    #[test]
    fn try_queue_matches_css_tagged_tpl() {
        let mut state = state_with_css_local("css");
        let expr = css_tagged_tpl("css", 400);
        assert!(try_queue_cleanup(&expr, &mut state));
        let actions = state.paths_to_cleanup();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, 400);
    }

    #[test]
    fn try_queue_skips_unrelated_tagged_tpl() {
        let mut state = state_with_css_local("css");
        let expr = css_tagged_tpl("keyframes", 500);
        assert!(!try_queue_cleanup(&expr, &mut state));
        assert!(state.paths_to_cleanup().is_empty());
    }

    #[test]
    fn try_queue_skips_when_compiled_imports_empty() {
        // No `import { css } ...` was recorded — even a call
        // named `css` must NOT match (parity with upstream's
        // `getCompiledNames` returning empty).
        let mut state = State::default();
        let expr = css_call("css", 600);
        assert!(!try_queue_cleanup(&expr, &mut state));
    }
}
