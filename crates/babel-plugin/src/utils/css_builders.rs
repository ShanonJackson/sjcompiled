//! 1:1 port of `packages/babel-plugin/src/utils/css-builders.ts`
//! (1084 LOC upstream).
//!
//! ### §4.6 bridge — current state
//!
//! Phase 4 §4.6 closed: the per-Compiled-API handlers in this file
//! are wired against the real evaluator + resolver. Each fn that
//! reaches into `evaluateExpression` / `resolveBinding` carries the
//! §5.5 explicit-param trio (`scope_index`, `parent_scope`,
//! `own_scope`). Callers thread those from the visitor's
//! `Program::enter` ScopeIndex bootstrap.
//!
//! * `evaluateExpression` →
//!   [`crate::utils::evaluate_expression::evaluate_expression`]
//!   (Phase 5 §5.6).
//! * `resolveBinding` →
//!   [`crate::utils::resolve_binding::resolve_binding`]
//!   (Phase 5 §5.4e).
//! * `visitCssMapPath` — Phase 6 §6.3 (css-map handler). The single
//!   call site here panics with a phase-citing `unimplemented!()`
//!   message; deleted when Phase 6 §6.3 lands the real fn.
//!
//! Surrounding-logic ports for branches that consume evaluator output
//! (e.g. recursive re-dispatch on a folded Identifier branch) live in
//! the per-API Phase 6 handlers — this file ships the dispatch shape
//! and discards the ResultPair where the surrounding port is gated on
//! Phase 6.
//!
//! `addUnitIfNeeded` and `cssAffixInterpolation` were initially flagged
//! as missing from `crates/css`. The CSS-port agent shipped both as
//! re-exports per `crates/babel-plugin/CSS_BUILDERS_DEPS.md` (RESOLVED
//! 2026-05-04); this file uses them directly via `css::` — same import
//! shape as the JS source.
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
use crate::compat::scope::{ScopeId, ScopeIndex};
use crate::mutation_recorder::MutationRecorder;
use crate::state::State;
use crate::types::{Metadata, MetadataContext};
use crate::utils::ast::{build_code_frame_error, CssBuildError};
use crate::utils::evaluate_expression::evaluate_expression;
use crate::utils::is_compiled::{
    is_compiled_css_call_expression, is_compiled_css_map_call_expression,
    is_compiled_css_tagged_template_expression, is_compiled_keyframes_call_expression,
    is_compiled_keyframes_tagged_template_expression,
};
use crate::utils::is_empty::is_empty_value;
use crate::utils::manipulate_template_literal::{
    has_nested_template_literals_with_conditional_rules, is_quasi_mid_statement,
    optimize_conditional_statement, recompose_template_literal,
};
use crate::utils::object_property_to_string::object_property_to_string;
use crate::utils::resolve_binding::resolve_binding;
use crate::utils::types::{
    BindingSource, CSSOutput, ConditionalCssItem, CssItem, CssMapItem, LogicalCssItem,
    PartialBindingWithMeta, SheetCssItem, UnconditionalCssItem, Variable,
};

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

