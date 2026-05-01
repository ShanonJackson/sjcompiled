//! Port of `packages/css/src/plugins/expand-shorthands/text-decoration.ts`.
//!
//! Up to three nodes. Each node may be:
//! - a line value (multiple allowed: `underline overline` etc; sorted
//!   alphabetically before joining).
//! - a color (single).
//! - a style (single).
//! Defaults: color=`currentColor`, line=`none`, style=`solid`.

use postcss_values_parser::{Node, NodeKind, Root};

use super::types::Longform;
use super::utils::{is_color, GLOBAL_VALUES};

const LINE_VALUES: &[&str] = &["none", "underline", "overline", "line-through", "blink"];
const STYLE_VALUES: &[&str] = &["solid", "double", "dotted", "dashed", "wavy"];

#[derive(Default)]
struct Slots {
    color: String,
    style: String,
    line: Vec<String>,
}

fn extract(node: Option<&Node>, slots: &mut Slots) -> bool {
    let Some(n) = node else { return false; };
    if let NodeKind::Word(w) = &n.kind {
        let v = w.common.value.clone();
        // Match upstream order: line set FIRST (includes globals).
        if GLOBAL_VALUES.contains(&v.as_str()) || LINE_VALUES.contains(&v.as_str()) {
            // Empty list OR not already containing this value → push.
            // Otherwise: invalid (duplicate) → return true.
            if slots.line.is_empty() || !slots.line.contains(&v) {
                slots.line.push(v);
            } else {
                return true;
            }
        } else if is_color(n) {
            slots.color = v;
        } else if GLOBAL_VALUES.contains(&v.as_str()) || STYLE_VALUES.contains(&v.as_str()) {
            slots.style = v;
        }
        // Anything else: silently ignored upstream (no early-return).
    }
    false
}

pub fn text_decoration(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    let mut slots = Slots::default();
    if extract(nodes.first(), &mut slots)
        || extract(nodes.get(1), &mut slots)
        || extract(nodes.get(2), &mut slots)
    {
        return Vec::new();
    }
    // Upstream sorts the line values for deterministic ordering.
    slots.line.sort();
    let line_value = if slots.line.is_empty() {
        "none".to_string()
    } else {
        slots.line.join(" ")
    };

    vec![
        Longform::new(
            "text-decoration-color",
            if slots.color.is_empty() { "currentColor".to_string() } else { slots.color },
        ),
        Longform::new("text-decoration-line", line_value),
        Longform::new(
            "text-decoration-style",
            if slots.style.is_empty() { "solid".to_string() } else { slots.style },
        ),
    ]
}
