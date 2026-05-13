//! Port of `src/rules/border.js`.
//!
//! `border: <line-width> || <line-style> || <color>`
//! `outline: <outline-color> || <outline-style> || <outline-width>`
//!
//! Upstream `border.walk((node) => { ... return false; })` aborts the
//! walk's recursion into Function children on every branch — the walk
//! visits ONLY top-level value-parser nodes.

use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::stringify;
use postcss_value_parser::unit::parse_unit;

use super::super::helpers::math_functions::is_math_function;

fn is_border_width(lower: &str) -> bool {
    matches!(lower, "thin" | "medium" | "thick")
}

fn is_border_style(lower: &str) -> bool {
    matches!(
        lower,
        "none" | "auto" | "hidden" | "dotted" | "dashed" | "solid" | "double"
            | "groove" | "ridge" | "inset" | "outset"
    )
}

#[derive(Default, Debug)]
struct Order {
    width: String,
    style: String,
    color: String,
}

pub fn normalize_border(parsed_nodes: &[Node]) -> String {
    let mut order = Order::default();

    // Top-level only — upstream returns `false` from the cb on every branch
    // so the walk never recurses into Function children.
    for node in parsed_nodes {
        match node.kind {
            NodeKind::Word => {
                let lower = node.value.to_lowercase();
                if is_border_style(&lower) {
                    order.style = node.value.clone();
                    continue;
                }
                if is_border_width(&lower) || parse_unit(&lower).is_some() {
                    if !order.width.is_empty() {
                        order.width = format!("{} {}", order.width, node.value);
                    } else {
                        order.width = node.value.clone();
                    }
                    continue;
                }
                order.color = node.value.clone();
            }
            NodeKind::Function => {
                let lower = node.value.to_lowercase();
                if is_math_function(&lower) {
                    order.width = stringify(std::slice::from_ref(node));
                } else {
                    order.color = stringify(std::slice::from_ref(node));
                }
            }
            _ => {}
        }
    }

    format!("{} {} {}", order.width, order.style, order.color)
        .trim()
        .to_string()
}
