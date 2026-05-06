//! 1:1 port of `packages/babel-plugin/src/utils/manipulate-template-literal.ts`.
//!
//! Pure-data helpers for rewriting template literals during CSS
//! extraction. The single function that depends on Babel-NodePath
//! ancestry (`hasNestedTemplateLiteralsWithConditionalRules`) is
//! gated on Phase 5 §5.6's parent-traversal index.

use once_cell::sync::Lazy;
use regex::Regex;
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    ArrowExpr, CondExpr, Expr, Lit, Number, Str, Tpl, TplElement,
};

use crate::types::Metadata;

/// 1:1 port of upstream's `hasNestedTemplateLiteralsWithConditionalRules`
/// (`packages/babel-plugin/src/utils/manipulate-template-literal.ts:22-52`).
///
/// Upstream walks `meta.parentPath` (specifically: the result of
/// `getPathOfNode(node, meta.parentPath).parent`) for every
/// `ConditionalExpression` in the subtree, then for each match checks
/// `path.node[c]` for `c` in `CONDITIONAL_PATHS` — and **upstream's
/// `CONDITIONAL_PATHS` is `['consequent', 'alternate']` only** (see
/// `packages/babel-plugin/src/utils/constants.ts:1`). The cond's
/// `test` is INTENTIONALLY skipped — a `LogicalExpression` in the test
/// position (e.g. `(a && b) ? X : Y`) does NOT trip the gate.
///
/// For each Cond's `consequent` / `alternate`, the gate trips iff that
/// child node is one of:
/// 1. a `TaggedTemplateExpression` whose `.quasi === node` (the current
///    template literal sits inside a ternary branch), OR
/// 2. a `TemplateLiteral` whose `.expressions` include any
///    `ArrowFunctionExpression`, OR
/// 3. a `LogicalExpression`.
///
/// The Rust port lacks a NodePath ancestry channel, so we approximate
/// upstream's `traverse(parent, { ConditionalExpression(...) })` by
/// walking the synthetic Tpl `node` ourselves. The synthetic Tpl is
/// always built unattached at the call site (`extract_object_expression`
/// / `extract_template_literal`), so upstream's
/// `getPathOfNode(node, meta.parentPath)` returns a path whose `parent`
/// is the `meta.parentPath`'s container (e.g. the `styled.div(...)`
/// CallExpression for the `styled` cluster). Walking the Tpl's own
/// expressions covers every Conditional reachable from that parent
/// because the synthetic Tpl is wrapping the same arrow upstream
/// would also reach via parent traversal.
///
/// The walk descends through:
/// - `Tpl.exprs[i]` (the interpolations).
/// - `Arrow.body` (Expr-form only; BlockStmt bodies are not visited
///   by upstream's `traverse` either — `noScope: true` doesn't enter
///   nested Function bodies for ConditionalExpression matching here).
/// - `Cond.test` / `Cond.cons` / `Cond.alt` (the descent path; matches
///   upstream's `traverse` walking the entire subtree).
/// - `Paren.expr` (Babel parser strips parens; SWC keeps them — peek
///   through so the shape match doesn't drift).
///
/// AT each `Cond` node found, the *match* checks ONLY `c.cons` and
/// `c.alt` (NOT `c.test`) against patterns 1/2/3 directly — exactly
/// upstream's `CONDITIONAL_PATHS.map(c => path.node[c])` semantics.
///
/// **Closed follow-up (this comment block).** The previous Rust port
/// walked `c.test` AND treated any `LogicalExpression` anywhere in the
/// subtree as a positive match — both deltas vs upstream. Reproduction:
/// `fixtures/ct-minheight-calc-fg-stack` —
/// `({...}) => isFlexible && !isSwimlaneMode ? (fg() ? '100%' :
/// 'calc(...)') : undefined` would have its outer Cond's
/// `test = LogicalExpr` flag the gate, suppressing
/// `optimizeConditionalStatement` and falling through to the catch-all
/// CSS-variable path. Upstream's gate returns false here because the
/// outer Cond's cons is a Cond (not Logical) and alt is an Identifier;
/// the inner Cond's cons/alt are both StringLiterals. See
/// `packages/babel-plugin/src/utils/constants.ts:1` for upstream's
/// canonical paths list.
pub fn has_nested_template_literals_with_conditional_rules(
    node: &Tpl,
    _meta: &mut Metadata<'_>,
) -> bool {
    // Pointer-identity proxy for upstream's `expression.quasi === node`
    // check inside the per-Cond branch test. Upstream uses JS reference
    // equality between the matched Cond branch's `.quasi` and the
    // template literal we were called with. The Rust analog: address
    // equality of the borrowed `&Tpl`. The walk takes `&Tpl` by
    // reference, so the address is stable for the duration of the call.
    let node_ptr: *const Tpl = node;
    for expr in &node.exprs {
        if walk_for_conditional_match(expr, node_ptr) {
            return true;
        }
    }
    // case 1 (outer ternary wrap) — skipped: requires the parent walk.
    // For our styled / css-prop / xcss cluster the template is never
    // directly a ternary branch; raise as Drift if a fixture surfaces.
    false
}

