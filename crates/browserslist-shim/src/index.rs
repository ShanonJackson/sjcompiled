//! Port of `browserslist/index.js` — query resolution entry point.

use crate::node::{default_query, load_config};
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
///   3. The `browserslist@4.24.4` defaults (see [`node::DEFAULT_QUERIES`]).
pub fn resolve_with(query: &str, opts: &ResolveOpts) -> Vec<String> {
    let trimmed = query.trim();
    let q: String = if !trimmed.is_empty() {
        trimmed.to_string()
    } else if let Some(loaded) = load_config(opts.path, opts.env) {
        loaded.join(", ")
    } else {
        default_query()
    };
    match browserslist::resolve(&[q.as_str()], &browserslist::Opts::default()) {
        Ok(distribs) => distribs.into_iter().map(|d| d.to_string()).collect(),
        Err(_) => Vec::new(),
    }
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
}
