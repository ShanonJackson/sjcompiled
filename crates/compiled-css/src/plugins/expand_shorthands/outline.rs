//! Port of `packages/css/src/plugins/expand-shorthands/outline.ts`.
//!
//! Three-arg destructure. `extractValues` reads each node and assigns
//! one of color / style / width once. Setting the same slot twice is
//! invalid → return `[]`. Defaults: color=`currentColor`,
//! style=`none`, width=`medium`.

use postcss_values_parser::{Node, NodeKind, Root};

use super::types::Longform;
use super::utils::{is_color, GLOBAL_VALUES};

#[derive(Default)]
struct Slots {
    color: String,
    style: String,
    width: String,
}

const STYLE_VALUES: &[&str] = &[
    "auto", "none", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
];
const SIZE_VALUES: &[&str] = &["thin", "medium", "thick"];

fn extract(node: Option<&Node>, slots: &mut Slots) -> bool {
    let Some(n) = node else { return false; };
    match &n.kind {
        NodeKind::Word(w) => {
            let v = w.common.value.clone();
            if is_color(n) {
                if !slots.color.is_empty() { return true; }
                slots.color = v;
            } else if SIZE_VALUES.contains(&v.as_str()) {
                if !slots.width.is_empty() { return true; }
                slots.width = v;
            } else if GLOBAL_VALUES.contains(&v.as_str()) || STYLE_VALUES.contains(&v.as_str()) {
                if !slots.style.is_empty() { return true; }
                slots.style = v;
            } else {
                return true;
            }
        }
        NodeKind::Numeric(num) => {
            if !slots.width.is_empty() { return true; }
            slots.width = format!("{}{}", num.common.value, num.unit);
        }
        _ => {}
    }
    false
}

pub fn outline(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    let mut slots = Slots::default();
    if extract(nodes.first(), &mut slots)
        || extract(nodes.get(1), &mut slots)
        || extract(nodes.get(2), &mut slots)
    {
        return Vec::new();
    }
    vec![
        Longform::new(
            "outline-color",
            if slots.color.is_empty() { "currentColor".to_string() } else { slots.color },
        ),
        Longform::new(
            "outline-style",
            if slots.style.is_empty() { "none".to_string() } else { slots.style },
        ),
        Longform::new(
            "outline-width",
            if slots.width.is_empty() { "medium".to_string() } else { slots.width },
        ),
    ]
}
