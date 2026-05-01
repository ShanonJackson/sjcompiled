//! Port of `packages/utils/src/shorthand.ts`.
//!
//! Two tables:
//!
//! 1. [`shorthand_for`] — `Record<ShorthandProperties, true | string[]>`.
//!    `true` is encoded as [`ShorthandValue::All`]; an explicit list is
//!    [`ShorthandValue::Properties`]. Iteration order matches upstream.
//! 2. [`shorthand_buckets`] — `Record<ShorthandProperties, 0..=5>`.
//!
//! Both are insertion-ordered (`IndexMap`) because consumers iterate them
//! and ordering is observable.

use indexmap::IndexMap;
use once_cell::sync::Lazy;

/// One of the 60 supported shorthand property names. Reusing `&'static str`
/// instead of an enum keeps the call sites simple and matches upstream
/// (which is a TS string union).
pub type ShorthandProperty = &'static str;

#[derive(Debug, Clone)]
pub enum ShorthandValue {
    /// Upstream `true` — meaning "all properties" (only `all` qualifies).
    All,
    /// Constituent properties this shorthand expands to.
    Properties(&'static [&'static str]),
}

/// Mirrors upstream `shorthandFor` (line 87). Returns the table itself so
/// callers can iterate; use [`shorthand_constituents`] for direct lookups.
pub fn shorthand_for() -> &'static IndexMap<ShorthandProperty, ShorthandValue> {
    &SHORTHAND_FOR
}

/// Direct lookup helper — `Some(props)` when constituents exist, `None`
/// for `all` or unknown.
pub fn shorthand_constituents(prop: &str) -> Option<&'static [&'static str]> {
    match SHORTHAND_FOR.get(prop)? {
        ShorthandValue::All => None,
        ShorthandValue::Properties(p) => Some(*p),
    }
}

/// Mirrors upstream `shorthandBuckets` (line 511).
pub fn shorthand_buckets() -> &'static IndexMap<ShorthandProperty, u8> {
    &SHORTHAND_BUCKETS
}

