//! Port of `packages/css/src/utils/css-property.ts`.
//!
//! Re-exported by `crates/css/src/lib.rs` to mirror the public surface
//! that `packages/css/src/index.ts:1` exposes (`@compiled/css`'s
//! `addUnitIfNeeded`). Consumed by `crates/babel-plugin`'s
//! `utils/css_builders.rs` at the numeric-literal branch.
//!
//! 1:1 port — the `unitless` table and the `units` array MUST match
//! upstream byte-for-byte; any drift renames classes downstream.

use postcss_core::js_number_to_string;

/// Mirrors `packages/css/src/utils/css-property.ts`'s `unitless` object.
/// JS `propertyName in unitless` becomes `is_unitless_property`.
fn is_unitless_property(name: &str) -> bool {
    matches!(
        name,
        "animationIterationCount"
            | "basePalette"
            | "borderImageOutset"
            | "borderImageSlice"
            | "borderImageWidth"
            | "boxFlex"
            | "boxFlexGroup"
            | "boxOrdinalGroup"
            | "columnCount"
            | "columns"
            | "flex"
            | "flexGrow"
            | "flexPositive"
            | "flexShrink"
            | "flexNegative"
            | "flexOrder"
            | "fontSizeAdjust"
            | "fontWeight"
            | "gridArea"
            | "gridRow"
            | "gridRowEnd"
            | "gridRowSpan"
            | "gridRowStart"
            | "gridColumn"
            | "gridColumnEnd"
            | "gridColumnSpan"
            | "gridColumnStart"
            | "lineClamp"
            | "lineHeight"
            | "opacity"
            | "order"
            | "orphans"
            | "tabSize"
            | "WebkitLineClamp"
            | "widows"
            | "zIndex"
            | "zoom"
            | "fillOpacity"
            | "floodOpacity"
            | "stopOpacity"
            | "strokeDasharray"
            | "strokeDashoffset"
            | "strokeMiterlimit"
            | "strokeOpacity"
            | "strokeWidth"
    )
}

/// Mirrors the order-sensitive `units` array in
/// `packages/css/src/utils/css-property.ts`. **Order is load-bearing**
/// — the regex in `cssAffixInterpolation` joins this array with `|`,
/// and JS regex alternation is leftmost-first, so re-ordering changes
/// which suffix a value resolves to (e.g. `s` before `ms` would
/// shadow `ms` matches if you flipped them).
pub const UNITS: &[&str] = &[
    // font relative lengths
    "em", "ex", "cap", "ch", "ic", "rem", "lh", "rlh",
    // viewport percentage lengths
    "vw", "vh", "vi", "vb", "vmin", "vmax",
    // absolute lengths
    "cm", "mm", "Q", "in", "pc", "pt", "px",
    // angle units
    "deg", "grad", "rad", "turn",
    // duration units
    "s", "ms",
    // frequency units
    "Hz", "kHz",
    // resolution units
    "dpi", "dpcm", "dppx", "x",
    // grid fraction
    "fr",
    // percentages
    "%",
];

/// Public-API value shape for `add_unit_if_needed`. Mirrors the
/// `null | undefined | boolean | string | number` union in JS.
/// `Null` covers both `null` and `undefined`.
pub enum AddUnitValue<'a> {
    Null,
    Bool(bool),
    Str(&'a str),
    Number(f64),
}

/// Port of `addUnitIfNeeded` in `packages/css/src/utils/css-property.ts:118`.
///
/// Will append `'px'` to a property value if the property is not unitless.
/// Replicates Emotion's behaviour for numeric properties.
pub fn add_unit_if_needed(name: &str, value: AddUnitValue<'_>) -> String {
    match value {
        AddUnitValue::Null => String::new(),
        AddUnitValue::Bool(_) => String::new(),
        AddUnitValue::Str(s) if s.is_empty() => String::new(),
        AddUnitValue::Number(n) if n != 0.0 && !is_unitless_property(name) => {
            format!("{}px", js_number_to_string(n))
        }
        AddUnitValue::Number(n) => js_number_to_string(n).trim().to_string(),
        AddUnitValue::Str(s) => s.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_value_returns_empty() {
        assert_eq!(add_unit_if_needed("color", AddUnitValue::Null), "");
    }

    #[test]
    fn boolean_value_returns_empty() {
        assert_eq!(add_unit_if_needed("color", AddUnitValue::Bool(true)), "");
        assert_eq!(add_unit_if_needed("color", AddUnitValue::Bool(false)), "");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(add_unit_if_needed("fontSize", AddUnitValue::Str("")), "");
    }

    #[test]
    fn numeric_non_zero_unitful_property_appends_px() {
        assert_eq!(
            add_unit_if_needed("fontSize", AddUnitValue::Number(12.0)),
            "12px"
        );
    }

    #[test]
    fn numeric_zero_does_not_append_px_even_for_unitful_property() {
        assert_eq!(
            add_unit_if_needed("fontSize", AddUnitValue::Number(0.0)),
            "0"
        );
    }

    #[test]
    fn numeric_unitless_property_does_not_append_px() {
        assert_eq!(
            add_unit_if_needed("lineHeight", AddUnitValue::Number(1.5)),
            "1.5"
        );
        assert_eq!(
            add_unit_if_needed("zIndex", AddUnitValue::Number(10.0)),
            "10"
        );
        assert_eq!(
            add_unit_if_needed("fontWeight", AddUnitValue::Number(700.0)),
            "700"
        );
    }

    #[test]
    fn string_value_is_trimmed() {
        assert_eq!(
            add_unit_if_needed("color", AddUnitValue::Str("  red  ")),
            "red"
        );
    }

    #[test]
    fn webkit_line_clamp_is_unitless() {
        // The `WebkitLineClamp` (capital W) is the camelCased React-style key
        // upstream lists; the JS lookup is `propertyName in unitless`, so
        // the casing must match exactly.
        assert_eq!(
            add_unit_if_needed("WebkitLineClamp", AddUnitValue::Number(3.0)),
            "3"
        );
        // Negative case — wrong casing should NOT be treated as unitless
        // (mirrors JS object-key semantics).
        assert_eq!(
            add_unit_if_needed("webkitLineClamp", AddUnitValue::Number(3.0)),
            "3px"
        );
    }

    #[test]
    fn fractional_numbers_use_js_number_formatting() {
        // `js_number_to_string` is the JS-parity formatter; this guards
        // against future Rust f64 Display drift on edge cases.
        assert_eq!(
            add_unit_if_needed("width", AddUnitValue::Number(0.5)),
            "0.5px"
        );
    }
}
