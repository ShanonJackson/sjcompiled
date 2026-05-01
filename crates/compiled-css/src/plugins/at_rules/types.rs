//! Port of `packages/css/src/plugins/at-rules/types.ts`.

use postcss_core::Node;

/// `Property` upstream — one of `'width'`, `'height'`, `'device-width'`,
/// `'device-height'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Width,
    Height,
    DeviceWidth,
    DeviceHeight,
}

impl Property {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "width" => Some(Property::Width),
            "height" => Some(Property::Height),
            "device-width" => Some(Property::DeviceWidth),
            "device-height" => Some(Property::DeviceHeight),
            _ => None,
        }
    }

    /// Sort weight from `sort-at-rules.ts::SORT_ORDER`.
    pub fn sort_weight(&self) -> i32 {
        match self {
            Property::Width => 1,
            Property::Height => 2,
            Property::DeviceWidth => 101,
            Property::DeviceHeight => 102,
        }
    }
}

/// `ComparisonOperator` upstream — `'<='`, `'='`, `'>='`, `'<'`, `'>'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    Le,
    Eq,
    Ge,
    Lt,
    Gt,
}

impl ComparisonOperator {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "<=" => Some(ComparisonOperator::Le),
            "=" => Some(ComparisonOperator::Eq),
            ">=" => Some(ComparisonOperator::Ge),
            "<" => Some(ComparisonOperator::Lt),
            ">" => Some(ComparisonOperator::Gt),
            _ => None,
        }
    }

    /// `>`-family check used by `sort-at-rules` to decide ascending
    /// (min-width / `>`) vs descending (max-width / `<`) sort within a
    /// matching property+operator bucket. Upstream uses
    /// `first.comparisonOperator.includes('>')` — i.e. `>` and `>=`.
    pub fn includes_gt(&self) -> bool {
        matches!(self, ComparisonOperator::Gt | ComparisonOperator::Ge)
    }

    /// Reverses the operator — used by `parseReversedRangeSyntax` to
    /// normalize `200px >= width` → `width <= 200px`. `=` is its own
    /// reverse.
    pub fn reverse(&self) -> Self {
        match self {
            ComparisonOperator::Lt => ComparisonOperator::Gt,
            ComparisonOperator::Gt => ComparisonOperator::Lt,
            ComparisonOperator::Le => ComparisonOperator::Ge,
            ComparisonOperator::Ge => ComparisonOperator::Le,
            ComparisonOperator::Eq => ComparisonOperator::Eq,
        }
    }

    /// Sort weight from `sort-at-rules.ts::SORT_ORDER`.
    pub fn sort_weight(&self) -> i32 {
        match self {
            ComparisonOperator::Gt => 10,
            ComparisonOperator::Ge => 20,
            ComparisonOperator::Lt => 30,
            ComparisonOperator::Le => 40,
            ComparisonOperator::Eq => 50,
        }
    }
}

/// `LengthUnit` upstream — `'ch'`, `'em'`, `'ex'`, `'px'`, `'rem'`.
/// Other units fall through (the regex won't match them) and the
/// `getLengthInfo` returns undefined → match discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit { Ch, Em, Ex, Px, Rem }

impl LengthUnit {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ch" => Some(LengthUnit::Ch),
            "em" => Some(LengthUnit::Em),
            "ex" => Some(LengthUnit::Ex),
            "px" => Some(LengthUnit::Px),
            "rem" => Some(LengthUnit::Rem),
            _ => None,
        }
    }
}

/// `ParsedAtRule` upstream — one breakpoint comparison extracted from
/// a media query, normalized to the canonical
/// `<width|height> <comparisonOperator> <length><lengthUnit>` form.
#[derive(Debug, Clone)]
pub struct ParsedAtRule {
    pub property: Property,
    pub comparison_operator: ComparisonOperator,
    /// Length normalized to pixels via `parsers::get_length_info`.
    pub length: f64,
    /// Match position within the original params string. Used by
    /// `parse_media_query` to re-sort by source index after collecting
    /// matches across the three regex situations.
    pub index: usize,
    /// Original match text — preserved for parity diagnostics; not used
    /// in sorting itself.
    pub matched: String,
}

/// `AtRuleInfo` upstream — the shape `sort-at-rules` consumes.
///
/// Upstream's `node` field is `Rule | AtRule` because
/// `sort-atomic-style-sheet.ts` may treat a Rule whose first child is
/// an AtRule (e.g. atomicified at-rule wrapper) as if it were the
/// at-rule for sorting purposes. We match by holding a
/// [`postcss_core::Node`].
#[derive(Debug, Clone)]
pub struct AtRuleInfo {
    pub parsed: Vec<ParsedAtRule>,
    pub node: Node,
    pub at_rule_name: String,
    pub query: String,
}
