//! crates/cssnano-postcss-normalize-unicode
//! Byte-for-byte Rust port of `postcss-normalize-unicode@5.1.1`.
//!
//! Folder/file mapping (1:1 with upstream
//! `node_modules/postcss-normalize-unicode/`):
//!   - `src/index.js` -> `src/lib.rs` (this file).
//!
//! Browserslist-aware. Per upstream `prepare(result)`:
//!
//!   const browsers = browserslist(null, { stats, path: __dirname, env });
//!   const isLegacy = browsers.some(hasLowerCaseUPrefixBug);
//!
//! `hasLowerCaseUPrefixBug(b)` returns true iff `b` is in
//! `browserslist('ie <=11, edge <= 15')`. With the workspace's locked
//! `browserslist@4.24.2` defaults — `> 0.5%, last 2 versions, Firefox ESR,
//! not dead` — IE/old-Edge are out of range, so `isLegacy = false` in
//! practice. We still compute it for parity with upstream behaviour.
//!
//! `OnceExit`-only plugin in upstream. We resolve browserslist once per
//! `postcss_normalize_unicode` call, then walk every decl whose prop
//! matches `/^unicode-range$/i`, transforming the value via
//! `postcss-value-parser`'s walk (function children intentionally
//! unwalked — upstream cb returns `false`).

use indexmap::IndexMap;
use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::NodeKind as VpKind;
use postcss_value_parser::{parse as value_parse, stringify as value_stringify, walk};

/// Upstream `hasLowerCaseUPrefixBug` query (index.js:68).
const LEGACY_BROWSERS_QUERY: &str = "ie <=11, edge <= 15";

/// Plugin entry. Default options pass-through; AFM consumer
/// (`cssnano-preset-default@5.2.14`) calls `creator()` with no opts.
pub fn postcss_normalize_unicode(root: &mut Root) -> PluginResult {
    // `prepare(result)`:
    //   - `browsers = browserslist(null, { path: __dirname, ... })`
    //   - `isLegacy = browsers.some(hasLowerCaseUPrefixBug)`
    //
    // The `__dirname` argument is what upstream uses; in practice this
    // walks up from `node_modules/postcss-normalize-unicode/src/` and
    // ends up resolving the workspace's effective browserslist config
    // (or the 4.24.2 defaults if none is set). We resolve via the shim
    // with an empty query, which mirrors `browserslist(null, ...)`.
    let browsers = browserslist_shim::resolve("", true);
    let legacy_browsers = browserslist_shim::resolve(LEGACY_BROWSERS_QUERY, true);
    let is_legacy = browsers.iter().any(|b| legacy_browsers.contains(b));

    // `prepare` instantiates `cache = new Map()` per process() call.
    // Cache is a memo only — never iterated — but we still use IndexMap
    // per the cardinal-rule ban on HashMap in output paths.
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        // `walkDecls(/^unicode-range$/i, ...)` — case-insensitive prop match.
        if let NodeKind::Declaration(decl) = &mut node.kind {
            if !decl.prop.eq_ignore_ascii_case("unicode-range") {
                return Mutation::Keep;
            }
            let value = decl.value.clone();
            if let Some(cached) = cache.get(&value) {
                decl.value = cached.clone();
            } else {
                let new_value = transform(&value, is_legacy);
                decl.value = new_value.clone();
                cache.insert(value, new_value);
            }
        }
        Mutation::Keep
    });
    Ok(())
}

/// `transform(value, isLegacy)` — index.js:75. Bubble=false walk over
/// the value-parser tree; cb returns `false` so function children are
/// not descended into (matching upstream).
fn transform(value: &str, is_legacy: bool) -> String {
    let mut nodes = value_parse(value);
    walk(
        &mut nodes,
        |child, _idx| {
            if child.kind == VpKind::UnicodeRange {
                let lower = child.value.to_lowercase();
                let transformed = unicode(&lower);
                child.value = if is_legacy {
                    replace_lower_case_u_prefix(&transformed)
                } else {
                    transformed
                };
            }
            // Upstream cb always `return false` (skip function recursion).
            Some(false)
        },
        false,
    );
    value_stringify(&nodes)
}

