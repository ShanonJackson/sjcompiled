//! 1:1 port of `@babel/generator@7.23.0/lib/generators/expressions.js`.

use crate::compat::generator::printer::Printer;

use swc_core::ecma::ast::{
    BinExpr, BinaryOp, CallExpr, Callee, CondExpr, Expr, ExprOrSpread, MemberExpr, MemberProp,
    ParenExpr, UnaryExpr, UnaryOp,
};

/// `UnaryExpression(node)`.
pub fn unary(p: &mut Printer, node: &UnaryExpr, parent_expr: &Expr) {
    let op_str = match node.op {
        UnaryOp::Minus => "-",
        UnaryOp::Plus => "+",
        UnaryOp::Bang => "!",
        UnaryOp::Tilde => "~",
        UnaryOp::TypeOf => "typeof",
        UnaryOp::Void => "void",
        UnaryOp::Delete => "delete",
    };
    let is_word = matches!(
        node.op,
        UnaryOp::TypeOf | UnaryOp::Void | UnaryOp::Delete
    );
    if is_word {
        p.word(op_str);
        p.space();
    } else {
        p.token(op_str);
    }
    p.print(&node.arg, Some(parent_expr));
}

/// `ConditionalExpression(node)`.
pub fn conditional(p: &mut Printer, node: &CondExpr, parent_expr: &Expr) {
    p.print(&node.test, Some(parent_expr));
    p.space();
    p.token_char(b'?');
    p.space();
    p.print(&node.cons, Some(parent_expr));
    p.space();
    p.token_char(b':');
    p.space();
    p.print(&node.alt, Some(parent_expr));
}

/// `BinaryExpression(node)` — also serves `LogicalExpression(node)`
/// because SWC's `BinExpr` carries both binary and logical operators
/// in the same `BinaryOp` enum (in/instanceof/===/etc plus &&/||/??).
/// Babel splits them at the AST level, but the printer emits both
/// shapes identically — this is actually upstream's
/// `expressions.js:exports.LogicalExpression = exports.BinaryExpression
/// = AssignmentExpression` aliasing pattern in disguise.
pub fn binary(p: &mut Printer, node: &BinExpr, parent_expr: &Expr) {
    p.print(&node.left, Some(parent_expr));
    p.space();
    let op_str = bin_op_str(node.op);
    let is_word_op = matches!(node.op, BinaryOp::In | BinaryOp::InstanceOf);
    if is_word_op {
        p.word(op_str);
    } else {
        p.token(op_str);
    }
    p.space();
    p.print(&node.right, Some(parent_expr));
}

fn bin_op_str(op: BinaryOp) -> &'static str {
    use BinaryOp::*;
    match op {
        EqEq => "==",
        NotEq => "!=",
        EqEqEq => "===",
        NotEqEq => "!==",
        Lt => "<",
        LtEq => "<=",
        Gt => ">",
        GtEq => ">=",
        LShift => "<<",
        RShift => ">>",
        ZeroFillRShift => ">>>",
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Mod => "%",
        BitOr => "|",
        BitXor => "^",
        BitAnd => "&",
        LogicalOr => "||",
        LogicalAnd => "&&",
        In => "in",
        InstanceOf => "instanceof",
        Exp => "**",
        NullishCoalescing => "??",
    }
}

/// `MemberExpression(node)`.
pub fn member(p: &mut Printer, node: &MemberExpr, parent_expr: &Expr) {
    p.print(&node.obj, Some(parent_expr));
    match &node.prop {
        MemberProp::Ident(i) => {
            p.token_char(b'.');
            p.word(i.sym.as_ref());
        }
        MemberProp::PrivateName(pn) => {
            p.token_char(b'.');
            p.token_char(b'#');
            p.word(pn.name.as_ref());
        }
        MemberProp::Computed(c) => {
            p.token_char(b'[');
            p.print(&c.expr, Some(parent_expr));
            p.token_char(b']');
        }
    }
}

/// `CallExpression(node)`.
pub fn call(p: &mut Printer, node: &CallExpr, parent_expr: &Expr) {
    match &node.callee {
        Callee::Expr(e) => p.print(e, Some(parent_expr)),
        Callee::Super(_) => p.word("super"),
        Callee::Import(_) => p.word("import"),
    }
    p.token_char(b'(');
    for (i, arg) in node.args.iter().enumerate() {
        if i > 0 {
            p.token_char(b',');
            p.space();
        }
        call_arg(p, arg, parent_expr);
    }
    p.token_char(b')');
}

fn call_arg(p: &mut Printer, arg: &ExprOrSpread, parent: &Expr) {
    if arg.spread.is_some() {
        p.token("...");
    }
    p.print(&arg.expr, Some(parent));
}

/// `ParenthesizedExpression(node)`.
///
/// Source-tree note: SWC's parser ALWAYS wraps author-parenthesised
/// expressions in `Expr::Paren`, whereas `@babel/parser` flattens
/// them and sets `node.extra.parenthesized = true` on the inner
/// expression. Babel's `needsParens(node, parent)` then decides
/// whether to ACTUALLY emit parens based on precedence. To match
/// Babel's bytes we treat `ParenExpr` as TRANSPARENT — print the
/// inner with the OUTER parent so paren policy gets the same
/// signal it would on a flattened Babel tree. Without this, every
/// author-written `(a && b) || c` would emit literally rather than
/// dropping the redundant parens (a real corpus divergence).
pub fn parenthesized(p: &mut Printer, node: &ParenExpr, outer_parent: &Expr) {
    // `outer_parent` is the GRANDPARENT of `node.expr` from Babel's
    // perspective (since Babel flattens the ParenExpr layer). Print
    // the inner expression with that grandparent as the parent — so
    // `needs_parens_for(inner_expr, outer_parent)` decides paren
    // policy on the FLATTENED shape, matching Babel exactly.
    p.print(&node.expr, Some(outer_parent));
}
