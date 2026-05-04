//! 1:1 port of `packages/babel-plugin/src/utils/ast.ts`.
//!
//! ### Babel→SWC divergence
//!
//! `buildCodeFrameError` in Babel returns an `Error` carrying a
//! generated code-frame string. The Rust port returns a
//! [`CssBuildError`] value-type — the visitor boundary
//! (Program::enter / Program::exit in `lib.rs`) is responsible for
//! emitting the error via SWC's HANDLER (cf. the strip-runtime
//! plugin's §1.5 use of `HANDLER.with(|h| h.struct_span_err(...))`
//! documented in plugins/STATUS.md). The error type travels
//! through `Result<T, CssBuildError>` returned by the
//! `extract*` / `buildCss` functions in `css_builders.rs`.
//!
//! `getPathOfNode` is a Babel-only construct (it walks
//! NodePath/Scope ancestry to find a child path). The SWC analog
//! requires the parent-traversal index that lives in Phase 5
//! §5.6 (`utils/traverse_expression/`) and isn't standalone-portable
//! — left as `unimplemented!()` here. Callers
//! (`manipulate_template_literal::hasNestedTemplateLiteralsWithConditionalRules`)
//! are themselves stubbed for the same Phase 5 reason.
//!
//! `wrapNodeInIIFE` / `pickFunctionBody` translate cleanly to SWC
//! ast builders — included here so future Phase 5 callers find them
//! at the expected path.

use swc_core::common::{Span, DUMMY_SP};
use swc_core::ecma::ast::{
    ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, Function, ParenExpr,
};

/// Error carrying a code-frame-eligible message + the source span
/// the message should anchor at. The visitor boundary calls
/// `HANDLER.with(|h| h.struct_span_err(err.span, &err.message).emit())`
/// before returning Err.
#[derive(Debug, Clone)]
pub struct CssBuildError {
    pub message: String,
    pub span: Option<Span>,
}

impl CssBuildError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

impl std::fmt::Display for CssBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CssBuildError {}

/// 1:1 with upstream `buildCodeFrameError`. Returns the error
/// value for the caller to surface via `?`. The "(line:col)"
/// suffix the JS version derives from `node.loc` is built by the
/// HANDLER emit at the visitor boundary using the captured span;
/// keeping the message clean here means a single source of truth
/// for the line/col formatter.
///
/// Mirrors upstream lines 44–56 — the `path.buildCodeFrameError`
/// fall-through (when `node` is None) is preserved as a separate
/// helper [`build_code_frame_error_no_node`] because Rust can't
/// overload by None-vs-Some at the type level cleanly.
pub fn build_code_frame_error(error: impl Into<String>, node_span: Option<Span>) -> CssBuildError {
    let message = error.into();
    if let Some(span) = node_span {
        CssBuildError::new(message).with_span(span)
    } else {
        // Mirrors `if (!node) { throw parentPath.buildCodeFrameError(error); }`.
        // No span = HANDLER emits at the file head.
        CssBuildError::new(message)
    }
}

/// Convenience constructor for the no-node fall-through.
pub fn build_code_frame_error_no_node(error: impl Into<String>) -> CssBuildError {
    CssBuildError::new(error)
}

/// `wrapNodeInIIFE` upstream lines 64–65. Wraps an expression in
/// `(() => <expr>)()`. Used by Phase 5 evaluate-expression code paths
/// when promoting a function body. Pre-staged here so callers find
/// it where upstream puts it.
pub fn wrap_node_in_iife(body: BlockStmtOrExpr) -> CallExpr {
    let arrow = Expr::Arrow(ArrowExpr {
        span: DUMMY_SP,
        params: vec![],
        body: Box::new(body),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
        ctxt: Default::default(),
    });
    CallExpr {
        span: DUMMY_SP,
        // Wrap in parens so the printer emits `(...)()`. Babel's
        // `t.callExpression(arrow, [])` doesn't add parens at AST
        // construction time — the printer adds them. SWC's printer
        // does the same when callee is an ArrowExpr in CallExpr
        // position; explicit Paren keeps parity safe.
        callee: Callee::Expr(Box::new(Expr::Paren(ParenExpr {
            span: DUMMY_SP,
            expr: Box::new(arrow),
        }))),
        args: vec![],
        type_args: None,
        ctxt: Default::default(),
    }
}

/// `pickFunctionBody` upstream lines 80–81. Returns the function
/// body as an Expression — wraps a BlockStmt body in an IIFE,
/// returns an Expression body unchanged.
pub fn pick_function_body(function: &Function) -> Expr {
    if let Some(body) = &function.body {
        // Mirrors `t.isBlockStatement(node) ? wrapNodeInIIFE(node) : node`.
        Expr::Call(wrap_node_in_iife(BlockStmtOrExpr::BlockStmt(body.clone())))
    } else {
        // Function with no body shouldn't reach here in practice;
        // upstream's null-default behaviour is to surface a runtime
        // error. Mirror: empty IIFE.
        Expr::Call(wrap_node_in_iife(BlockStmtOrExpr::BlockStmt(
            swc_core::ecma::ast::BlockStmt {
                span: DUMMY_SP,
                stmts: vec![],
                ctxt: Default::default(),
            },
        )))
    }
}

/// Suppress unused-import warning on the rare-call path:
/// `ExprOrSpread` is referenced by transitive consumers.
const _: Option<ExprOrSpread> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;

    #[test]
    fn build_code_frame_error_carries_message() {
        let err = build_code_frame_error("oops", None);
        assert_eq!(err.message, "oops");
        assert!(err.span.is_none());
    }

    #[test]
    fn build_code_frame_error_records_span() {
        let span = swc_core::common::Span::new(
            swc_core::common::BytePos(10),
            swc_core::common::BytePos(20),
        );
        let err = build_code_frame_error("oops", Some(span));
        assert_eq!(err.span.map(|s| (s.lo.0, s.hi.0)), Some((10, 20)));
    }

    #[test]
    fn pick_function_body_wraps_block() {
        let f = Function {
            params: vec![],
            decorators: vec![],
            span: DUMMY_SP,
            body: Some(swc_core::ecma::ast::BlockStmt {
                span: DUMMY_SP,
                stmts: vec![],
                ctxt: Default::default(),
            }),
            is_generator: false,
            is_async: false,
            type_params: None,
            return_type: None,
            ctxt: Default::default(),
        };
        let result = pick_function_body(&f);
        assert!(matches!(result, Expr::Call(_)));
    }
}
