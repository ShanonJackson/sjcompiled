//! Port of `src/rules/flexFlow.js`.
//!
//! `flex-flow: <flex-direction> || <flex-wrap>`. Last-match-wins.

use postcss_value_parser::parse::Node;
use postcss_value_parser::walk;

fn is_flex_direction(lower: &str) -> bool {
    matches!(lower, "row" | "row-reverse" | "column" | "column-reverse")
}

fn is_flex_wrap(lower: &str) -> bool {
    matches!(lower, "nowrap" | "wrap" | "wrap-reverse")
}

pub fn normalize_flex_flow(mut parsed_nodes: Vec<Node>) -> String {
    let mut direction = String::new();
    let mut wrap = String::new();

    walk(
        &mut parsed_nodes,
        |node, _i| -> Option<bool> {
            let lower = node.value.to_lowercase();
            if is_flex_direction(&lower) {
                direction = node.value.clone();
                return None;
            }
            if is_flex_wrap(&lower) {
                wrap = node.value.clone();
                return None;
            }
            None
        },
        false,
    );

    format!("{direction} {wrap}").trim().to_string()
}