static SHORTHAND_FOR: Lazy<IndexMap<ShorthandProperty, ShorthandValue>> = Lazy::new(|| {
    let mut m: IndexMap<ShorthandProperty, ShorthandValue> = IndexMap::new();
    m.insert("all", ShorthandValue::All);
    m.insert("animation", ShorthandValue::Properties(&[
        "animation-delay", "animation-direction", "animation-duration",
        "animation-fill-mode", "animation-iteration-count", "animation-name",
        "animation-play-state", "animation-timeline", "animation-timing-function",
    ]));
    m.insert("animation-range", ShorthandValue::Properties(&[
        "animation-range-end", "animation-range-start",
    ]));
    m.insert("background", ShorthandValue::Properties(&[
        "background-attachment", "background-clip", "background-color",
        "background-image", "background-origin", "background-position",
        "background-repeat", "background-size",
    ]));
    m.insert("border", ShorthandValue::Properties(&[
        "border-block", "border-block-color", "border-block-end",
        "border-block-end-color", "border-block-end-style", "border-block-end-width",
        "border-block-start", "border-block-start-color", "border-block-start-style",
        "border-block-start-width", "border-block-style", "border-block-width",
        "border-bottom", "border-bottom-color", "border-bottom-style",
        "border-bottom-width", "border-color", "border-inline",
        "border-inline-color", "border-inline-end", "border-inline-end-color",
        "border-inline-end-style", "border-inline-end-width", "border-inline-start",
        "border-inline-start-color", "border-inline-start-style",
        "border-inline-start-width", "border-inline-style", "border-inline-width",
        "border-left", "border-left-color", "border-left-style", "border-left-width",
        "border-right", "border-right-color", "border-right-style", "border-right-width",
        "border-style", "border-top", "border-top-color", "border-top-style",
        "border-top-width", "border-width",
    ]));
    m.insert("border-block", ShorthandValue::Properties(&[
        "border-block-end", "border-block-end-color", "border-block-end-style",
        "border-block-end-width", "border-block-start", "border-block-start-color",
        "border-block-start-style", "border-block-start-width", "border-bottom-color",
        "border-bottom-style", "border-bottom-width", "border-top-color",
        "border-top-style", "border-top-width",
    ]));
    m.insert("border-block-end", ShorthandValue::Properties(&[
        "border-block-end-color", "border-block-end-style", "border-block-end-width",
        "border-bottom-color", "border-bottom-style", "border-bottom-width",
    ]));
    m.insert("border-block-start", ShorthandValue::Properties(&[
        "border-block-start-color", "border-block-start-style", "border-block-start-width",
        "border-top-color", "border-top-style", "border-top-width",
    ]));
    m.insert("border-bottom", ShorthandValue::Properties(&[
        "border-block-end-color", "border-block-end-style", "border-block-end-width",
        "border-bottom-color", "border-bottom-style", "border-bottom-width",
    ]));
    m.insert("border-color", ShorthandValue::Properties(&[
        "border-block-color", "border-block-start-color", "border-block-end-color",
        "border-bottom-color", "border-inline-color", "border-inline-start-color",
        "border-inline-end-color", "border-left-color", "border-right-color",
        "border-top-color",
    ]));
    m.insert("border-image", ShorthandValue::Properties(&[
        "border-image-outset", "border-image-repeat", "border-image-slice",
        "border-image-source", "border-image-width",
    ]));
    m.insert("border-inline", ShorthandValue::Properties(&[
        "border-inline-end", "border-inline-end-color", "border-inline-end-style",
        "border-inline-end-width", "border-inline-start", "border-inline-start-color",
        "border-inline-start-style", "border-inline-start-width", "border-left-color",
        "border-left-style", "border-left-width", "border-right-color",
        "border-right-style", "border-right-width",
    ]));
    m.insert("border-inline-end", ShorthandValue::Properties(&[
        "border-inline-end-color", "border-inline-end-style", "border-inline-end-width",
        "border-right-color", "border-right-style", "border-right-width",
    ]));
    m.insert("border-inline-start", ShorthandValue::Properties(&[
        "border-inline-start-color", "border-inline-start-style", "border-inline-start-width",
        "border-left-color", "border-left-style", "border-left-width",
    ]));
    m.insert("border-left", ShorthandValue::Properties(&[
        "border-inline-start-color", "border-inline-start-style", "border-inline-start-width",
        "border-left-color", "border-left-style", "border-left-width",
    ]));
    m.insert("border-radius", ShorthandValue::Properties(&[
        "border-bottom-left-radius", "border-bottom-right-radius",
        "border-end-end-radius", "border-end-start-radius",
        "border-start-end-radius", "border-start-start-radius",
        "border-top-left-radius", "border-top-right-radius",
    ]));
    m.insert("border-right", ShorthandValue::Properties(&[
        "border-inline-end-color", "border-inline-end-style", "border-inline-end-width",
        "border-right-color", "border-right-style", "border-right-width",
    ]));
    m.insert("border-style", ShorthandValue::Properties(&[
        "border-block-style", "border-block-start-style", "border-block-end-style",
        "border-bottom-style", "border-inline-style", "border-inline-start-style",
        "border-inline-end-style", "border-left-style", "border-right-style",
        "border-top-style",
    ]));
    m.insert("border-top", ShorthandValue::Properties(&[
        "border-block-start-color", "border-block-start-style", "border-block-start-width",
        "border-top-color", "border-top-style", "border-top-width",
    ]));
    m.insert("border-width", ShorthandValue::Properties(&[
        "border-block-width", "border-block-start-width", "border-block-end-width",
        "border-bottom-width", "border-inline-width", "border-inline-start-width",
        "border-inline-end-width", "border-left-width", "border-right-width",
        "border-top-width",
    ]));
    m.insert("column-rule", ShorthandValue::Properties(&[
        "column-rule-color", "column-rule-style", "column-rule-width",
    ]));
    m.insert("columns", ShorthandValue::Properties(&["column-count", "column-width"]));
    m.insert("contain-intrinsic-size", ShorthandValue::Properties(&[
        "contain-intrinsic-block-size", "contain-intrinsic-height",
        "contain-intrinsic-inline-size", "contain-intrinsic-width",
    ]));
    m.insert("container", ShorthandValue::Properties(&["container-name", "container-type"]));
    m.insert("flex", ShorthandValue::Properties(&["flex-basis", "flex-grow", "flex-shrink"]));
    m.insert("flex-flow", ShorthandValue::Properties(&["flex-direction", "flex-wrap"]));
    m.insert("font", ShorthandValue::Properties(&[
        "font-family", "font-size", "font-stretch", "font-style", "font-variant",
        "font-variant-alternates", "font-variant-caps", "font-variant-east-asian",
        "font-variant-emoji", "font-variant-ligatures", "font-variant-numeric",
        "font-variant-position", "font-weight", "line-height",
    ]));
    m.insert("font-synthesis", ShorthandValue::Properties(&[
        "font-synthesis-position", "font-synthesis-small-caps",
        "font-synthesis-style", "font-synthesis-weight",
    ]));
    m.insert("font-variant", ShorthandValue::Properties(&[
        "font-variant-alternates", "font-variant-caps", "font-variant-east-asian",
        "font-variant-emoji", "font-variant-ligatures", "font-variant-numeric",
        "font-variant-position",
    ]));
    m.insert("gap", ShorthandValue::Properties(&["column-gap", "row-gap"]));
    m.insert("grid", ShorthandValue::Properties(&[
        "grid-auto-columns", "grid-auto-flow", "grid-auto-rows",
        "grid-template", "grid-template-areas", "grid-template-columns",
        "grid-template-rows",
    ]));
    m.insert("grid-area", ShorthandValue::Properties(&[
        "grid-column", "grid-column-end", "grid-column-start",
        "grid-row", "grid-row-end", "grid-row-start",
    ]));
    m.insert("grid-column", ShorthandValue::Properties(&["grid-column-end", "grid-column-start"]));
    m.insert("grid-row", ShorthandValue::Properties(&["grid-row-end", "grid-row-start"]));
    m.insert("grid-template", ShorthandValue::Properties(&[
        "grid-template-rows", "grid-template-columns", "grid-template-areas",
    ]));
    m.insert("inset", ShorthandValue::Properties(&[
        "bottom", "inset-block", "inset-block-start", "inset-block-end",
        "inset-inline", "inset-inline-start", "inset-inline-end", "left", "right", "top",
    ]));
    m.insert("inset-block", ShorthandValue::Properties(&[
        "inset-block-start", "inset-block-end", "top", "bottom",
    ]));
    m.insert("inset-inline", ShorthandValue::Properties(&[
        "inset-inline-start", "inset-inline-end", "left", "right",
    ]));
    m.insert("list-style", ShorthandValue::Properties(&[
        "list-style-image", "list-style-position", "list-style-type",
    ]));
    m.insert("margin", ShorthandValue::Properties(&[
        "margin-block", "margin-block-end", "margin-block-start", "margin-bottom",
        "margin-inline", "margin-inline-end", "margin-inline-start",
        "margin-left", "margin-right", "margin-top",
    ]));
    m.insert("margin-block", ShorthandValue::Properties(&[
        "margin-block-start", "margin-block-end", "margin-top", "margin-bottom",
    ]));
    m.insert("margin-inline", ShorthandValue::Properties(&[
        "margin-inline-start", "margin-inline-end", "margin-left", "margin-right",
    ]));
    m.insert("mask", ShorthandValue::Properties(&[
        "mask-clip", "mask-composite", "mask-image", "mask-mode",
        "mask-origin", "mask-position", "mask-repeat", "mask-size",
    ]));
    m.insert("mask-border", ShorthandValue::Properties(&[
        "mask-border-mode", "mask-border-outset", "mask-border-repeat",
        "mask-border-slice", "mask-border-source", "mask-border-width",
    ]));
    m.insert("offset", ShorthandValue::Properties(&[
        "offset-anchor", "offset-distance", "offset-path", "offset-position", "offset-rotate",
    ]));
    m.insert("outline", ShorthandValue::Properties(&[
        "outline-color", "outline-style", "outline-width",
    ]));
    m.insert("overflow", ShorthandValue::Properties(&[
        "overflow-x", "overflow-y", "overflow-block", "overflow-inline",
    ]));
    m.insert("overscroll-behavior", ShorthandValue::Properties(&[
        "overscroll-behavior-x", "overscroll-behavior-y",
        "overscroll-behavior-inline", "overscroll-behavior-block",
    ]));
    m.insert("padding", ShorthandValue::Properties(&[
        "padding-block", "padding-block-end", "padding-block-start", "padding-bottom",
        "padding-inline", "padding-inline-end", "padding-inline-start",
        "padding-left", "padding-right", "padding-top",
    ]));
    m.insert("padding-block", ShorthandValue::Properties(&[
        "padding-block-start", "padding-block-end", "padding-top", "padding-bottom",
    ]));
    m.insert("padding-inline", ShorthandValue::Properties(&[
        "padding-inline-start", "padding-inline-end", "padding-left", "padding-right",
    ]));
    m.insert("place-content", ShorthandValue::Properties(&["align-content", "justify-content"]));
    m.insert("place-items", ShorthandValue::Properties(&["align-items", "justify-items"]));
    m.insert("place-self", ShorthandValue::Properties(&["align-self", "justify-self"]));
    m.insert("position-try", ShorthandValue::Properties(&[
        "position-try-order", "position-try-fallbacks",
    ]));
    m.insert("scroll-margin", ShorthandValue::Properties(&[
        "scroll-margin-block", "scroll-margin-block-end", "scroll-margin-block-start",
        "scroll-margin-bottom", "scroll-margin-inline", "scroll-margin-inline-end",
        "scroll-margin-inline-start", "scroll-margin-left", "scroll-margin-right",
        "scroll-margin-top",
    ]));
    m.insert("scroll-margin-block", ShorthandValue::Properties(&[
        "scroll-margin-block-start", "scroll-margin-block-end",
        "scroll-margin-bottom", "scroll-margin-top",
    ]));
    m.insert("scroll-margin-inline", ShorthandValue::Properties(&[
        "scroll-margin-inline-start", "scroll-margin-inline-end",
        "scroll-margin-left", "scroll-margin-right",
    ]));
    m.insert("scroll-padding", ShorthandValue::Properties(&[
        "scroll-padding-block", "scroll-padding-block-end", "scroll-padding-block-start",
        "scroll-padding-bottom", "scroll-padding-inline", "scroll-padding-inline-end",
        "scroll-padding-inline-start", "scroll-padding-left", "scroll-padding-right",
        "scroll-padding-top",
    ]));
    m.insert("scroll-padding-block", ShorthandValue::Properties(&[
        "scroll-padding-block-start", "scroll-padding-block-end",
        "scroll-padding-top", "scroll-padding-bottom",
    ]));
    m.insert("scroll-padding-inline", ShorthandValue::Properties(&[
        "scroll-padding-inline-start", "scroll-padding-inline-end",
        "scroll-padding-left", "scroll-padding-right",
    ]));
    m.insert("scroll-timeline", ShorthandValue::Properties(&[
        "scroll-timeline-name", "scroll-timeline-axis",
    ]));
    m.insert("text-decoration", ShorthandValue::Properties(&[
        "text-decoration-color", "text-decoration-line",
        "text-decoration-style", "text-decoration-thickness",
    ]));
    m.insert("text-emphasis", ShorthandValue::Properties(&[
        "text-emphasis-color", "text-emphasis-style",
    ]));
    m.insert("text-wrap", ShorthandValue::Properties(&["text-wrap-mode", "text-wrap-style"]));
    m.insert("transition", ShorthandValue::Properties(&[
        "transition-behavior", "transition-delay", "transition-duration",
        "transition-property", "transition-timing-function",
    ]));
    m.insert("view-timeline", ShorthandValue::Properties(&[
        "view-timeline-name", "view-timeline-axis",
    ]));
    m
});

