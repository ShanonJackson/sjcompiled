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
/// Upstream walks `meta.parentPath` for `ConditionalExpression`s, then
/// for each match checks if any of `test`/`consequent`/`alternate`:
/// 1. is a `TaggedTemplateExpression` whose `quasi === node` (the
///    current template literal is sitting INSIDE a ternary branch), OR
/// 2. is a `TemplateLiteral` whose expressions include an
///    `ArrowFunctionExpression` (a nested template with arrow-body
///    interpolations), OR
/// 3. is a `LogicalExpression`.
///
/// The Rust port lacks the parent-NodePath analog (Phase 5 §5.6 chose
/// the side-table `ScopeIndex` + `parent_scope` + `own_scope` model
/// over a NodePath replica). Without parent-walk, we can only check
/// cases 2 and 3 directly off the input `node` — i.e. detect nested
/// template literals with arrow-body expressions OR logical expressions
/// that live INSIDE the current template's `expressions` array.
///
/// **Open follow-up.** The case 1 outer-ternary-wrap shape (template
/// itself is a branch of a parent ConditionalExpression) cannot be
/// detected without §5.6 parent-walk. For our styled / css-prop / xcss
/// cluster the template is always the body of a TaggedTemplateExpression
/// (`styled.div\`...\``) or the value of a JSXAttribute (`css={...}`)
/// — never directly inside a ternary branch. If a fixture surfaces
/// `<div css={cond ? css\`...\` : css\`...\`} />` shape, this needs
/// the parent walk; raise as Drift and add §6.8g.
pub fn has_nested_template_literals_with_conditional_rules(
    node: &Tpl,
    _meta: &mut Metadata<'_>,
) -> bool {
    for expr in &node.exprs {
        if expr_contains_nested_conditional_rules(expr) {
            return true;
        }
    }
    // case 1 (outer ternary wrap) — skipped pending §6.8g parent walk.
    false
}

/// Recursive walker for `has_nested_template_literals_with_conditional_rules`.
/// Returns true when:
/// - The expression IS a nested template literal whose own exprs include
///   an arrow-function expression (case 2 from upstream), OR
/// - The expression IS a logical expression (case 3), OR
/// - The expression CONTAINS one of the above somewhere in its subtree.
///
/// Recursion targets cover arrow bodies and ternary branches because
/// upstream's `traverse(parent, { ConditionalExpression })` walks the
/// entire subtree below the matched parent. The Rust port's input is
/// the template literal itself — so we walk DOWN from each interpolation
/// rather than up from a parent path. The result is a superset of cases
/// 2 and 3 (we may flag templates upstream wouldn't reach via its
/// parent walk if the template lives BELOW our `node`); for the styled
/// / css-prop cluster the branching shapes coincide.
fn expr_contains_nested_conditional_rules(expr: &swc_core::ecma::ast::Expr) -> bool {
    use swc_core::ecma::ast::{BinaryOp, BlockStmtOrExpr, Expr as E};
    match expr {
        E::Tpl(inner) => {
            // case 2: nested template with arrow-body interpolation.
            if inner.exprs.iter().any(|e| matches!(&**e, E::Arrow(_))) {
                return true;
            }
            // Recurse into nested templates' interpolations to catch
            // deeper conditional/logical/arrow shapes.
            for e in &inner.exprs {
                if expr_contains_nested_conditional_rules(e) {
                    return true;
                }
            }
            false
        }
        E::Bin(b) if matches!(b.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing) => {
            // case 3: logical expression.
            true
        }
        E::Cond(c) => {
            // Walk both branches AND test for nested templates / arrows /
            // logicals. Mirrors upstream's `traverse(parent,
            // { ConditionalExpression })` visiting CONDITIONAL_PATHS
            // (`test` / `consequent` / `alternate`).
            expr_contains_nested_conditional_rules(&c.test)
                || expr_contains_nested_conditional_rules(&c.cons)
                || expr_contains_nested_conditional_rules(&c.alt)
        }
        E::Arrow(arrow) => {
            // Recurse into arrow body so e.g.
            // `(p) => (p.x ? \`...${q => q.y}...\` : 'red')` reaches the
            // nested template inside the consequent.
            match &*arrow.body {
                BlockStmtOrExpr::Expr(e) => expr_contains_nested_conditional_rules(e),
                BlockStmtOrExpr::BlockStmt(_) => false,
            }
        }
        E::Paren(p) => expr_contains_nested_conditional_rules(&p.expr),
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
