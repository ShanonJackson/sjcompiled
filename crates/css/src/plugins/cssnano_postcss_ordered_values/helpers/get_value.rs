//! Port of `src/lib/getValue.js`.
//!
//! Upstream: stringifies `Node[][]` after flattening. Drops the trailing
//! `space` of the LAST segment, mutates the LAST node of every non-final
//! segment to `{type: 'div', value: ','}`. Mutation is in place upstream;
//! we mutate the Vec we own here.

use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::stringify;

pub fn get_value(values: Vec<Vec<Node>>) -> String {
    stringify(&flatten(values))
}

fn flatten(values: Vec<Vec<Node>>) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let total = values.len();
    for (index, arg) in values.into_iter().enumerate() {
        let arg_len = arg.len();
        for (idx, val) in arg.into_iter().enumerate() {
            // Mirrors upstream `if (idx === arg.length - 1 && index === values.length - 1 && val.type === 'space') return;`
            if idx == arg_len.saturating_sub(1)
                && index == total.saturating_sub(1)
                && val.kind == NodeKind::Space
            {
                continue;
            }
            nodes.push(val);
        }

        // For all non-final segments, flip the last pushed node to `,` div.
        if index != total.saturating_sub(1) {
            if let Some(last) = nodes.last_mut() {
                last.kind = NodeKind::Div;
                last.value = ",".to_string();
            }
        }
    }
    nodes
}
