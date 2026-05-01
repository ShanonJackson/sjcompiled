//! Port of `packages/css/src/plugins/expand-shorthands/padding.ts`.
//! Identical structure to `margin.ts` — just different prop names.

use postcss_values_parser::{stringify_standalone, Node, Root};

use super::types::Longform;

pub fn padding(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.is_empty() {
        return Vec::new();
    }
    let top: &Node = &nodes[0];
    let right: &Node = nodes.get(1).unwrap_or(top);
    let bottom: &Node = nodes.get(2).unwrap_or(top);
    let left: &Node = nodes.get(3).unwrap_or(right);

    vec![
        Longform::new("padding-top", stringify_standalone(top)),
        Longform::new("padding-right", stringify_standalone(right)),
        Longform::new("padding-bottom", stringify_standalone(bottom)),
        Longform::new("padding-left", stringify_standalone(left)),
    ]
}
