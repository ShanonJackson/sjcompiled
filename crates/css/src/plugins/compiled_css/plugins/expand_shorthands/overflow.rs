//! Port of `packages/css/src/plugins/expand-shorthands/overflow.ts`.
//!
//! `[overflowX, overflowY = overflowX] = value.nodes`.

use crate::vendor::postcss_values_parser::{stringify_standalone, Node, Root};

use super::types::Longform;

pub fn overflow(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.is_empty() {
        return Vec::new();
    }
    let x: &Node = &nodes[0];
    let y: &Node = nodes.get(1).unwrap_or(x);
    vec![
        Longform::new("overflow-x", stringify_standalone(x)),
        Longform::new("overflow-y", stringify_standalone(y)),
    ]
}
