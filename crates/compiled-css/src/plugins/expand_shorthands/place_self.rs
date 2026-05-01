//! Port of `packages/css/src/plugins/expand-shorthands/place-self.ts`.
//!
//! `[alignSelf, justifySelf = alignSelf] = value.nodes`.

use postcss_values_parser::{stringify_standalone, Node, Root};

use super::types::Longform;

pub fn place_self(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.is_empty() {
        return Vec::new();
    }
    let align: &Node = &nodes[0];
    let justify: &Node = nodes.get(1).unwrap_or(align);
    vec![
        Longform::new("align-self", stringify_standalone(align)),
        Longform::new("justify-self", stringify_standalone(justify)),
    ]
}