/// Walks the expression subtree looking for `ConditionalExpression`
/// nodes. AT each Cond, checks ONLY its `consequent` and `alternate`
/// (NOT its `test`) against the three patterns. Returns true on first
/// match (upstream's `path.stop()` behaviour). Otherwise descends
/// through test/cons/alt and other container nodes.
///
/// `node_ptr` threads the address of the input `&Tpl` through so the
/// case-1 test (`TaggedTpl.quasi === node`) can match upstream's
/// reference-equality semantics via pointer comparison.
fn walk_for_conditional_match(
    expr: &swc_core::ecma::ast::Expr,
    node_ptr: *const Tpl,
) -> bool {
    use swc_core::ecma::ast::{BlockStmtOrExpr, Expr as E};
    match expr {
        E::Cond(c) => {
            // AT a ConditionalExpression: check cons/alt directly
            // (mirrors upstream's `CONDITIONAL_PATHS = ['consequent',
            // 'alternate']`).
            if branch_matches_conditional_rules(&c.cons, node_ptr)
                || branch_matches_conditional_rules(&c.alt, node_ptr)
            {
                return true;
            }
            // Otherwise descend into all three children — upstream's
            // `traverse` walks the whole subtree (test included) for
            // nested `ConditionalExpression`s, even though the per-match
            // check skips `test`.
            walk_for_conditional_match(&c.test, node_ptr)
                || walk_for_conditional_match(&c.cons, node_ptr)
                || walk_for_conditional_match(&c.alt, node_ptr)
        }
        E::Tpl(inner) => {
            for e in &inner.exprs {
                if walk_for_conditional_match(e, node_ptr) {
                    return true;
                }
            }
            false
        }
        E::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::Expr(e) => walk_for_conditional_match(e, node_ptr),
            BlockStmtOrExpr::BlockStmt(_) => false,
        },
        E::Paren(p) => walk_for_conditional_match(&p.expr, node_ptr),
        E::Bin(b) => {
            walk_for_conditional_match(&b.left, node_ptr)
                || walk_for_conditional_match(&b.right, node_ptr)
        }
        E::Unary(u) => walk_for_conditional_match(&u.arg, node_ptr),
        _ => false,
    }
}

/// 1:1 port of upstream's per-Cond branch test (manipulate-template-literal.ts:36-46).
/// Returns true iff `branch` is:
/// 1. a `TaggedTemplateExpression` whose `.quasi` is the same node
///    as the input template literal (upstream `expression.quasi ===
///    node`, JS reference equality — Rust analog: pointer identity), OR
/// 2. a `TemplateLiteral` with at least one `ArrowFunctionExpression`
///    in its expressions, OR
/// 3. a `LogicalExpression`.
fn branch_matches_conditional_rules(
    expr: &swc_core::ecma::ast::Expr,
    node_ptr: *const Tpl,
) -> bool {
    use swc_core::ecma::ast::{BinaryOp, Expr as E};
    match expr {
        // case 1: TaggedTemplateExpression with identity-matching
        // `.tpl`. SWC's `TaggedTpl.tpl` is the `Tpl` analog of Babel's
        // `TaggedTemplateExpression.quasi`. Pointer-equality on `&Tpl`
        // mirrors JS `===`.
        E::TaggedTpl(tt) => std::ptr::eq(&*tt.tpl as *const Tpl, node_ptr),
        // case 2: TemplateLiteral with arrow exprs.
        E::Tpl(t) => t.exprs.iter().any(|e| matches!(&**e, E::Arrow(_))),
        // case 3: LogicalExpression.
        E::Bin(b) => matches!(
            b.op,
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
        ),
        _ => false,
    }
}

