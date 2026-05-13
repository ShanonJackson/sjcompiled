//! Port of `packages/css/src/plugins/expand-shorthands/flex-flow.ts`.
//!
//! Two-arg destructure on `value.nodes`. `extractValues` mutates two
//! captured strings; if it returns `true` we bail out with `[]`.

use crate::vendor::postcss_values_parser::{Node, NodeKind, Root};

use super::types::Longform;
use super::utils::GLOBAL_VALUES;

const DIRECTION_VALUES: &[&str] = &["row", "row-reverse", "column", "column-reverse"];
const WRAP_VALUES: &[&str] = &["nowrap", "wrap", "reverse"];

fn extract(node: Option<&Node>, direction: &mut String, wrap: &mut String) -> bool {
    let Some(n) = node else { return false; };
    if let NodeKind::Word(w) = &n.kind {
        let v = w.common.value.as_str();
        // Direction set: globals + DIRECTION_VALUES.
        if GLOBAL_VALUES.contains(&v) || DIRECTION_VALUES.contains(&v) {
            if !direction.is_empty() {
                return true; // already set — invalid
            }
            *direction = v.to_string();
        } else if GLOBAL_VALUES.contains(&v) || WRAP_VALUES.contains(&v) {
            if !wrap.is_empty() {
                return true;
            }
            *wrap = v.to_string();
        } else {
            return true; // invalid keyword
        }
    }
    false
}

pub fn flex_flow(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    let mut direction = String::new();
    let mut wrap = String::new();

    if extract(nodes.first(), &mut direction, &mut wrap)
        || extract(nodes.get(1), &mut direction, &mut wrap)
    {
        return Vec::new();
    }

    vec![
        Longform::new("flex-direction", if direction.is_empty() { "row".to_string() } else { direction }),
        Longform::new("flex-wrap", if wrap.is_empty() { "nowrap".to_string() } else { wrap }),
    ]
}
