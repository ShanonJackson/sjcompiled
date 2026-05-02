//! crates/cssnano-postcss-ordered-values
//! Byte-for-byte Rust port of `postcss-ordered-values@5.1.3`.
//! See `crates/PARITY_VERSIONS.md` and
//! `crates/_vendor/POSTCSS_ORDERED_VALUES_5.1.3_REAUDIT.md`.
//!
//! Folder/file mapping (1:1 with upstream `src/`, modulo `lib/` →
//! `helpers/` to avoid shadowing Rust's crate root `lib.rs`):
//!
//!   - `src/index.js`              -> `src/lib.rs` (this file)
//!   - `src/lib/addSpace.js`       -> `src/helpers/add_space.rs`
//!   - `src/lib/getValue.js`       -> `src/helpers/get_value.rs`
//!   - `src/lib/joinGridValue.js`  -> `src/helpers/join_grid_value.rs`
//!   - `src/lib/mathfunctions.js`  -> `src/helpers/math_functions.rs`
//!   - `src/lib/vendorUnprefixed.js` -> `src/helpers/vendor_unprefixed.rs`
//!   - `src/rules/animation.js`    -> `src/rules/animation.rs`
//!   - `src/rules/border.js`       -> `src/rules/border.rs`
//!   - `src/rules/boxShadow.js`    -> `src/rules/box_shadow.rs`
//!   - `src/rules/columns.js`      -> `src/rules/columns.rs`
//!   - `src/rules/flexFlow.js`     -> `src/rules/flex_flow.rs`
//!   - `src/rules/grid.js`         -> `src/rules/grid.rs`
//!   - `src/rules/listStyle.js`    -> `src/rules/list_style.rs`
//!   - `src/rules/transition.js`   -> `src/rules/transition.rs`
//!   - `src/rules/listStyleTypes.json` -> `src/rules/list_style_types.rs`

pub mod helpers;
pub mod rules;

use indexmap::IndexMap;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::{Node, NodeKind};
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, walk as vp_walk};

use crate::helpers::vendor_unprefixed::vendor_unprefixed;

#[derive(Debug, Clone, Copy)]
enum RuleKind {
    Animation,
    Border,
    BoxShadow,
    FlexFlow,
    Transition,
    ListStyle,
    Columns,
    GridAutoFlow,
    GridColumnRowGap,
    GridColumnRow,
}

/// Mirrors upstream `rules.get(normalizedProp)`. `outline` and
/// `column-rule` route to `border`; `columns` to the columns reorderer.
fn rule_for_prop(prop: &str) -> Option<RuleKind> {
    match prop {
        "animation" => Some(RuleKind::Animation),
        "outline" => Some(RuleKind::Border),
        "box-shadow" => Some(RuleKind::BoxShadow),
        "flex-flow" => Some(RuleKind::FlexFlow),
        "list-style" => Some(RuleKind::ListStyle),
        "transition" => Some(RuleKind::Transition),
        "border"
        | "border-block"
        | "border-inline"
        | "border-block-end"
        | "border-block-start"
        | "border-inline-end"
        | "border-inline-start"
        | "border-top"
        | "border-right"
        | "border-bottom"
        | "border-left" => Some(RuleKind::Border),
        "grid-auto-flow" => Some(RuleKind::GridAutoFlow),
        "grid-column-gap" | "grid-row-gap" => Some(RuleKind::GridColumnRowGap),
        "grid-column" | "grid-row" | "grid-row-start" | "grid-row-end" | "grid-column-start"
        | "grid-column-end" => Some(RuleKind::GridColumnRow),
        "column-rule" => Some(RuleKind::Border),
        "columns" => Some(RuleKind::Columns),
        _ => None,
    }
}

const VARIABLE_FUNCTIONS: &[&str] = &["var", "env", "constant"];

fn is_variable_function(node: &VNode) -> bool {
    if !matches!(node.kind, VKind::Function) { return false; }
    let lower = node.value.to_lowercase();
    VARIABLE_FUNCTIONS.iter().any(|&v| v == lower)
}

/// Mirrors upstream `shouldAbort(parsed)` — bails on:
///   - any comment node anywhere in the tree
///   - any function whose lowercased name is `var`/`env`/`constant`
///   - any word containing `___CSS_LOADER_IMPORT___`
fn should_abort(parsed: &mut [VNode]) -> bool {
    let mut abort = false;
    vp_walk(
        parsed,
        |node, _i| -> Option<bool> {
            if node.kind == VKind::Comment {
                abort = true;
                return Some(false);
            }
            if is_variable_function(node) {
                abort = true;
                return Some(false);
            }
            if node.kind == VKind::Word && node.value.contains("___CSS_LOADER_IMPORT___") {
                abort = true;
                return Some(false);
            }
            None
        },
        false,
    );
    abort
}

/// Mirrors upstream `getValue(decl)` — prefers `raws.value.raw` over
/// `decl.value` when present.
fn decl_input_value(decl_node: &Node) -> String {
    if let NodeKind::Declaration(d) = &decl_node.kind {
        if let Some(raw) = decl_node.raws.value.as_ref() {
            return raw.raw.clone();
        }
        return d.value.clone();
    }
    String::new()
}

