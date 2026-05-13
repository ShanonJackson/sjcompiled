//! Port of `src/lib/addSpace.js`.

use postcss_value_parser::parse::{Node, NodeKind};

pub fn add_space() -> Node {
    Node {
        kind: NodeKind::Space,
        value: " ".to_string(),
        before: String::new(),
        after: String::new(),
        quote: None,
        unclosed: false,
        nodes: Vec::new(),
        source_index: 0,
        source_end_index: 0,
    }
}
