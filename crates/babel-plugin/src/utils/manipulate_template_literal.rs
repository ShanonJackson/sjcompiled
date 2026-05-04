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

/// Returns true when the current template literal sits inside a
/// nesting shape upstream wants to skip optimising. The walk crosses
/// the AST boundary between the template's containing node and the
/// outer ConditionalExpression / LogicalExpression — Babel uses
/// `getPathOfNode + traverse(parent, …)`. The SWC analog requires
/// the parent-traversal index from Phase 5 §5.6.
pub fn has_nested_template_literals_with_conditional_rules(
    _node: &Tpl,
    _meta: &Metadata<'_>,
) -> bool {
    unimplemented!(
        "hasNestedTemplateLiteralsWithConditionalRules requires parent-traversal — \
         Phase 5 §5.6 (utils/traverse-expression/) lands the SWC analog of \
         Babel's `getPathOfNode + traverse(parent, ...)`. Until then, callers \
         under §4.4 trip this stub."
    )
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
        let new_raw = format!("{}{}", prefix, lead.raw);
        let new_cooked = lead
            .cooked
            .as_ref()
            .map(|c| format!("{}{}", prefix, c));
        lead.raw = new_raw.into();
        lead.cooked = new_cooked.map(|c| c.into());
    }
    {
        let tail = &mut template.quasis[n - 1];
        let new_raw = format!("{}{}", tail.raw, suffix);
        let new_cooked = tail
            .cooked
            .as_ref()
            .map(|c| format!("{}{}", c, suffix));
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
                value: format!("{}{}{}", prefix, value, suffix).into(),
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

    let body_is_conditional = matches!(&*expression.body, swc_core::ecma::ast::BlockStmtOrExpr::Expr(e) if matches!(&**e, Expr::Cond(_)));

    if !(body_is_conditional && !prefix.is_empty() && next_quasi_ends_statement) {
        return;
    }

    let end_idx = end_of_statement_index.expect("checked above");
    let suffix = &next_quasi_value[..end_idx];

    // Pull the cond expr out for optimisation.
    let original_cond = match &*expression.body {
        swc_core::ecma::ast::BlockStmtOrExpr::Expr(e) => match &**e {
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
            tpl.quasis[0].cooked.as_ref().map(|c| c.to_string()),
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