fn dispatch(kind: RuleKind, parsed: Vec<VNode>) -> String {
    match kind {
        RuleKind::Animation => rules::animation::normalize_animation(parsed),
        RuleKind::Border => rules::border::normalize_border(&parsed),
        RuleKind::BoxShadow => rules::box_shadow::normalize_box_shadow(parsed),
        RuleKind::FlexFlow => rules::flex_flow::normalize_flex_flow(parsed),
        RuleKind::Transition => rules::transition::normalize_transition(parsed),
        RuleKind::ListStyle => rules::list_style::normalize_list_style(parsed),
        RuleKind::Columns => rules::columns::normalize_columns(parsed),
        RuleKind::GridAutoFlow => rules::grid::normalize_grid_auto_flow(parsed),
        RuleKind::GridColumnRowGap => rules::grid::normalize_grid_column_row_gap(parsed),
        RuleKind::GridColumnRow => rules::grid::normalize_grid_column_row(parsed),
    }
}

pub fn postcss_ordered_values(root: &mut Root) -> PluginResult {
    // Mirrors upstream `prepare()` returning a closure with a per-CSS
    // cache. Keys: original input `value` (post-`getValue`). Stores the
    // mapped output, OR `value` itself on bail (length<2 / shouldAbort).
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_decls_mut(&mut root.root, &mut |decl_node, _ctx| {
        let prop = match &decl_node.kind {
            NodeKind::Declaration(d) => d.prop.clone(),
            _ => return Mutation::Keep,
        };

        let normalized_prop = vendor_unprefixed(&prop.to_lowercase());
        let kind = match rule_for_prop(&normalized_prop) {
            Some(k) => k,
            None => return Mutation::Keep,
        };

        let value = decl_input_value(decl_node);

        // Cache hit branch — JS unconditionally assigns `decl.value`.
        if let Some(cached) = cache.get(&value).cloned() {
            if let NodeKind::Declaration(d) = &mut decl_node.kind {
                d.value = cached;
            }
            return Mutation::Keep;
        }

        // Cache miss — parse + bail check + dispatch.
        let mut parsed = vp_parse(&value);
        if parsed.len() < 2 || should_abort(&mut parsed) {
            // JS: `cache.set(value, value); return;` — DOES NOT touch
            // `decl.value`. Preserve the raws.value relationship.
            cache.insert(value.clone(), value);
            return Mutation::Keep;
        }

        let result = dispatch(kind, parsed);
        cache.insert(value, result.clone());
        if let NodeKind::Declaration(d) = &mut decl_node.kind {
            d.value = result;
        }

        Mutation::Keep
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_ordered_values(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_blank() { assert_eq!(run(""), ""); }

    #[test]
    fn no_op_unmapped_prop() {
        // `padding` is not in the rule map; left untouched.
        let css = ".a { padding: 1px 2px 3px 4px; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn border_reorders_to_width_style_color() {
        let out = run(".a { border: red solid 1px; }");
        assert!(out.contains("border: 1px solid red"), "got: {out:?}");
    }

    #[test]
    fn outline_routes_to_border() {
        let out = run(".a { outline: red solid 2px; }");
        assert!(out.contains("outline: 2px solid red"), "got: {out:?}");
    }

    #[test]
    fn flex_flow_last_match_wins_anomaly() {
        // Anomaly #2 in audit: bare assignment, last wrap/direction wins.
        let out = run(".a { flex-flow: row column nowrap; }");
        assert!(out.contains("flex-flow: column nowrap"), "got: {out:?}");
    }

    #[test]
    fn box_shadow_math_fn_aborts() {
        // Anomaly #4: math function in shadow → return original.
        let css = ".a { box-shadow: 1px 1px calc(2px + 1px) red; }";
        let out = run(css);
        assert!(out.contains("calc(2px + 1px)"), "got: {out:?}");
    }

    #[test]
    fn var_function_short_circuits() {
        // shouldAbort: var() bails the transform.
        let css = ".a { border: 1px solid var(--color); }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn comment_in_value_short_circuits() {
        let css = ".a { border: /* hi */ 1px solid red; }";
        // Decl input value (raws.value.raw) preserves the comment;
        // shouldAbort sees the value-parser comment node and bails.
        let out = run(css);
        assert!(out.contains("/* hi */"), "got: {out:?}");
    }

    #[test]
    fn css_loader_import_marker_short_circuits() {
        let css = ".a { border: 1px ___CSS_LOADER_IMPORT___1 solid; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn vendor_prefixed_animation_routes_correctly() {
        // `-webkit-animation` → `vendorUnprefixed` → `animation`.
        let out = run(".a { -webkit-animation: 1s ease myAnim; }");
        assert!(out.contains("myAnim 1s ease"), "got: {out:?}");
    }

    #[test]
    fn list_style_double_none_anomaly() {
        // Anomaly: second `none` goes to `image`, not `type`. With an
        // empty `position`, the output template `${type} ${position} ${image}`
        // emits TWO spaces between the two `none` tokens — verbatim with
        // upstream JS (verified by parity test
        // `14_list_style.css` line `e`).
        let out = run(".a { list-style: none none; }");
        assert!(out.contains("list-style: none  none"), "got: {out:?}");
    }

    #[test]
    fn columns_unit_plus_int() {
        let out = run(".a { columns: 2 200px; }");
        assert!(out.contains("columns: 200px 2"), "got: {out:?}");
    }

    #[test]
    fn cache_hit_assigns_decl_value() {
        // Same input value seen twice — second decl hits cache. Output
        // must be identical for both.
        let out = run(".a { border: red 1px solid; } .b { border: red 1px solid; }");
        let count = out.matches("border: 1px solid red").count();
        assert_eq!(count, 2, "got: {out:?}");
    }

    #[test]
    fn short_value_does_not_modify_decl() {
        // `parsed.nodes.length < 2` → cache.set(value, value) but do NOT
        // touch decl.value. Output should byte-equal input.
        let css = ".a { border: red; }";
        assert_eq!(run(css), css);
    }
}