/// `recomposeTemplateLiteral` upstream lines 54–72. In-place: prefix
/// is prepended to the leading quasi, suffix is appended to the
/// trailing quasi.
///
/// Mirrors the JS pattern of mutating `leadQuasi.value = { raw,
/// cooked }` and `tailQuasi.value = { raw, cooked }`. Rust's
/// `TplElement` has `raw: Atom` and `cooked: Option<Atom>` —
/// preserve `cooked = None` if upstream had it None (template
/// elements with cooked-omitted are permitted by the spec for raw
/// strings).
pub fn recompose_template_literal(template: &mut Tpl, prefix: &str, suffix: &str) {
    let n = template.quasis.len();
    if n == 0 {
        return;
    }

    {
        // Babel always reads BOTH leadQuasi and tailQuasi indices,
        // even when n == 1 (in that case lead == tail and the
        // mutations stack). Mirror with a borrowed scope.
        let lead = &mut template.quasis[0];
        let new_raw = format!("{}{}", prefix, lead.raw.as_str());
        let new_cooked = lead
            .cooked
            .as_ref()
            .map(|c| format!("{}{}", prefix, c.to_atom_lossy().as_str()));
        lead.raw = new_raw.into();
        lead.cooked = new_cooked.map(|c| c.into());
    }
    {
        let tail = &mut template.quasis[n - 1];
        let new_raw = format!("{}{}", tail.raw.as_str(), suffix);
        let new_cooked = tail
            .cooked
            .as_ref()
            .map(|c| format!("{}{}", c.to_atom_lossy().as_str(), suffix));
        tail.raw = new_raw.into();
        tail.cooked = new_cooked.map(|c| c.into());
    }
}

/// CSS-property-end regex from upstream:
/// `/(-?[a-z]+)+$/`. The `+` outside captures repeated kebab-case
/// segments at end-of-string.
static VALID_CSS_PROPERTY_END: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(-?[a-z]+)+$").expect("static regex compiles"));

/// `optimizeConditionalExpression` upstream lines 80–122. Tries to
/// fold a `<prefix>${cond ? a : b}<suffix>` into `cond ? '<prefix>a<suffix>'
/// : '<prefix>b<suffix>'` so each branch can be hashed to its own
/// atomic class.
///
/// Returns the optimised expression; if the prefix isn't a valid CSS
/// property OR ends in a quote (string-context), returns the input
/// unchanged.
pub fn optimize_conditional_expression(
    prefix: &str,
    suffix: &str,
    expression: &CondExpr,
) -> CondExpr {
    let trimmed_prefix = prefix.trim();
    let style_property = trimmed_prefix.split(':').next().unwrap_or("");
    let is_valid_css_property = VALID_CSS_PROPERTY_END.is_match(style_property.trim_end());
    let is_not_part_of_string = !prefix.ends_with('\'') && !prefix.ends_with('"');

    if !(is_valid_css_property && is_not_part_of_string) {
        return expression.clone();
    }

    let optimise_branch = |branch: &Expr| -> Expr {
        match branch {
            Expr::Lit(Lit::Num(Number { value, .. })) => Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: format!("{}{}{}", prefix, num_to_js_string(*value), suffix).into(),
                raw: None,
            })),
            Expr::Lit(Lit::Str(Str { value, .. })) => Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: format!("{}{}{}", prefix, value.to_atom_lossy().as_str(), suffix).into(),
                raw: None,
            })),
            Expr::Tpl(tpl) => {
                let mut tpl_clone = tpl.clone();
                recompose_template_literal(&mut tpl_clone, prefix, suffix);
                Expr::Tpl(tpl_clone)
            }
            Expr::Cond(cond) => Expr::Cond(optimize_conditional_expression(prefix, suffix, cond)),
            other => {
                let is_value_empty = crate::utils::is_empty::is_empty_value(other);
                let inner: Expr = if is_value_empty {
                    Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: "".into(),
                        raw: None,
                    }))
                } else {
                    other.clone()
                };
                Expr::Tpl(Tpl {
                    span: DUMMY_SP,
                    quasis: vec![
                        TplElement {
                            span: DUMMY_SP,
                            tail: false,
                            cooked: Some(prefix.to_string().into()),
                            raw: prefix.to_string().into(),
                        },
                        TplElement {
                            span: DUMMY_SP,
                            tail: true,
                            cooked: Some(suffix.to_string().into()),
                            raw: suffix.to_string().into(),
                        },
                    ],
                    exprs: vec![Box::new(inner)],
                })
            }
        }
    };

    CondExpr {
        span: DUMMY_SP,
        test: expression.test.clone(),
        cons: Box::new(optimise_branch(&expression.cons)),
        alt: Box::new(optimise_branch(&expression.alt)),
    }
}

