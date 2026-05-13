//! Port of `packages/css/src/plugins/expand-shorthands/*.ts`.
//!
//! Folder/file mapping (1:1 with upstream `expand-shorthands/`):
//!   - `index.ts`            -> `expand_shorthands.rs` (this file — entry)
//!   - `background.ts`       -> `background.rs`
//!   - `flex.ts`             -> `flex.rs`
//!   - `flex-flow.ts`        -> `flex_flow.rs`
//!   - `margin.ts`           -> `margin.rs`
//!   - `outline.ts`          -> `outline.rs`
//!   - `overflow.ts`         -> `overflow.rs`
//!   - `padding.ts`          -> `padding.rs`
//!   - `place-content.ts`    -> `place_content.rs`
//!   - `place-items.ts`      -> `place_items.rs`
//!   - `place-self.ts`       -> `place_self.rs`
//!   - `text-decoration.ts`  -> `text_decoration.rs`
//!   - `utils.ts`            -> `utils.rs`
//!   - `types.ts`            -> `types.rs`

pub mod background;
pub mod flex;
pub mod flex_flow;
pub mod margin;
pub mod outline;
pub mod overflow;
pub mod padding;
pub mod place_content;
pub mod place_items;
pub mod place_self;
pub mod text_decoration;
pub mod utils;
pub mod types;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::{Node, NodeKind, PluginError, PluginResult, Root};

use self::types::{ConversionFunction, Longform};
use self::utils::value_is_not_safe_to_expand;

/// `expandShorthands()` factory upstream. Walks every Declaration in
/// the tree; if the prop matches one of the supported shorthands and
/// the value is "safe to expand" (no `var(...)`), the decl is replaced
/// with N longform decls.
///
/// Returns an error only if a conversion function returns `None` — but
/// in this port every conversion function returns `Vec<Longform>` (no
/// `Option`), so this never fires. We keep the `PluginResult` signature
/// for API consistency with the rest of the plugin family.
pub fn expand_shorthands(root: &mut Root) -> PluginResult {
    let mut error: Option<PluginError> = None;
    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        if error.is_some() {
            return Mutation::Keep;
        }
        match process_declaration(node) {
            Some(new_decls) => Mutation::ReplaceMany(new_decls),
            None => Mutation::Keep,
        }
    });

    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Per-Declaration entry point. Mirrors the *body* of upstream's
/// `Declaration(decl)` visitor (packages/css/src/plugins/expand-
/// shorthands/index.ts:74-106).
///
/// - `Some(new_decls)`: the decl matched a shorthand prop and was
///   expanded; the caller must `replaceWith(new_decls)` (i.e.
///   `Mutation::ReplaceMany(new_decls)` in walk-time terms).
/// - `None`: not a shorthand, value contained `var(...)`, or the
///   expansion produced the no-op sentinel — leave the decl as-is.
///
/// Used by `crates/css/src/transform.rs` to interleave this visitor
/// with the other two Declaration visitors at each decl node in a
/// single walk (per `crates/PHASE_8B_LIFECYCLE_AUDIT.md` walk round).
pub fn process_declaration(node: &mut Node) -> Option<Vec<Node>> {
    let prop = match &node.kind {
        NodeKind::Declaration(d) => d.prop.clone(),
        _ => return None,
    };

    // Dispatch on prop name. Anything not in the table → no-op.
    let expand: ConversionFunction = match prop.as_str() {
        // Fully-expanded properties.
        "margin" => margin::margin,
        "padding" => padding::padding,
        "place-content" => place_content::place_content,
        "place-items" => place_items::place_items,
        "place-self" => place_self::place_self,
        "overflow" => overflow::overflow,
        "flex" => flex::flex,
        "flex-flow" => flex_flow::flex_flow,
        "outline" => outline::outline,
        "text-decoration" => text_decoration::text_decoration,
        // Partially-expanded.
        "background" => background::background,
        _ => return None,
    };

    let decl_value = match &node.kind {
        NodeKind::Declaration(d) => d.value.clone(),
        _ => return None,
    };
    let value_root = crate::vendor::postcss_values_parser::parse(&decl_value);

    // Bail if any top-level value node is `var(...)` — output isn't
    // determinable so we leave the decl alone.
    if value_root.nodes.iter().any(value_is_not_safe_to_expand) {
        return None;
    }

    let longforms: Vec<Longform> = expand(&value_root);

    // Early-exit when the expansion returned a single no-op
    // Longform (`prop: undefined`) — leave decl unchanged.
    if longforms.len() == 1 && longforms[0].prop.is_none() {
        return None;
    }

    // Build new decl nodes by cloning `node` and overriding prop+value.
    let new_decls: Vec<Node> = longforms
        .into_iter()
        .filter_map(|lf| {
            let mut clone = node.clone();
            if let NodeKind::Declaration(d) = &mut clone.kind {
                if let Some(p) = lf.prop {
                    d.prop = p;
                }
                d.value = lf.value;
                // Drop the cached raw value so the stringifier
                // re-emits the new value rather than the original
                // bytes. Mirrors upstream `decl.clone({ ...val })`
                // which carries raws.value but the stringifier's
                // `rawValue` check `raw.value === value` fails when
                // value changed.
                clone.raws.value = None;
                Some(clone)
            } else {
                None
            }
        })
        .collect();

    Some(new_decls)
}
