//! Port of `packages/css/src/plugins/at-rules/sort-at-rules.ts`.
//!
//! Three-stage comparator used by `sort-atomic-style-sheet`:
//! 1. `localeCompare` on at-rule name (`@layer` < `@media` < `@supports`).
//! 2. Per-breakpoint sort within matching at-rule names — sort key is
//!    `Property.weight + ComparisonOperator.weight`. Within a tied
//!    bucket, length sorts ascending for `>`-family operators and
//!    descending for `<`-family.
//! 3. If both queries reach the end of their breakpoints with all keys
//!    equal, the at-rule with FEWER breakpoints sorts first.
//! 4. Tiebreaker: `localeCompare` on the original `query` string.
//!
//! ## Locale compare parity
//! Upstream uses `String.prototype.localeCompare(other, 'en')`. JS's
//! `'en'` locale follows the Unicode Collation Algorithm — for ASCII
//! strings this matches `cmp` byte-by-byte; for non-ASCII it folds
//! diacritics. The atomic CSS pipeline only emits ASCII at-rule names
//! and queries, so byte `cmp` is byte-identical for the corpus we care
//! about. If real-world inputs ever include non-ASCII at-rule names,
//! revisit and bind a Unicode collator.

use std::cmp::Ordering;

use super::types::AtRuleInfo;

pub fn sort_at_rules(rule1: &AtRuleInfo, rule2: &AtRuleInfo) -> Ordering {
    // Stage 1: at-rule name comparison.
    match locale_compare_en(&rule1.at_rule_name, &rule2.at_rule_name) {
        Ordering::Equal => {}
        other => return other,
    }

    // Stage 2: per-breakpoint comparison.
    let len = rule1.parsed.len().min(rule2.parsed.len());
    for i in 0..len {
        let first = &rule1.parsed[i];
        let second = &rule2.parsed[i];
        let first_key = first.property.sort_weight() + first.comparison_operator.sort_weight();
        let second_key = second.property.sort_weight() + second.comparison_operator.sort_weight();
        if first_key != second_key {
            return first_key.cmp(&second_key);
        }
        // Within a matching property+operator bucket, sort by length.
        // `min-width` (>=) ascends; `max-width` (<=) descends.
        if first.length != second.length {
            let asc = first.comparison_operator.includes_gt();
            return if asc {
                f64_total_cmp(first.length, second.length)
            } else {
                f64_total_cmp(second.length, first.length)
            };
        }
    }

    // Stage 3: shorter parsed list first when both have parsed entries.
    let r1_len = rule1.parsed.len();
    let r2_len = rule2.parsed.len();
    if r1_len + r2_len > 0 && r1_len != r2_len {
        return r1_len.cmp(&r2_len);
    }

    // Stage 4: tiebreaker on raw query string.
    locale_compare_en(&rule1.query, &rule2.query)
}

/// `String.prototype.localeCompare(other, 'en')` for ASCII inputs is
/// equivalent to byte ordering; for non-ASCII it folds diacritics. We
/// fall back to `cmp` here — see module-level note about parity scope.
fn locale_compare_en(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// Total ordering for f64 — sort keys never include NaN (parsed
/// lengths come from `parseFloat` which would skip NaN strings), but
/// we use `total_cmp` for safety.
fn f64_total_cmp(a: f64, b: f64) -> Ordering {
    a.total_cmp(&b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{ComparisonOperator, ParsedAtRule, Property};
    use postcss_core::Node;

    fn rule(name: &str, query: &str, parsed: Vec<ParsedAtRule>) -> AtRuleInfo {
        AtRuleInfo {
            parsed,
            node: Node {
                kind: postcss_core::NodeKind::Root(postcss_core::root::RootInner::default()),
                raws: postcss_core::Raws::default(),
                source: postcss_core::Source::default(),
                ..Node::default()
            },
            at_rule_name: name.to_string(),
            query: query.to_string(),
        }
    }

    fn parsed(prop: Property, op: ComparisonOperator, len: f64) -> ParsedAtRule {
        ParsedAtRule {
            property: prop,
            comparison_operator: op,
            length: len,
            index: 0,
            matched: String::new(),
        }
    }

    #[test]
    fn at_rule_name_sorts_alphabetically() {
        let m = rule("media", "", vec![]);
        let s = rule("supports", "", vec![]);
        let l = rule("layer", "", vec![]);
        assert_eq!(sort_at_rules(&l, &m), Ordering::Less);
        assert_eq!(sort_at_rules(&m, &s), Ordering::Less);
    }

    #[test]
    fn min_width_ascends() {
        let a = rule("media", "(min-width: 100px)", vec![parsed(Property::Width, ComparisonOperator::Ge, 100.0)]);
        let b = rule("media", "(min-width: 200px)", vec![parsed(Property::Width, ComparisonOperator::Ge, 200.0)]);
        assert_eq!(sort_at_rules(&a, &b), Ordering::Less);
    }

    #[test]
    fn max_width_descends() {
        let a = rule("media", "(max-width: 100px)", vec![parsed(Property::Width, ComparisonOperator::Le, 100.0)]);
        let b = rule("media", "(max-width: 200px)", vec![parsed(Property::Width, ComparisonOperator::Le, 200.0)]);
        // Bigger max-width sorts first (descending).
        assert_eq!(sort_at_rules(&b, &a), Ordering::Less);
    }

    #[test]
    fn shorter_parsed_first_when_same_prefix() {
        let short = rule("media", "(min-width: 100px)", vec![parsed(Property::Width, ComparisonOperator::Ge, 100.0)]);
        let long = rule(
            "media",
            "(min-width: 100px) and (max-width: 200px)",
            vec![
                parsed(Property::Width, ComparisonOperator::Ge, 100.0),
                parsed(Property::Width, ComparisonOperator::Le, 200.0),
            ],
        );
        assert_eq!(sort_at_rules(&short, &long), Ordering::Less);
    }

    #[test]
    fn no_breakpoints_falls_through_to_query_compare() {
        let a = rule("media", "print", vec![]);
        let b = rule("media", "screen", vec![]);
        assert_eq!(sort_at_rules(&a, &b), Ordering::Less);
    }
}
