//! 1:1 port of `packages/babel-plugin/src/utils/transform-css-items.ts`.
//!
//! Three exports mirror upstream:
//!
//! * `transform_css_item` (private, recursive) — switches on
//!   `CssItem::{Conditional, Logical, Map, _}`, calling
//!   `compiled_css::transform_css` for the leaf paths and recursing
//!   into both branches of conditional items.
//! * `transform_css_items` (public) — drives the `transform_css_item`
//!   recursion across a slice and concatenates the (sheets, classNames)
//!   pair.
//! * `apply_selectors` (public) — wraps each terminal `CssItem`'s
//!   `css` field in nested `<sel>{ ... }` braces; recurses through
//!   `Conditional` branches.
//!
//! Field-name divergences:
//! * Babel `t.identifier('undefined')` → SWC
//!   `Expr::Ident(Ident::new("undefined".into(), DUMMY_SP, _))`. Note
//!   that `undefined` is an Ident in both ASTs, NOT a void expression.
//! * Babel `t.logicalExpression(op, left, right)` → SWC
//!   `Expr::Bin(BinExpr { op, left, right, .. })` with the LogicalAnd
//!   / LogicalOr / NullishCoalescing variant of `BinaryOp`. SWC unifies
//!   binary + logical ops in `BinaryOp`.
//! * Babel `t.unaryExpression('!', expr)` → SWC
//!   `Expr::Unary(UnaryExpr { op: UnaryOp::Bang, arg, .. })`.
//! * Babel `t.conditionalExpression(test, cons, alt)` → SWC
//!   `Expr::Cond(CondExpr { test, cons, alt, .. })`. Field name `alt`
//!   on SWC vs `alternate` on Babel.
//! * Babel `t.stringLiteral(s)` → SWC `Lit::Str(Str { value, raw: None, .. })`.
//!
//! Error semantics: upstream JS lets `transformCss` throw and bubble.
//! The Rust port mirrors with `expect()` — a malformed CSS string
//! reaches this function only via a fixture mistake, not a real
//! consumer path. Phase 4 §4.6+ lands the proper visitor-level error
//! channel.

use css::{transform_css, TransformOpts};
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    BinExpr, BinaryOp, CondExpr, Expr, Ident, Lit, Str, UnaryExpr, UnaryOp,
};

use crate::types::{Metadata, PluginOptions};
use crate::utils::compress_class_names_for_runtime::compress_class_names_for_runtime;
use crate::utils::css_builders::{get_item_css, logical_op_to_swc};
use crate::utils::types::CssItem;

/// Per-item return shape — mirrors upstream's
/// `{ sheets: string[]; classExpression?: t.Expression }`.
#[derive(Debug, Default)]
pub struct TransformedCssItem {
    pub sheets: Vec<String>,
    pub class_expression: Option<Box<Expr>>,
}

/// Convert babel-plugin `PluginOptions` to `css::TransformOpts`.
///
/// Upstream JS plugin passes `meta.state.opts` straight to
/// `transformCss` (duck-typed). Rust requires an explicit field
/// projection; the field set is stable and matches PARITY_VERSIONS.md
/// "AFM-pinned 0.19.0" surface (no `flattenMultipleSelectors`,
/// `sortShorthand` is NOT on `PluginOptions` so it threads as `None`).
fn plugin_opts_to_transform_opts(opts: &PluginOptions) -> TransformOpts {
    TransformOpts {
        optimize_css: opts.optimize_css,
        class_name_compression_map: opts.class_name_compression_map.clone(),
        increase_specificity: opts.increase_specificity,
        sort_at_rules: opts.sort_at_rules,
        sort_shorthand: None,
        class_hash_prefix: opts.class_hash_prefix.clone(),
        precomputed_prefixes: None,
        precomputed_prefixes_path: None,
    }
}

fn undefined_ident() -> Box<Expr> {
    Box::new(Expr::Ident(Ident::new(
        "undefined".into(),
        DUMMY_SP,
        Default::default(),
    )))
}

fn string_literal(value: &str) -> Box<Expr> {
    Box::new(Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: value.into(),
        raw: None,
    })))
}

