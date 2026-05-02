//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/browsers.js`.

/// Static set of vendor prefixes autoprefixer recognises.
/// JS: `Browsers.prefixes()` returns the union over caniuse-db prefix
/// columns. We pin the same union for parity.
const PREFIX_LIST: &[&str] = &[
    "-webkit-",
    "-moz-",
    "-o-",
    "-ms-",
    "-khtml-",
];

#[derive(Debug, Clone, Default)]
pub struct Browsers {
    pub selected: Vec<String>,
    pub data: serde_json::Value,
    pub options: BrowsersOptions,
}

#[derive(Debug, Clone, Default)]
pub struct BrowsersOptions {
    pub ignore_unknown_versions: bool,
    pub stats: Option<serde_json::Value>,
    pub env: Option<String>,
    pub path: Option<String>,
}

impl Browsers {
    /// Static list of vendor prefixes.
    pub fn prefixes() -> &'static [&'static str] {
        PREFIX_LIST
    }

    /// True if `prefix` is one of the recognised vendor prefixes.
    pub fn is_prefix(prefix: &str) -> bool {
        PREFIX_LIST.contains(&prefix)
    }

    pub fn new(_data: serde_json::Value, _requirements: Vec<String>, _options: BrowsersOptions) -> Self {
        unimplemented!("Phase 7 — port browsers.js constructor + cleanBrowsers + selected resolution")
    }

    pub fn selected(&self) -> &[String] {
        &self.selected
    }

    /// Return prefix for browser name (e.g. "ios_saf" → "-webkit-").
    pub fn prefix(&self, _browser: &str) -> String {
        unimplemented!("Phase 7 — port browsers.js::prefix")
    }

    /// Is browser in selected list.
    pub fn is_selected(&self, _browser: &str) -> bool {
        unimplemented!("Phase 7 — port browsers.js::isSelected")
    }
}
