//! Port of `postcss-values-parser/lib/nodes/Operator.js`.

use super::node::Common;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Operator {
    pub common: Common,
}

// 1:1 with upstream `Operator.js:15`:
//   const operators = ['+', '-', '/', '*', '%', '=', '<=', '>=', '<', '>'];
// Distinct from `tokenize.js:14` which uses only `['*', '-', '%', '+', '/']`
// (the 5 chars that get retagged from word→operator at the tokenizer layer).
// `Operator.chars` is the 10-element set that `unknownWord` consults when
// classifying an already-tokenized word (e.g. `=`, `<`, `<=` in calc()).
pub static OPERATOR_CHARS: &[&str] = &["+", "-", "/", "*", "%", "=", "<=", ">=", "<", ">"];

// 1:1 with upstream `Operator.js:16`:
//   const operRegex = new RegExp(`([/|*}])`);
// Captures `/`, `|`, `*`, or `}` — the literal upstream char class
// (the `|` and `}` look anomalous but are upstream-faithful).
pub static OPERATOR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"([/|*}])").unwrap());

// 1:1 with upstream `Operator.js:17`:
//   const compactRegex = /^[*/]\b/;
// Used by `Operator.test()` to detect the compact `*` / `/` operator
// that immediately follows a function call without whitespace.
pub static COMPACT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[*/]\b").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn chars_match_upstream_count() {
        assert_eq!(OPERATOR_CHARS.len(), 10);
        assert!(OPERATOR_CHARS.contains(&"="));
        assert!(OPERATOR_CHARS.contains(&"<="));
        assert!(OPERATOR_CHARS.contains(&">="));
    }

    #[test] fn oper_regex_matches_pipe() {
        // Upstream char class `[/|*}]` includes `|` and `}` literally.
        assert!(OPERATOR_REGEX.is_match("a|b"));
        assert!(OPERATOR_REGEX.is_match("a*b"));
        assert!(OPERATOR_REGEX.is_match("a/b"));
        assert!(OPERATOR_REGEX.is_match("a}b"));
    }
}