/// Splits a single item's styles into sheets and an expression that
/// handles className logic at runtime. Mirrors upstream
/// `transformCssItem` (lines 17–95).
fn transform_css_item(item: &CssItem, meta: &mut Metadata<'_>) -> TransformedCssItem {
    match item {
        CssItem::Conditional(c) => {
            let consequent = transform_css_item(&c.consequent, meta);
            let alternate = transform_css_item(&c.alternate, meta);

            let has_consequent_sheets = !consequent.sheets.is_empty();
            let has_alternate_sheets = !alternate.sheets.is_empty();

            // Both branches empty → drop the conditional entirely.
            if !has_consequent_sheets && !has_alternate_sheets {
                return TransformedCssItem {
                    sheets: vec![],
                    class_expression: None,
                };
            }

            // Exactly one branch carries sheets → fold to
            // `<test|!test> && <classExpression || undefined>`.
            if !has_consequent_sheets || !has_alternate_sheets {
                let class_expression = if has_consequent_sheets {
                    consequent.class_expression
                } else {
                    alternate.class_expression
                };

                let left = if has_consequent_sheets {
                    c.test.clone()
                } else {
                    Box::new(Expr::Unary(UnaryExpr {
                        span: DUMMY_SP,
                        op: UnaryOp::Bang,
                        arg: c.test.clone(),
                    }))
                };

                return TransformedCssItem {
                    sheets: if has_consequent_sheets {
                        consequent.sheets
                    } else {
                        alternate.sheets
                    },
                    class_expression: Some(Box::new(Expr::Bin(BinExpr {
                        span: DUMMY_SP,
                        op: BinaryOp::LogicalAnd,
                        left,
                        right: class_expression.unwrap_or_else(undefined_ident),
                    }))),
                };
            }

            // Both branches non-empty → ternary.
            let mut sheets = consequent.sheets;
            sheets.extend(alternate.sheets);
            TransformedCssItem {
                sheets,
                class_expression: Some(Box::new(Expr::Cond(CondExpr {
                    span: DUMMY_SP,
                    test: c.test.clone(),
                    cons: consequent.class_expression.unwrap_or_else(undefined_ident),
                    alt: alternate.class_expression.unwrap_or_else(undefined_ident),
                }))),
            }
        }
        CssItem::Logical(l) => {
            let opts = plugin_opts_to_transform_opts(meta.state.opts());
            let logical_css = transform_css(&get_item_css(item), &opts)
                .unwrap_or_else(|e| panic!("transform_css failed in transform_css_item (logical): {e}"));
            let class_names = compress_class_names_for_runtime(
                logical_css.class_names,
                meta.state.opts().class_name_compression_map.as_ref(),
            )
            .join(" ");

            TransformedCssItem {
                sheets: logical_css.sheets,
                class_expression: Some(Box::new(Expr::Bin(BinExpr {
                    span: DUMMY_SP,
                    op: logical_op_to_swc(l.operator),
                    left: l.expression.clone(),
                    right: string_literal(&class_names),
                }))),
            }
        }
        CssItem::Map(m) => TransformedCssItem {
            sheets: meta.state.css_map().get(&m.name).cloned().unwrap_or_default(),
            class_expression: Some(m.expression.clone()),
        },
        // Default branch: Unconditional / Sheet — rendered atomically.
        _ => {
            let opts = plugin_opts_to_transform_opts(meta.state.opts());
            let css = transform_css(&get_item_css(item), &opts)
                .unwrap_or_else(|e| panic!("transform_css failed in transform_css_item (default): {e}"));
            let class_name = compress_class_names_for_runtime(
                css.class_names,
                meta.state.opts().class_name_compression_map.as_ref(),
            )
            .join(" ");

            TransformedCssItem {
                sheets: css.sheets,
                class_expression: if class_name.trim().is_empty() {
                    None
                } else {
                    Some(string_literal(&class_name))
                },
            }
        }
    }
}

/// `{ sheets, classNames }` — mirrors upstream's return shape for
/// `transformCssItems`.
#[derive(Debug, Default)]
pub struct TransformCssItemsResult {
    pub sheets: Vec<String>,
    pub class_names: Vec<Box<Expr>>,
}

/// Public entry. Mirrors upstream `transformCssItems` (lines 103–120).
pub fn transform_css_items(
    css_items: &[CssItem],
    meta: &mut Metadata<'_>,
) -> TransformCssItemsResult {
    let mut sheets = Vec::new();
    let mut class_names: Vec<Box<Expr>> = Vec::new();

    for item in css_items {
        let result = transform_css_item(item, meta);
        sheets.extend(result.sheets);
        if let Some(expr) = result.class_expression {
            class_names.push(expr);
        }
    }

    TransformCssItemsResult { sheets, class_names }
}

