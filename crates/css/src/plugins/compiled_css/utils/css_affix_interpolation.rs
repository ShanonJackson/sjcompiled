//! Port of `packages/css/src/utils/css-affix-interpolation.ts`.
//!
//! Re-exported by `crates/css/src/lib.rs` to mirror the public surface
//! that `packages/css/src/index.ts:4-7` exposes (`@compiled/css`'s
//! `cssAffixInterpolation`, `BeforeInterpolation`, `AfterInterpolation`).
//! Consumed by `crates/babel-plugin/src/utils/css_builders.rs` at the
//! catch-all template-literal expression branch.
//!
//! ## Regex parity
//!
//! Upstream builds the suffix regex as:
//!
//! ```js
//! new RegExp(`^(${units.join('|')}|"|')(;|,|\n| |\\))?`)
//! ```
//!
//! The Rust `regex` crate uses **leftmost-first** alternation matching
//! (same as ECMAScript) — so the alternative listed first wins on a tie.
//! That makes the order of `units` (e.g. `cm` before `mm`, `s` before `ms`)
//! load-bearing and matches the JS regex byte-for-byte.
//!
//! All literal chars in the regex (`;`, `,`, `\n`, space, `)`) are ASCII;
//! `units` is also pure-ASCII (`%`, `Q`, `Hz`, etc.). No Unicode classes,
//! no backreferences, no lookaround — the alternation is simple enough that
//! `regex` and JS `RegExp` produce byte-identical match offsets.
//!
//! `String.prototype.replace(string, '')` in JS strips the FIRST occurrence;
//! since group 1 is `^`-anchored, that first occurrence is always at index 0.
//! The Rust port uses `replacen(.., "", 1)` to preserve the same semantics
//! for any pathological input that contains `result[1]` repeated later in
//! the string (defensive parity — current call sites never hit it).

use once_cell::sync::Lazy;
use regex::Regex;

use super::css_property::UNITS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfterInterpolation {
    pub css: String,
    pub variable_suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeforeInterpolation {
    pub css: String,
    pub variable_prefix: String,
}

static AFTER_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Mirrors `new RegExp('^(' + units.join('|') + '|"|\'')(;|,|\\n| |\\))?')`.
    // `%` is regex-literal; no escaping needed in a non-class context.
    // `\n` in the source is a literal newline char — we use `\n` here too.
    let units_alt = UNITS.join("|");
    let pattern = format!(r#"^({}|"|')(;|,|\n| |\))?"#, units_alt);
    Regex::new(&pattern).expect("css-affix-interpolation regex must compile")
});

/// Port of the file-private `cssAfterInterpolation` in
/// `packages/css/src/utils/css-affix-interpolation.ts:19`.
fn css_after_interpolation(css: &str) -> AfterInterpolation {
    if let Some(caps) = AFTER_REGEX.captures(css) {
        let group_1 = caps.get(1).expect("group 1 is required by regex").as_str();
        AfterInterpolation {
            variable_suffix: group_1.to_string(),
            css: css.replacen(group_1, "", 1),
        }
    } else {
        AfterInterpolation {
            variable_suffix: String::new(),
            css: css.to_string(),
        }
    }
}

/// Port of the file-private `cssBeforeInterpolation` in
/// `packages/css/src/utils/css-affix-interpolation.ts:42`.
fn css_before_interpolation(css: &str) -> BeforeInterpolation {
    let last_char = css.chars().last();
    match last_char {
        Some('"') | Some('\'') | Some('-') => {
            let ch = last_char.unwrap();
            // All three candidates are single-byte ASCII — slicing by byte
            // length is safe and matches JS `slice(0, -1)` which counts
            // UTF-16 code units (also 1 for these chars).
            let byte_len = ch.len_utf8();
            BeforeInterpolation {
                variable_prefix: ch.to_string(),
                css: css[..css.len() - byte_len].to_string(),
            }
        }
        _ => BeforeInterpolation {
            variable_prefix: String::new(),
            css: css.to_string(),
        },
    }
}

