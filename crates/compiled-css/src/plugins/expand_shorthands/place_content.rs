//! Port of `packages/css/src/plugins/expand-shorthands/place-content.ts`.
//!
//! Special case from MDN: with a single value, `left / right / baseline`
//! aren't valid in BOTH longforms — the spec says invalidate the whole
//! decl in that case. Upstream returns `[]` to drop the decl.

use postcss_values_parser::{stringify_standalone, NodeKind, Root};

use super::types::Longform;

pub fn place_content(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.is_empty() {
        return Vec::new();
    }
    let align_content = &nodes[0];
    let justify_content = nodes.get(1);

    if justify_content.is_none() {
        if let NodeKind::Word(w) = &align_content.kind {
            if matches!(w.common.value.as_str(), "left" | "right" | "baseline") {
                return Vec::new();
            }
        }
    }

    let align = stringify_standalone(align_content);
    let justify = match justify_content {
        Some(j) => stringify_standalone(j),
        None => stringify_standalone(align_content),
    };

    vec![
        Longform::new("align-content", align),
        Longform::new("justify-content", justify),
    ]
}
