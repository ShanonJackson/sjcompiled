//! Port of `src/rules/transition.js`.
//!
//! `transition: [ none | <single-transition-property> ] || <time> ||
//!  <single-transition-timing-function> || <time>`. State buckets:
//! property / time1 / timing-function / time2.

use cssnano_utils::get_arguments;
use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::unit::parse_unit;

use crate::helpers::add_space::add_space;
use crate::helpers::get_value::get_value;

fn is_keyword_timing(value: &str) -> bool {
    matches!(
        value,
        "ease" | "linear" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end"
    )
}

fn is_function_timing(value: &str, kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Function) && matches!(value, "steps" | "cubic-bezier")
}

#[derive(Default)]
struct State {
    timing_function: Vec<Node>,
    property: Vec<Node>,
    time1: Vec<Node>,
    time2: Vec<Node>,
}

fn normalize(args: Vec<Vec<Node>>) -> Vec<Vec<Node>> {
    let mut list: Vec<Vec<Node>> = Vec::with_capacity(args.len());

    for arg in args {
        let mut state = State::default();

        for node in arg {
            if node.kind == NodeKind::Space { continue; }
            let lower = node.value.to_lowercase();
            if is_function_timing(&lower, &node.kind) {
                state.timing_function.push(node);
                state.timing_function.push(add_space());
            } else if parse_unit(&node.value).is_some() {
                if state.time1.is_empty() {
                    state.time1.push(node);
                    state.time1.push(add_space());
                } else {
                    state.time2.push(node);
                    state.time2.push(add_space());
                }
            } else if is_keyword_timing(&lower) {
                state.timing_function.push(node);
                state.timing_function.push(add_space());
            } else {
                state.property.push(node);
                state.property.push(add_space());
            }
        }

        let mut combined: Vec<Node> = Vec::new();
        combined.extend(state.property);
        combined.extend(state.time1);
        combined.extend(state.timing_function);
        combined.extend(state.time2);
        list.push(combined);
    }

    list
}

pub fn normalize_transition(parsed_nodes: Vec<Node>) -> String {
    let args = get_arguments(&parsed_nodes, |n: &Node| {
        n.kind == NodeKind::Div && n.value == ","
    });
    let values = normalize(args);
    get_value(values)
}
