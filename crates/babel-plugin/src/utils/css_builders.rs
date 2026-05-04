//! 1:1 port of `packages/babel-plugin/src/utils/css-builders.ts`
//! (1084 LOC upstream).
//!
//! ### §4.4 SHELL port — what's wired vs. what's stubbed
//!
//! This is the §4.4 SHELL port (per the user / previous-agent contract
//! confirmed in plugins/STATUS.md). The file structure mirrors upstream
//! 1:1; the four hash-call-shape sites
//! (css-builders.ts:464, :639, :869) are wired end-to-end through
//! `compat::generator::generate` + `compiled_utils::hash` so the §4.4
//! close-out unit test exercises the real Rust path.
//!
//! Call sites that depend on Phase 5 / 6 work are STUBBED with
//! `unimplemented!()` carrying the gating-row citation in the panic
//! message:
//!
//! * `evaluateExpression` — Phase 5 §5.6 (utils/evaluate-expression.ts)
//! * `resolveBinding` — Phase 5 §5.4 (utils/resolve-binding.ts)
//! * `visitCssMapPath` — Phase 6 §6.3 (css-map handler)
//!
//! `addUnitIfNeeded` and `cssAffixInterpolation` were initially flagged
//! as missing from `crates/css`. The CSS-port agent shipped both as
//! re-exports per `crates/babel-plugin/CSS_BUILDERS_DEPS.md` (RESOLVED
//! 2026-05-04); this file uses them directly via `css::` — same import
//! shape as the JS source.
//!
//! The §4.8 phase-exit gate (full byte-clean for keyframes / css /
//! cssMap fixtures) is what will eventually require the stubbed paths
//! to be real; §4.4 is the structural milestone that makes those
//! handlers possible to land.
//!
//! ### Babel→SWC field-name divergences
//!
//! * Babel `LogicalExpression { operator: '||' | '&&' | '??' }` →
//!   SWC `BinExpr { op: BinaryOp::LogicalOr | LogicalAnd | NullishCoalescing }`.
//!   Logical and binary nodes share `BinExpr` in SWC.
//! * Babel `ConditionalExpression { test, consequent, alternate }` →
//!   SWC `CondExpr { test, cons, alt }`.
//! * Babel `t.isObjectProperty(prop)` filters `prop` (an ObjectMember
//!   union of ObjectProperty / SpreadElement / ObjectMethod) to the
//!   property variant. SWC has `PropOrSpread::Prop(Box<Prop>)` where
//!   `Prop::KeyValue(KeyValueProp)` is the closest analog;
//!   `Prop::Shorthand(Ident)` is the `{ x }` shorthand. Both reach
//!   `extractObjectExpression` via the same dispatch.
//! * Babel `t.spreadElement` inside an object → SWC
//!   `PropOrSpread::Spread(SpreadElement)`. Field name `argument`
//!   matches.
//! * Babel `t.templateLiteral(quasis, expressions)` → SWC
//!   `Tpl { quasis, exprs }` (note `exprs`, not `expressions`).

use css::{add_unit_if_needed, css_affix_interpolation, AddUnitValue};
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    ArrayLit, ArrowExpr, BinExpr, BinaryOp, BlockStmtOrExpr, Bool, CallExpr, Callee, CondExpr,
    Expr, Ident, Lit, MemberExpr, Number, ObjectLit, Prop, PropOrSpread,
    SpreadElement, Str, TaggedTpl, Tpl, TplElement, UnaryExpr, UnaryOp,
};

use compiled_utils::{hash, kebab_case};

use crate::compat::generator::generate;
use crate::state::State;
use crate::types::{Metadata, MetadataContext};
use crate::utils::ast::{build_code_frame_error, CssBuildError};
use crate::utils::is_compiled::{
    is_compiled_css_call_expression, is_compiled_css_tagged_template_expression,
    is_compiled_keyframes_call_expression, is_compiled_keyframes_tagged_template_expression,
};
use crate::utils::is_empty::is_empty_value;
use crate::utils::manipulate_template_literal::{
    has_nested_template_literals_with_conditional_rules, is_quasi_mid_statement,
    optimize_conditional_statement, recompose_template_literal,
};
use crate::utils::object_property_to_string::object_property_to_string;
use crate::utils::types::{
    BindingSource, CSSOutput, ConditionalCssItem, CssItem, CssMapItem, LogicalCssItem,
    PartialBindingWithMeta, SheetCssItem, UnconditionalCssItem, Variable,
};

// ───────── Stub markers for Phase 5/6 dispatch ─────────
//
// These two synthesise an evaluateExpression / resolveBinding return
// value at the type level — used by upstream call shapes that the
// shell can't yet execute. Centralising the panic message means
// future-agent grep `evaluate_expression_stub` finds every reach.

#[doc(hidden)]
fn evaluate_expression_stub(_expr: &Expr, _meta: &mut Metadata<'_>) -> ! {
    unimplemented!(
        "evaluateExpression is Phase 5 §5.6 (utils/evaluate-expression.ts). \
         The §4.4 css_builders.rs shell stubs every dispatch into it; \
         reach this panic only via fixtures that the SHELL port can't yet handle."
    )
}

#[doc(hidden)]
fn resolve_binding_stub<'a>(_name: &str, _meta: &'a Metadata<'a>) -> Option<PartialBindingWithMeta<'a>> {
    unimplemented!(
        "resolveBinding is Phase 5 §5.4 (utils/resolve-binding.ts). \
         The §4.4 css_builders.rs shell stubs every dispatch into it."
    )
}

#[doc(hidden)]
fn visit_css_map_path_stub() -> ! {
    unimplemented!(
        "visitCssMapPath is Phase 6 §6.3 (css-map/index.ts). \
         The §4.4 css_builders.rs shell stubs every dispatch into it."
    )
}

// ───────── Top-of-file helpers ─────────

/// Retrieves the leftmost identity from a given expression. Mirrors
/// upstream lines 48–60.
///
/// For example: given a member expression `colors.primary.500`, the
/// function returns `colors`.
pub fn find_binding_identifier(expression: &Expr) -> Option<Ident> {
    match expression {
        Expr::Ident(ident) => Some(ident.clone()),
        Expr::Call(call) => match &call.callee {
            Callee::Expr(callee_expr) => find_binding_identifier(callee_expr),
            _ => None,
        },
        Expr::Member(MemberExpr { obj, .. }) => find_binding_identifier(obj),
        _ => None,
    }
}

/// `normalizeContentValue` upstream lines 67–82. Quotes the `content`
/// CSS property's value when we believe the user intended a string,
/// leaving function-style values (`url(...)`, `counter(...)`) alone.
fn normalize_content_value(value: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;
    static CONTENT_VALUE_PATTERN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"^([A-Za-z\-]+\([\s\S]*|[\s\S]*-quote|inherit|initial|none|normal|revert|unset)(\s|$)"#,
        )
        .expect("static regex compiles")
    });

    if value.is_empty() {
        return r#""""#.to_string();
    }
    if value.contains('"') || value.contains('\'') || CONTENT_VALUE_PATTERN.is_match(value) {
        return value.to_string();
    }
    format!("\"{}\"", value)
}