pub(crate) fn logical_op_to_swc(op: crate::utils::types::LogicalOperator) -> BinaryOp {
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
/// `dead_code` allow: §4.6 bridge flips `resolveBinding` to the real
/// fn but discards the `PartialBindingWithMeta` (surrounding logic
/// is Phase 6 work); this helper is the upstream consumer of that
/// resolved-binding shape and ports alongside the first Phase 6
/// handler that consumes the result.
#[allow(dead_code)]
fn assert_no_imported_css_variables(
    reference_node_span: Option<swc_core::common::Span>,
    resolved_binding: &PartialBindingWithMeta,
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
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();

    let consequent_css = extract_branch(&node.cons, meta, node, scope_index, parent_scope, own_scope, recorder)?;
    let alternate_css = extract_branch(&node.alt, meta, node, scope_index, parent_scope, own_scope, recorder)?;

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
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
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
                    Some(build_css_inner(path_node, meta, scope_index, parent_scope, own_scope, recorder)?)
                } else {
                    None
                }
            }
        Expr::TaggedTpl(_) | Expr::Call(_)
            if path_is_compiled_css_shape(path_node, meta) =>
        {
            Some(build_css_inner(path_node, meta, scope_index, parent_scope, own_scope, recorder)?)
        }
        Expr::Ident(_) => {
            // §4.6 bridge: real evaluator dispatch. The surrounding
            // JS branch (re-dispatch on the folded value) is Phase 6
            // handler work; the bridge ships the call shape and
            // discards the ResultPair until that surrounding port lands.
            let _ = evaluate_expression(path_node, meta, scope_index, parent_scope, own_scope);
            None
        }
        Expr::Cond(c) => Some(extract_conditional_expression(c, meta, scope_index, parent_scope, own_scope, recorder)?),
        Expr::Member(m) => extract_member_expression_optional(m, meta, false, scope_index, parent_scope, own_scope, recorder)?,
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
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    _recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    // Mirrors upstream `if (t.isExpression(node.body))`. The body
    // walk would be `evaluateExpression(node.body, meta)`.
    if let BlockStmtOrExpr::Expr(_) = &*node.body {
        // §4.6 bridge: real evaluator dispatch. Surrounding JS branch
        // (LogicalCssItem emission keyed on the folded value) is
        // Phase 6 work; bridge discards the ResultPair.
        let _ = evaluate_expression(
            &node.body_as_expr().clone(),
            meta,
            scope_index,
            parent_scope,
            own_scope,
        );
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
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
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
            extract_array(&arg_exprs, &mut child, scope_index, parent_scope, own_scope, recorder)?
        }
        Expr::TaggedTpl(tpl) => {
            let mut child = meta.reborrow_with_context(kf_context.clone());
            build_css_inner(
                &Expr::Tpl((*tpl.tpl).clone()),
                &mut child,
                scope_index,
                parent_scope,
                own_scope,
                recorder,
            )?
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
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
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
                let key = object_property_to_string(&kv.key, meta, scope_index, parent_scope, own_scope)?;
                // §6.8a-vi: 1:1 port of upstream `evaluateExpression(prop.value, meta)`.
                // Resolves Ident / Member / Call / etc. through `resolve_binding`
                // → recursive evaluator, returning either a literal-folded
                // node (StringLit / NumLit / ObjectExpr / TaggedTpl / a
                // recognised keyframes CallExpr) or the original
                // expression as the babel-evaluator fallback. Without
                // this, references like `animationName: fadeOut` (where
                // `fadeOut = keyframes({...})`) bypass the keyframes
                // matcher below and fall through to the catch-all
                // CSS-variable emit, producing `var(--_xxx)` plus a
                // dangling `style={ '--_xxx': ix(fadeOut) }` instead of
                // hoisting the `@keyframes` sheet and inlining the
                // generated keyframes name.
                //
                // Returned value is owned (Box<Expr>); we borrow into it
                // for the rest of this prop iteration.
                let evaluated = evaluate_expression(
                    &kv.value,
                    meta,
                    scope_index,
                    parent_scope,
                    own_scope,
                )
                .value
                .unwrap_or_else(|| Box::new((*kv.value).clone()));
                let prop_value: &Expr = &evaluated;
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
                    let result = to_css_rule(
                        &key,
                        build_css_inner(prop_value, meta, scope_index, parent_scope, own_scope, recorder)?,
                    );
                    css.extend(result.css);
                    variables.extend(result.variables);
                    continue;
                }

                if let Expr::Tpl(tpl) = prop_value {
                    let mut tpl_clone = tpl.clone();
                    let first_expr = tpl_clone.exprs.first().map(|e| (**e).clone());
                    let result = if tpl_clone.exprs.len() == 1
                        && matches!(
                            &first_expr,
                            Some(Expr::Arrow(arrow))
                                if matches!(
                                    &*arrow.body,
                                    BlockStmtOrExpr::Expr(e) if matches!(
                                        crate::compat::paren::unwrap_paren(e),
                                        Expr::Cond(_)
                                    )
                                )
                        )
                    {
                        recompose_template_literal(&mut tpl_clone, &format!("{}:", kebab_case(&key)), ";");
                        extract_template_literal(&tpl_clone, meta, scope_index, parent_scope, own_scope, recorder)?
                    } else {
                        let inner = extract_template_literal(&tpl_clone, meta, scope_index, parent_scope, own_scope, recorder)?;
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
                                // walker. Phase 5 §5.6 ☑ shipped the
                                // evaluator but kept this site's &-only
                                // walker shape; the proper mutable-walk
                                // wire-up is Phase 4 §4.6 / Phase 6
                                // territory. Until then the optimised
                                // wrap stands without the body swap.
                                // The §4.4 corpus does not exercise
                                // this path — stub-safe.
                            }
                        }
                    }
                    if let Some(mut opt) = optimised {
                        recompose_template_literal(&mut opt, &format!("{}:", kebab_case(&key)), ";");
                        let result = extract_template_literal(&opt, meta, scope_index, parent_scope, own_scope, recorder)?;
                        css.extend(result.css);
                        variables.extend(result.variables);
                        continue;
                    }
                }

                if is_compiled_keyframes_call_expression(prop_value, meta.state)
                    || is_compiled_keyframes_tagged_template_expression(prop_value, meta.state)
                {
                    let kf_prefix = format!("{}: ", kebab_case(&key));
                    let result = extract_keyframes(
                        prop_value,
                        meta,
                        &kf_prefix,
                        ";",
                        scope_index,
                        parent_scope,
                        own_scope,
                        recorder,
                    )?;
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
                // §4.6 bridge: real resolver + evaluator dispatch. The
                // surrounding JS branch (consume the resolved Variable
                // shape into the CSS emit) is Phase 6 handler work;
                // bridge discards both results.
                if let Expr::Ident(i) = &**expr {
                    let _ = resolve_binding(
                        i.sym.as_str(),
                        &*meta,
                        &*scope_index,
                        parent_scope,
                        own_scope,
                    );
                }
                let _ = evaluate_expression(expr, meta, scope_index, parent_scope, own_scope);
            }
        }
    }

    Ok(CSSOutput {
        css: merge_subsequent_unconditional_css_items(css),
        variables,
    })
}

