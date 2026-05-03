//! Port of `browserslist/parse.js` — restricted to the AFM query surface.
//!
//! ## Scope contract
//!
//! Only the query atoms reachable from AFM's pinned `.browserslistrc`
//! (see `BROWSER_LIST_FROM_AFM.md` and `AFM_PORT_NOTES.md`) are recognised
//! here. Unrecognised atoms return `None` from [`try_parse_atom_afm`] —
//! the resolver in `index.rs` then falls back to `oxc_browserslist` for
//! drift-tolerant handling. The fallback is documented behaviour, not a
//! bug; existing Phase 6 cssnano consumers (`postcss-normalize-unicode`,
//! `postcss-colormin`, `caniuse-api`) rely on it for `> X%`, `<= X`,
//! `not all`, `last N versions` (no browser), defaults, etc.
//!
//! Adding a new AFM atom = add a `QueryAtom` variant + extend the regex
//! cascade below + extend the resolver in `index.rs`. Do NOT silently
//! widen the fast path without updating `AFM_PORT_NOTES.md` so the next
//! agent can audit what's covered.

use once_cell::sync::Lazy;
use regex::Regex;

/// A query atom recognised by the AFM-fast-path resolver.
///
/// Variants intentionally cover the SMALLEST surface that resolves
/// AFM's `.browserslistrc` byte-correctly (plus the `BrowserVersion`
/// literal needed by the Firefox ESR rewrite). Everything else routes
/// through the oxc fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryAtom {
    /// `last N <browser> version[s]?` — e.g. `last 2 Edge version`,
    /// `last 5 Chrome version`. The single atom AFM's `.browserslistrc`
    /// contains. Browser name is canonicalised (lowercased + aliased)
    /// at parse time.
    LastNBrowserVersions { n: u32, browser: String },
    /// `<browser> <version>` — single literal version, e.g. `firefox 115`.
    /// Used by the Firefox ESR rewrite (`Firefox ESR` →
    /// `firefox 115, firefox 128`). Browser name canonicalised at parse
    /// time.
    BrowserVersion { browser: String, version: String },
}

static LAST_N_BROWSER_VERSIONS_RE: Lazy<Regex> = Lazy::new(|| {
    // Mirrors `last_browser_versions.regexp` in
    // `crates/_vendor/browserslist-4.24.4/package/index.js:703`.
    Regex::new(r"(?i)^last\s+(\d+)\s+(\w+)\s+versions?$").unwrap()
});

static BROWSER_VERSION_RE: Lazy<Regex> = Lazy::new(|| {
    // Allows `firefox 115`, `ios_saf 18.5-18.7`, `chrome 144`, etc.
    // Intentionally narrow (digits-led, no operators) so it does NOT
    // swallow `ie <=11` or `> 1%` style atoms.
    Regex::new(r"(?i)^(\w+)\s+([0-9][0-9.\-]*)$").unwrap()
});

/// Try to parse a single query atom against the AFM-fast-path grammar.
///
/// Returns `None` for any atom outside the AFM surface — the caller
/// should treat `None` as "not handled here, fall through to oxc".
pub fn try_parse_atom_afm(query: &str) -> Option<QueryAtom> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }

    if let Some(caps) = LAST_N_BROWSER_VERSIONS_RE.captures(q) {
        let n: u32 = caps.get(1)?.as_str().parse().ok()?;
        let browser = canonical_browser_name(caps.get(2)?.as_str());
        return Some(QueryAtom::LastNBrowserVersions { n, browser });
    }

    if let Some(caps) = BROWSER_VERSION_RE.captures(q) {
        let browser = canonical_browser_name(caps.get(1)?.as_str());
        let version = caps.get(2)?.as_str().to_string();
        return Some(QueryAtom::BrowserVersion { browser, version });
    }

    None
}

/// Try to parse every atom in `queries`. If ALL atoms parse, returns
/// the vector. If any atom is outside the AFM surface, returns `None`
/// — signalling the resolver to use the oxc fallback for the whole
/// query (a partial mix would silently drift, which we do not allow).
pub fn try_parse_all_afm(queries: &[String]) -> Option<Vec<QueryAtom>> {
    queries.iter().map(|q| try_parse_atom_afm(q)).collect()
}

