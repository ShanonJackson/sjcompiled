//! Port of `packages/css/src/utils/style-ordering.ts`.
//!
//! Ordered pseudo-selector buckets used by `sort-pseudo-selectors.ts`.
//! Drift here means the upstream constant changed — mirror upstream
//! exactly to keep class-ordering hashes stable.

/// `styleOrder` upstream — `LVFHA` plus `:focus-within`/`:focus-visible`.
/// Index in this list is one less than the sort score returned by
/// `getPseudoSelectorScore` (upstream returns `index + 1`, so unmatched
/// selectors score `0` and sort first).
pub const STYLE_ORDER: &[&str] = &[
    ":link",
    ":visited",
    ":focus-within",
    ":focus",
    ":focus-visible",
    ":hover",
    ":active",
];