/// JS `String(numericLiteral.value)` — `12` → `"12"`, `1.5` → `"1.5"`.
fn num_to_js_string(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e16 {
        (value as i64).to_string()
    } else {
        value.to_string()
    }
}

/// `optimizeConditionalStatement` upstream lines 131–162. Mutates
/// `quasi`, `nextQuasi`, and `expression.body` in place.
///
/// Returns nothing — the JS version is `void`; the Rust port mirrors
/// with `()`.
pub fn optimize_conditional_statement(
    quasi: &mut TplElement,
    next_quasi: &mut TplElement,
    expression: &mut ArrowExpr,
) {
    let quasi_value = quasi.raw.to_string();
    // Breaks down quasi into individual statements.
    let quasi_statements: Vec<&str> = SPLIT_STATEMENT_RE
        .split(&quasi_value)
        .collect();
    let prefix = quasi_statements
        .last()
        .copied()
        .unwrap_or("")
        .to_string();
    let next_quasi_value = next_quasi.raw.to_string();
    let end_of_statement_index = next_quasi_value.find(';');
    let next_quasi_ends_statement = end_of_statement_index.is_some();

    // Babel's parser strips `ParenthesizedExpression`; SWC keeps it.
    // `(props) => (props.isPrimary ? 'blue' : 'red')` arrives as
    // `arrow.body = Expr::Paren(Expr::Cond(...))` from SWC, but
    // `Expr::Cond(...)` from Babel. Unwrap before pattern-matching so
    // both shapes hit the same branch. See `crates/babel-plugin/src/compat/paren.rs`.
    let body_is_conditional = matches!(
        &*expression.body,
        swc_core::ecma::ast::BlockStmtOrExpr::Expr(e)
            if matches!(crate::compat::paren::unwrap_paren(e), Expr::Cond(_))
    );

    if !(body_is_conditional && !prefix.is_empty() && next_quasi_ends_statement) {
        return;
    }

    let end_idx = end_of_statement_index.expect("checked above");
    let suffix = &next_quasi_value[..end_idx];

    // Pull the cond expr out for optimisation. Same paren-unwrap as the
    // body-shape gate above.
    let original_cond = match &*expression.body {
        swc_core::ecma::ast::BlockStmtOrExpr::Expr(e) => match crate::compat::paren::unwrap_paren(e) {
            Expr::Cond(c) => c.clone(),
            _ => return,
        },
        _ => return,
    };
    let optimised = optimize_conditional_expression(&prefix, suffix, &original_cond);

    // upstream: `if (optimizedConditional !== expression.body)`. This
    // is reference inequality — when the no-op branch returns the
    // same object, the JS version skips the mutation. Rust analog:
    // structural inequality. The no-op branch (invalid CSS property
    // or string context) returns `expression.clone()` so structural
    // equality holds — skip the mutation in that case.
    if cond_expr_eq(&optimised, &original_cond) {
        return;
    }

    let last_prefix_pos = quasi_value.rfind(&prefix).unwrap_or(quasi_value.len());
    let quasi_value_without_prefix = &quasi_value[..last_prefix_pos];

    expression.body = Box::new(swc_core::ecma::ast::BlockStmtOrExpr::Expr(Box::new(
        Expr::Cond(optimised),
    )));
    quasi.raw = quasi_value_without_prefix.to_string().into();
    quasi.cooked = Some(quasi_value_without_prefix.to_string().into());

    let quasi_value_without_suffix = &next_quasi_value[end_idx + 1..];
    next_quasi.raw = quasi_value_without_suffix.to_string().into();
    next_quasi.cooked = Some(quasi_value_without_suffix.to_string().into());
}

