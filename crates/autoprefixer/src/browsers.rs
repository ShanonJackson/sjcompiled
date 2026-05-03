//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/browsers.js`.

use std::sync::OnceLock;

use indexmap::IndexMap;

use crate::utils;

/// `Browsers.prefixes()` — derived from `caniuse-lite/dist/unpacker/agents`.
/// JS pushes `-${agents[name].prefix}-` for each agent, dedupes via
/// `utils.uniq`, and sorts by descending length.
fn build_prefixes() -> Vec<String> {
    let agents: &IndexMap<String, caniuse_db::agents::Agent> = &caniuse_db::agents::AGENTS;
    let raw: Vec<String> = agents
        .values()
        .map(|a| format!("-{}-", a.prefix))
        .collect();
    let mut deduped = utils::uniq(&raw);
    // JS: `.sort((a, b) => b.length - a.length)` — stable on ties; matches
    // V8's TimSort. Rust's `sort_by` is also stable, so equal-length
    // entries keep their first-seen order from `uniq`.
    deduped.sort_by(|a, b| b.len().cmp(&a.len()));
    deduped
}

static PREFIXES_CACHE: OnceLock<Vec<String>> = OnceLock::new();
static PREFIXES_REGEXP: OnceLock<regex::Regex> = OnceLock::new();

/// Holds caniuse data + browserslist-resolved selection. JS constructor:
/// `constructor(data, requirements, options, browserslistOpts)`.
#[derive(Debug, Clone)]
pub struct Browsers {
    pub selected: Vec<String>,
    pub options: BrowsersOptions,
    pub browserslist_opts: BrowserslistOpts,
}

#[derive(Debug, Clone, Default)]
pub struct BrowsersOptions {
    /// JS `options.from` — feeds into browserslist as `opts.path`.
    pub from: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserslistOpts {
    pub ignore_unknown_versions: bool,
}

impl Browsers {
    /// Static — return all prefixes for default browser data. Cached
    /// across calls (JS uses a class-static `prefixesCache`).
    pub fn prefixes() -> &'static [String] {
        PREFIXES_CACHE.get_or_init(build_prefixes)
    }

    /// Static — `withPrefix(value)` — does `value` contain any of the
    /// recognised prefixes? JS builds `new RegExp(prefixes.join('|'))`.
    pub fn with_prefix(value: &str) -> bool {
        let re = PREFIXES_REGEXP.get_or_init(|| {
            let pattern = Self::prefixes().join("|");
            regex::Regex::new(&pattern).expect("valid prefix-union regex")
        });
        re.is_match(value)
    }

    /// True if `prefix` is one of the recognised vendor prefixes.
    /// (Convenience helper used by `prefixer.rs::sanitize`.)
    pub fn is_prefix(prefix: &str) -> bool {
        Self::prefixes().iter().any(|p| p == prefix)
    }

    /// JS: `constructor(data, requirements, options, browserslistOpts)`.
    pub fn new(
        requirements: Vec<String>,
        options: BrowsersOptions,
        browserslist_opts: BrowserslistOpts,
    ) -> Self {
        let selected = Self::parse_static(&requirements, &options, &browserslist_opts);
        Self { selected, options, browserslist_opts }
    }

    /// JS: `this.data` — the caniuse-lite agents table. Static singleton
    /// in our world.
    pub fn data() -> &'static IndexMap<String, caniuse_db::agents::Agent> {
        &caniuse_db::agents::AGENTS
    }

    /// JS: `parse(requirements)` — calls `browserslist(requirements, opts)`.
    /// `opts.path = this.options.from`. We split out the static helper so
    /// the constructor can call it without holding `&self`.
    ///
    /// `options.from` is plumbed through to the shim as `ResolveOpts::path`,
    /// matching `browserslist@4.24.2`'s `prepareOpts` (index.js:366) which
    /// defaults `opts.path` to `path.resolve('.')` when absent. We mirror
    /// that fallback by reading `std::env::current_dir()` if `from` is
    /// unset — this is what AFM's autoprefixer call sees at build time
    /// (`browserslist(null, { path: cwd })`, see `BROWSER_LIST_FROM_AFM.md`).
    fn parse_static(
        requirements: &[String],
        options: &BrowsersOptions,
        browserslist_opts: &BrowserslistOpts,
    ) -> Vec<String> {
        let query = requirements.join(", ");
        let path_owned: Option<std::path::PathBuf> = options
            .from
            .as_ref()
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok());
        let opts = browserslist_shim::index::ResolveOpts {
            path: path_owned.as_deref(),
            env: None,
            ignore_unknown_versions: browserslist_opts.ignore_unknown_versions,
        };
        browserslist_shim::index::resolve_with(&query, &opts)
    }

    /// Re-resolve `selected` against new requirements (matches the JS
    /// instance method, used by tests).
    pub fn parse(&mut self, requirements: &[String]) -> Vec<String> {
        Self::parse_static(requirements, &self.options, &self.browserslist_opts)
    }

    /// JS: `prefix(browser)` — `browser` is "name version".
    /// ```js
    /// let [name, version] = browser.split(' ')
    /// let data = this.data[name]
    /// let prefix = data.prefix_exceptions && data.prefix_exceptions[version]
    /// if (!prefix) prefix = data.prefix
    /// return `-${prefix}-`
    /// ```
    pub fn prefix(&self, browser: &str) -> String {
        let mut parts = browser.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let version = parts.next().unwrap_or("");
        let agent = Self::data()
            .get(name)
            .expect("Browsers::prefix called with unknown agent name");
        let p = agent
            .prefix_exceptions
            .get(version)
            .map(String::as_str)
            .unwrap_or(agent.prefix.as_str());
        format!("-{p}-")
    }

    /// JS: `isSelected(browser) { return this.selected.includes(browser) }`.
    pub fn is_selected(&self, browser: &str) -> bool {
        self.selected.iter().any(|b| b == browser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_includes_webkit_and_moz() {
        let p = Browsers::prefixes();
        assert!(p.iter().any(|s| s == "-webkit-"));
        assert!(p.iter().any(|s| s == "-moz-"));
    }

    #[test]
    fn prefixes_sorted_by_descending_length_stable() {
        let p = Browsers::prefixes();
        for w in p.windows(2) {
            assert!(
                w[0].len() >= w[1].len(),
                "expected descending length, got {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn with_prefix_detects_known_prefixes() {
        assert!(Browsers::with_prefix("display: -webkit-flex"));
        assert!(Browsers::with_prefix("color: -moz-something"));
        assert!(!Browsers::with_prefix("display: flex"));
    }

    #[test]
    fn is_prefix_recognises_vendor_strings() {
        assert!(Browsers::is_prefix("-webkit-"));
        assert!(Browsers::is_prefix("-moz-"));
        assert!(!Browsers::is_prefix("-zzz-"));
        assert!(!Browsers::is_prefix(""));
    }
}
