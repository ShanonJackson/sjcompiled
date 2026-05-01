//! Port of `packages/css/src/plugins/expand-shorthands/flex.ts`.
//!
//! Three-arg destructure on `value.nodes`. Eight branches:
//! - 1 arg keyword: `auto` / `none` / `initial` / `revert` /
//!   `revert-layer` / `unset` / `inherit` (inherit-family returns
//!   `[{value: …}]` no-op).
//! - 1 arg unitless number → flex-grow.
//! - 1 arg basis (numeric `0` / `0%`-default / width / `content`).
//! - 2 args: grow + shrink (number/number) OR grow + basis (number/basis).
//! - 3 args: grow + shrink + basis (number/number/basis).
//! - Anything else → invalid CSS, return `[]`.

use postcss_values_parser::{Node, NodeKind, Root};

use super::types::Longform;
use super::utils::{get_width, is_width};

/// Spec default. `0` would be more correct per the spec, but major
/// browsers use `0%` due to compatibility — see comment in upstream
/// `flex.ts`.
const FLEX_BASIS_DEFAULT: &str = "0%";

fn is_flex_number(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Numeric(n) if n.unit.is_empty())
}

/// Mirrors upstream `isFlexBasis` — `(word === 'content') OR (numeric === 0 unitless) OR isWidth`.
fn is_flex_basis(node: &Node) -> bool {
    if let NodeKind::Word(w) = &node.kind {
        if w.common.value == "content" { return true; }
    }
    if let NodeKind::Numeric(n) = &node.kind {
        if n.unit.is_empty() && n.common.value == "0" { return true; }
    }
    is_width(node)
}

/// Mirrors upstream `getBasisWidth` — special-case unitless `0` to
/// `0%`, else fall through to `getWidth`.
fn get_basis_width(node: &Node) -> String {
    if let NodeKind::Numeric(n) = &node.kind {
        if n.unit.is_empty() && n.common.value == "0" {
            return FLEX_BASIS_DEFAULT.to_string();
        }
    }
    get_width(node)
}

pub fn flex(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    let len = nodes.len();
    if len == 0 {
        return Vec::new();
    }

    match len {
        1 => {
            let left = &nodes[0];
            if let NodeKind::Word(w) = &left.kind {
                let v = w.common.value.as_str();
                match v {
                    "auto" => {
                        // `flex: auto` ↔ `flex: 1 1 auto`
                        return vec![
                            Longform::new("flex-grow", "1"),
                            Longform::new("flex-shrink", "1"),
                            Longform::new("flex-basis", "auto"),
                        ];
                    }
                    "none" => {
                        return vec![
                            Longform::new("flex-grow", "0"),
                            Longform::new("flex-shrink", "0"),
                            Longform::new("flex-basis", "auto"),
                        ];
                    }
                    "initial" => {
                        return vec![
                            Longform::new("flex-grow", "0"),
                            Longform::new("flex-shrink", "1"),
                            Longform::new("flex-basis", "auto"),
                        ];
                    }
                    "revert" | "revert-layer" | "unset" | "inherit" => {
                        // No-op. Upstream returns `[{ value: left.value }]`
                        // — a single Longform with prop=undefined. The
                        // caller's early-exit branch leaves the decl
                        // unchanged.
                        return vec![Longform::no_op(v)];
                    }
                    _ => {}
                }
            }
            if is_flex_number(left) {
                let n = match &left.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                return vec![
                    Longform::new("flex-grow", n.common.value.clone()),
                    Longform::new("flex-shrink", "1"),
                    Longform::new("flex-basis", FLEX_BASIS_DEFAULT),
                ];
            }
            if is_flex_basis(left) {
                return vec![
                    Longform::new("flex-grow", "1"),
                    Longform::new("flex-shrink", "1"),
                    Longform::new("flex-basis", get_width(left)),
                ];
            }
        }
        2 => {
            let left = &nodes[0];
            let middle = &nodes[1];
            if is_flex_number(left) {
                if is_flex_number(middle) {
                    let l = match &left.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                    let m = match &middle.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                    return vec![
                        Longform::new("flex-grow", l.common.value.clone()),
                        Longform::new("flex-shrink", m.common.value.clone()),
                        Longform::new("flex-basis", FLEX_BASIS_DEFAULT),
                    ];
                }
                if is_flex_basis(middle) {
                    let l = match &left.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                    return vec![
                        Longform::new("flex-grow", l.common.value.clone()),
                        Longform::new("flex-shrink", "1"),
                        Longform::new("flex-basis", get_width(middle)),
                    ];
                }
            }
        }
        3 => {
            let left = &nodes[0];
            let middle = &nodes[1];
            let right = &nodes[2];
            if is_flex_number(left) && is_flex_number(middle) && is_flex_basis(right) {
                let l = match &left.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                let m = match &middle.kind { NodeKind::Numeric(n) => n, _ => unreachable!() };
                return vec![
                    Longform::new("flex-grow", l.common.value.clone()),
                    Longform::new("flex-shrink", m.common.value.clone()),
                    Longform::new("flex-basis", get_basis_width(right)),
                ];
            }
        }
        _ => {}
    }

    // Invalid CSS — upstream returns `[]`, which the caller maps to
    // `decl.replaceWith([])` (drops the decl).
    Vec::new()
}