/// `regexLowerCaseUPrefix = /^u(?=\+)/` — replace leading `u` with `U`,
/// but only if the very next character is `+`. Index.js:5 + index.js:82.
fn replace_lower_case_u_prefix(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'u' && bytes[1] == b'+' {
        let mut out = String::with_capacity(s.len());
        out.push('U');
        out.push_str(&s[1..]);
        out
    } else {
        s.to_string()
    }
}

/// `unicode(range)` — index.js:11. Slice the `u+` prefix, split on `-`,
/// and try to merge the two bounds into a wildcard form.
fn unicode(range: &str) -> String {
    // `range.slice(2)` — JS string-slice. The unicode-range token always
    // starts with `u+` after `to_lowercase()`. If it's shorter than 2
    // chars somehow, slice(2) on JS returns "" and we'd hit the
    // `values.length < 2` bail; mirror that exactly.
    let after_prefix: &str = if range.len() >= 2 { &range[2..] } else { "" };
    let values: Vec<&str> = after_prefix.split('-').collect();
    if values.len() < 2 {
        return range.to_string();
    }
    // `values[0].split('')` / `values[1].split('')` — JS char split.
    // The unicode-range syntax is ASCII (hex digits / `?`), so chars()
    // and JS split('') agree byte-for-byte.
    let left: Vec<char> = values[0].chars().collect();
    let right: Vec<char> = values[1].chars().collect();
    if left.len() != right.len() {
        return range.to_string();
    }
    if let Some(merged) = merge_range_bounds(&left, &right) {
        merged
    } else {
        range.to_string()
    }
}

/// `mergeRangeBounds(left, right)` — index.js:38. Returns `None` for
/// the JS `false` falsy return; Some(group) for a successful merge.
fn merge_range_bounds(left: &[char], right: &[char]) -> Option<String> {
    let mut question_counter: usize = 0;
    let mut group = String::from("u+");
    for (index, &value) in left.iter().enumerate() {
        if value == right[index] && question_counter == 0 {
            group.push(value);
        } else if value == '0' && right[index] == 'f' {
            question_counter += 1;
            group.push('?');
        } else {
            return None;
        }
    }
    // The maximum number of wildcard characters (?) for ranges is 5.
    if question_counter < 6 {
        Some(group)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_unicode_simple_range_lowercases() {
        assert_eq!(unicode("u+0025-00ff"), "u+0025-00ff");
    }

    #[test]
    fn unit_unicode_collapses_to_wildcard() {
        assert_eq!(unicode("u+0000-00ff"), "u+00??");
        assert_eq!(unicode("u+0000-ffff"), "u+????");
    }

    #[test]
    fn unit_unicode_keeps_partial_wildcard() {
        // After first ? appears, only `0` vs `f` continues the wildcard
        // run; any other equal pair returns false (range unchanged).
        // u+0a00-0aff: index 0 '0'=='0' (counter=0, group="u+0"),
        //              index 1 'a'=='a' (counter=0, group="u+0a"),
        //              index 2 '0' vs 'f' (0==0,r==f, counter=1, group="u+0a?"),
        //              index 3 '0' vs 'f' (counter=2, group="u+0a??") → "u+0a??"
        assert_eq!(unicode("u+0a00-0aff"), "u+0a??");
    }

    #[test]
    fn unit_unicode_unequal_lengths_passthrough() {
        assert_eq!(unicode("u+0-ffff"), "u+0-ffff");
    }

    #[test]
    fn unit_unicode_no_dash_passthrough() {
        assert_eq!(unicode("u+25"), "u+25");
    }

    #[test]
    fn unit_replace_u_prefix_only_when_plus_follows() {
        assert_eq!(replace_lower_case_u_prefix("u+0025"), "U+0025");
        assert_eq!(replace_lower_case_u_prefix("u0025"), "u0025"); // no `+` → no replace
    }

    #[test]
    fn unit_max_five_wildcards() {
        // 6 wildcards is rejected → original returned.
        // u+000000-ffffff would yield 6 ?s; but length check needs 6 chars
        // each side. Build: left "000000", right "ffffff".
        // counter goes 1→6, fails the `<6` check.
        let l: Vec<char> = "000000".chars().collect();
        let r: Vec<char> = "ffffff".chars().collect();
        assert_eq!(merge_range_bounds(&l, &r), None);
    }
}
