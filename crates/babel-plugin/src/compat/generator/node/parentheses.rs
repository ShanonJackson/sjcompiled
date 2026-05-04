//! 1:1 port of `@babel/generator@7.23.0/lib/node/parentheses.js`.
//!
//! Babel determines whether a child expression needs parens by
//! inspecting (child, parent) shape. For our 5 call sites the
//! reachable pairs are:
//! - Binary / Logical inside Binary / Logical (precedence + RHS)
//! - Conditional inside Binary / Conditional / Unary (always parens)
//! - Object/Array literal as bare statement (covered by `isFirstInContext`,
//!   not reachable from our `generate(&Expr)` entry point — the upstream
//!   call sites pass an Expression node, not a Program).
//!
//! Upstream signature: `(node, parent, printStack) -> bool`.
//! `printStack` is the chain of ancestors; we only need the immediate
//! parent for the reachable cases. If a future fixture exercises
//! `isFirstInContext` (object as bare statement, etc.), extend then.

use swc_core::ecma::ast::{
    BinExpr, BinaryOp, CallExpr, Callee, Expr, MemberExpr, MemberProp, NewExpr, OptCall,
    OptChainBase, OptChainExpr,
};

/// Operator precedence per upstream's `PRECEDENCE` table. Used by
/// `binary_needs_parens` to decide whether a child binary expression
/// needs parens given its parent's precedence and the child's
/// position (left vs right).
fn precedence(op: BinaryOp) -> i32 {
    use BinaryOp::*;
    match op {
        LogicalOr | NullishCoalescing => 0,
        LogicalAnd => 1,
        BitOr => 2,
        BitXor => 3,
        BitAnd => 4,
        EqEq | EqEqEq | NotEq | NotEqEq => 5,
        Lt | Gt | LtEq | GtEq | In | InstanceOf => 6,
        RShift | LShift | ZeroFillRShift => 7,
        Add | Sub => 8,
        Mul | Div | Mod => 9,
        Exp => 10,
    }
}

fn is_logical_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing
    )
}

/// `hasPostfixPart(node, parent)` — child sits in the head of a
/// member/call/new/tagged-template expression where the parent walks
/// the dot-chain. Used to force parens around child expressions that
/// would otherwise re-bind the postfix.
fn has_postfix_part(child: &Expr, parent: &Expr) -> bool {
    match parent {
        Expr::Member(m) => std::ptr::eq(&*m.obj as *const Expr, child as *const Expr),
        Expr::OptChain(OptChainExpr { base, .. }) => match &**base {
            OptChainBase::Member(m) => std::ptr::eq(&*m.obj as *const Expr, child as *const Expr),
            OptChainBase::Call(c) => {
                std::ptr::eq(&*c.callee as *const Expr, child as *const Expr)
            }
        },
        Expr::Call(c) => match &c.callee {
            Callee::Expr(e) => std::ptr::eq(&**e as *const Expr, child as *const Expr),
            _ => false,
        },
        Expr::New(NewExpr { callee, .. }) => {
            std::ptr::eq(&**callee as *const Expr, child as *const Expr)
        }
        Expr::TaggedTpl(t) => std::ptr::eq(&*t.tag as *const Expr, child as *const Expr),
        Expr::TsNonNull(_) => true,
        _ => false,
    }
}

fn is_unary_like(parent: &Expr) -> bool {
    matches!(
        parent,
        Expr::Unary(_) | Expr::Update(_) | Expr::Yield(_) | Expr::Await(_)
    )
}

fn is_binary(parent: &Expr) -> bool {
    matches!(parent, Expr::Bin(_))
}

/// `Binary(node, parent)` — does a binary/logical child need parens?
/// Returns true when child precedence is below the parent's, or when
/// they're equal but child is on the RHS (left-assoc default), or
/// when the parent wraps the child as a postfix-head / unary-like /
/// await.
pub fn binary_needs_parens(node: &BinExpr, parent: &Expr, child: &Expr) -> bool {
    // `**` left-associates with itself: `2 ** 3 ** 4` is a parse
    // error if you elide the parens on the RHS, but if the child
    // is on the LEFT and the parent is also `**`, parens are
    // required to disambiguate.
    if node.op == BinaryOp::Exp {
        if let Expr::Bin(p) = parent {
            if p.op == BinaryOp::Exp
                && std::ptr::eq(&*p.left as *const Expr, child as *const Expr)
            {
                return true;
            }
        }
    }

    if has_postfix_part(child, parent) || is_unary_like(parent) {
        return true;
    }

    if let Expr::Bin(p) = parent {
        let parent_pos = precedence(p.op);
        let node_pos = precedence(node.op);
        // Lower-precedence child inside higher-precedence parent
        // always needs parens. Equal-precedence on the RIGHT side
        // also needs parens (default left-associativity), EXCEPT
        // for logical operators which are explicitly allowed to
        // chain right-to-left without parens.
        if (parent_pos == node_pos
            && std::ptr::eq(&*p.right as *const Expr, child as *const Expr)
            && !is_logical_op(p.op))
            || parent_pos > node_pos
        {
            return true;
        }
    }

    false
}

/// `LogicalExpression(node, parent)` — extra rules on top of the
/// shared `Binary` policy:
///   - `||` inside `??` or `&&` parent → parens required.
///   - `&&` inside `??` parent → parens required.
///   - `??` inside any logical parent (unless parent is also `??`)
///     → parens required.
pub fn logical_needs_parens(node: &BinExpr, parent: &Expr) -> bool {
    if let Expr::Bin(p) = parent {
        if !is_logical_op(p.op) {
            // Non-logical-op parent: defer to Binary rule.
            return false;
        }
        match node.op {
            BinaryOp::LogicalOr => p.op == BinaryOp::NullishCoalescing || p.op == BinaryOp::LogicalAnd,
            BinaryOp::LogicalAnd => p.op == BinaryOp::NullishCoalescing,
            BinaryOp::NullishCoalescing => p.op != BinaryOp::NullishCoalescing,
            _ => false,
        }
    } else {
        false
    }
}

/// `ConditionalExpression(node, parent)` — conditionals always need
/// parens inside any binary, unary-like, await, or another conditional's
/// test position.
pub fn conditional_needs_parens(node_ptr: *const Expr, parent: &Expr) -> bool {
    if is_unary_like(parent) || is_binary(parent) || matches!(parent, Expr::Await(_)) {
        return true;
    }
    if let Expr::Cond(p) = parent {
        if std::ptr::eq(&*p.test as *const Expr, node_ptr) {
            return true;
        }
    }
    // Plus the UnaryLike policy: postfix-part child placement.
    // (We don't have access to `child` here — the caller passes
    //  `node_ptr` for pointer-equality with parent's slots.)
    false
}

// `_t` helpers used by callers that need to peek at a Member/Call
// without going through SWC's own enums — kept here so the dispatcher
// in printer.rs reads close to upstream's printer.js.
#[allow(dead_code)]
pub(crate) fn member_object<'a>(m: &'a MemberExpr) -> &'a Expr {
    &m.obj
}
#[allow(dead_code)]
pub(crate) fn member_property_is_computed(m: &MemberExpr) -> bool {
    matches!(m.prop, MemberProp::Computed(_))
}
#[allow(dead_code)]
pub(crate) fn call_callee<'a>(c: &'a CallExpr) -> Option<&'a Expr> {
    match &c.callee {
        Callee::Expr(e) => Some(e),
        _ => None,
    }
}
#[allow(dead_code)]
pub(crate) fn opt_call_callee<'a>(c: &'a OptCall) -> &'a Expr {
    &c.callee
}