/// Reference-inequality check shape mirrored as structural-equality.
/// Two CondExprs are equal iff they pretty-print identically — for
/// the no-op branch we know we cloned, so this returns true.
/// (For the optimised branch the bodies differ structurally and
/// this returns false. Sufficient for the upstream `!==` semantics
/// at this call site.)
fn cond_expr_eq(a: &CondExpr, b: &CondExpr) -> bool {
    use std::ptr;
    // Cheap check first: same Box pointers → same node. Falls back
    // to structural pretty-print equality (rare path; the optimiser
    // always produces a fresh CondExpr when it does work).
    ptr::eq(a as *const _, b as *const _)
        || (matches!((a.cons.as_ref(), b.cons.as_ref()), (l, r) if format!("{:?}", l) == format!("{:?}", r))
            && matches!((a.alt.as_ref(), b.alt.as_ref()), (l, r) if format!("{:?}", l) == format!("{:?}", r))
            && matches!((a.test.as_ref(), b.test.as_ref()), (l, r) if format!("{:?}", l) == format!("{:?}", r)))
}

/// Upstream: `quasiValue.split(/[;|{|}]/g)`. Note: Babel's class is
/// `[;|{|}]` which accepts `;`, `|`, `{`, `}` — including `|`. The
/// regex looks like a typo (`|` between bracket entries is literal
/// in a char class) but the JS test set has been stable on this for
/// years. Reproduce verbatim for parity.
static SPLIT_STATEMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[;|{|}]").expect("static regex compiles"));

/// `isQuasiMidStatement` upstream lines 168–181. Strips block
/// comments, trims trailing whitespace, and asks "does this quasi
/// end mid-CSS-statement?" — i.e. it is non-empty AND doesn't end on
/// `;`, `{`, or `}`.
pub fn is_quasi_mid_statement(quasi: &TplElement) -> bool {
    let raw = quasi.raw.to_string();
    let stripped = BLOCK_COMMENT_RE.replace_all(&raw, "");
    let stripped = stripped.trim_end();
    !stripped.is_empty()
        && !stripped.ends_with(';')
        && !stripped.ends_with('{')
        && !stripped.ends_with('}')
}

static BLOCK_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"/\*(.|\n)*?\*/").expect("static regex compiles"));

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::ecma::ast::TplElement;

    fn quasi(raw: &str) -> TplElement {
        TplElement {
            span: DUMMY_SP,
            tail: true,
            cooked: Some(raw.into()),
            raw: raw.into(),
        }
    }

    #[test]
    fn is_quasi_mid_statement_empty_returns_false() {
        assert!(!is_quasi_mid_statement(&quasi("")));
    }

    #[test]
    fn is_quasi_mid_statement_ends_with_semicolon_returns_false() {
        assert!(!is_quasi_mid_statement(&quasi("color: red;")));
    }

    #[test]
    fn is_quasi_mid_statement_ends_with_open_brace_returns_false() {
        assert!(!is_quasi_mid_statement(&quasi(".foo {")));
    }

    #[test]
    fn is_quasi_mid_statement_mid_property_returns_true() {
        assert!(is_quasi_mid_statement(&quasi("color: ")));
    }

    #[test]
    fn is_quasi_mid_statement_strips_block_comments() {
        // After stripping `/* hi */` we have `color: ` — mid-statement.
        assert!(is_quasi_mid_statement(&quasi("color: /* hi */")));
    }

    #[test]
    fn is_quasi_mid_statement_trailing_whitespace_only() {
        assert!(!is_quasi_mid_statement(&quasi("   ")));
    }

    #[test]
    fn recompose_template_literal_prepends_and_appends() {
        let mut tpl = Tpl {
            span: DUMMY_SP,
            exprs: vec![],
            quasis: vec![quasi("body")],
        };
        recompose_template_literal(&mut tpl, "color: ", ";");
        assert_eq!(&*tpl.quasis[0].raw, "color: body;");
        assert_eq!(
            tpl.quasis[0]
                .cooked
                .as_ref()
                .map(|c| c.to_atom_lossy().as_str().to_string()),
            Some("color: body;".to_string())
        );
    }

    #[test]
    fn recompose_template_literal_handles_multi_quasi() {
        let mut tpl = Tpl {
            span: DUMMY_SP,
            exprs: vec![Box::new(Expr::Ident(swc_core::ecma::ast::Ident::new(
                "x".into(),
                DUMMY_SP,
                Default::default(),
            )))],
            quasis: vec![quasi("a"), quasi("b")],
        };
        recompose_template_literal(&mut tpl, "P:", ";");
        assert_eq!(&*tpl.quasis[0].raw, "P:a");
        assert_eq!(&*tpl.quasis[1].raw, "b;");
    }
}