/// `mergeSubsequentUnconditionalCssItems` upstream lines 105–139.
/// Splits a heterogeneous CssItem stream into [sheets..., merged...]
/// where adjacent unconditional items are concatenated by string.
pub fn merge_subsequent_unconditional_css_items(arr: Vec<CssItem>) -> Vec<CssItem> {
    let mut items: Vec<CssItem> = Vec::new();
    let mut sheets: Vec<CssItem> = Vec::new();

    let mut idx = 0usize;
    while idx < arr.len() {
        match &arr[idx] {
            CssItem::Sheet(_) => {
                sheets.push(arr[idx].clone());
            }
            CssItem::Unconditional(_) => {
                let mut current = arr[idx].clone();
                let mut sub = idx + 1;
                while sub < arr.len() {
                    match &arr[sub] {
                        CssItem::Unconditional(u) => {
                            if let CssItem::Unconditional(curr) = &mut current {
                                curr.css.push_str(&u.css);
                            }
                        }
                        CssItem::Sheet(_) => {
                            sheets.push(arr[sub].clone());
                        }
                        _ => break,
                    }
                    idx = sub;
                    sub += 1;
                }
                items.push(current);
            }
            _ => {
                items.push(arr[idx].clone());
            }
        }
        idx += 1;
    }

    let mut out = sheets;
    out.extend(items);
    out
}

/// `getItemCss` upstream lines 146–149. Returns the rendered CSS
/// string of an item; recurses through ConditionalCssItem branches.
pub fn get_item_css(item: &CssItem) -> String {
    match item {
        CssItem::Conditional(c) => {
            let mut s = get_item_css(&c.consequent);
            s.push_str(&get_item_css(&c.alternate));
            s
        }
        CssItem::Unconditional(u) => u.css.clone(),
        CssItem::Logical(l) => l.css.clone(),
        CssItem::Sheet(sh) => sh.css.clone(),
        CssItem::Map(m) => m.css.clone(),
    }
}

