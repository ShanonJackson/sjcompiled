//! Port of `src/rules/listStyle.js` + `listStyleTypes.json`.

use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::stringify;
use postcss_value_parser::walk;

use crate::rules::list_style_types::LIST_STYLE_TYPES;

fn is_defined_type(value: &str) -> bool {
    LIST_STYLE_TYPES.iter().any(|&t| t == value)
}

fn is_defined_position(value: &str) -> bool {
    matches!(value, "inside" | "outside")
}

pub fn normalize_list_style(mut parsed_nodes: Vec<Node>) -> String {
    let mut type_ = String::new();
    let mut position = String::new();
    let mut image = String::new();

    walk(
        &mut parsed_nodes,
        |node, _i| -> Option<bool> {
            if node.kind == NodeKind::Word {
                if is_defined_type(&node.value) {
                    type_ = format!("{type_} {}", node.value);
                } else if is_defined_position(&node.value) {
                    position = format!("{position} {}", node.value);
                } else if node.value == "none" {
                    let already_has_none = type_
                        .split(' ')
                        .any(|e| e != "" && e != " " && e == "none");
                    if already_has_none {
                        image = format!("{image} {}", node.value);
                    } else {
                        type_ = format!("{type_} {}", node.value);
                    }
                } else {
                    type_ = format!("{type_} {}", node.value);
                }
            }
            if node.kind == NodeKind::Function {
                image = format!("{image} {}", stringify(std::slice::from_ref(node)));
            }
            None
        },
        false,
    );

    let t = type_.trim();
    let p = position.trim();
    let i = image.trim();
    format!("{t} {p} {i}").trim().to_string()
}