/// `generateCacheForCSSMap` upstream lines 683–709. Resolve-binding
/// + visitCssMapPath path — closed in Phase 6 §6.5 with the
/// MutationRecorder threading through the build_css call graph.
///
/// Mirrors upstream verbatim:
/// 1. Cache-hit / ignore-list check — bail.
/// 2. `resolveBinding(node.name, meta, evaluateExpression)`.
/// 3. If resolved.node is a Compiled cssMap call, run
///    `visitCssMapPath` against the resolved init expression. The
///    visit publishes `state.cssMap[binding] = sheets` via
///    StateDiff::CssMapInsert.
/// 4. Otherwise mark `state.ignoreMemberExpressions[node.name] = true`
///    so future references skip the resolver.
///
/// **Diff-log replay note (§5.3 / PLAN.md §3.9.8):** the visit_css_map_path
/// call below routes its mutations through the SAME `recorder` the
/// caller threads in. When the §5.3 cache wires into State, late-resolve
/// cssMap publications appear in the consumer's diff_log alongside any
/// other mutations the consumer's pass produced. No special replay
/// handling needed.
fn generate_cache_for_css_map(
    node: &Ident,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) {
    let name = node.sym.to_string();
    if meta.state.css_map().contains_key(&name)
        || meta.state.ignore_member_expressions().contains_key(&name)
    {
        return;
    }

    // Resolve the binding. Upstream pulls the resolver from
    // `meta.state.resolver` plus filename; the Rust port's
    // `resolve_binding` does the same via `state.resolver()` /
    // `state.filename()`. A None result means the binding could not
    // be resolved (no scope binding, no cross-file resolution
    // available); upstream then falls through to the ignore-marking
    // branch, which we mirror.
    let resolved = resolve_binding(name.as_str(), &*meta, &*scope_index, parent_scope, own_scope);

    if let Some(resolved_pair) = resolved {
        // Upstream: `if (resolved && isCompiledCSSMapCallExpression(resolved.node, meta.state))`.
        // PartialBindingWithMeta::node is `Option<Box<Expr>>` because
        // some bindings (cross-file imports without a direct AST anchor
        // in this file) arrive as None. Cache-generation only fires
        // when we have a concrete in-scope CallExpr to visit.
        let Some(boxed_node) = resolved_pair.node.as_deref() else {
            return;
        };
        if is_compiled_css_map_call_expression(boxed_node, meta.state) {
            // Upstream: `let resolvedCallPath = resolved.path.get('init');`
            // followed by `if (Array.isArray(resolvedCallPath))
            // resolvedCallPath = resolvedCallPath[0]`. The Rust port's
            // `PartialBindingWithMeta::node` is the init-side `Expr`
            // already (the call expression itself). No path navigation
            // needed.
            if let Expr::Call(call) = boxed_node {
                let visit_result = crate::css_map::visit_css_map_path(
                    call,
                    name.as_str(),
                    meta,
                    recorder,
                    scope_index,
                    parent_scope,
                    own_scope,
                );
                // Upstream silently skips this branch on cssMap-shape
                // failures (visitCssMapPath panics on shape errors,
                // matching our port). A success populates
                // state.cssMap[name].
                if let Err(e) = visit_result {
                    panic!("{}", e.message);
                }
            }
        }
    }

    // Upstream: `if (!meta.state.cssMap[node.name]) {
    //              meta.state.ignoreMemberExpressions[node.name] = true; }`
    if !meta.state.css_map().contains_key(&name) {
        recorder.apply(
            crate::mutation_recorder::StateDiff::IgnoreMemberExprMark { name: name.clone() },
            meta.state,
        );
    }
}

