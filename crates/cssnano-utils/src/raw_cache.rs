//! Port of `cssnano-utils/src/rawCache.js`.
//!
//! Upstream attaches a `rawCache` object to `result.root` in `OnceExit` so
//! downstream stringification uses minified raws. We mirror by exposing a
//! plain struct that postcss-core's stringifier can consult.

#[derive(Debug, Clone)]
pub struct RawCache {
    pub colon: &'static str,
    pub indent: &'static str,
    pub before_decl: &'static str,
    pub before_rule: &'static str,
    pub before_open: &'static str,
    pub before_close: &'static str,
    pub before_comment: &'static str,
    pub after: &'static str,
    pub empty_body: &'static str,
    pub comment_left: &'static str,
    pub comment_right: &'static str,
}

pub fn raw_cache_plugin() -> RawCache {
    // Field-by-field match with upstream defaults (line 16-26).
    RawCache {
        colon: ":",
        indent: "",
        before_decl: "",
        before_rule: "",
        before_open: "",
        before_close: "",
        before_comment: "",
        after: "",
        empty_body: "",
        comment_left: "",
        comment_right: "",
    }
}

pub const POSTCSS_PLUGIN: &str = "cssnano-util-raw-cache";
