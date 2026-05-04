//! 1:1 port of `packages/babel-plugin/src/utils/create-result-pair.ts`.
//!
//! Tiny shape-builder used by the traverse-expression leaves and (when
//! it lands in §5.6) `evaluate-expression.rs`. Mirrors upstream:
//!
//! ```ts
//! export const createResultPair = (
//!   value: t.Expression,
//!   meta: Metadata
//! ): {
//!   value: t.Expression;
//!   meta: Metadata;
//! } => ({
//!   value,
//!   meta,
//! });
//! ```
//!
//! ## Rust shape divergence: `value: Option<Box<Expr>>`
//!
//! JS types `value: t.Expression`, but several callers (the most
//! visible are `utils/traverse-expression/traverse-function.ts:23` and
//! `utils/traverse-expression/traverse-call-expression.ts:24`) declare
//! `let value: t.Node | undefined | null = undefined;` and assign to
//! `value` only on a successful path. The trailing
//! `createResultPair(value as t.Expression, ...)` cast is a TS lie —
//! at runtime the field can be JS `undefined`, and downstream
//! consumers (`hasNumericValue`, the `t.isXxx` checks scattered
//! through the §5.5/§5.6 subtree) treat `undefined` as
//! "type-discriminator returns false → deopt".
//!
//! The Rust shape mirrors this with `Option<Box<Expr>>` so the
//! "no fold possible" path stays representable without inventing a
//! sentinel `Expr` variant that downstream `is_*` checks would
//! mis-classify (see `is_empty.rs::is_empty_value` — substituting
//! `Expr::Ident("undefined")` would silently flip its return value
//! relative to JS-undefined input).
//!
//! ## Meta-threading: `&mut Metadata` is implicit
//!
//! JS returns `{ value, meta }` as object spread. Rust threads
//! `&mut Metadata<'a>` by reference through every traverser/evaluator
//! call — the "returned meta" is the same one the caller already
//! owns. Storing `Metadata<'a>` in the result struct is impossible
//! without copying `&mut State`, which Rust's aliasing rules forbid.
//! The `_meta` parameter is preserved in the helper signature for
//! grep parity with upstream `createResultPair(value, meta)` call
//! sites; it's intentionally unused.

use swc_core::ecma::ast::Expr;

use crate::types::Metadata;

/// `{ value, meta }` shape. See module docs for why `value` is
/// `Option<Box<Expr>>` (1:1 fidelity to JS's
/// `t.Node | undefined | null` runtime shape).
#[derive(Debug)]
pub struct ResultPair {
    pub value: Option<Box<Expr>>,
}

/// 1:1 port of `createResultPair`. The `_meta` argument is unused but
/// kept for grep parity with upstream call sites.
pub fn create_result_pair(value: Option<Box<Expr>>, _meta: &Metadata<'_>) -> ResultPair {
    ResultPair { value }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{Lit, Number};

    fn meta_for_test() -> State {
        State::default()
    }

    #[test]
    fn create_result_pair_with_some_value() {
        let mut state = meta_for_test();
        let meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        };
        let value = Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: 1.0,
            raw: None,
        })));
        let pair = create_result_pair(Some(value), &meta);
        assert!(pair.value.is_some());
    }

    #[test]
    fn create_result_pair_with_none_models_js_undefined() {
        let mut state = meta_for_test();
        let meta = Metadata {
            state: &mut state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        };
        let pair = create_result_pair(None, &meta);
        assert!(pair.value.is_none());
    }
}
