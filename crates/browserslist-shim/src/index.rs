//! Port of `browserslist/index.js` — query resolution entry point.
//!
//! ## Architecture: hybrid AFM-fast-path + oxc fallback
//!
//! The shim has two resolution paths:
//!
//! 1. **AFM fast path** (byte-correct against the pinned `caniuse-db`
//!    snapshot at `1.0.30001766`). Activated automatically when EVERY
//!    atom in the resolved query parses against the AFM grammar in
//!    [`crate::parse::try_parse_atom_afm`]. Today that's
//!    `last N <browser> version[s]?` and `<browser> <version>` literals
//!    — exactly what AFM's `.browserslistrc` produces (and what the
//!    Firefox ESR rewrite expands into).
//!
//! 2. **`oxc_browserslist` fallback** (drift-tolerant; uses oxc's
//!    bundled snapshot, which is ~2 chrome releases newer than our
//!    pin). Activated when any atom is outside the AFM grammar — used
//!    today by `cssnano-postcss-normalize-unicode` (`> 0.5%`, `<= 15`),
//!    `caniuse-api::clean_browsers_list` (arbitrary user queries), the
//!    defaults path, etc. The drift on these consumers is documented
//!    as acceptable in `crates/STATUS.md` (the consumers reduce to a
//!    boolean from a set intersection that is drift-stable).
//!
//! See `AFM_PORT_NOTES.md` for the full rationale and the closure
//! history of the autoprefixer parity gate.

use crate::node::{default_query, load_config};
use crate::parse::{try_parse_all_afm, QueryAtom};
use once_cell::sync::Lazy;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ResolveOpts<'a> {
    pub path: Option<&'a Path>,
    pub env: Option<&'a str>,
    pub ignore_unknown_versions: bool,
}

/// Resolve a query into a list of `"<name> <version>"` entries.
///
/// Convenience wrapper around [`resolve_with`] with `path` / `env`
/// defaulted. Use [`resolve_with`] when you need to plumb a `path` for
/// `.browserslistrc` discovery (autoprefixer's AFM call site does this).
pub fn resolve(query: &str, ignore_unknown_versions: bool) -> Vec<String> {
    resolve_with(query, &ResolveOpts { ignore_unknown_versions, ..Default::default() })
}