/// `extractMemberExpression` upstream lines 728–752. Two-arg shape
/// (`fallbackToEvaluate: true | false`) — caller chooses whether to
/// fall back to the evaluate path on miss.
pub fn extract_member_expression(
    node: &MemberExpr,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    extract_member_expression_optional(node, meta, true, scope_index, parent_scope, own_scope, recorder)?
        .ok_or_else(|| build_code_frame_error("MemberExpression yielded no CSS", Some(node.span)))
}

fn extract_member_expression_optional(
    node: &MemberExpr,
    meta: &mut Metadata<'_>,
    fallback_to_evaluate: bool,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<Option<CSSOutput>, CssBuildError> {
    let binding_identifier = find_binding_identifier(&Expr::Member(node.clone()));
    if let Some(ident) = &binding_identifier {
        generate_cache_for_css_map(ident, meta, scope_index, parent_scope, own_scope, recorder);
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
        // 1:1 port of upstream `extractMemberExpression` lines 746–749:
        //   const { value, meta: updatedMeta } = evaluateExpression(node, meta);
        //   return buildCss(value, updatedMeta);
        // Resolves member-expression accesses such as `styles.success`
        // where `styles = {success: {color: 'green'}}` to the inner
        // ObjectExpression and recurses `build_css_inner`.
        let pair = evaluate_expression(
            &Expr::Member(node.clone()),
            meta,
            scope_index,
            parent_scope,
            own_scope,
        );
        // `evaluateExpression` always returns a value (babel fallback
        // re-emits the input on deopt), so `value` is never None in
        // upstream's flow. The Rust port mirrors via fallback to the
        // original member expression on the rare None.
        let value = pair
            .value
            .unwrap_or_else(|| Box::new(Expr::Member(node.clone())));
        return Ok(Some(build_css_inner(
            &value,
            meta,
            scope_index,
            parent_scope,
            own_scope,
            recorder,
        )?));
    }
    Ok(None)
}

/// `extractTemplateLiteral` upstream lines 760–907 — the §4.4
/// hash-call-shape site at line 869 (`hash(variableName)` catch-all
/// with cssAffixInterpolation prefix-detection).
pub fn extract_template_literal(
    node: &Tpl,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();

    let mut acc = String::new();
    // Mutable working copy of `quasi.raw` values. Upstream
    // `extractTemplateLiteral` does `nextQuasis.value.raw = after.css`
    // in the catch-all branch (build-css.ts:874), and the next
    // iteration reads its `quasi.value.raw` from that mutated
    // `node.quasis[index + 1]`. The Rust port walks `node.quasis` by
    // `&` borrow so it can't mutate in place; instead we keep a parallel
    // `Vec<String>` and update both `quasi_raws[index + 1]` for the
    // next iteration AND `quasi_raws[index]` if an earlier iteration
    // had already updated it.
    let mut quasi_raws: Vec<String> = node
        .quasis
        .iter()
        .map(|q| q.raw.as_str().to_string())
        .collect();
    for index in 0..node.quasis.len() {
        let quasi = &node.quasis[index];
        let raw = quasi_raws[index].clone();
        let node_expression = node.exprs.get(index).map(|e| (**e).clone());

        // No expression OR arrow-body that is logical → just append.
        // Unwrap `Expr::Paren` on the body before matching since SWC
        // keeps parens that Babel strips.
        let is_terminal_or_logical = match &node_expression {
            None => true,
            Some(Expr::Arrow(arrow)) => matches!(
                &*arrow.body,
                BlockStmtOrExpr::Expr(e) if matches!(
                    crate::compat::paren::unwrap_paren(e),
                    Expr::Bin(b) if matches!(
                        b.op,
                        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
                    )
                )
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
            Expr::Arrow(arrow) => matches!(
                &*arrow.body,
                BlockStmtOrExpr::Expr(e) if matches!(
                    crate::compat::paren::unwrap_paren(e),
                    Expr::Cond(_)
                )
            ),
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

        // §6.8a-vi: 1:1 port of upstream `evaluateExpression(nodeExpression, meta)`.
        // Resolves the interpolation expression through the binding
        // resolver / recursive evaluator, returning the folded literal,
        // a recognised keyframes CallExpr, or the original expression
        // (via babelEvaluateExpression fallback). Without this, e.g.
        // `` `${fadeOut} 2s ease-in-out` `` where `fadeOut =
        // keyframes(...)` bypasses the keyframes detector below and
        // emits `var(--_xxx)` plus an inline `style` prop instead of
        // hoisting the `@keyframes` sheet and inlining the generated
        // keyframes name.
        let evaluated_interp = evaluate_expression(
            &node_expression,
            meta,
            scope_index,
            parent_scope,
            own_scope,
        )
        .value
        .unwrap_or_else(|| Box::new(node_expression.clone()));

        // §6.8b: 1:1 port of upstream `canBuildExpressionAsCss`
        // branch (build-css.ts:803-838). When the evaluated
        // interpolation is an ObjectExpression (or a Compiled CSS
        // tagged-template / call-expression), recurse buildCss into
        // it and emit its CSS items. Without this, e.g.
        // `` css`${color}` `` where `color = { color: 'blue' }`
        // panics inside the catch-all CSS-variable path that
        // expects scalar interpolations only.
        // Babel parser strips ParenthesizedExpression; SWC keeps it.
        // Unwrap before pattern-matching `Expr::Object` so the
        // SWC-shape `Paren(Object(...))` (from `() => ({...})`) is
        // recognised. See `crates/babel-plugin/src/compat/paren.rs`.
        let evaluated_inner = crate::compat::paren::unwrap_paren(&evaluated_interp);
        let does_expression_contain_css_block = matches!(evaluated_inner, Expr::Object(_))
            || is_compiled_css_tagged_template_expression(&evaluated_interp, meta.state)
            || is_compiled_css_call_expression(&evaluated_interp, meta.state);

        let does_expression_have_conditional_css = matches!(
            &node_expression,
            Expr::Arrow(arrow) if matches!(
                &*arrow.body,
                BlockStmtOrExpr::Expr(e) if matches!(
                    crate::compat::paren::unwrap_paren(e),
                    Expr::Cond(_)
                )
            )
        );

        let can_build_expression_as_css = (!_is_mid_statement && does_expression_contain_css_block)
            || does_expression_have_conditional_css
            || matches!(&node_expression, Expr::Tpl(_));

        if can_build_expression_as_css {
            // Upstream nests a `fragment`-context Metadata for the
            // template-literal recursion case. For the common
            // ObjectExpression / css-call shapes we keep the existing
            // meta context (matches upstream's `updatedMeta` branch).
            let saved_ctx = meta.context.clone();
            if matches!(&node_expression, Expr::Tpl(_)) {
                meta.context = MetadataContext::Fragment;
            }
            let result = build_css_inner(
                &evaluated_interp,
                meta,
                scope_index,
                parent_scope,
                own_scope,
                recorder,
            )?;
            meta.context = saved_ctx;

            if !result.css.is_empty() {
                // Upstream lines 832-836:
                //   css.push({ type: 'unconditional', css: acc + quasi.value.raw }, ...result.css);
                //   variables.push(...result.variables);
                //   return '';
                let prefix = std::mem::take(&mut acc);
                css.push(CssItem::Unconditional(UnconditionalCssItem {
                    css: format!("{}{}", prefix, raw),
                }));
                css.extend(result.css);
                variables.extend(result.variables);
                continue;
            }
        }

        // Reaching ANY of the below dispatch arms requires the evaluator.
        if try_keyframes_branch(
            &evaluated_interp,
            meta,
            &raw,
            &mut css,
            &mut variables,
            &mut acc,
            scope_index,
            parent_scope,
            own_scope,
            recorder,
        )? {
            continue;
        }

        // §4.4 hash-call-shape #3 (line 869): catch-all CSS-variable
        // emit with cssAffixInterpolation prefix-detection.
        let (expression, variable_name) =
            get_variable_declarator_value_for_own_path((*node.exprs[index]).clone().into(), meta);
        let next_quasi_raw = quasi_raws.get(index + 1).cloned().unwrap_or_default();
        let (before, after) = css_affix_interpolation(&raw, &next_quasi_raw);
        let suffix_marker = if before.variable_prefix == "-" {
            "-"
        } else {
            ""
        };
        let name = format!("--_{}{}", hash(&variable_name), suffix_marker);

        // 1:1 port of upstream `nextQuasis.value.raw = after.css`
        // (build-css.ts:874). Subsequent iterations read the mutated
        // value from `quasi_raws[index + 1]`. Strips the affix the
        // runtime call (`ix(value, prefix, suffix)`) re-adds, so the
        // CSS sheet doesn't double-wrap. Without this, e.g.
        // `content: "${dynamic}"` keeps the closing `"` in the CSS,
        // producing `content:"var(--_x);"<unclosed string>`.
        if let Some(next) = quasi_raws.get_mut(index + 1) {
            *next = after.css.clone();
        }

        let suffix = if after.variable_suffix.is_empty() {
            None
        } else {
            Some(after.variable_suffix.clone())
        };

        variables.push(Variable {
            name: name.clone(),
            expression,
            prefix: if before.variable_prefix.is_empty() {
                None
            } else {
                Some(before.variable_prefix.clone())
            },
            suffix,
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
                BlockStmtOrExpr::Expr(e) if matches!(
                    crate::compat::paren::unwrap_paren(e),
                    Expr::Bin(b) if matches!(
                        b.op,
                        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
                    )
                )
            ) {
                // §4.6 bridge: real evaluator dispatch. Surrounding JS
                // branch (LogicalCssItem emission keyed on the folded
                // value) is Phase 6 work; bridge discards the ResultPair.
                let _ = evaluate_expression(
                    &Expr::Arrow(arrow.clone()),
                    meta,
                    scope_index,
                    parent_scope,
                    own_scope,
                );
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
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<bool, CssBuildError> {
    if !is_compiled_keyframes_call_expression(expr, meta.state)
        && !is_compiled_keyframes_tagged_template_expression(expr, meta.state)
    {
        return Ok(false);
    }
    let result = extract_keyframes(expr, meta, raw, "", scope_index, parent_scope, own_scope, recorder)?;
    let mut iter = result.css.into_iter();
    let sheet = iter.next().expect("extract_keyframes returns ≥1 item");
    let unconditional = iter.next().expect("extract_keyframes returns ≥2 items");
    css.push(sheet);
    variables.extend(result.variables);
    acc.push_str(&get_item_css(&unconditional));
    Ok(true)
}

/// `extractArray` upstream lines 915–941.
pub fn extract_array(
    elements: &[Expr],
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    let mut css: Vec<CssItem> = Vec::new();
    let mut variables: Vec<Variable> = Vec::new();
    for element in elements {
        let result = if let Expr::Cond(c) = element {
            extract_conditional_expression(c, meta, scope_index, parent_scope, own_scope, recorder)?
        } else {
            build_css_inner(element, meta, scope_index, parent_scope, own_scope, recorder)?
        };
        css.extend(result.css);
        variables.extend(result.variables);
    }
    Ok(CSSOutput { css, variables })
}

/// `buildCss` upstream lines 949–1084 — the public dispatcher.
pub fn build_css(
    node: &Expr,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    build_css_inner(node, meta, scope_index, parent_scope, own_scope, recorder)
}

/// Internal entry — exists so the top-level extractArray path can
/// reach it without re-routing through the public API name.
fn build_css_inner(
    node: &Expr,
    meta: &mut Metadata<'_>,
    scope_index: &mut ScopeIndex,
    parent_scope: ScopeId,
    own_scope: Option<ScopeId>,
    recorder: &mut MutationRecorder,
) -> Result<CSSOutput, CssBuildError> {
    if let Expr::Lit(Lit::Str(s)) = node {
        return Ok(CSSOutput {
            css: vec![CssItem::Unconditional(UnconditionalCssItem {
                css: s.value.to_atom_lossy().as_str().to_string(),
            })],
            variables: vec![],
        });
    }

    if let Expr::TsAs(ts_as) = node {
        return build_css_inner(&ts_as.expr, meta, scope_index, parent_scope, own_scope, recorder);
    }

    if let Expr::Tpl(tpl) = node {
        return extract_template_literal(tpl, meta, scope_index, parent_scope, own_scope, recorder);
    }

    if let Expr::Object(obj) = node {
        return extract_object_expression(obj, meta, scope_index, parent_scope, own_scope, recorder);
    }

    if let Expr::Member(m) = node {
        return extract_member_expression(m, meta, scope_index, parent_scope, own_scope, recorder);
    }

    if let Expr::Arrow(arrow) = node {
        if let BlockStmtOrExpr::Expr(body_expr) = &*arrow.body {
            // SWC parser keeps `Expr::Paren` (e.g. `() => ({ x: 1 })`
            // body is `Paren(Object)`); Babel's parser strips it. Unwrap
            // before pattern-matching so the `t.isObjectExpression(node.body)`
            // check in upstream `buildCss` line 974 fires identically.
            // See `crates/babel-plugin/src/compat/paren.rs`.
            let body_inner = crate::compat::paren::unwrap_paren(body_expr);
            match body_inner {
                Expr::Object(obj) => {
                    return extract_object_expression(obj, meta, scope_index, parent_scope, own_scope, recorder)
                }
                Expr::Bin(b)
                    if matches!(
                        b.op,
                        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
                    ) =>
                {
                    return extract_logical_expression(arrow, meta, scope_index, parent_scope, own_scope, recorder);
                }
                Expr::Cond(c) => {
                    return extract_conditional_expression(c, meta, scope_index, parent_scope, own_scope, recorder)
                }
                Expr::Member(m) => {
                    return extract_member_expression(m, meta, scope_index, parent_scope, own_scope, recorder)
                }
                _ => {}
            }
        }
    }

    if let Expr::Ident(i) = node {
        // §6.8b: 1:1 port of upstream `buildCss` Identifier branch
        // (build-css.ts:992-1024). Replaces the §4.6 stub that
        // discarded the resolved binding. Without this, references
        // like `styled.div([styles, ...])` where `styles = {...}` fall
        // through to the catch-all "unable to extract" error.
        let resolved = resolve_binding(
            i.sym.as_str(),
            &*meta,
            &*scope_index,
            parent_scope,
            own_scope,
        );
        let Some(resolved) = resolved else {
            return Err(build_code_frame_error(
                "Variable could not be found".to_string(),
                Some(i.span),
            ));
        };
        let Some(node_expr) = resolved.node.as_ref() else {
            // Upstream throws `${node.type} isn't a supported CSS
            // type` — the only non-Expression resolvedBinding.node
            // shape that surfaces is when the binding has no init
            // (e.g. `let x; ... <div css={x} />`). Use a generic
            // node-type label since `binding.node` is `Option<Box<Expr>>`
            // and we don't carry the original AST node kind.
            return Err(build_code_frame_error(
                "Identifier isn't a supported CSS type - try using an object or string".to_string(),
                Some(i.span),
            ));
        };
        // cssMap-collision check — upstream's `meta.state.cssMap[node.name]`
        // throw. The cssMap registry is populated by §6.3's cssMap handler.
        if meta.state.css_map().contains_key(i.sym.as_str()) {
            return Err(build_code_frame_error(
                crate::utils::css_map::create_error_message(
                    "You must use the variant of a CSS Map object (eg. `styles.root`), not the root object itself, eg. `styles`."
                ),
                Some(i.span),
            ));
        };

        // Recurse with the appropriate scope. For same-file, keep the
        // current scope chain; for cross-file imports, build a fresh
        // ScopeIndex from the imported module's AST and route through
        // its program scope (mirrors §5.6's cross-file dispatch).
        let result = if resolved.source == BindingSource::Import {
            if let Some(imported_module) = resolved.imported_module.as_ref() {
                let mut imp_idx = ScopeIndex::build(&**imported_module);
                let imp_prog = imp_idx.program_scope();
                let cloned = node_expr.clone();
                build_css_inner(&cloned, meta, &mut imp_idx, imp_prog, None, recorder)?
            } else {
                let cloned = node_expr.clone();
                build_css_inner(&cloned, meta, scope_index, parent_scope, own_scope, recorder)?
            }
        } else {
            let cloned = node_expr.clone();
            build_css_inner(&cloned, meta, scope_index, parent_scope, own_scope, recorder)?
        };

        // assertNoImportedCssVariables — upstream lines 333-346:
        // imported binding that produced CSS variables means we'd
        // need to ensure all identifiers are added to the owning
        // file. Throws to deopt.
        if resolved.source == BindingSource::Import && !result.variables.is_empty() {
            return Err(build_code_frame_error(
                "Identifier contains values that can't be statically evaluated".to_string(),
                Some(i.span),
            ));
        }

        return Ok(result);
    }

    if let Expr::Array(ArrayLit { elems, .. }) = node {
        let exprs: Vec<Expr> = elems
            .iter()
            .filter_map(|opt| opt.as_ref().map(|e| (*e.expr).clone()))
            .collect();
        return extract_array(&exprs, meta, scope_index, parent_scope, own_scope, recorder);
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
            let result = build_css_inner(right, meta, scope_index, parent_scope, own_scope, recorder)?;
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
            return build_css_inner(
                &Expr::Tpl((**tpl).clone()),
                meta,
                scope_index,
                parent_scope,
                own_scope,
                recorder,
            );
        }
    }

    if is_compiled_css_call_expression(node, meta.state) {
        if let Expr::Call(CallExpr { args, .. }) = node {
            if let Some(first) = args.first() {
                if let Expr::Object(obj) = &*first.expr {
                    return build_css_inner(
                        &Expr::Object(obj.clone()),
                        meta,
                        scope_index,
                        parent_scope,
                        own_scope,
                        recorder,
                    );
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
    use crate::compat::scope::ScopeIndex;
    use crate::state::State;
    use crate::types::{Metadata, MetadataContext};
    use compiled_utils::hash;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{ExprOrSpread, Ident, Module, Number, PropName, Str};

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
            own_scope_override: None,
        }
    }

    /// Build a `(ScopeIndex, ScopeId)` pair from an empty Module —
    /// the §4.4 hash-site tests don't exercise binding lookup, so an
    /// empty scope is sufficient. Phase 6 handler tests will build
    /// from a real Module containing the test expression.
    fn empty_scope() -> (ScopeIndex, crate::compat::scope::ScopeId) {
        let module = Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        };
        let idx = ScopeIndex::build(&module);
        let prog = idx.program_scope();
        (idx, prog)
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
        let (mut idx, prog) = empty_scope();
        let mut recorder = MutationRecorder::new();
        let result = extract_keyframes(&call, &mut meta, "", "", &mut idx, prog, None, &mut recorder)
            .expect("extracts");
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
        // `--_${hash(variableName)}`.
        //
        // §6.8a-vi: prior shape used `UnaryExpression(-1)` to reach the
        // catch-all, but that relied on `extract_object_expression`'s
        // evaluator-stub bypassing the resolver. With the evaluator
        // wired, `-1` folds to `NumLit(-1)` and hits the unit-applied
        // numeric branch (matching upstream Babel). Switch to an
        // unresolved Ident — `evaluate_expression` returns the Ident
        // unchanged via `babel_evaluate_expression`'s fallback, and
        // `Expr::Ident` matches none of the typed branches → catch-all
        // fires.
        let mut state = State::default();
        let unresolved = Box::new(Expr::Ident(Ident::new(
            "unresolved".into(),
            DUMMY_SP,
            Default::default(),
        )));
        let obj = ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(
                swc_core::ecma::ast::KeyValueProp {
                    key: PropName::Ident(swc_core::ecma::ast::IdentName::new(
                        "marginTop".into(),
                        DUMMY_SP,
                    )),
                    value: unresolved.clone(),
                },
            )))],
        };

        let expected_var_name = format!("--_{}", hash(&generate(&unresolved)));
        let mut meta = fresh_meta(&mut state);
        let (mut idx, prog) = empty_scope();
        let mut recorder = MutationRecorder::new();
        let result = extract_object_expression(&obj, &mut meta, &mut idx, prog, None, &mut recorder)
            .expect("extracts");
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
        let (mut idx, prog) = empty_scope();
        let mut recorder = MutationRecorder::new();
        let result = extract_template_literal(&tpl, &mut meta, &mut idx, prog, None, &mut recorder)
            .expect("extracts");
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