static SHORTHAND_BUCKETS: Lazy<IndexMap<ShorthandProperty, u8>> = Lazy::new(|| {
    let mut m: IndexMap<ShorthandProperty, u8> = IndexMap::new();
    m.insert("all", 0);
    m.insert("animation", 1);
    m.insert("animation-range", 1);
    m.insert("background", 1);
    m.insert("border", 1);
    m.insert("border-color", 2);
    m.insert("border-style", 2);
    m.insert("border-width", 2);
    m.insert("border-block", 3);
    m.insert("border-inline", 3);
    m.insert("border-top", 4);
    m.insert("border-right", 4);
    m.insert("border-bottom", 4);
    m.insert("border-left", 4);
    m.insert("border-block-start", 5);
    m.insert("border-block-end", 5);
    m.insert("border-inline-start", 5);
    m.insert("border-inline-end", 5);
    m.insert("border-image", 1);
    m.insert("border-radius", 1);
    m.insert("column-rule", 1);
    m.insert("columns", 1);
    m.insert("contain-intrinsic-size", 1);
    m.insert("container", 1);
    m.insert("flex", 1);
    m.insert("flex-flow", 1);
    m.insert("font", 1);
    m.insert("font-synthesis", 1);
    m.insert("font-variant", 2);
    m.insert("gap", 1);
    m.insert("grid", 1);
    m.insert("grid-area", 1);
    m.insert("grid-column", 2);
    m.insert("grid-row", 2);
    m.insert("grid-template", 2);
    m.insert("inset", 1);
    m.insert("inset-block", 2);
    m.insert("inset-inline", 2);
    m.insert("list-style", 1);
    m.insert("margin", 1);
    m.insert("margin-block", 2);
    m.insert("margin-inline", 2);
    m.insert("mask", 1);
    m.insert("mask-border", 1);
    m.insert("offset", 1);
    m.insert("outline", 1);
    m.insert("overflow", 1);
    m.insert("overscroll-behavior", 1);
    m.insert("padding", 1);
    m.insert("padding-block", 2);
    m.insert("padding-inline", 2);
    m.insert("place-content", 1);
    m.insert("place-items", 1);
    m.insert("place-self", 1);
    m.insert("position-try", 1);
    m.insert("scroll-margin", 1);
    m.insert("scroll-margin-block", 2);
    m.insert("scroll-margin-inline", 2);
    m.insert("scroll-padding", 1);
    m.insert("scroll-padding-block", 2);
    m.insert("scroll-padding-inline", 2);
    m.insert("scroll-timeline", 1);
    m.insert("text-decoration", 1);
    m.insert("text-emphasis", 1);
    m.insert("text-wrap", 1);
    m.insert("transition", 1);
    m.insert("view-timeline", 1);
    m
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_for_table_size() {
        // 67 keys total in upstream `shorthandFor` (counted from
        // packages/utils/src/shorthand.ts as of the parity snapshot).
        // Drift here means the upstream table changed — investigate before
        // bumping; class-name compression depends on this list.
        assert_eq!(SHORTHAND_FOR.len(), 67);
    }

    #[test]
    fn buckets_table_size() {
        assert_eq!(SHORTHAND_BUCKETS.len(), 67);
    }

    #[test]
    fn margin_constituents() {
        let c = shorthand_constituents("margin").expect("margin is a shorthand");
        assert!(c.contains(&"margin-top"));
        assert!(c.contains(&"margin-bottom"));
    }

    #[test]
    fn all_is_marker() {
        assert!(matches!(SHORTHAND_FOR.get("all"), Some(ShorthandValue::All)));
        assert_eq!(shorthand_constituents("all"), None);
    }

    #[test]
    fn buckets_match_upstream() {
        assert_eq!(SHORTHAND_BUCKETS.get("all").copied(), Some(0));
        assert_eq!(SHORTHAND_BUCKETS.get("border").copied(), Some(1));
        assert_eq!(SHORTHAND_BUCKETS.get("border-color").copied(), Some(2));
        assert_eq!(SHORTHAND_BUCKETS.get("border-block").copied(), Some(3));
        assert_eq!(SHORTHAND_BUCKETS.get("border-top").copied(), Some(4));
        assert_eq!(SHORTHAND_BUCKETS.get("border-block-start").copied(), Some(5));
    }
}