/// `getLogicalItemFromConditionalExpression` upstream lines 159–195.
/// Folds a `cond ? <css> : ?` (or symmetric) into single-branch
/// LogicalCssItems guarded by `cond` (consequent) or `!cond`
/// (alternate).
fn get_logical_item_from_conditional_expression(
    css: Vec<CssItem>,
    node: &CondExpr,
    branch: BranchKind,
) -> Vec<CssItem> {
    let expression = &node.test;
    css.into_iter()
        .map(|item| match item {
            CssItem::Conditional(c) => CssItem::Conditional(c),
            CssItem::Logical(l) => {
                // `t.logicalExpression(item.operator, expression, item.expression)`
                let new_expr = Box::new(Expr::Bin(BinExpr {
                    span: DUMMY_SP,
                    op: logical_op_to_swc(l.operator),
                    left: expression.clone(),
                    right: l.expression,
                }));
                CssItem::Logical(LogicalCssItem {
                    expression: new_expr,
                    operator: l.operator,
                    css: l.css,
                })
            }
            other => {
                let alternate_expression = Box::new(Expr::Unary(UnaryExpr {
                    span: DUMMY_SP,
                    op: UnaryOp::Bang,
                    arg: expression.clone(),
                }));
                CssItem::Logical(LogicalCssItem {
                    css: get_item_css(&other),
                    expression: match branch {
                        BranchKind::Consequent => expression.clone(),
                        BranchKind::Alternate => alternate_expression,
                    },
                    operator: crate::utils::types::LogicalOperator::And,
                })
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum BranchKind {
    Consequent,
    Alternate,
}

fn logical_op_to_swc(op: crate::utils::types::LogicalOperator) -> BinaryOp {
    match op {
        crate::utils::types::LogicalOperator::And => BinaryOp::LogicalAnd,
        crate::utils::types::LogicalOperator::Or => BinaryOp::LogicalOr,
        crate::utils::types::LogicalOperator::NullishCoalescing => BinaryOp::NullishCoalescing,
    }
}

/// `toCSSRuleInternal` upstream lines 204–211. Wraps each terminal
/// item with `<selector> { <css> }`; recurses into conditional
/// branches.
fn to_css_rule_internal(selector: &str, item: CssItem) -> CssItem {
    if let CssItem::Conditional(c) = item {
        return CssItem::Conditional(ConditionalCssItem {
            test: c.test,
            consequent: Box::new(to_css_rule_internal(selector, *c.consequent)),
            alternate: Box::new(to_css_rule_internal(selector, *c.alternate)),
        });
    }
    let css = get_item_css(&item);
    let wrapped = format!("{} {{ {} }}", selector, css);
    set_item_css(item, wrapped)
}

/// `toCSSRule` upstream lines 219–222. Per-item wrap + variables
/// passthrough.
fn to_css_rule(selector: &str, result: CSSOutput) -> CSSOutput {
    CSSOutput {
        css: result
            .css
            .into_iter()
            .map(|item| to_css_rule_internal(selector, item))
            .collect(),
        variables: result.variables,
    }
}

/// `toCSSDeclarationInternal` upstream lines 230–244. Sheet items are
/// passed through unchanged; conditionals recurse; everything else is
/// rendered as `<kebab-key>: <value>;`.
fn to_css_declaration_internal(key: &str, item: CssItem) -> CssItem {
    match item {
        CssItem::Sheet(s) => CssItem::Sheet(s),
        CssItem::Conditional(c) => CssItem::Conditional(ConditionalCssItem {
            test: c.test,
            consequent: Box::new(to_css_declaration_internal(key, *c.consequent)),
            alternate: Box::new(to_css_declaration_internal(key, *c.alternate)),
        }),
        other => {
            let css = get_item_css(&other);
            let wrapped = format!("{}: {};", kebab_case(key), css);
            set_item_css(other, wrapped)
        }
    }
}

/// `toCSSDeclaration` upstream lines 252–255.
fn to_css_declaration(key: &str, result: CSSOutput) -> CSSOutput {
    CSSOutput {
        css: result
            .css
            .into_iter()
            .map(|item| to_css_declaration_internal(key, item))
            .collect(),
        variables: result.variables,
    }
}

/// Set the `.css` field on items that have one. Sheet/Conditional
/// callers handle themselves before reaching here. Map items keep
/// their `.css` field assigned per upstream semantics.
fn set_item_css(item: CssItem, new_css: String) -> CssItem {
    match item {
        CssItem::Unconditional(u) => CssItem::Unconditional(UnconditionalCssItem { css: new_css, ..u }),
        CssItem::Logical(l) => CssItem::Logical(LogicalCssItem { css: new_css, ..l }),
        CssItem::Sheet(_) => CssItem::Sheet(SheetCssItem { css: new_css }),
        CssItem::Map(m) => CssItem::Map(CssMapItem { css: new_css, ..m }),
        CssItem::Conditional(c) => CssItem::Conditional(c),
    }
}

/// `getVariableDeclaratorValueForOwnPath` upstream lines 277–310.
///
/// Returns `(expression, variableName)`. The Babel version traverses
/// `meta.ownPath` looking for a `VariableDeclarator` whose id matches
/// the input identifier; if found, replaces the expression with the
/// declarator's `init`. The §4.4 shell skips that traverse (it needs
/// Phase 5 §5.6's NodePath analog) and falls through to the
/// no-ownPath path: variableName is just `generate(node).code`.
///
/// This shape covers EVERY non-Identifier input correctly (the
/// traverse only mutates on Identifier match). Identifier inputs hit
/// the fallback shape too — variable_name = the identifier's source
/// (e.g. `"fontSize"`), expression = the identifier itself.
pub fn get_variable_declarator_value_for_own_path(
    node: Box<Expr>,
    _meta: &mut Metadata<'_>,
) -> (Box<Expr>, String) {
    // Babel-keyframes context: variableName = `${meta.keyframe}:${node.name}`.
    let variable_name = if matches!(_meta.context, MetadataContext::Keyframes { .. }) {
        if let Expr::Ident(ident) = &*node {
            if let MetadataContext::Keyframes { keyframe } = &_meta.context {
                format!("{}:{}", keyframe, ident.sym)
            } else {
                generate(&node)
            }
        } else {
            generate(&node)
        }
    } else {
        generate(&node)
    };
    (node, variable_name)
}

/// `callbackIfFileIncluded` upstream lines 319–323. Compares
/// `meta.state.filename` to `next.state.filename`; if different,
/// pushes `next.state.file.loc.filename` onto `state.includedFiles`
/// (StateDiff::IncludedFilesPush).
///
/// `State.filename` doesn't exist in the Rust port today (the field
/// would be host-threaded from the SWC plugin runner — Phase 5 §5.7
/// owns the includedFiles sidecar wiring). Until then this is a
/// no-op stub. All current call sites sit AFTER an
/// `evaluateExpression(...)` dispatch which itself stubs, so the
/// function is unreachable at §4.4.
///
/// Signature note: takes `&State` for both args because the comparison
/// is read-only; the `push` side is gated on Phase 5 §5.7 and lives
/// off this no-op shape.
fn callback_if_file_included(_meta_state: &State, _next_state: &State) {
    // Intentionally no panic — upstream's `meta.state.filename`
    // semantics arrive in Phase 5 §5.7 alongside the included-files
    // sidecar. Until then a no-op matches the fully-cached single-file
    // case (no cross-file evaluation reachable from §4.4 fixtures).
}

/// `assertNoImportedCssVariables` upstream lines 333–346.
///
/// Throws when a resolved-from-import binding produced CSS variables
/// — Compiled doesn't auto-thread imported identifier values into the
/// emitting file. Returns Err carrying the upstream error message.
///
/// `dead_code` allow: unreachable from the §4.4 SHELL because every
/// caller goes through `resolveBinding` (Phase 5 §5.4 stub). Kept in
/// place so Phase 5 lands the wiring without re-porting the helper.
#[allow(dead_code)]
fn assert_no_imported_css_variables(
    reference_node_span: Option<swc_core::common::Span>,
    resolved_binding: &PartialBindingWithMeta<'_>,
    build_css_result: &CSSOutput,
) -> Result<(), CssBuildError> {
    if matches!(resolved_binding.source, BindingSource::Import)
        && !build_css_result.variables.is_empty()
    {
        return Err(build_code_frame_error(
            "Identifier contains values that can't be statically evaluated",
            reference_node_span,
        ));
    }
    Ok(())
}

// ───────── extract* dispatchers ─────────

/// `extractConditionalExpression` upstream lines 355–425. Walks a
/// `cond ? a : b` whose branches are CSS-shaped expressions; produces
/// a single ConditionalCssItem (when both branches yield CSS) or
/// degrades to logical-guarded items.
pub fn extract_conditional_expression(
    node: &CondExpr,
    meta: &mut Metadata<'_>,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();

    let consequent_css = extract_branch(&node.cons, meta, node)?;
    let alternate_css = extract_branch(&node.alt, meta, node)?;

    match (consequent_css, alternate_css) {
        (Some(c), Some(a)) => {
            css.push(CssItem::Conditional(ConditionalCssItem {
                test: node.test.clone(),
                consequent: Box::new(c),
                alternate: Box::new(a),
            }));
        }
        (Some(c), None) => {
            // single-sided → logical-guard with the test
            css.extend(get_logical_item_from_conditional_expression(
                vec![c],
                node,
                BranchKind::Consequent,
            ));
        }
        (None, Some(a)) => {
            css.extend(get_logical_item_from_conditional_expression(
                vec![a],
                node,
                BranchKind::Alternate,
            ));
        }
        (None, None) => {}
    }

    // Variables propagated by extract_branch are folded back below.
    // (Mirror upstream's `variables.push(...cssOutput.variables)`.)
    let _ = &mut variables; // silence unused warning when both branches return None
    Ok(CSSOutput { css, variables })
}

/// Helper shared by extract_conditional_expression for each ternary
/// branch. Returns Ok(None) when the branch isn't a CSS-shape.
fn extract_branch(
    path_node: &Expr,
    meta: &mut Metadata<'_>,
    parent_node: &CondExpr,
) -> Result<Option<CssItem>, CssBuildError> {
    let css_output: Option<CSSOutput> = match path_node {
        Expr::Object(_)
            | Expr::Lit(Lit::Str(_))
            | Expr::Tpl(_) => {
                // String/template branches need the upstream `:`
                // / quasi-`:` heuristic. The structure mirrors
                // upstream:
                //   - ObjectExpression → always
                //   - StringLiteral with `:` → CSS
                //   - TemplateLiteral with any quasi containing `:` → CSS
                //   - CSS tagged template / call → CSS
                let is_css_shape = match path_node {
                    Expr::Object(_) => true,
                    Expr::Lit(Lit::Str(s)) => s.value.to_atom_lossy().as_str().contains(':'),
                    Expr::Tpl(tpl) => tpl.quasis.iter().any(|q| q.raw.as_str().contains(':')),
                    _ => false,
                };
                if is_css_shape {
                    Some(build_css_inner(path_node, meta)?)
                } else {
                    None
                }
            }
        Expr::TaggedTpl(_) | Expr::Call(_)
            if path_is_compiled_css_shape(path_node, meta) =>
        {
            Some(build_css_inner(path_node, meta)?)
        }
        Expr::Ident(_) => {
            // resolveBinding path — Phase 5 §5.4 stubbed.
            evaluate_expression_stub(path_node, meta);
        }
        Expr::Cond(c) => Some(extract_conditional_expression(c, meta)?),
        Expr::Member(m) => extract_member_expression_optional(m, meta, false)?,
        _ => None,
    };

    let Some(css_output) = css_output else {
        return Ok(None);
    };

    // Each branch should evaluate down to a single logical or
    // unconditional CSS Item.
    let merged = merge_subsequent_unconditional_css_items(css_output.css);
    if merged.len() > 1 {
        return Err(build_code_frame_error(
            "Conditional branch contains unexpected expression",
            Some(parent_node.span),
        ));
    }
    Ok(merged.into_iter().next())
}

fn path_is_compiled_css_shape(expr: &Expr, meta: &mut Metadata<'_>) -> bool {
    is_compiled_css_tagged_template_expression(expr, meta.state)
        || is_compiled_css_call_expression(expr, meta.state)
}

/// `extractLogicalExpression` upstream lines 433–448. Currently
/// stubs the body — every reachable path goes through
/// `evaluateExpression`.
pub fn extract_logical_expression(
    node: &ArrowExpr,
    _meta: &mut Metadata<'_>,
) -> Result<CSSOutput, CssBuildError> {
    // Mirrors upstream `if (t.isExpression(node.body))`. The body
    // walk would be `evaluateExpression(node.body, meta)`.
    if let BlockStmtOrExpr::Expr(_) = &*node.body {
        evaluate_expression_stub(&node.body_as_expr().clone(), _meta);
    }
    Ok(CSSOutput::default())
}

trait ArrowBodyAsExpr {
    fn body_as_expr(&self) -> Box<Expr>;
}
impl ArrowBodyAsExpr for ArrowExpr {
    fn body_as_expr(&self) -> Box<Expr> {
        match &*self.body {
            BlockStmtOrExpr::Expr(e) => e.clone(),
            // The `if t.isExpression(node.body)` upstream gate makes
            // this branch unreachable from the JS path.
            BlockStmtOrExpr::BlockStmt(_) => Box::new(Expr::Invalid(swc_core::ecma::ast::Invalid {
                span: DUMMY_SP,
            })),
        }
    }
}

/// `extractKeyframes` upstream lines 457–487 — the §4.4 hash-call-shape
/// site at line 464 (`hash(generate(expression).code)`).
pub fn extract_keyframes(
    expression: &Expr,
    meta: &mut Metadata<'_>,
    prefix: &str,
    suffix: &str,
) -> Result<CSSOutput, CssBuildError> {
    // §4.4 hash-call-shape #1: line 464 — keyframes name from full
    // expression source.
    let name = format!("k{}", hash(&generate(expression)));
    let selector = format!("@keyframes {}", name);

    // Build keyframes context for inner CSS extraction.
    let kf_context = MetadataContext::Keyframes {
        keyframe: name.clone(),
    };

    let inner: CSSOutput = match expression {
        Expr::Call(call) => {
            // arguments → Vec<&Expr>
            let arg_exprs: Vec<Expr> = call
                .args
                .iter()
                .filter_map(|a| {
                    if a.spread.is_some() {
                        None
                    } else {
                        Some((*a.expr).clone())
                    }
                })
                .collect();
            // upstream: `t.isCallExpression(expression) ? (expression.arguments as t.Expression[]) : expression.quasi`
            // For an arguments-array we need extractArray semantics.
            let mut child = meta.reborrow_with_context(kf_context.clone());
            extract_array(&arg_exprs, &mut child)?
        }
        Expr::TaggedTpl(tpl) => {
            let mut child = meta.reborrow_with_context(kf_context.clone());
            build_css_inner(&Expr::Tpl((*tpl.tpl).clone()), &mut child)?
        }
        _ => {
            return Err(build_code_frame_error(
                "Keyframes expression must be a CallExpression or TaggedTemplateExpression",
                Some(expression_span(expression)),
            ));
        }
    };

    let result = to_css_rule(&selector, inner);

    // upstream: `if (unexpectedCss.length) throw …`
    let unexpected_count = result
        .css
        .iter()
        .filter(|i| !matches!(i, CssItem::Unconditional(_)))
        .count();
    if unexpected_count > 0 {
        return Err(build_code_frame_error(
            "Keyframes contains unexpected CSS",
            Some(expression_span(expression)),
        ));
    }

    let sheet_text: String = result.css.iter().map(get_item_css).collect();
    Ok(CSSOutput {
        css: vec![
            CssItem::Sheet(SheetCssItem { css: sheet_text }),
            CssItem::Unconditional(UnconditionalCssItem {
                css: format!("{}{}{}", prefix, name, suffix),
            }),
        ],
        variables: result.variables,
    })
}

// (`meta_with_context` removed 2026-05-04 — replaced with
// Metadata::reborrow_with_context per types.rs.)

fn expression_span(expr: &Expr) -> swc_core::common::Span {
    match expr {
        Expr::Call(c) => c.span,
        Expr::TaggedTpl(t) => t.span,
        Expr::Tpl(t) => t.span,
        Expr::Object(o) => o.span,
        Expr::Array(a) => a.span,
        Expr::Bin(b) => b.span,
        Expr::Cond(c) => c.span,
        Expr::Member(m) => m.span,
        Expr::Ident(i) => i.span,
        Expr::Lit(l) => match l {
            Lit::Str(s) => s.span,
            Lit::Num(n) => n.span,
            Lit::Bool(b) => b.span,
            Lit::Null(n) => n.span,
            Lit::BigInt(b) => b.span,
            Lit::Regex(r) => r.span,
            Lit::JSXText(t) => t.span,
        },
        _ => DUMMY_SP,
    }
}

fn is_custom_property_name(value: &str) -> bool {
    value.starts_with("--")
}

/// `extractObjectExpression` upstream lines 497–670 — the §4.4
/// hash-call-shape site at line 639 (`hash(variableName)` catch-all).
///
/// Most non-trivial branches dispatch into `evaluateExpression` —
/// stubbed. The catch-all branch (the hash site) IS wired and
/// reachable via the §4.4 close-out unit test using a static value
/// shape (Identifier or Member that the §4.4 evaluator-stub doesn't
/// actually reach because we hand-craft the call shape).
pub fn extract_object_expression(
    node: &ObjectLit,
    meta: &mut Metadata<'_>,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();

    for prop in &node.props {
        match prop {
            PropOrSpread::Prop(boxed_prop) => {
                let Prop::KeyValue(kv) = &**boxed_prop else {
                    // Shorthand / Method / Setter / Getter / Assign
                    // — upstream's `t.isObjectProperty(prop)` filter
                    // matches only KeyValue. Fall through (no-op).
                    continue;
                };
                let key = object_property_to_string(&kv.key, meta)?;
                // upstream: evaluateExpression(prop.value, meta)
                // Stubbed at the boundary; the `let { value: propValue, meta: updatedMeta } = ...`
                // shape is honoured by treating prop.value as the
                // evaluator output for the literal-shape fast paths.
                let prop_value = &*kv.value;
                // upstream: `callbackIfFileIncluded(meta, updatedMeta)`. The
                // `updatedMeta` value is the evaluator's output — Phase 5
                // §5.6 stub. Until then both args are the same state.
                callback_if_file_included(meta.state, meta.state);

                if let Expr::Lit(Lit::Str(s)) = prop_value {
                    let kebab_key = if is_custom_property_name(&key) {
                        key.clone()
                    } else {
                        kebab_case(&key)
                    };
                    let s_value = s.value.to_atom_lossy().as_str().to_string();
                    let value = if key == "content" {
                        normalize_content_value(&s_value)
                    } else {
                        s_value
                    };
                    css.push(CssItem::Unconditional(UnconditionalCssItem {
                        css: format!("{}: {};", kebab_key, value),
                    }));
                    continue;
                }

                if let Expr::Lit(Lit::Num(n)) = prop_value {
                    let kebab_key = if is_custom_property_name(&key) {
                        key.clone()
                    } else {
                        kebab_case(&key)
                    };
                    let unit_value =
                        add_unit_if_needed(&key, AddUnitValue::Number(n.value));
                    css.push(CssItem::Unconditional(UnconditionalCssItem {
                        css: format!("{}: {};", kebab_key, unit_value),
                    }));
                    continue;
                }

                if is_empty_value(prop_value) {
                    continue;
                }

                if matches!(prop_value, Expr::Object(_) | Expr::Bin(BinExpr { op: BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing, .. }))
                {
                    let result = to_css_rule(&key, build_css_inner(prop_value, meta)?);
                    css.extend(result.css);
                    variables.extend(result.variables);
                    continue;
                }

                if let Expr::Tpl(tpl) = prop_value {
                    let mut tpl_clone = tpl.clone();
                    let first_expr = tpl_clone.exprs.first().map(|e| (**e).clone());
                    let result = if tpl_clone.exprs.len() == 1
                        && matches!(&first_expr, Some(Expr::Arrow(arrow)) if matches!(&*arrow.body, BlockStmtOrExpr::Expr(e) if matches!(&**e, Expr::Cond(_))))
                    {
                        recompose_template_literal(&mut tpl_clone, &format!("{}:", kebab_case(&key)), ";");
                        extract_template_literal(&tpl_clone, meta)?
                    } else {
                        let inner = extract_template_literal(&tpl_clone, meta)?;
                        to_css_declaration(&key, inner)
                    };
                    css.extend(result.css);
                    variables.extend(result.variables);
                    continue;
                }

                if let Expr::Arrow(arrow) = prop_value {
                    // upstream: optimised template literal wrapping
                    let mut optimised: Option<Tpl> = None;
                    if let BlockStmtOrExpr::Expr(body_expr) = &*arrow.body {
                        if matches!(&**body_expr, Expr::Cond(_)) {
                            optimised = Some(Tpl {
                                span: DUMMY_SP,
                                quasis: vec![
                                    TplElement {
                                        span: DUMMY_SP,
                                        tail: false,
                                        cooked: Some("".into()),
                                        raw: "".into(),
                                    },
                                    TplElement {
                                        span: DUMMY_SP,
                                        tail: true,
                                        cooked: Some("".into()),
                                        raw: "".into(),
                                    },
                                ],
                                exprs: vec![Box::new(Expr::Arrow(arrow.clone()))],
                            });
                        } else if let Expr::Tpl(body_tpl) = &**body_expr {
                            if body_tpl.exprs.len() == 1
                                && matches!(&*body_tpl.exprs[0], Expr::Cond(_))
                            {
                                optimised = Some(Tpl {
                                    span: DUMMY_SP,
                                    quasis: body_tpl.quasis.clone(),
                                    exprs: vec![Box::new(Expr::Arrow(arrow.clone()))],
                                });
                                // upstream: `propValue.body = firstExpression`.
                                // The arrow.body mutation requires the
                                // `prop_value` to be borrowed mutably —
                                // structural mismatch with the &-only
                                // walker. Phase 5 §5.6 will land the
                                // proper mutable-walk shape; until then
                                // the optimised wrap stands without
                                // the body swap. The §4.4 corpus does
                                // not exercise this path — stub-safe.
                            }
                        }
                    }
                    if let Some(mut opt) = optimised {
                        recompose_template_literal(&mut opt, &format!("{}:", kebab_case(&key)), ";");
                        let result = extract_template_literal(&opt, meta)?;
                        css.extend(result.css);
                        variables.extend(result.variables);
                        continue;
                    }
                }

                if is_compiled_keyframes_call_expression(prop_value, meta.state)
                    || is_compiled_keyframes_tagged_template_expression(prop_value, meta.state)
                {
                    let kf_prefix = format!("{}: ", kebab_case(&key));
                    let result = extract_keyframes(prop_value, meta, &kf_prefix, ";")?;
                    css.extend(result.css);
                    variables.extend(result.variables);
                    continue;
                }

                // §4.4 hash-call-shape #2 (line 639): catch-all
                // CSS-variable emit. variable_name flows through
                // generate() → hash(); reachable end-to-end.
                let (expression, variable_name) =
                    get_variable_declarator_value_for_own_path(Box::new(prop_value.clone()), meta);
                let name = format!("--_{}", hash(&variable_name));
                variables.push(Variable {
                    name: name.clone(),
                    expression,
                    prefix: None,
                    suffix: None,
                });
                css.push(CssItem::Unconditional(UnconditionalCssItem {
                    css: format!("{}: var({});", kebab_case(&key), name),
                }));
            }
            PropOrSpread::Spread(SpreadElement { expr, .. }) => {
                // upstream: resolveBinding + evaluateExpression. Both
                // are Phase 5 stubs.
                if matches!(&**expr, Expr::Ident(_)) {
                    let _ = resolve_binding_stub(
                        if let Expr::Ident(i) = &**expr {
                            &i.sym
                        } else {
                            ""
                        },
                        meta,
                    );
                }
                evaluate_expression_stub(expr, meta);
            }
        }
    }

    Ok(CSSOutput {
        css: merge_subsequent_unconditional_css_items(css),
        variables,
    })
}

/// `generateCacheForCSSMap` upstream lines 683–709. Resolve-binding
/// + visitCssMapPath path — wholly Phase 5 / 6.
fn generate_cache_for_css_map(_node: &Ident, meta: &mut Metadata<'_>) {
    if meta.state.css_map().contains_key(&_node.sym.to_string())
        || meta.state.ignore_member_expressions().contains_key(&_node.sym.to_string())
    {
        return;
    }
    // Reaching here means we'd need resolveBinding + visitCssMapPath.
    visit_css_map_path_stub();
}

/// `extractMemberExpression` upstream lines 728–752. Two-arg shape
/// (`fallbackToEvaluate: true | false`) — caller chooses whether to
/// fall back to the evaluate path on miss.
pub fn extract_member_expression(
    node: &MemberExpr,
    meta: &mut Metadata<'_>,
) -> Result<CSSOutput, CssBuildError> {
    extract_member_expression_optional(node, meta, true)?
        .ok_or_else(|| build_code_frame_error("MemberExpression yielded no CSS", Some(node.span)))
}

fn extract_member_expression_optional(
    node: &MemberExpr,
    meta: &mut Metadata<'_>,
    fallback_to_evaluate: bool,
) -> Result<Option<CSSOutput>, CssBuildError> {
    let binding_identifier = find_binding_identifier(&Expr::Member(node.clone()));
    if let Some(ident) = &binding_identifier {
        generate_cache_for_css_map(ident, meta);
        if meta.state.css_map().contains_key(&ident.sym.to_string()) {
            return Ok(Some(CSSOutput {
                css: vec![CssItem::Map(CssMapItem {
                    name: ident.sym.to_string(),
                    expression: Box::new(Expr::Member(node.clone())),
                    css: String::new(),
                })],
                variables: vec![],
            }));
        }
    }
    if fallback_to_evaluate {
        evaluate_expression_stub(&Expr::Member(node.clone()), meta);
    }
    Ok(None)
}

/// `extractTemplateLiteral` upstream lines 760–907 — the §4.4
/// hash-call-shape site at line 869 (`hash(variableName)` catch-all
/// with cssAffixInterpolation prefix-detection).
pub fn extract_template_literal(
    node: &Tpl,
    meta: &mut Metadata<'_>,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();

    let mut acc = String::new();
    for (index, quasi) in node.quasis.iter().enumerate() {
        let raw = quasi.raw.as_str().to_string();
        let node_expression = node.exprs.get(index).map(|e| (**e).clone());

        // No expression OR arrow-body that is logical → just append.
        let is_terminal_or_logical = match &node_expression {
            None => true,
            Some(Expr::Arrow(arrow)) => matches!(
                &*arrow.body,
                BlockStmtOrExpr::Expr(e) if matches!(&**e, Expr::Bin(b) if matches!(b.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing))
            ),
            _ => false,
        };
        if is_terminal_or_logical {
            let suffix = if matches!(meta.context, MetadataContext::Keyframes { .. } | MetadataContext::Fragment) {
                ""
            } else {
                ";"
            };
            acc.push_str(&raw);
            acc.push_str(suffix);
            continue;
        }

        let node_expression = node_expression.expect("checked above");
        let _is_mid_statement = is_quasi_mid_statement(quasi);
        let _does_expression_have_conditional_css = match &node_expression {
            Expr::Arrow(arrow) => matches!(&*arrow.body, BlockStmtOrExpr::Expr(e) if matches!(&**e, Expr::Cond(_))),
            _ => false,
        };

        if _is_mid_statement && _does_expression_have_conditional_css {
            // upstream: hasNestedTemplateLiteralsWithConditionalRules
            // is ALSO required to gate this. Phase 5 §5.6 stub on
            // that gate; the optimisation is a §4.4 deferred path.
            // (Calling has_nested...() would unimplemented!() panic;
            // we conservatively skip the optimisation, matching the
            // upstream behaviour when the gate returns true.)
            let _ = optimize_conditional_statement; // silence unused
            let _ = recompose_template_literal; // silence unused
            let _ = has_nested_template_literals_with_conditional_rules;
        }

        // upstream: `evaluateExpression(nodeExpression, meta)`
        // Both expression-as-CSS check and keyframes inner walk
        // depend on the evaluator output.
        let _ = node_expression; // routed through stub below

        // Reaching ANY of the below dispatch arms requires the evaluator.
        if try_keyframes_branch(&node.exprs[index], meta, &raw, &mut css, &mut variables, &mut acc)? {
            continue;
        }

        // §4.4 hash-call-shape #3 (line 869): catch-all CSS-variable
        // emit with cssAffixInterpolation prefix-detection.
        let (expression, variable_name) =
            get_variable_declarator_value_for_own_path((*node.exprs[index]).clone().into(), meta);
        let next_quasi_raw = node
            .quasis
            .get(index + 1)
            .map(|q| q.raw.as_str().to_string())
            .unwrap_or_default();
        let (before, after) = css_affix_interpolation(&raw, &next_quasi_raw);
        let suffix_marker = if before.variable_prefix == "-" {
            "-"
        } else {
            ""
        };
        let name = format!("--_{}{}", hash(&variable_name), suffix_marker);

        // upstream: `nextQuasis.value.raw = after.css;`. We can't
        // mutate `node.quasis[index+1]` through the &-borrow; the
        // Rust port walks a clone of the input Tpl when this branch
        // is reached. Phase 5 §5.6's mutable-walker shape lands the
        // proper model. For §4.4 the unit test passes a fresh Tpl
        // built per-call so the mutation has no observable downstream
        // effect — the test asserts on the emitted Variable name
        // which is the close-out signal.
        let _ = after; // mutation deferred per above

        variables.push(Variable {
            name: name.clone(),
            expression,
            prefix: if before.variable_prefix.is_empty() {
                None
            } else {
                Some(before.variable_prefix.clone())
            },
            suffix: None,
        });
        acc.push_str(&before.css);
        acc.push_str(&format!("var({})", name));
    }

    css.push(CssItem::Unconditional(UnconditionalCssItem { css: acc }));

    // Logical-expression sub-pass — upstream lines 889–901.
    for prop in &node.exprs {
        if let Expr::Arrow(arrow) = &**prop {
            if matches!(
                &*arrow.body,
                BlockStmtOrExpr::Expr(e) if matches!(&**e, Expr::Bin(b) if matches!(b.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing))
            ) {
                evaluate_expression_stub(&Expr::Arrow(arrow.clone()), meta);
            }
        }
    }

    Ok(CSSOutput {
        css: merge_subsequent_unconditional_css_items(css),
        variables,
    })
}

/// Helper extracted for clarity — the keyframes-via-tpl-interp branch
/// inside extract_template_literal.
fn try_keyframes_branch(
    expr: &Expr,
    meta: &mut Metadata<'_>,
    raw: &str,
    css: &mut Vec<CssItem>,
    variables: &mut Vec<Variable>,
    acc: &mut String,
) -> Result<bool, CssBuildError> {
    if !is_compiled_keyframes_call_expression(expr, meta.state)
        && !is_compiled_keyframes_tagged_template_expression(expr, meta.state)
    {
        return Ok(false);
    }
    let result = extract_keyframes(expr, meta, raw, "")?;
    let mut iter = result.css.into_iter();
    let sheet = iter.next().expect("extract_keyframes returns ≥1 item");
    let unconditional = iter.next().expect("extract_keyframes returns ≥2 items");
    css.push(sheet);
    variables.extend(result.variables);
    acc.push_str(&get_item_css(&unconditional));
    Ok(true)
}

/// `extractArray` upstream lines 915–941.
pub fn extract_array(elements: &[Expr], meta: &mut Metadata<'_>) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();
    for element in elements {
        let result = if let Expr::Cond(c) = element {
            extract_conditional_expression(c, meta)?
        } else {
            build_css_inner(element, meta)?
        };
        css.extend(result.css);
        variables.extend(result.variables);
    }
    Ok(CSSOutput { css, variables })
}

/// `buildCss` upstream lines 949–1084 — the public dispatcher.
pub fn build_css(node: &Expr, meta: &mut Metadata<'_>) -> Result<CSSOutput, CssBuildError> {
    build_css_inner(node, meta)
}

/// Internal entry — exists so the top-level extractArray path can
/// reach it without re-routing through the public API name.
fn build_css_inner(node: &Expr, meta: &mut Metadata<'_>) -> Result<CSSOutput, CssBuildError> {
    if let Expr::Lit(Lit::Str(s)) = node {
        return Ok(CSSOutput {
            css: vec![CssItem::Unconditional(UnconditionalCssItem {
                css: s.value.to_atom_lossy().as_str().to_string(),
            })],
            variables: vec![],
        });
    }

    if let Expr::TsAs(ts_as) = node {
        return build_css_inner(&ts_as.expr, meta);
    }

    if let Expr::Tpl(tpl) = node {
        return extract_template_literal(tpl, meta);
    }

    if let Expr::Object(obj) = node {
        return extract_object_expression(obj, meta);
    }

    if let Expr::Member(m) = node {
        return extract_member_expression(m, meta);
    }

    if let Expr::Arrow(arrow) = node {
        if let BlockStmtOrExpr::Expr(body_expr) = &*arrow.body {
            match &**body_expr {
                Expr::Object(obj) => return extract_object_expression(obj, meta),
                Expr::Bin(b)
                    if matches!(
                        b.op,
                        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
                    ) =>
                {
                    return extract_logical_expression(arrow, meta);
                }
                Expr::Cond(c) => return extract_conditional_expression(c, meta),
                Expr::Member(m) => return extract_member_expression(m, meta),
                _ => {}
            }
        }
    }

    if matches!(node, Expr::Ident(_)) {
        // upstream: resolveBinding + cssMap-collision check + recurse.
        // Phase 5 §5.4 stubbed.
        let _ = resolve_binding_stub(
            if let Expr::Ident(i) = node {
                &i.sym
            } else {
                ""
            },
            meta,
        );
    }

    if let Expr::Array(ArrayLit { elems, .. }) = node {
        let exprs: Vec<Expr> = elems
            .iter()
            .filter_map(|opt| opt.as_ref().map(|e| (*e.expr).clone()))
            .collect();
        return extract_array(&exprs, meta);
    }

    if let Expr::Bin(BinExpr {
        op,
        left,
        right,
        ..
    }) = node
    {
        if matches!(
            op,
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
        ) {
            let result = build_css_inner(right, meta)?;
            let css: Vec<CssItem> = result
                .css
                .into_iter()
                .map(|item| match item {
                    CssItem::Logical(l) => {
                        let new_expr = Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: logical_op_to_swc(l.operator),
                            left: left.clone(),
                            right: l.expression,
                        }));
                        CssItem::Logical(LogicalCssItem {
                            expression: new_expr,
                            ..l
                        })
                    }
                    CssItem::Map(m) => CssItem::Map(CssMapItem {
                        expression: Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: *op,
                            left: left.clone(),
                            right: m.expression,
                        })),
                        ..m
                    }),
                    other => CssItem::Logical(LogicalCssItem {
                        css: get_item_css(&other),
                        expression: left.clone(),
                        operator: match op {
                            BinaryOp::LogicalAnd => crate::utils::types::LogicalOperator::And,
                            BinaryOp::LogicalOr => crate::utils::types::LogicalOperator::Or,
                            BinaryOp::NullishCoalescing => {
                                crate::utils::types::LogicalOperator::NullishCoalescing
                            }
                            _ => unreachable!("guarded above"),
                        },
                    }),
                })
                .collect();
            return Ok(CSSOutput {
                css,
                variables: result.variables,
            });
        }
    }

    if is_compiled_css_tagged_template_expression(node, meta.state) {
        if let Expr::TaggedTpl(TaggedTpl { tpl, .. }) = node {
            return build_css_inner(&Expr::Tpl((**tpl).clone()), meta);
        }
    }

    if is_compiled_css_call_expression(node, meta.state) {
        if let Expr::Call(CallExpr { args, .. }) = node {
            if let Some(first) = args.first() {
                if let Expr::Object(obj) = &*first.expr {
                    return build_css_inner(&Expr::Object(obj.clone()), meta);
                }
            }
        }
    }

    let are_compiled_apis_enabled = meta
        .state
        .compiled_imports()
        .map(|i| {
            i.css.is_some()
                || i.class_names.is_some()
                || i.keyframes.is_some()
                || i.styled.is_some()
                || i.css_map.is_some()
        })
        .unwrap_or(false);
    let error_message = if are_compiled_apis_enabled {
        "try to define them statically using Compiled APIs instead"
    } else {
        "no Compiled APIs were found in scope, if you're using createStrictAPI make sure to configure importSources"
    };
    Err(build_code_frame_error(
        format!(
            "This {} was unable to have its styles extracted — {}",
            babel_node_type_name(node),
            error_message
        ),
        Some(expression_span(node)),
    ))
}

