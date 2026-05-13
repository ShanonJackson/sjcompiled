//! Port of `src/rules/grid.js`.

use postcss_value_parser::parse::Node;
use postcss_value_parser::stringify;
use postcss_value_parser::walk;

use super::super::helpers::join_grid_value::join_grid_value;

pub fn normalize_grid_auto_flow(mut parsed_nodes: Vec<Node>) -> String {
    let mut front = String::new();
    let mut back = String::new();
    let mut should_normalize = false;

    walk(
        &mut parsed_nodes,
        |node, _i| -> Option<bool> {
            // Upstream uses bare `===` (no toLowerCase / no trim) for the
            // first branch and `.trim().toLowerCase()` for the second —
            // asymmetric. Port verbatim.
            if node.value == "dense" {
                should_normalize = true;
                back = node.value.clone();
            } else if {
                let trimmed = node.value.trim().to_lowercase();
                trimmed == "row" || trimmed == "column"
            } {
                should_normalize = true;
                front = node.value.clone();
            } else {
                should_normalize = false;
            }
            None
        },
        false,
    );

    if should_normalize {
        let f = front.trim();
        let b = back.trim();
        return format!("{f} {b}");
    }
    stringify(&parsed_nodes)
}

pub fn normalize_grid_column_row_gap(mut parsed_nodes: Vec<Node>) -> String {
    let mut front = String::new();
    let mut back = String::new();
    let mut should_normalize = false;

    walk(
        &mut parsed_nodes,
        |node, _i| -> Option<bool> {
            if node.value == "normal" {
                should_normalize = true;
                front = node.value.clone();
            } else {
                back = format!("{back} {}", node.value);
            }
            None
        },
        false,
    );

    if should_normalize {
        let f = front.trim();
        let b = back.trim();
        return format!("{f} {b}");
    }
    stringify(&parsed_nodes)
}

pub fn normalize_grid_column_row(parsed_nodes: Vec<Node>) -> String {
    // Upstream: `grid.toString().split('/')`. JS `.split(' ')` is a literal
    // single-space split; multiple spaces produce empty-string segments.
    let raw = stringify(&parsed_nodes);
    let segments: Vec<&str> = raw.split('/').collect();

    if segments.len() > 1 {
        let mapped: Vec<String> = segments.iter().map(|seg| normalize_segment(seg)).collect();
        return join_grid_value(&mapped);
    }

    // Single-segment branch: upstream returns `gridValue.map(...)` — a
    // 1-element string array. The caller does `result.toString()`, which on
    // a 1-element array equals the element. So we return the element.
    normalize_segment(segments[0])
}

fn normalize_segment(line: &str) -> String {
    let mut front = String::new();
    let mut back = String::new();
    let trimmed = line.trim();
    for token in trimmed.split(' ') {
        if token == "span" {
            front = token.to_string();
        } else {
            back = format!("{back} {token}");
        }
    }
    let f = front.trim();
    let b = back.trim();
    format!("{f} {b}")
}
