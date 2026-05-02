//! Port of `browserslist/index.js` — query resolution entry point.

use crate::node::{default_query, load_config};
use once_cell::sync::Lazy;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ResolveOpts<'a> {
    pub path: Option<&'a Path>,
    pub env: Option<&'a str>,
    pub ignore_unknown_versions: bool,
}

/// Resolve a query into a list of `"<name> <version>"` entries.
pub fn resolve(query: &str, ignore_unknown_versions: bool) -> Vec<String> {
    resolve_with(query, &ResolveOpts { ignore_unknown_versions, ..Default::default() })
}

/// Resolve with full opts. The query is resolved against an effective query
/// list determined by:
///   1. The explicit `query` argument (if non-empty).
///   2. `BROWSERSLIST` env / `BROWSERSLIST_CONFIG` env / nearest config file.
///   3. The `browserslist@4.24.2` defaults (see [`node::DEFAULT_QUERIES`]).
pub fn resolve_with(query: &str, opts: &ResolveOpts) -> Vec<String> {
    let trimmed = query.trim();
    let q: String = if !trimmed.is_empty() {
        trimmed.to_string()
    } else if let Some(loaded) = load_config(opts.path, opts.env) {
        loaded.join(", ")
    } else {
        default_query()
    };
    // 4.24.2: Firefox ESR `select()` returns ['firefox 115', 'firefox 128']
    // (index.js line ~1024). oxc_browserslist v3 bundles a newer snapshot and
    // returns just `firefox 140` — override by rewriting the query before
    // dispatch. Handles the bare and `not`-prefixed atom; `X and Firefox ESR`
    // is not supported (no AFM consumer uses it).
    let q = rewrite_firefox_esr(&q);
    match browserslist::resolve(&[q.as_str()], &browserslist::Opts::default()) {
        Ok(distribs) => distribs.into_iter().map(|d| d.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Rewrites comma-separated query atoms matching `(firefox|ff|fx) esr`
/// (optionally prefixed with `not `) into the explicit pair
/// `firefox 115, firefox 128` (or two `not` atoms). Mirrors 4.24.2's
/// `select` for the `firefox_esr` query (index.js ~1018-1025).
fn rewrite_firefox_esr(query: &str) -> String {
    static ESR_RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"(?i)^\s*(not\s+)?(?:firefox|ff|fx)\s+esr\s*$").unwrap()
    });
    let parts: Vec<String> = query.split(',').map(|p| {
        if let Some(caps) = ESR_RE.captures(p) {
            let prefix = if caps.get(1).is_some() { "not " } else { "" };
            format!("{p1}firefox 115, {p2}firefox 128", p1 = prefix, p2 = prefix)
        } else {
            p.to_string()
        }
    }).collect();
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve() {
        let out = resolve("", true);
        // Should produce non-empty browser list from the default query.
        assert!(!out.is_empty(), "default query should resolve to >0 browsers");
    }

    #[test]
    fn explicit_query_wins() {
        let out = resolve("ie <= 6", true);
        assert!(out.iter().any(|b| b.starts_with("ie ")), "expected ie versions, got {:?}", out);
    }

    #[test]
    fn firefox_esr_returns_two_versions() {
        let out = resolve("Firefox ESR", true);
        assert!(out.iter().any(|b| b == "firefox 115"),
            "expected firefox 115 in {:?}", out);
        assert!(out.iter().any(|b| b == "firefox 128"),
            "expected firefox 128 in {:?}", out);
        assert!(!out.iter().any(|b| b == "firefox 140"),
            "must not return oxc's bundled firefox 140 ESR; got {:?}", out);
    }

    #[test]
    fn firefox_esr_aliases() {
        for q in &["ff esr", "fx esr", "FIREFOX ESR", "Ff   ESR"] {
            let out = resolve(q, true);
            assert!(out.iter().any(|b| b == "firefox 115"), "{} -> {:?}", q, out);
            assert!(out.iter().any(|b| b == "firefox 128"), "{} -> {:?}", q, out);
        }
    }

    #[test]
    fn firefox_esr_combined_with_other_query() {
        let out = resolve("ie 11, Firefox ESR", true);
        assert!(out.iter().any(|b| b == "firefox 115"), "got {:?}", out);
        assert!(out.iter().any(|b| b == "firefox 128"), "got {:?}", out);
        assert!(out.iter().any(|b| b == "ie 11"), "got {:?}", out);
    }

    #[test]
    fn rewrite_firefox_esr_unit() {
        assert_eq!(rewrite_firefox_esr("Firefox ESR"),
            "firefox 115, firefox 128");
        assert_eq!(rewrite_firefox_esr("not Firefox ESR"),
            "not firefox 115, not firefox 128");
        assert_eq!(rewrite_firefox_esr("ie 11, Firefox ESR, last 2 chrome versions"),
            "ie 11,firefox 115, firefox 128, last 2 chrome versions");
        assert_eq!(rewrite_firefox_esr("ie 11"), "ie 11");
        // Don't match `Firefox ESR foo` etc.
        assert_eq!(rewrite_firefox_esr("Firefox ESR extra"), "Firefox ESR extra");
    }
}
