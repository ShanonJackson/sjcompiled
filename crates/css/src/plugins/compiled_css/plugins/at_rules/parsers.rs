//! Port of `packages/css/src/plugins/at-rules/parsers.ts`.
//!
//! Each `parse*` function takes a regex-capture map (named groups) and
//! produces a [`ParsedAtRule`] (or `None` when fields are missing).
//! Three parsers correspond to the three regex situations in
//! `parse-at-rule.ts`:
//!
//! 1. `parseMinMaxSyntax` — `(min|max)-(width|height): <length><unit>`.
//! 2. `parseReversedRangeSyntax` — `<length><unit> <op> <prop>`.
//! 3. `parseRangeSyntax` — `<prop> <op> <length><unit>`.

use super::types::{ComparisonOperator, LengthUnit, ParsedAtRule, Property};

/// `REM_SIZE` upstream — used to normalise em/rem/ch/ex to pixels for
/// sort comparison.
const REM_SIZE: f64 = 16.0;

/// Regex capture: named groups extracted from a single `RegExp` match.
/// We carry index + matched + a tiny string-keyed map so each parser
/// looks like upstream's `match.groups?.x`.
pub struct CaptureGroups<'a> {
    pub index: usize,
    pub matched: &'a str,
    pub property: Option<&'a str>,
    pub operator: Option<&'a str>,
    pub length: Option<&'a str>,
    pub length_unit: Option<&'a str>,
    pub colon: Option<&'a str>,
}

/// `getLengthInfo` upstream — converts the raw length+unit captures
/// to a single f64 in pixels. Returns `None` when:
/// - length string is missing, OR
/// - unit is missing AND length isn't `"0"` (the `"0"` special case).
///
/// Falls into `Some(0.0)` when length == `"0"` regardless of unit
/// (mirrors upstream's `if (length_ === '0') return { length: 0 }`).
pub fn get_length_info(g: &CaptureGroups) -> Option<f64> {
    let length_str = g.length?;
    if length_str == "0" {
        return Some(0.0);
    }
    let unit_str = g.length_unit?;
    let unit = LengthUnit::parse(unit_str)?;
    let n: f64 = length_str.parse().ok()?;
    Some(match unit {
        // `1ch` and `1ex` assumed to be `0.5em` — we cannot rely on
        // more specific font-relative information at sort time.
        LengthUnit::Ch | LengthUnit::Ex => n * 0.5 * REM_SIZE,
        LengthUnit::Em | LengthUnit::Rem => n * REM_SIZE,
        LengthUnit::Px => n,
    })
}

/// `convertMinMaxMediaQuery` upstream — maps `min-width`/`max-width`
/// (and the device-* / -height variants) to the canonical
/// `(property, comparison_operator)` pair. Throws on unexpected
/// property names; we surface that as `None` (the calling parser
/// will skip the match).
pub fn convert_min_max(g: &CaptureGroups) -> Option<(Property, ComparisonOperator)> {
    let p = g.property?;
    match p {
        "min-width" => Some((Property::Width, ComparisonOperator::Ge)),
        "min-device-width" => Some((Property::DeviceWidth, ComparisonOperator::Ge)),
        "max-width" => Some((Property::Width, ComparisonOperator::Le)),
        "max-device-width" => Some((Property::DeviceWidth, ComparisonOperator::Le)),
        "min-height" => Some((Property::Height, ComparisonOperator::Ge)),
        "min-device-height" => Some((Property::DeviceHeight, ComparisonOperator::Ge)),
        "max-height" => Some((Property::Height, ComparisonOperator::Le)),
        "max-device-height" => Some((Property::DeviceHeight, ComparisonOperator::Le)),
        _ => None,
    }
}

/// `getBasicMatchInfo` upstream — returns `undefined` when
/// `!match.index` (i.e. the match starts at byte 0). JS treats `0` as
/// falsy in the truthiness check, which silently drops position-0
/// matches from `parsedMatches`. We mirror that quirk: callers gate on
/// this returning `Some` before constructing a `ParsedAtRule`.
fn basic_match_ok(g: &CaptureGroups) -> Option<()> {
    if g.index == 0 {
        None
    } else {
        Some(())
    }
}

