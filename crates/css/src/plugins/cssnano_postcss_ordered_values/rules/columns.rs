//! Port of `src/rules/columns.js`.

use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::stringify;
use postcss_value_parser::unit::parse_unit;
use postcss_value_parser::walk;

fn has_unit(value: &str) -> bool {
    match parse_unit(value) {
        Some(q) => !q.unit.is_empty(),
        None => false,
    }
}

pub fn normalize_columns(mut parsed_nodes: Vec<Node>) -> String {
    let mut widths: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();

    walk(
        &mut parsed_nodes,
        |node, _i| -> Option<bool> {
            if node.kind == NodeKind::Word {
                if has_unit(&node.value) {
                    widths.push(node.value.clone());
                } else {
                    other.push(node.value.clone());
                }
            }
            None
        },
        false,
    );

    if other.len() == 1 && widths.len() == 1 {
        // Upstream `value.trimStart()` — the values come from value-parser
        // Word tokens whose tokenization terminates on whitespace, so they
        // never contain leading whitespace. Mirror the JS call anyway for
        // byte fidelity.
        let w = widths[0].trim_start();
        let o = other[0].trim_start();
        return format!("{w} {o}");
    }

    stringify(&parsed_nodes)
}