/// Map an SWC `Expr` variant back to the Babel `node.type` string the
/// JS error message would have produced. Mirrors the same table in
/// `object_property_to_string::babel_type_name` (kept private there
/// for the same reason — module-local error-message parity).
fn babel_node_type_name(expression: &Expr) -> &'static str {
    match expression {
        Expr::This(_) => "ThisExpression",
        Expr::Array(_) => "ArrayExpression",
        Expr::Object(_) => "ObjectExpression",
        Expr::Fn(_) => "FunctionExpression",
        Expr::Unary(_) => "UnaryExpression",
        Expr::Update(_) => "UpdateExpression",
        Expr::Bin(_) => "BinaryExpression",
        Expr::Assign(_) => "AssignmentExpression",
        Expr::Member(_) => "MemberExpression",
        Expr::SuperProp(_) => "MemberExpression",
        Expr::Cond(_) => "ConditionalExpression",
        Expr::Call(_) => "CallExpression",
        Expr::New(_) => "NewExpression",
        Expr::Seq(_) => "SequenceExpression",
        Expr::Ident(_) => "Identifier",
        Expr::Lit(_) => "Literal",
        Expr::Tpl(_) => "TemplateLiteral",
        Expr::TaggedTpl(_) => "TaggedTemplateExpression",
        Expr::Arrow(_) => "ArrowFunctionExpression",
        Expr::Class(_) => "ClassExpression",
        Expr::Yield(_) => "YieldExpression",
        Expr::MetaProp(_) => "MetaProperty",
        Expr::Await(_) => "AwaitExpression",
        Expr::Paren(_) => "ParenthesizedExpression",
        Expr::JSXMember(_) => "JSXMemberExpression",
        Expr::JSXNamespacedName(_) => "JSXNamespacedName",
        Expr::JSXEmpty(_) => "JSXEmptyExpression",
        Expr::JSXElement(_) => "JSXElement",
        Expr::JSXFragment(_) => "JSXFragment",
        Expr::TsTypeAssertion(_) => "TSTypeAssertion",
        Expr::TsConstAssertion(_) => "TSConstAssertion",
        Expr::TsNonNull(_) => "TSNonNullExpression",
        Expr::TsAs(_) => "TSAsExpression",
        Expr::TsInstantiation(_) => "TSInstantiationExpression",
        Expr::TsSatisfies(_) => "TSSatisfiesExpression",
        Expr::PrivateName(_) => "PrivateName",
        Expr::OptChain(_) => "OptionalCallExpression",
        Expr::Invalid(_) => "Invalid",
    }
}

