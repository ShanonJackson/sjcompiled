//! Port of `packages/css/src/plugins/expand-shorthands/utils.ts`.
//!
//! Helpers used by multiple conversion functions:
//! - [`GLOBAL_VALUES`] — the CSS `inherit / initial / unset / revert /
//!   revert-layer` keywords accepted in any value position.
//! - [`is_color`] — `(word|func) && isColor` plus the `transparent /
//!   currentcolor` special-cases.
//! - [`is_width`] — width-unit Numeric, or `auto / min-content /
//!   max-content / fit-content / inherit / initial / ...` Word, or
//!   any Func (we don't introspect return types).
//! - [`get_width`] — render Numeric/Word/Func into the longform
//!   `value` string for a width position.

use crate::vendor::colord::names::NAME_TO_HEX;
use once_cell::sync::Lazy;
use crate::vendor::postcss_values_parser::{stringify_standalone, Node, NodeKind};
use std::collections::HashSet;

pub const GLOBAL_VALUES: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

/// Functions whose return type is a color — mirrors the small set
/// upstream tags via `node.isColor`. Anything else (e.g. `var()`,
/// `calc()`) is NOT a color.
const COLOR_FUNCTIONS: &[&str] = &[
    "rgb", "rgba", "hsl", "hsla", "hwb", "color", "lab", "lch", "oklab", "oklch",
];

/// Hex-color regex: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, case-
/// insensitive. Mirrors the colorRegex in `postcss-values-parser/Word.js`.
fn is_hex_color(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'#') { return false; }
    let rest = &bytes[1..];
    if !matches!(rest.len(), 3 | 4 | 6 | 8) { return false; }
    rest.iter().all(|b| b.is_ascii_hexdigit())
}

static WIDTH_UNITS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "%", "cap", "ch", "cm", "em", "ex", "fr", "ic", "in", "lh", "mm", "pc",
        "pt", "px", "Q", "rem", "rlh", "vb", "vh", "vi", "vmax", "vmin", "vw",
    ])
});

/// `isColor(node)` upstream — `(word|func) && isColor` plus the
/// `transparent / currentcolor` special-cases. We compute the
/// `isColor` predicate inline (named-color lookup, hex regex, color
/// function name list) since our values-parser doesn't tag nodes
/// during parse.
pub fn is_color(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Word(w) => {
            let v = &w.common.value;
            // Special two cases (https://drafts.csswg.org/css-color/#named-colors).
            if v == "transparent" || v == "currentcolor" { return true; }
            // Hex.
            if is_hex_color(v) { return true; }
            // Named colors (case-insensitive: upstream `colorNames` array
            // is lowercase but JS comparison uses `colorNames.includes(value.toLowerCase())`).
            NAME_TO_HEX.contains_key(v.to_ascii_lowercase().as_str())
        }
        NodeKind::Func(f) => {
            COLOR_FUNCTIONS.iter().any(|&n| n.eq_ignore_ascii_case(&f.name))
        }
        _ => false,
    }
}

/// `isWidth(node)` upstream.
pub fn is_width(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Numeric(n) => WIDTH_UNITS.contains(n.unit.as_str()),
        NodeKind::Word(w) => {
            let v = w.common.value.as_str();
            if matches!(v, "auto" | "min-content" | "max-content" | "fit-content") {
                return true;
            }
            GLOBAL_VALUES.contains(&v)
        }
        // Upstream comment: "We don't want to be strict about functions,
        // as we don't know the return type."
        NodeKind::Func(_) => true,
        _ => false,
    }
}

/// `getWidth(node)` upstream.
pub fn get_width(node: &Node) -> String {
    match &node.kind {
        NodeKind::Numeric(n) => format!("{}{}", n.common.value, n.unit),
        NodeKind::Func(f) => {
            // Upstream: `${node.name}${node.params}`. `node.params` is
            // the params string between `(` and `)`. We rebuild from the
            // standalone stringification of the func, which yields
            // `name(params)`, then peel off the leading `name(` —
            // matching upstream's `name + params`. Easier: build directly.
            let mut out = String::new();
            out.push_str(&f.name);
            out.push('(');
            for child in &f.nodes {
                out.push_str(&child.raws_before);
                out.push_str(&stringify_standalone(child));
            }
            if !f.unclosed { out.push(')'); }
            out
        }
        NodeKind::Word(w) => w.common.value.clone(),
        // Upstream `getWidth` is typed `(Numeric|Word|Func)` — caller
        // must have gated via `isWidth`. We fall back to standalone
        // stringification for unexpected kinds.
        _ => stringify_standalone(node),
    }
}

/// Helper: returns true when the node should disqualify the entire
/// decl from expansion. Upstream:
/// ```ts
/// node.type === 'func' && node.isVar
/// ```
/// We compute `is_var` as `func.name == "var"` since our parser doesn't
/// tag the field during parse.
pub fn value_is_not_safe_to_expand(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Func(f) if f.name == "var")
}
