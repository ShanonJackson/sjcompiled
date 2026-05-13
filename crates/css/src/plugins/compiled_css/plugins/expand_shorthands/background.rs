//! Port of `packages/css/src/plugins/expand-shorthands/background.ts`.
//!
//! Only background-color is expanded. Anything else falls through as a
//! single Longform with prop=undefined → caller's early-exit branch
//! leaves the decl unchanged.
//!
//! Upstream:
//! ```ts
//! if (value.nodes.length === 1 && isColor(value.nodes[0])) {
//!   return [{ prop: 'background-color', value: value.nodes[0].toString() }];
//! }
//! return [{ value: value.nodes.join(' ') }];
//! ```

use crate::vendor::postcss_values_parser::{stringify_standalone, Root};

use super::types::Longform;
use super::utils::is_color;

pub fn background(value: &Root) -> Vec<Longform> {
    let nodes = &value.nodes;
    if nodes.len() == 1 && is_color(&nodes[0]) {
        return vec![Longform::new(
            "background-color",
            stringify_standalone(&nodes[0]),
        )];
    }
    // Upstream `value.nodes.join(' ')` — Array.prototype.join coerces
    // each element to its `toString()` (the standalone form) and joins
    // with " ". This is the early-exit case (no prop).
    let joined = nodes
        .iter()
        .map(stringify_standalone)
        .collect::<Vec<_>>()
        .join(" ");
    vec![Longform::no_op(joined)]
}