/// `parseMinMaxSyntax(match)`. Note that the regex only matches against
/// the `colon` group, so the `colon` field must be `Some` for this path
/// to fire (the `parse-media-query` regex pre-filters via the `colon`
/// alternation).
pub fn parse_min_max(g: &CaptureGroups) -> Option<ParsedAtRule> {
    basic_match_ok(g)?;
    let (property, comparison_operator) = convert_min_max(g)?;
    let length = get_length_info(g)?;
    Some(ParsedAtRule {
        property,
        comparison_operator,
        length,
        index: g.index,
        matched: g.matched.to_string(),
    })
}

/// `parseReversedRangeSyntax(match)` — `<length> <op> <property>`. The
/// operator is reversed so the resulting `ParsedAtRule` is canonical.
pub fn parse_reversed_range(g: &CaptureGroups) -> Option<ParsedAtRule> {
    basic_match_ok(g)?;
    let property = Property::parse(g.property?)?;
    let raw_op = ComparisonOperator::parse(g.operator?)?;
    let comparison_operator = raw_op.reverse();
    let length = get_length_info(g)?;
    Some(ParsedAtRule {
        property,
        comparison_operator,
        length,
        index: g.index,
        matched: g.matched.to_string(),
    })
}

/// `parseRangeSyntax(match)` — `<property> <op> <length>`.
pub fn parse_range(g: &CaptureGroups) -> Option<ParsedAtRule> {
    basic_match_ok(g)?;
    let property = Property::parse(g.property?)?;
    let comparison_operator = ComparisonOperator::parse(g.operator?)?;
    let length = get_length_info(g)?;
    Some(ParsedAtRule {
        property,
        comparison_operator,
        length,
        index: g.index,
        matched: g.matched.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap<'a>(
        property: Option<&'a str>,
        operator: Option<&'a str>,
        length: Option<&'a str>,
        unit: Option<&'a str>,
    ) -> CaptureGroups<'a> {
        // index = 1 so we skip the `getBasicMatchInfo` falsy-drop;
        // tests want to exercise the property/operator/length paths.
        CaptureGroups {
            index: 1,
            matched: "",
            property,
            operator,
            length,
            length_unit: unit,
            colon: None,
        }
    }

    #[test]
    fn min_width_normalises_to_ge() {
        let r = parse_min_max(&cap(Some("min-width"), None, Some("200"), Some("px"))).unwrap();
        assert_eq!(r.property, Property::Width);
        assert_eq!(r.comparison_operator, ComparisonOperator::Ge);
        assert_eq!(r.length, 200.0);
    }

    #[test]
    fn max_height_normalises_to_le() {
        let r = parse_min_max(&cap(Some("max-height"), None, Some("100"), Some("px"))).unwrap();
        assert_eq!(r.property, Property::Height);
        assert_eq!(r.comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn rem_normalises_to_pixels() {
        let r = parse_range(&cap(Some("width"), Some("<="), Some("10"), Some("rem"))).unwrap();
        assert_eq!(r.length, 160.0);
    }

    #[test]
    fn zero_no_unit() {
        let r = parse_range(&cap(Some("width"), Some(">="), Some("0"), None)).unwrap();
        assert_eq!(r.length, 0.0);
    }

    #[test]
    fn reversed_range_reverses_operator() {
        // 200px >= width → width <= 200px
        let r = parse_reversed_range(&cap(Some("width"), Some(">="), Some("200"), Some("px"))).unwrap();
        assert_eq!(r.comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn unknown_property_returns_none() {
        assert!(parse_range(&cap(Some("nope"), Some("<="), Some("1"), Some("px"))).is_none());
    }

    #[test]
    fn unknown_operator_returns_none() {
        assert!(parse_range(&cap(Some("width"), Some("!="), Some("1"), Some("px"))).is_none());
    }

    #[test]
    fn index_zero_is_dropped() {
        // Mirrors JS `getBasicMatchInfo`: `if (!match.index) return undefined`
        // — a position-0 match is silently rejected even when all other
        // fields are valid. All three parsers must replicate this.
        let g = CaptureGroups {
            index: 0,
            matched: "",
            property: Some("min-width"),
            operator: Some(">="),
            length: Some("200"),
            length_unit: Some("px"),
            colon: None,
        };
        assert!(parse_min_max(&g).is_none());
        assert!(parse_range(&g).is_none());
        assert!(parse_reversed_range(&g).is_none());
    }
}