/// Lowercase + apply browserslist's name aliases. Mirrors
/// `browserslist.aliases` in
/// `crates/_vendor/browserslist-4.24.4/package/index.js:480`.
pub fn canonical_browser_name(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "fx" | "ff" => "firefox".to_string(),
        "ios" => "ios_saf".to_string(),
        "explorer" => "ie".to_string(),
        "blackberry" => "bb".to_string(),
        "explorermobile" => "ie_mob".to_string(),
        "operamini" => "op_mini".to_string(),
        "operamobile" => "op_mob".to_string(),
        "chromeandroid" => "and_chr".to_string(),
        "firefoxandroid" => "and_ff".to_string(),
        "ucandroid" => "and_uc".to_string(),
        "qqandroid" => "and_qq".to_string(),
        _ => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_afm_atoms() {
        assert_eq!(
            try_parse_atom_afm("last 2 Edge version"),
            Some(QueryAtom::LastNBrowserVersions { n: 2, browser: "edge".into() })
        );
        assert_eq!(
            try_parse_atom_afm("last 5 Chrome version"),
            Some(QueryAtom::LastNBrowserVersions { n: 5, browser: "chrome".into() })
        );
        assert_eq!(
            try_parse_atom_afm("last 2 ChromeAndroid version"),
            Some(QueryAtom::LastNBrowserVersions { n: 2, browser: "and_chr".into() })
        );
        assert_eq!(
            try_parse_atom_afm("last 2 iOS version"),
            Some(QueryAtom::LastNBrowserVersions { n: 2, browser: "ios_saf".into() })
        );
    }

    #[test]
    fn parses_versions_form() {
        assert_eq!(
            try_parse_atom_afm("last 2 Firefox versions"),
            Some(QueryAtom::LastNBrowserVersions { n: 2, browser: "firefox".into() })
        );
    }

    #[test]
    fn parses_browser_version_literal() {
        assert_eq!(
            try_parse_atom_afm("firefox 115"),
            Some(QueryAtom::BrowserVersion { browser: "firefox".into(), version: "115".into() })
        );
        assert_eq!(
            try_parse_atom_afm("ios_saf 18.5-18.7"),
            Some(QueryAtom::BrowserVersion { browser: "ios_saf".into(), version: "18.5-18.7".into() })
        );
    }

    #[test]
    fn returns_none_for_unsupported_atoms() {
        // These all route through the oxc fallback (today's behaviour).
        assert_eq!(try_parse_atom_afm("> 0.5%"), None);
        assert_eq!(try_parse_atom_afm("last 2 versions"), None);
        assert_eq!(try_parse_atom_afm("not dead"), None);
        assert_eq!(try_parse_atom_afm("not all"), None);
        assert_eq!(try_parse_atom_afm("Firefox ESR"), None);
        assert_eq!(try_parse_atom_afm("ie <=11"), None);
        assert_eq!(try_parse_atom_afm("chrome >= 50"), None);
        assert_eq!(try_parse_atom_afm("defaults"), None);
        assert_eq!(try_parse_atom_afm(""), None);
    }

    #[test]
    fn try_parse_all_afm_unanimous_or_none() {
        let all_afm: Vec<String> = vec![
            "last 2 Edge version".into(),
            "last 5 Chrome version".into(),
        ];
        assert!(try_parse_all_afm(&all_afm).is_some());

        let mixed: Vec<String> = vec![
            "last 2 Edge version".into(),
            "> 1%".into(),
        ];
        assert!(try_parse_all_afm(&mixed).is_none());
    }

    #[test]
    fn canonical_browser_name_aliases() {
        assert_eq!(canonical_browser_name("Chrome"), "chrome");
        assert_eq!(canonical_browser_name("FF"), "firefox");
        assert_eq!(canonical_browser_name("ChromeAndroid"), "and_chr");
        assert_eq!(canonical_browser_name("iOS"), "ios_saf");
        assert_eq!(canonical_browser_name("Edge"), "edge");
        assert_eq!(canonical_browser_name("Firefox"), "firefox");
        assert_eq!(canonical_browser_name("Safari"), "safari");
    }
}
