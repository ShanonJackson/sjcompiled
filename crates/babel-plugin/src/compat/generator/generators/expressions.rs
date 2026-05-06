//! 1:1 port of `@babel/generator@7.23.0/lib/generators/expressions.js`.

use crate::compat::generator::printer::Printer;

use swc_core::ecma::ast::{
    ArrayPat, ArrowExpr, AssignPat, AssignPatProp, BinExpr, BinaryOp, BindingIdent, BlockStmtOrExpr,
    CallExpr, Callee, CondExpr, Expr, ExprOrSpread, KeyValuePatProp, MemberExpr, MemberProp,
    ObjectPat, ObjectPatProp, OptCall, OptChainBase, OptChainExpr, ParenExpr, Pat, PropName,
    RestPat, UnaryExpr, UnaryOp,
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

/// `OptionalMemberExpression(node)` / `OptionalCallExpression(node)` —
/// 1:1 port of `@babel/generator@7.23.0/lib/generators/expressions.js:150-189`.
///
/// SWC unifies both shapes under `Expr::OptChain(OptChainExpr { optional, base })`
/// where `base` is either `Member` (→ `OptionalMemberExpression`) or
/// `Call` (→ `OptionalCallExpression`). The `optional: bool` flag matches
/// Babel's `node.optional` exactly: `true` emits `?.`, `false` emits `.`
/// (or just `(` for calls). Without this arm the printer's catch-all
/// emits `/*UNHANDLED-EXPR*/` for every `?.` expression, collapsing
/// every CSS-variable hash to the same constant — see
/// `ct-optional-chain-dynamic-style` divergence (2026-05-07).
pub fn opt_chain(p: &mut Printer, node: &OptChainExpr, parent_expr: &Expr) {
    match &*node.base {
        OptChainBase::Member(m) => optional_member(p, m, node.optional, parent_expr),
        OptChainBase::Call(c) => optional_call(p, c, node.optional, parent_expr),
    }
}

fn optional_member(p: &mut Printer, node: &MemberExpr, optional: bool, parent_expr: &Expr) {
    p.print(&node.obj, Some(parent_expr));
    match &node.prop {
        MemberProp::Ident(i) => {
            if optional {
                p.token("?.");
            } else {
                p.token_char(b'.');
            }
            p.word(i.sym.as_ref());
        }
        MemberProp::PrivateName(pn) => {
            if optional {
                p.token("?.");
            } else {
                p.token_char(b'.');
            }
            p.token_char(b'#');
            p.word(pn.name.as_ref());
        }
        MemberProp::Computed(c) => {
            if optional {
                p.token("?.");
            }
            p.token_char(b'[');
            p.print(&c.expr, Some(parent_expr));
            p.token_char(b']');
        }
    }
}

fn optional_call(p: &mut Printer, node: &OptCall, optional: bool, parent_expr: &Expr) {
    p.print(&node.callee, Some(parent_expr));
    if optional {
        p.token("?.");
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

/// `ArrowFunctionExpression(node, parent)` — 1:1 port of
/// `@babel/generator@7.23.0/lib/generators/methods.js:111`.
///
/// Reachable from `getVariableDeclaratorValueForOwnPath` →
/// `generate(arrow).code` whenever a styled / css-prop / keyframes
/// interpolation is `${(props) => props.x}` or similar. Without this
/// arm, the printer's catch-all emits `/*UNHANDLED-EXPR*/` for every
/// arrow, collapsing every CSS-variable hash to the same constant
/// (`hash("/*UNHANDLED-EXPR*/") = "2wqa78"`).
///
/// Body coverage: expression bodies recurse via `print()`. Block
/// bodies are not yet ported (would need `BlockStatement` printer);
/// emit a placeholder until a fixture surfaces the gap.
pub fn arrow(p: &mut Printer, node: &ArrowExpr, parent: &Expr) {
    if node.is_async {
        p.word("async");
        p.space();
    }
    let single_simple_ident = node.params.len() == 1
        && matches!(&node.params[0], Pat::Ident(bi) if bi.type_ann.is_none() && !bi.id.optional);
    if single_simple_ident {
        // Single Identifier parameter without TypeAnnotation /
        // optional / decorators / comments — emit without parens
        // (`x => x` rather than `(x) => x`).
        pat(p, &node.params[0], parent);
    } else {
        p.token_char(b'(');
        for (i, par) in node.params.iter().enumerate() {
            if i > 0 {
                p.token_char(b',');
                p.space();
            }
            pat(p, par, parent);
        }
        p.token_char(b')');
    }
    p.space();
    p.token("=>");
    p.space();
    match &*node.body {
        BlockStmtOrExpr::Expr(e) => p.print(e, Some(parent)),
        BlockStmtOrExpr::BlockStmt(b) => {
            // §6.8e: BlockStatement printer ported in
            // `generators/statements.rs::block_statement`. Mirrors
            // upstream `base.js::BlockStatement` byte-for-byte for
            // the styled / css-prop dynamic-arrow-body cluster.
            super::statements::block_statement(p, b);
        }
    }
}

/// Minimal `Pat` printer covering the shapes reachable from arrow
/// parameter lists in our hash-call corpus. Extend as new shapes
/// surface.
fn pat(p: &mut Printer, node: &Pat, parent: &Expr) {
    match node {
        Pat::Ident(bi) => binding_ident(p, bi, parent),
        Pat::Object(o) => object_pat(p, o, parent),
        Pat::Array(a) => array_pat(p, a, parent),
        Pat::Rest(r) => rest_pat(p, r, parent),
        Pat::Assign(a) => assign_pat(p, a, parent),
        Pat::Expr(e) => p.print(e, Some(parent)),
        Pat::Invalid(_) => {
            p.buf.append("/*UNHANDLED-PAT*/");
        }
    }
}

fn binding_ident(p: &mut Printer, bi: &BindingIdent, parent: &Expr) {
    p.word(bi.id.sym.as_ref());
    // Babel emits Identifier `?` then `: TypeAnnotation` when present.
    // Upstream: `@babel/generator/lib/generators/types.js::Identifier`
    // (the print() pipeline appends typeAnnotation after the name) +
    // the `OptionalMemberExpression`-style `?` token when `optional`.
    // The TS compat slice lives in `super::typescript`.
    if bi.id.optional {
        p.token_char(b'?');
    }
    if let Some(type_ann) = &bi.type_ann {
        p.token_char(b':');
        p.space();
        super::typescript::ts_type_inner(p, &type_ann.type_ann, parent);
    }
}

fn object_pat(p: &mut Printer, node: &ObjectPat, parent: &Expr) {
    p.token_char(b'{');
    for (i, prop) in node.props.iter().enumerate() {
        if i > 0 {
            p.token_char(b',');
            p.space();
        }
        match prop {
            ObjectPatProp::KeyValue(KeyValuePatProp { key, value }) => {
                prop_name(p, key, parent);
                p.token_char(b':');
                p.space();
                pat(p, value, parent);
            }
            ObjectPatProp::Assign(AssignPatProp { key, value, .. }) => {
                p.word(key.sym.as_ref());
                if let Some(v) = value {
                    p.space();
                    p.token_char(b'=');
                    p.space();
                    p.print(v, Some(parent));
                }
            }
            ObjectPatProp::Rest(r) => rest_pat(p, r, parent),
        }
    }
    p.token_char(b'}');
}

fn array_pat(p: &mut Printer, node: &ArrayPat, parent: &Expr) {
    p.token_char(b'[');
    for (i, elem) in node.elems.iter().enumerate() {
        if i > 0 {
            p.token_char(b',');
            p.space();
        }
        if let Some(e) = elem {
            pat(p, e, parent);
        }
    }
    p.token_char(b']');
}

fn rest_pat(p: &mut Printer, node: &RestPat, parent: &Expr) {
    p.token("...");
    pat(p, &node.arg, parent);
}

fn assign_pat(p: &mut Printer, node: &AssignPat, parent: &Expr) {
    pat(p, &node.left, parent);
    p.space();
    p.token_char(b'=');
    p.space();
    p.print(&node.right, Some(parent));
}

fn prop_name(p: &mut Printer, key: &PropName, parent: &Expr) {
    match key {
        PropName::Ident(i) => p.word(i.sym.as_ref()),
        PropName::Str(s) => {
            p.token_char(b'"');
            let v = s.value.to_atom_lossy();
            p.buf.append(v.as_str());
            p.token_char(b'"');
        }
        PropName::Num(n) => {
            if let Some(raw) = &n.raw {
                p.number(raw.as_ref());
            } else {
                p.number(&n.value.to_string());
            }
        }
        PropName::Computed(c) => {
            p.token_char(b'[');
            p.print(&c.expr, Some(parent));
            p.token_char(b']');
        }
        PropName::BigInt(b) => {
            p.number(&b.value.to_string());
            p.token_char(b'n');
        }
    }
}
