//! Port of `packages/css/src/plugins/at-rules/parse-media-query.ts`.
//!
//! Upstream uses three distinct JS regexes ("situations") and runs each
//! with the global flag, then sorts the resulting matches by source
//! index. We replicate via the `regex` crate, building the three regexes
//! once at module load.
//!
//! ## Regex sources
//! ```text
//! comparisonOperators = (?P<operator>(?:<=?)|(?:>=?)|=)\s*
//! property            = (?P<property>((?:min|max)-)?(?:device-)?(?:width|height))\s*
//! colon               = (?P<colon>:\s*)
//! length              = (?P<length>-?\d*\.?\d+)(?P<lengthUnit>ch|em|ex|px|rem)?\s*
//! ```
//!
//! Situation regexes:
//! 1. `parseMinMaxSyntax`        — `property + colon + length`.
//! 2. `parseReversedRangeSyntax` — `length + comparisonOperators + property`.
//! 3. `parseRangeSyntax`         — `property + comparisonOperators + length`.

use once_cell::sync::Lazy;
use regex::Regex;

use super::parsers::{parse_min_max, parse_range, parse_reversed_range, CaptureGroups};
use super::types::ParsedAtRule;

/// `comparisonOperators` upstream — captures `<=`, `>=`, `<`, `>`, `=`.
const COMPARISON_OPERATORS: &str = r"(?P<operator>(?:<=?)|(?:>=?)|=)\s*";

/// `property` upstream — captures `(min|max-)?(device-)?(width|height)`.
const PROPERTY: &str = r"(?P<property>((?:min|max)-)?(?:device-)?(?:width|height))\s*";

/// `colon` upstream — captures `: ` (the separator in `min-width: …`).
const COLON: &str = r"(?P<colon>:\s*)";

/// `length` upstream — captures number + optional unit. Matches `0`,
/// `200`, `1.5`, `-3`, etc.
const LENGTH: &str = r"(?P<length>-?\d*\.?\d+)(?P<lengthUnit>ch|em|ex|px|rem)?\s*";

static SITUATION_ONE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!("{}{}{}", PROPERTY, COLON, LENGTH)).expect("min-max regex")
});
static SITUATION_TWO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!("{}{}{}", LENGTH, COMPARISON_OPERATORS, PROPERTY)).expect("reversed range regex")
});
static SITUATION_THREE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!("{}{}{}", PROPERTY, COMPARISON_OPERATORS, LENGTH)).expect("range regex")
});

/// `parseMediaQuery(params)` upstream. Returns parsed breakpoints in
/// source-position order so downstream sorters see the same sequence
/// as upstream (the per-situation collection order would otherwise
/// differ when the three syntaxes interleave).
pub fn parse_media_query(params: &str) -> Vec<ParsedAtRule> {
    let mut out: Vec<ParsedAtRule> = Vec::new();

    // Situation one — min/max syntax.
    for caps in SITUATION_ONE.captures_iter(params) {
        let g = capture_groups_from(&caps, params);
        if let Some(p) = parse_min_max(&g) {
            out.push(p);
        }
    }
    // Situation two — reversed range.
    for caps in SITUATION_TWO.captures_iter(params) {
        let g = capture_groups_from(&caps, params);
        if let Some(p) = parse_reversed_range(&g) {
            out.push(p);
        }
    }
    // Situation three — range.
    for caps in SITUATION_THREE.captures_iter(params) {
        let g = capture_groups_from(&caps, params);
        if let Some(p) = parse_range(&g) {
            out.push(p);
        }
    }

    // Re-sort by source index so the breakpoints appear in the order
    // they were in the original media query. Upstream comment: "The
    // above `for` loop checks for matches for each of the three
    // situations / syntaxes sequentially. This may result in situations
    // where one part of the media query erroneously appears after
    // another in `parsedMatches`."
    out.sort_by_key(|p| p.index);
    out
}

fn capture_groups_from<'a>(caps: &regex::Captures<'a>, src: &'a str) -> CaptureGroups<'a> {
    let m = caps.get(0).expect("regex captures have a full match");
    // Upstream's `match.index` is the start position. JS regexes return
    // `index === 0` as falsy in `if (!match.index)`, which means
    // matches at index 0 are silently rejected by `getBasicMatchInfo`.
    // We replicate that quirk: a match at byte 0 is dropped.
    let index = m.start();
    let matched = &src[m.start()..m.end()];
    CaptureGroups {
        // Map JS's "index 0 is falsy" to None so the parsers see
        // missing-info and reject the match.
        index: if index == 0 { 0 } else { index },
        matched,
        property: caps.name("property").map(|m| m.as_str()),
        operator: caps.name("operator").map(|m| m.as_str()),
        length: caps.name("length").map(|m| m.as_str()),
        length_unit: caps.name("lengthUnit").map(|m| m.as_str()),
        colon: caps.name("colon").map(|m| m.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{ComparisonOperator, Property};

    #[test]
    fn parses_single_min_width() {
        let r = parse_media_query("(min-width: 200px)");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].property, Property::Width);
        assert_eq!(r[0].comparison_operator, ComparisonOperator::Ge);
        assert_eq!(r[0].length, 200.0);
    }

    #[test]
    fn parses_max_width_to_le() {
        let r = parse_media_query("(max-width: 400px)");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn parses_range_syntax() {
        let r = parse_media_query("(width <= 200px)");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].property, Property::Width);
        assert_eq!(r[0].comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn parses_reversed_range_syntax_normalised() {
        // `200px >= width` → `width <= 200px`.
        let r = parse_media_query("(200px >= width)");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn parses_combined_query_in_source_order() {
        let r = parse_media_query("(min-width: 100px) and (max-width: 500px)");
        assert_eq!(r.len(), 2);
        // Source-position sort: min comes first, max second.
        assert_eq!(r[0].comparison_operator, ComparisonOperator::Ge);
        assert_eq!(r[1].comparison_operator, ComparisonOperator::Le);
    }

    #[test]
    fn drops_unknown_units() {
        // `vw` isn't in the unit alternation; the regex still matches
        // `100` (no unit), but `getLengthInfo` requires a unit unless
        // length is `"0"`, so this discards.
        let r = parse_media_query("(min-width: 100vw)");
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn print_query_returns_no_breakpoints() {
        let r = parse_media_query("print");
        assert!(r.is_empty());
    }
}