/// Wrap each terminal CssItem's `css` field in nested `<sel>{ ... }`
/// braces. Mirrors upstream `applySelectors` (lines 129–136).
///
/// JS: `${selectors.join('')}${item.css}${''.padEnd(selectors.length, '}')}`
/// → prefix is the concatenation of all selectors (no separator);
///   suffix is `'}'` repeated `selectors.length` times.
pub fn apply_selectors(item: &mut CssItem, selectors: &[String]) {
    match item {
        CssItem::Conditional(c) => {
            apply_selectors(&mut c.consequent, selectors);
            apply_selectors(&mut c.alternate, selectors);
        }
        CssItem::Unconditional(u) => apply_in_place(&mut u.css, selectors),
        CssItem::Logical(l) => apply_in_place(&mut l.css, selectors),
        CssItem::Sheet(s) => apply_in_place(&mut s.css, selectors),
        CssItem::Map(m) => apply_in_place(&mut m.css, selectors),
    }
}

fn apply_in_place(css: &mut String, selectors: &[String]) {
    let prefix: String = selectors.concat();
    let suffix: String = "}".repeat(selectors.len());
    *css = format!("{}{}{}", prefix, css, suffix);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::{Metadata, MetadataContext};
    use crate::utils::types::{
        ConditionalCssItem, CssMapItem, LogicalCssItem, LogicalOperator, SheetCssItem,
        UnconditionalCssItem,
    };

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
            in_conditional_branch: false,
        }
    }

    fn ident_expr(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            Default::default(),
        )))
    }

    fn unconditional(css: &str) -> CssItem {
        CssItem::Unconditional(UnconditionalCssItem {
            css: css.to_string(),
        })
    }

    fn conditional(test: Box<Expr>, cons: CssItem, alt: CssItem) -> CssItem {
        CssItem::Conditional(ConditionalCssItem {
            test,
            consequent: Box::new(cons),
            alternate: Box::new(alt),
        })
    }

    // ───────── transform_css_items default branch ─────────

    #[test]
    fn default_branch_runs_transform_css_and_emits_class_expression() {
        let mut state = State::default();
        let items = vec![unconditional("color: red;")];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);

        // transform_css emits one sheet for `color: red;` (atomicified).
        assert!(!result.sheets.is_empty(), "expected at least one sheet");
        assert_eq!(result.class_names.len(), 1);
        let Expr::Lit(Lit::Str(s)) = &*result.class_names[0] else {
            panic!("class_expression not StringLiteral")
        };
        let value = s.value.to_atom_lossy();
        assert!(
            value.as_str().starts_with('_'),
            "class name should start with `_`, got {:?}",
            value.as_str()
        );
    }

    #[test]
    fn default_branch_blank_css_returns_no_class_expression() {
        let mut state = State::default();
        // CSS that produces no atomic class names. JS's `''.padEnd`
        // and our `class_name.trim().is_empty()` short-circuit wraps
        // this case to None.
        let items = vec![unconditional("/* nothing */")];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert!(
            result.class_names.is_empty(),
            "expected no class expressions for empty-output CSS"
        );
    }

    // ───────── transform_css_items conditional branch ─────────

    #[test]
    fn conditional_both_empty_drops_to_no_op() {
        let mut state = State::default();
        let items = vec![conditional(
            ident_expr("flag"),
            unconditional("/* nothing */"),
            unconditional("/* nothing */"),
        )];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert!(result.sheets.is_empty());
        assert!(result.class_names.is_empty());
    }

    #[test]
    fn conditional_only_consequent_emits_test_and_classname() {
        let mut state = State::default();
        let items = vec![conditional(
            ident_expr("flag"),
            unconditional("color: red;"),
            unconditional("/* nothing */"),
        )];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert_eq!(result.class_names.len(), 1);
        let Expr::Bin(b) = &*result.class_names[0] else {
            panic!("class_expression not BinExpr")
        };
        assert!(matches!(b.op, BinaryOp::LogicalAnd));
        // left is `flag`, no UnaryExpr wrapper.
        assert!(matches!(&*b.left, Expr::Ident(i) if i.sym.as_str() == "flag"));
    }

    #[test]
    fn conditional_only_alternate_emits_negated_test() {
        let mut state = State::default();
        let items = vec![conditional(
            ident_expr("flag"),
            unconditional("/* nothing */"),
            unconditional("color: blue;"),
        )];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert_eq!(result.class_names.len(), 1);
        let Expr::Bin(b) = &*result.class_names[0] else {
            panic!("class_expression not BinExpr")
        };
        assert!(matches!(b.op, BinaryOp::LogicalAnd));
        // left is `!flag`.
        let Expr::Unary(u) = &*b.left else {
            panic!("left not Unary")
        };
        assert!(matches!(u.op, UnaryOp::Bang));
        assert!(matches!(&*u.arg, Expr::Ident(i) if i.sym.as_str() == "flag"));
    }

    #[test]
    fn conditional_both_present_emits_ternary() {
        let mut state = State::default();
        let items = vec![conditional(
            ident_expr("flag"),
            unconditional("color: red;"),
            unconditional("color: blue;"),
        )];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert_eq!(result.class_names.len(), 1);
        assert!(
            matches!(&*result.class_names[0], Expr::Cond(_)),
            "expected ConditionalExpression"
        );
        // Both branches carry sheets — combined.
        assert!(result.sheets.len() >= 2);
    }

    // ───────── Logical branch ─────────

    #[test]
    fn logical_branch_emits_logical_expression_with_join() {
        let mut state = State::default();
        let items = vec![CssItem::Logical(LogicalCssItem {
            expression: ident_expr("isPrimary"),
            operator: LogicalOperator::And,
            css: "color: red;".to_string(),
        })];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert_eq!(result.class_names.len(), 1);
        let Expr::Bin(b) = &*result.class_names[0] else {
            panic!("class_expression not BinExpr")
        };
        assert!(matches!(b.op, BinaryOp::LogicalAnd));
        // right is the joined classnames StringLiteral.
        let Expr::Lit(Lit::Str(s)) = &*b.right else {
            panic!("right not Str")
        };
        assert!(s.value.to_atom_lossy().as_str().starts_with('_'));
    }

    // ───────── Map branch ─────────

    #[test]
    fn map_branch_pulls_sheets_from_state_css_map() {
        let mut state = State::default();
        // Direct pub(crate) field write — same-crate test, doesn't need
        // to route through MutationRecorder for fixture setup.
        state
            .css_map
            .insert("variants".to_string(), vec!["._sheet1{color:red}".to_string()]);
        let items = vec![CssItem::Map(CssMapItem {
            name: "variants".to_string(),
            expression: ident_expr("variants"),
            css: String::new(),
        })];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert_eq!(result.sheets, vec!["._sheet1{color:red}".to_string()]);
        assert_eq!(result.class_names.len(), 1);
        assert!(matches!(&*result.class_names[0], Expr::Ident(i) if i.sym.as_str() == "variants"));
    }

    #[test]
    fn map_branch_missing_name_yields_empty_sheets() {
        let mut state = State::default();
        let items = vec![CssItem::Map(CssMapItem {
            name: "absent".to_string(),
            expression: ident_expr("absent"),
            css: String::new(),
        })];
        let mut meta = fresh_meta(&mut state);
        let result = transform_css_items(&items, &mut meta);
        assert!(result.sheets.is_empty());
    }

    // ───────── apply_selectors ─────────

    #[test]
    fn apply_selectors_zero_selectors_is_identity() {
        let mut item = unconditional("color: red;");
        apply_selectors(&mut item, &[]);
        let CssItem::Unconditional(u) = &item else {
            panic!()
        };
        assert_eq!(u.css, "color: red;");
    }

    #[test]
    fn apply_selectors_single_wraps_in_one_brace_pair() {
        let mut item = unconditional("color: red;");
        apply_selectors(&mut item, &[":hover{".to_string()]);
        let CssItem::Unconditional(u) = &item else {
            panic!()
        };
        assert_eq!(u.css, ":hover{color: red;}");
    }

    #[test]
    fn apply_selectors_multiple_concatenates_prefix_and_pads_suffix() {
        let mut item = unconditional("color: red;");
        apply_selectors(
            &mut item,
            &[":hover{".to_string(), "& > span{".to_string()],
        );
        let CssItem::Unconditional(u) = &item else {
            panic!()
        };
        assert_eq!(u.css, ":hover{& > span{color: red;}}");
    }

    #[test]
    fn apply_selectors_recurses_into_conditional_branches() {
        let mut item = conditional(
            ident_expr("flag"),
            unconditional("color: red;"),
            CssItem::Sheet(SheetCssItem {
                css: "color: blue;".to_string(),
            }),
        );
        apply_selectors(&mut item, &[":hover{".to_string()]);
        let CssItem::Conditional(c) = &item else {
            panic!()
        };
        let CssItem::Unconditional(u) = &*c.consequent else {
            panic!()
        };
        assert_eq!(u.css, ":hover{color: red;}");
        let CssItem::Sheet(s) = &*c.alternate else {
            panic!()
        };
        assert_eq!(s.css, ":hover{color: blue;}");
    }
}