/// Resolve with full opts. The query is resolved against an effective
/// query list determined by:
///   1. The explicit `query` argument (if non-empty).
///   2. `BROWSERSLIST` env / `BROWSERSLIST_CONFIG` env / nearest config file.
///   3. The `browserslist@4.24.2` defaults (see [`crate::node::DEFAULT_QUERIES`]).
pub fn resolve_with(query: &str, opts: &ResolveOpts) -> Vec<String> {
    let queries = build_effective_queries(query, opts);

    // Apply Firefox ESR expansion BEFORE deciding fast-path-vs-fallback,
    // so the rewrite's literal `firefox 115, firefox 128` atoms go down
    // the AFM fast path. Mirrors `browserslist@4.24.2`'s `select()`
    // override at index.js ~line 1024.
    let queries: Vec<String> = queries.iter().flat_map(|q| expand_firefox_esr_atom(q)).collect();

    // AFM fast path: every atom recognised → resolve against caniuse-db
    // directly. Byte-correct for the pinned snapshot.
    if let Some(atoms) = try_parse_all_afm(&queries) {
        return resolve_afm_atoms(&atoms, opts);
    }

    // Fallback: at least one atom is outside the AFM grammar. Defer to
    // oxc_browserslist (drift-tolerant; documented in module docstring).
    // The `oxc-browserslist` crate publishes its query-resolver under
    // the `browserslist` module name (see its `pub use wasm::browserslist`
    // re-export in `oxc-browserslist/src/lib.rs`).
    let joined = queries.join(", ");
    match browserslist::resolve(&[joined.as_str()], &browserslist::Opts::default()) {
        Ok(distribs) => distribs.into_iter().map(|d| d.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Build the effective query list by walking the upstream resolution
/// chain (explicit > env/config > defaults). Each entry is a single
/// atom — comma-splitting the explicit query and the joined config
/// happens here.
fn build_effective_queries(query: &str, opts: &ResolveOpts) -> Vec<String> {
    let trimmed = query.trim();
    if !trimmed.is_empty() {
        return trimmed
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(loaded) = load_config(opts.path, opts.env) {
        // `loaded` is already split per-line by `parse_config`, but the
        // `parsePackage` array path may include comma-joined entries.
        // Re-split defensively.
        let mut out: Vec<String> = Vec::new();
        for entry in loaded {
            for part in entry.split(',') {
                let t = part.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
        return out;
    }
    default_query()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Resolve a parsed AFM atom list into the final `"<name> <version>"`
/// distribution vector. Mirrors browserslist's resolve loop: dedupe,
/// then apply the cross-browser/intra-browser sort.
fn resolve_afm_atoms(atoms: &[QueryAtom], opts: &ResolveOpts) -> Vec<String> {
    let mut acc: Vec<String> = Vec::new();
    for atom in atoms {
        match atom {
            QueryAtom::LastNBrowserVersions { n, browser } => {
                acc.extend(resolve_last_n_browser_versions(browser, *n, opts.ignore_unknown_versions));
            }
            QueryAtom::BrowserVersion { browser, version } => {
                acc.extend(resolve_browser_version(browser, version, opts.ignore_unknown_versions));
            }
        }
    }
    sort_distribs(uniq_preserve_first(acc))
}

/// `last N <browser> version[s]?` → `agent.released.slice(-N).map(name + ' ' + v)`.
///
/// `agent.released` is computed from the snapshot's `versions` array
/// (filtering Nones AND entries whose `release_date` is null/None — i.e.
/// future planned releases). Mirrors caniuse-lite's unpacker semantics.
fn resolve_last_n_browser_versions(browser: &str, n: u32, ignore_unknown: bool) -> Vec<String> {
    let agent = match caniuse_db::agents::agent(browser) {
        Some(a) => a,
        None => {
            if ignore_unknown {
                return Vec::new();
            }
            // Match upstream `checkName` which throws `Unknown browser`.
            // Panic instead of returning Err — the call surface is shaped
            // the way browserslist's JS surface is (Vec<String> return,
            // throw on logic error).
            panic!("browserslist-shim: Unknown browser `{browser}`");
        }
    };
    let released: Vec<&str> = agent
        .versions
        .iter()
        .filter_map(|v| v.as_deref())
        .filter(|v| matches!(agent.release_date.get(*v), Some(Some(_))))
        .collect();
    let take = n as usize;
    let slice = if released.len() <= take {
        &released[..]
    } else {
        &released[released.len() - take..]
    };
    slice.iter().map(|v| format!("{browser} {v}")).collect()
}

/// `<browser> <version>` literal lookup. Used by the Firefox ESR
/// rewrite; rare elsewhere on the AFM path.
fn resolve_browser_version(browser: &str, version: &str, ignore_unknown: bool) -> Vec<String> {
    let agent = match caniuse_db::agents::agent(browser) {
        Some(a) => a,
        None => {
            if ignore_unknown {
                return Vec::new();
            }
            panic!("browserslist-shim: Unknown browser `{browser}`");
        }
    };
    for v in agent.versions.iter().filter_map(|v| v.as_deref()) {
        if v == version || version_in_range(v, version) {
            return vec![format!("{browser} {v}")];
        }
    }
    if ignore_unknown {
        Vec::new()
    } else {
        panic!("browserslist-shim: Unknown version `{version}` of `{browser}`")
    }
}

/// Caniuse stores some entries as ranges (e.g. `"18.5-18.7"`). When a
/// caller asks for `"18.6"` we want to match the range. Numeric compare
/// is sufficient for the AFM surface; AFM doesn't currently trip this
/// path.
fn version_in_range(range: &str, query: &str) -> bool {
    let mut parts = range.splitn(2, '-');
    let lo = parts.next().unwrap_or("");
    let hi = match parts.next() {
        Some(h) => h,
        None => return false,
    };
    if let (Ok(qf), Ok(lof), Ok(hif)) = (
        query.parse::<f64>(),
        lo.parse::<f64>(),
        hi.parse::<f64>(),
    ) {
        return qf >= lof && qf <= hif;
    }
    false
}

/// Dedupe preserving first occurrence (matches JS `uniq` at
/// `index.js:67`).
fn uniq_preserve_first(v: Vec<String>) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(v.len());
    for s in v {
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    out
}

/// Final sort mirroring `browserslist@4.24.2` index.js:431-444.
/// Same browser → descending semver. Different browser → ascending name
/// (lexicographic). Ranges are compared by their lower bound.
fn sort_distribs(mut v: Vec<String>) -> Vec<String> {
    v.sort_by(|a, b| {
        let (an, av) = split_distrib(a);
        let (bn, bv) = split_distrib(b);
        if an == bn {
            // JS does compareSemver(version2.split('.'), version1.split('.'))
            // — i.e. arguments are flipped to produce DESCENDING order.
            compare_semver(bv, av)
        } else {
            an.cmp(bn)
        }
    });
    v
}

fn split_distrib(s: &str) -> (&str, &str) {
    let mut it = s.splitn(2, ' ');
    let n = it.next().unwrap_or("");
    let v = it.next().unwrap_or("");
    (n, v)
}

/// Mirror of JS `compareSemver` (index.js:143). Falls back to the lower
/// bound of a range (`"18.5-18.7"` → compare against `18.5`).
fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.split('-').next().unwrap_or(a);
    let b = b.split('-').next().unwrap_or(b);
    let parse_n = |s: &str| -> i64 { s.parse::<i64>().unwrap_or(0) };
    let mut ap = a.split('.').map(parse_n);
    let mut bp = b.split('.').map(parse_n);
    let a0 = ap.next().unwrap_or(0);
    let b0 = bp.next().unwrap_or(0);
    if a0 != b0 {
        return a0.cmp(&b0);
    }
    let a1 = ap.next().unwrap_or(0);
    let b1 = bp.next().unwrap_or(0);
    if a1 != b1 {
        return a1.cmp(&b1);
    }
    let a2 = ap.next().unwrap_or(0);
    let b2 = bp.next().unwrap_or(0);
    a2.cmp(&b2)
}

/// Per-atom Firefox ESR expansion. Mirrors the comma-string variant of
/// `rewrite_firefox_esr` but operates on a single already-split atom.
/// Returns one or two atoms.
fn expand_firefox_esr_atom(atom: &str) -> Vec<String> {
    static ESR_RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"(?i)^\s*(not\s+)?(?:firefox|ff|fx)\s+esr\s*$").unwrap()
    });
    if let Some(caps) = ESR_RE.captures(atom) {
        let prefix = if caps.get(1).is_some() { "not " } else { "" };
        return vec![format!("{prefix}firefox 115"), format!("{prefix}firefox 128")];
    }
    vec![atom.to_string()]
}

/// Rewrites comma-separated query atoms matching `(firefox|ff|fx) esr`
/// (optionally prefixed with `not `) into the explicit pair
/// `firefox 115, firefox 128` (or two `not` atoms). Mirrors 4.24.2's
/// `select` for the `firefox_esr` query (index.js ~1018-1025).
///
/// **Kept for backwards-compat with existing string-based callers and
/// tests.** Internally [`resolve_with`] uses the per-atom variant
/// [`expand_firefox_esr_atom`] instead.
pub fn rewrite_firefox_esr(query: &str) -> String {
    static ESR_RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"(?i)^\s*(not\s+)?(?:firefox|ff|fx)\s+esr\s*$").unwrap()
    });
    let parts: Vec<String> = query
        .split(',')
        .map(|p| {
            if let Some(caps) = ESR_RE.captures(p) {
                let prefix = if caps.get(1).is_some() { "not " } else { "" };
                format!("{p1}firefox 115, {p2}firefox 128", p1 = prefix, p2 = prefix)
            } else {
                p.to_string()
            }
        })
        .collect();
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve() {
        // Defaults query routes through the oxc fallback (atoms like
        // `> 0.5%` are outside the AFM fast-path grammar). The fallback
        // returns oxc's drift-tolerant snapshot output — non-empty for
        // any sane bundled DB. Test pinned at "non-empty" to stay
        // robust across oxc snapshot updates.
        let out = resolve("", true);
        assert!(!out.is_empty(), "default query should resolve to >0 browsers");
    }

    #[test]
    fn explicit_query_wins() {
        // `<= 6` is outside the AFM grammar → oxc fallback. Result
        // should contain ie versions per oxc's resolution.
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
        assert_eq!(rewrite_firefox_esr("Firefox ESR extra"), "Firefox ESR extra");
    }

    /// AFM-fast-path smoke test: a single atom from AFM's
    /// `.browserslistrc` should resolve byte-correctly via caniuse-db.
    /// Pinned exact-output check — drift here is a hash-rotation event.
    #[test]
    fn afm_fast_path_last_5_chrome_version() {
        let out = resolve("last 5 Chrome version", true);
        assert_eq!(
            out,
            vec!["chrome 144", "chrome 143", "chrome 142", "chrome 141", "chrome 140"],
            "AFM-fast-path drift — investigate caniuse-db pin"
        );
    }

    #[test]
    fn afm_fast_path_last_2_edge_version() {
        let out = resolve("last 2 Edge version", true);
        assert_eq!(out, vec!["edge 144", "edge 143"]);
    }

    #[test]
    fn afm_fast_path_last_2_chromeandroid_version_yields_one() {
        // and_chr only has one released entry in the pinned snapshot
        // (slot 144). Asking for last 2 still returns 1 — same as
        // browserslist's `slice(-2)` on a 1-element array.
        let out = resolve("last 2 ChromeAndroid version", true);
        assert_eq!(out, vec!["and_chr 144"]);
    }

    #[test]
    fn afm_fast_path_full_query_byte_clean() {
        // Joined exactly as AFM's `.browserslistrc` lines comma-join.
        let q = "last 2 Edge version, last 2 Firefox version, \
                 last 5 Chrome version, last 2 Safari version, \
                 last 2 iOS version, last 2 ChromeAndroid version";
        let out = resolve(q, true);
        // Frozen 14-entry list from BROWSER_LIST_FROM_AFM.md.
        assert_eq!(
            out,
            vec![
                "and_chr 144",
                "chrome 144", "chrome 143", "chrome 142", "chrome 141", "chrome 140",
                "edge 144", "edge 143",
                "firefox 147", "firefox 146",
                "ios_saf 26.2", "ios_saf 26.1",
                "safari 26.2", "safari 26.1",
            ],
            "AFM fast path drifted from frozen oracle output"
        );
    }
}
