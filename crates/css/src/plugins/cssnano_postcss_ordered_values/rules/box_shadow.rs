//! Port of `src/rules/boxShadow.js`.
//!
//! `box-shadow: inset? && <length>{2,4} && <color>?`
//!
//! Aborts (returns the original `parsed.toString()`) if any function is a
//! math function (after `vendorUnprefixed`).

use crate::vendor::cssnano_utils::get_arguments;
use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::stringify;
use postcss_value_parser::unit::parse_unit;

use super::super::helpers::add_space::add_space;
use super::super::helpers::get_value::get_value;
use super::super::helpers::math_functions::is_math_function;
use super::super::helpers::vendor_unprefixed::vendor_unprefixed;

#[derive(Default)]
struct ArgState {
    inset: Vec<Node>,
    color: Vec<Node>,
}

fn normalize(args: Vec<Vec<Node>>) -> Option<Vec<Vec<Node>>> {
    let mut list: Vec<Vec<Node>> = Vec::with_capacity(args.len());
    let mut abort = false;

    for arg in args {
        let mut val: Vec<Node> = Vec::new();
        let mut state = ArgState::default();

        for node in arg {
            let lower = node.value.to_lowercase();
            if node.kind == NodeKind::Function && is_math_function(&vendor_unprefixed(&lower)) {
                abort = true;
                continue;
            }
            if node.kind == NodeKind::Space { continue; }

            if parse_unit(&node.value).is_some() {
                val.push(node);
                val.push(add_space());
            } else if lower == "inset" {
                state.inset.push(node);
                state.inset.push(add_space());
            } else {
                state.color.push(node);
                state.color.push(add_space());
            }
        }

        if abort { return None; }

        let mut combined: Vec<Node> = Vec::with_capacity(state.inset.len() + val.len() + state.color.len());
        combined.extend(state.inset);
        combined.extend(val);
        combined.extend(state.color);
        list.push(combined);
    }

    Some(list)
}

pub fn normalize_box_shadow(parsed_nodes: Vec<Node>) -> String {
    let original = stringify(&parsed_nodes);
    let args = get_arguments(&parsed_nodes, |n: &Node| {
        n.kind == NodeKind::Div && n.value == ","
    });

    match normalize(args) {
        Some(list) => get_value(list),
        None => original,
    }
}