/// Port of `cssAffixInterpolation` in
/// `packages/css/src/utils/css-affix-interpolation.ts:77`.
///
/// Extracts both the prefix and suffix surrounding a CSS interpolation
/// hole. Handles the `url()` special case explicitly (interpolation
/// doesn't work inside `url()` per
/// https://stackoverflow.com/a/42331003).
pub fn css_affix_interpolation(
    before: &str,
    after: &str,
) -> (BeforeInterpolation, AfterInterpolation) {
    if before.ends_with("url(") && after.starts_with(')') {
        return (
            BeforeInterpolation {
                variable_prefix: "url(".to_string(),
                css: before[..before.len() - "url(".len()].to_string(),
            },
            AfterInterpolation {
                variable_suffix: ")".to_string(),
                css: after[")".len()..].to_string(),
            },
        );
    }
    (css_before_interpolation(before), css_after_interpolation(after))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The full block below is a 1:1 port of
    // `packages/css/src/utils/__tests__/css-affix-interpolation.test.ts`.
    // 35 cases. If any drift, this is the canary.

    fn before(s: &str, prefix: &str) -> BeforeInterpolation {
        BeforeInterpolation { css: s.to_string(), variable_prefix: prefix.to_string() }
    }

    fn after(s: &str, suffix: &str) -> AfterInterpolation {
        AfterInterpolation { css: s.to_string(), variable_suffix: suffix.to_string() }
    }

    // --- interpolations with surrounding css ---

    #[test]
    fn extracts_prefix_simple_template_literal() {
        let (b, _) = css_affix_interpolation("content: \"", "\";font-color:blue;");
        assert_eq!(b, before("content: ", "\""));
    }

    #[test]
    fn extracts_suffix_simple_template_literal() {
        let (_, a) = css_affix_interpolation("content: \"", "\";font-color:blue;");
        assert_eq!(a, after(";font-color:blue;", "\""));
    }

    #[test]
    fn retains_suffix_with_important_flag() {
        let (_, a) = css_affix_interpolation("color: ", "px !important;");
        assert_eq!(a, after(" !important;", "px"));
    }

    #[test]
    fn ignores_space_as_prefix() {
        let (b, _) = css_affix_interpolation("padding: 0 ", " 0");
        assert_eq!(b, before("padding: 0 ", ""));
    }

    #[test]
    fn ignores_space_as_suffix() {
        let (_, a) = css_affix_interpolation("padding: 0 ", " 0");
        assert_eq!(a, after(" 0", ""));
    }

    #[test]
    fn extracts_interpolation_with_suffix() {
        let (_, a) = css_affix_interpolation("padding: 0 ", "px 0");
        assert_eq!(a, after(" 0", "px"));
    }

    #[test]
    fn extracts_prefix_complex_template_literal() {
        let (b, _) = css_affix_interpolation("transform: translateX(", ");color:blue;");
        assert_eq!(b, before("transform: translateX(", ""));
    }

    #[test]
    fn extracts_suffix_complex_template_literal() {
        let (_, a) = css_affix_interpolation("transform: translateX(", ");color:blue;");
        assert_eq!(a, after(");color:blue;", ""));
    }

    #[test]
    fn extracts_first_part_of_three_part_value_before() {
        let (b, _) = css_affix_interpolation("transform: transform3d(", ", ");
        assert_eq!(b, before("transform: transform3d(", ""));
    }

    #[test]
    fn extracts_before_second_part_of_three_part_value() {
        let (b, _) = css_affix_interpolation(", ", ")");
        assert_eq!(b, before(", ", ""));
    }

    #[test]
    fn extracts_after_second_part_of_three_part_value() {
        let (_, a) = css_affix_interpolation("transform: transform3d(", ", ");
        assert_eq!(a, after(", ", ""));
    }

    #[test]
    fn extracts_second_part_of_three_part_value_after() {
        let (_, a) = css_affix_interpolation(", ", ")");
        assert_eq!(a, after(")", ""));
    }

    #[test]
    fn before_and_after_first_part_of_transform_interpolation() {
        let (b, a) = css_affix_interpolation("transform: translate3d(", "px, ");
        assert_eq!(b, before("transform: translate3d(", ""));
        assert_eq!(a, after(", ", "px"));
    }

    #[test]
    fn before_and_after_second_part_of_transform_interpolation() {
        let (b, a) = css_affix_interpolation(
            "\n            transform: translate3d(var(--_test), ",
            ", 0);",
        );
        assert_eq!(
            b,
            before("\n            transform: translate3d(var(--_test), ", "")
        );
        assert_eq!(a, after(", 0);", ""));
    }

    // --- interpolations with multiple groups ---

    #[test]
    fn first_part_of_first_group() {
        let (b, a) = css_affix_interpolation(
            "background-image: linear-gradient(45deg, ",
            " 25%, transparent 25%),",
        );
        assert_eq!(b, before("background-image: linear-gradient(45deg, ", ""));
        assert_eq!(a, after(" 25%, transparent 25%),", ""));
    }

    #[test]
    fn first_part_of_second_group() {
        let (b, a) = css_affix_interpolation(
            "background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%),",
            "linear-gradient(-45deg, ",
        );
        assert_eq!(
            b,
            before(
                "background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%),",
                ""
            )
        );
        assert_eq!(a, after("linear-gradient(-45deg, ", ""));
    }

    #[test]
    fn first_part_of_third_group() {
        let (b, a) = css_affix_interpolation(
            "background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%), linear-gradient(-45deg, var(--_test) 25%, transparent 25%),",
            "linear-gradient(45deg, transparent 75%, ",
        );
        assert_eq!(b, before("background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%), linear-gradient(-45deg, var(--_test) 25%, transparent 25%),", ""));
        assert_eq!(a, after("linear-gradient(45deg, transparent 75%, ", ""));
    }

    #[test]
    fn first_part_of_fourth_group() {
        let (b, a) = css_affix_interpolation(
            "background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%), linear-gradient(-45deg, var(--_test) 25%, transparent 25%), linear-gradient(45deg, transparent 75%, var(--_test) 75%),",
            "linear-gradient(-45deg, transparent 75%, ",
        );
        assert_eq!(b, before("background-image: linear-gradient(45deg, var(--_test) 25%, transparent 25%), linear-gradient(-45deg, var(--_test) 25%, transparent 25%), linear-gradient(45deg, transparent 75%, var(--_test) 75%),", ""));
        assert_eq!(a, after("linear-gradient(-45deg, transparent 75%, ", ""));
    }

    #[test]
    fn moves_only_minus_to_prefix() {
        let (b, _) = css_affix_interpolation("margin: 0 -", ";");
        assert_eq!(b, before("margin: 0 ", "-"));
    }

    #[test]
    fn moves_whole_prefix_out_with_quote() {
        let (b, _) = css_affix_interpolation("font-size: \"", "big;");
        assert_eq!(b, before("font-size: ", "\""));
    }

    // --- interpolations without surrounding css ---

    #[test]
    fn extracts_suffix_with_no_prefix() {
        let (_, a) = css_affix_interpolation("", "px;");
        assert_eq!(a, after(";", "px"));
    }

    #[test]
    fn extracts_prefix_simple_template_literal_no_surround() {
        let (b, _) = css_affix_interpolation("\"", "\"");
        assert_eq!(b, before("", "\""));
    }

    #[test]
    fn extracts_suffix_simple_template_literal_no_surround() {
        let (_, a) = css_affix_interpolation("\"", "\";");
        assert_eq!(a, after(";", "\""));
    }

    #[test]
    fn moves_whole_prefix_out_no_surround() {
        let (b, _) = css_affix_interpolation("\"", "big;");
        assert_eq!(b, before("", "\""));
    }

    #[test]
    fn extracts_prefix_from_calc() {
        let (b, _) = css_affix_interpolation("calc(100% - ", "px)");
        assert_eq!(b, before("calc(100% - ", ""));
    }

    #[test]
    fn extracts_suffix_from_calc() {
        let (_, a) = css_affix_interpolation("calc(100% - ", "px)");
        assert_eq!(a, after(")", "px"));
    }

    #[test]
    fn first_part_of_three_part_value_no_surround() {
        let (b, _) = css_affix_interpolation("transform3d(", ", ");
        assert_eq!(b.variable_prefix, "");
        assert_eq!(b.css, "transform3d(");
    }

    #[test]
    fn before_second_part_of_three_part_value_no_surround() {
        let (b, _) = css_affix_interpolation(", ", ")");
        assert_eq!(b, before(", ", ""));
    }

    #[test]
    fn after_three_part_value_no_surround() {
        let (_, a) = css_affix_interpolation("transform3d(", ", ");
        assert_eq!(a, after(", ", ""));
    }

    #[test]
    fn second_part_of_three_part_value_no_surround() {
        let (_, a) = css_affix_interpolation(", ", ")");
        assert_eq!(a, after(")", ""));
    }

    #[test]
    fn moves_only_minus_to_prefix_no_surround() {
        let (b, _) = css_affix_interpolation("0 -", ";");
        assert_eq!(b, before("0 ", "-"));
    }

    // --- interpolations with url ---

    #[test]
    fn handles_single_url() {
        let (b, a) = css_affix_interpolation("background-image: url(", ")");
        assert_eq!(b, before("background-image: ", "url("));
        assert_eq!(a, after("", ")"));
    }

    #[test]
    fn handles_multiple_urls() {
        let (b, a) = css_affix_interpolation("background-image: url(", "), url(");
        assert_eq!(b, before("background-image: ", "url("));
        assert_eq!(a, after(", url(", ")"));
    }
}
