//! Port of `packages/css/src/plugins/expand-shorthands/margin.ts`.
//!
//! ```ts
//! const [top, right = top, bottom = top, left = right] = value.nodes;
//! return [
//!   { prop: 'margin-top', value: top.toString() },
//!   { prop: 'margin-right', value: right.toString() },
//!   { prop: 'margin-bottom', value: bottom.toString() },
//!   { prop: 'margin-left', value: left.toString() },
//! ];
//! ```

use postcss_values_parser::{stringify_standalone, Node, Root};

use super::types::Longform;

pub fn margin(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.is_empty() {
        return Vec::new();
    }
    // top = nodes[0], right = nodes[1] ?? top, bottom = nodes[2] ?? top, left = nodes[3] ?? right.
    let top: &Node = &nodes[0];
    let right: &Node = nodes.get(1).unwrap_or(top);
    let bottom: &Node = nodes.get(2).unwrap_or(top);
    let left: &Node = nodes.get(3).unwrap_or(right);

    vec![
        Longform::new("margin-top", stringify_standalone(top)),
        Longform::new("margin-right", stringify_standalone(right)),
        Longform::new("margin-bottom", stringify_standalone(bottom)),
        Longform::new("margin-left", stringify_standalone(left)),
    ]
}
