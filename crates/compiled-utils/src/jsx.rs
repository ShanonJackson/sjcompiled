//! Port of `packages/utils/src/jsx.ts`.
//!
//! Copied from Babel's `babel-plugin-transform-react-jsx`. The JS side
//! uses a JS regex literal; the Rust side uses the `regex` crate's
//! syntax. Both must match the same byte ranges over the same input.
//!
//! Drift watch point: when upstream Babel updates this regex, both
//! sides MUST update in lockstep. See `crates/babel-plugin/` for the
//! consumer.

use regex::Regex;
use std::sync::OnceLock;

/// JS source: `/^\s*\*?\s*@jsx\s+([^\s]+)\s*$/m`.
///
/// Lazy-init via `OnceLock` so compilation cost is paid once per
/// process, not once per module the regex is consumed from.
pub fn jsx_annotation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^\s*\*?\s*@jsx\s+([^\s]+)\s*$")
            .expect("JSX_ANNOTATION_REGEX failed to compile")
    })
}

/// Mirrors `babel-plugin.ts`'s file-local `JSX_SOURCE_ANNOTATION_REGEX`
/// (`/\*?\s*@jsxImportSource\s+([^\s]+)/`). Hoisted here so the
/// constant lives next to its sibling `JSX_ANNOTATION_REGEX` and both
/// have a single drift watch point.
pub fn jsx_source_annotation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\*?\s*@jsxImportSource\s+([^\s]+)")
            .expect("JSX_SOURCE_ANNOTATION_REGEX failed to compile")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The regex consumes Babel's `comment.value` — i.e. the comment
    // content with its `/*`, `*/`, `//` delimiters stripped. Tests
    // mirror what babel-plugin actually feeds in (see upstream
    // babel-plugin.ts's `for (const comment of file.ast.comments)` loop
    // at `comment.value`).

    #[test]
    fn jsx_annotation_matches_block_comment_value() {
        // `/** @jsx jsx */` — Babel's comment.value is `* @jsx jsx `.
        let re = jsx_annotation_regex();
        let cap = re.captures("* @jsx jsx ").unwrap();
        assert_eq!(&cap[1], "jsx");
    }

    #[test]
    fn jsx_annotation_matches_renamed_pragma_value() {
        let re = jsx_annotation_regex();
        let cap = re.captures("* @jsx myJsx ").unwrap();
        assert_eq!(&cap[1], "myJsx");
    }

    #[test]
    fn jsx_annotation_handles_multi_line_block() {
        // `/**\n * @jsx jsx\n */` — comment.value is `*\n * @jsx jsx\n `.
        let re = jsx_annotation_regex();
        let block = "*\n * @jsx jsx\n ";
        let cap = re.captures(block).unwrap();
        assert_eq!(&cap[1], "jsx");
    }

    #[test]
    fn jsx_annotation_no_match_when_absent() {
        let re = jsx_annotation_regex();
        assert!(re.captures(" nothing here ").is_none());
    }

    #[test]
    fn jsx_source_matches_import_source_pragma() {
        let re = jsx_source_annotation_regex();
        let cap = re.captures("* @jsxImportSource @compiled/react ").unwrap();
        assert_eq!(&cap[1], "@compiled/react");
    }

    #[test]
    fn jsx_source_matches_emotion_pragma() {
        let re = jsx_source_annotation_regex();
        let cap = re.captures("* @jsxImportSource @emotion/react ").unwrap();
        assert_eq!(&cap[1], "@emotion/react");
    }
}
