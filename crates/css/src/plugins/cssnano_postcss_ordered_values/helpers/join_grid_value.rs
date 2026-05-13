//! Port of `src/lib/joinGridValue.js`.
//!
//! Upstream: `arr.join(' / ').trim()`. JS `trim` strips ECMA-262 Table 33
//! whitespace. We mirror with `str::trim` (Unicode `White_Space`) — the
//! superset gap (NEL / OGHAM) is irrelevant here because every value comes
//! from value-parser tokens that have already been split on ASCII space.

pub fn join_grid_value(grid: &[String]) -> String {
    grid.join(" / ").trim().to_string()
}