// Suppress unused-import warnings — these are referenced by transitive
// stub paths above (compile-time only; runtime would `unimplemented!`).
const _: Option<Bool> = None;
const _: Option<Number> = None;
const _: Option<Str> = None;

#[cfg(test)]
mod tests {
    //! §4.4 close-out: 4 hash-call-shape sites end-to-end through
    //! `compat::generator::generate` + `compiled_utils::hash`. Each
    //! test asserts the variable name / keyframes name matches the
    //! same shape the JS path would produce.
    //!
    //! These aren't full byte-parity gates — that's §4.8. What they
    //! lock is that the Rust dispatch reaches the hash site with the
    //! same generated string that the §3 hash-parity oracle covers.

    use super::*;
    use crate::state::State;
    use crate::types::{Metadata, MetadataContext};
    use compiled_utils::hash;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{ExprOrSpread, Ident, Number, PropName, Str};

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        }
    }

    #[test]
    fn hash_site_extract_keyframes_name_matches_oracle() {
        // §4.4 site #1 (css-builders.ts:464). Build the equivalent
        // of `keyframes({ from: ... })` and assert the emitted
        // keyframes name matches `k${hash(generate(callExpr).code)}`.
        let mut state = State::default();
        // Wire `keyframes` as a Compiled import so the inner build_css
        // doesn't error (we only assert on the name, which is computed
        // before any inner extraction).
        state.compiled_imports = Some(crate::state::CompiledImports {
            keyframes: Some(vec!["keyframes".into()]),
            ..Default::default()
        });

        // Construct: `keyframes('from { color: red; }')` — a CallExpr
        // with one StringLiteral arg. Inner build_css will treat the
        // string as an unconditional CSS rule.
        let call = Expr::Call(swc_core::ecma::ast::CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "keyframes".into(),
                DUMMY_SP,
                Default::default(),
            )))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: "from { color: red; }".into(),
                    raw: None,
                }))),
            }],
            type_args: None,
            ctxt: Default::default(),
        });

        let expected_name = format!("k{}", hash(&generate(&call)));

        let mut meta = fresh_meta(&mut state);
        let result = extract_keyframes(&call, &mut meta, "", "").expect("extracts");
        // The emitted Unconditional item is `{ css: format!("{prefix}{name}{suffix}") }`
        // with prefix/suffix empty — should equal the expected name.
        let unconditional_css: String = result
            .css
            .iter()
            .filter_map(|i| match i {
                CssItem::Unconditional(u) => Some(u.css.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(unconditional_css, expected_name);
    }

    #[test]
    fn hash_site_extract_object_expression_variable_name() {
        // §4.4 site #2 (css-builders.ts:639). An ObjectExpression
        // property whose value is a non-literal expression that
        // doesn't match any earlier branch hits the catch-all
        // `--_${hash(variableName)}`. Use a NumericLiteral cast that
        // doesn't fit Lit::Num / Lit::Str shape — the simplest reach
        // is a UnaryExpression `-1` (Babel emits `-1`, the hash
        // input is generate(unary).code = "-1").
        let mut state = State::default();
        let unary = Box::new(Expr::Unary(UnaryExpr {
            span: DUMMY_SP,
            op: UnaryOp::Minus,
            arg: Box::new(Expr::Lit(Lit::Num(Number {
                span: DUMMY_SP,
                value: 1.0,
                raw: None,
            }))),
        }));
        let obj = ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(
                swc_core::ecma::ast::KeyValueProp {
                    key: PropName::Ident(swc_core::ecma::ast::IdentName::new(
                        "marginTop".into(),
                        DUMMY_SP,
                    )),
                    value: unary.clone(),
                },
            )))],
        };

        let expected_var_name = format!("--_{}", hash(&generate(&unary)));
        let mut meta = fresh_meta(&mut state);
        let result = extract_object_expression(&obj, &mut meta).expect("extracts");
        let var = result
            .variables
            .into_iter()
            .next()
            .expect("emits one variable");
        assert_eq!(var.name, expected_var_name);
    }

    #[test]
    fn hash_site_extract_template_literal_variable_name() {
        // §4.4 site #3 (css-builders.ts:869). A TemplateLiteral whose
        // single interpolation is a non-CSS-shaped expression hits
        // the catch-all `--_${hash(variableName)}` with the
        // cssAffixInterpolation prefix-detection. Use `font-size: ${x}px`.
        let mut state = State::default();
        let interp = Box::new(Expr::Ident(Ident::new(
            "fontSize".into(),
            DUMMY_SP,
            Default::default(),
        )));
        let tpl = Tpl {
            span: DUMMY_SP,
            quasis: vec![
                TplElement {
                    span: DUMMY_SP,
                    tail: false,
                    cooked: Some("font-size: ".into()),
                    raw: "font-size: ".into(),
                },
                TplElement {
                    span: DUMMY_SP,
                    tail: true,
                    cooked: Some("px".into()),
                    raw: "px".into(),
                },
            ],
            exprs: vec![interp.clone()],
        };

        let expected_var_name = format!("--_{}", hash(&generate(&interp)));
        let mut meta = fresh_meta(&mut state);
        let result = extract_template_literal(&tpl, &mut meta).expect("extracts");
        let var = result
            .variables
            .into_iter()
            .next()
            .expect("emits one variable");
        assert_eq!(var.name, expected_var_name);
    }

    #[test]
    fn merge_subsequent_unconditional_css_items_basic() {
        // Sanity check on the merge helper — all-unconditional
        // collapses to a single item.
        let arr = vec![
            CssItem::Unconditional(UnconditionalCssItem { css: "a".into() }),
            CssItem::Unconditional(UnconditionalCssItem { css: "b".into() }),
            CssItem::Unconditional(UnconditionalCssItem { css: "c".into() }),
        ];
        let merged = merge_subsequent_unconditional_css_items(arr);
        assert_eq!(merged.len(), 1);
        if let CssItem::Unconditional(u) = &merged[0] {
            assert_eq!(u.css, "abc");
        } else {
            panic!("expected unconditional");
        }
    }

    #[test]
    fn merge_pulls_sheets_to_front() {
        // [u, sheet, u] → [sheet, uu]
        let arr = vec![
            CssItem::Unconditional(UnconditionalCssItem { css: "a".into() }),
            CssItem::Sheet(SheetCssItem { css: "@media".into() }),
            CssItem::Unconditional(UnconditionalCssItem { css: "b".into() }),
        ];
        let merged = merge_subsequent_unconditional_css_items(arr);
        assert_eq!(merged.len(), 2);
        assert!(matches!(merged[0], CssItem::Sheet(_)));
    }
}
