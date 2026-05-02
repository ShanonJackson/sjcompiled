//! Byte-for-byte Rust port of `postcss-colormin@5.3.1`.
//!
//! Phase 6g — **highest-risk cssnano plugin** per
//! `crates/EXECUTION_PLAN.md`. Color downgrade decisions hinge on
//! caniuse + colord rounding + original-vs-minified byte-length
//! comparison.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-colormin/src/`):
//!   - `index.js`        -> `src/lib.rs`   (this file)
//!   - `minifyColor.js`  -> `src/minify_color.rs`
//!
//! ## Port status
//!
//! - **Done:** `minifyColor.rs` (29 LOC helper, byte-tested via
//!   `crates/colord`'s minify parity vectors), helper constants
//!   (`BROWSERS_WITH_TRANSPARENT_BUG`, `MATH_FUNCTIONS`, `SKIP_PROP_RE`),
//!   helper fns (`is_math_function_node`, `add_plugin_defaults`).
//! - **TODO (next session):** `transform()` body (postcss-value-parser
//!   walk + minifyColor calls + space-splice on rgb/hsl→word rewrite),
//!   `postcss_colormin()` plugin entry (cache + walkDecls + prop-name
//!   skip regex + browserslist resolution), parity-runner stage,
//!   corpus, gate.
//!
//! ## Drift fix landed in this session
//!
//! `crates/colord/src/plugins/minify.rs` was a placeholder that bore no
//! resemblance to upstream `colord/plugins/minify.js@2.9.3`. Fixed
//! during the colormin commitment because `minifyColor.js` calls
//! straight through to it; without parity here every colormin output
//! would diverge. New JS-parity vector test at
//! `crates/colord/tests/minify_parity.rs` locks the fix in.

pub mod minify_color;

use once_cell::sync::Lazy;
use postcss_core::{PluginResult, Root};
use regex::Regex;
use std::collections::HashSet;

/// Upstream: `const browsersWithTransparentBug = new Set(['ie 8', 'ie 9'])`.
/// Used by `add_plugin_defaults` to disable the `transparent` shortcut when
/// any IE 8/9 target is present (those browsers mishandle clicks on
/// `transparent`-background elements).
pub static BROWSERS_WITH_TRANSPARENT_BUG: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("ie 8");
    s.insert("ie 9");
    s
});

/// Upstream: `const mathFunctions = new Set(['calc', 'min', 'max', 'clamp'])`.
/// Walked-into nodes whose `value.toLowerCase()` matches this set are
/// treated as opaque — children are NOT recursed (the `walk` callback
/// returns `false` from `is_math_function_node`).
pub static MATH_FUNCTIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("calc");
    s.insert("min");
    s.insert("max");
    s.insert("clamp");
    s
});

/// Upstream: `/^(composes|font|src$|filter|-webkit-tap-highlight-color)/i`.
/// `walkDecls` skips entirely when this regex matches `decl.prop`. The
/// `src$` anchor is intentional in upstream (matches the literal "src"
/// at end of a prop name only — `font-src` doesn't qualify; bare `src`
/// does). Compiling case-insensitive.
pub static SKIP_PROP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(composes|font|src$|filter|-webkit-tap-highlight-color)")
        .expect("colormin SKIP_PROP_RE compiles")
});

/// Upstream `isMathFunctionNode(node)`: `node.type === 'function' &&
/// mathFunctions.has(node.value.toLowerCase())`.
///
/// Wired to `postcss-value-parser`'s `Node` once `transform()` lands; the
/// signature uses the value-parser node shape there. Kept here as a
/// scaffold — caller passes the function name (already classified by
/// the parent walk).
pub fn is_math_function_name(value: &str) -> bool {
    MATH_FUNCTIONS.contains(value.to_lowercase().as_str())
}

/// Upstream `addPluginDefaults(options, browsers)`:
///
/// ```js
/// const defaults = {
///   transparent: browsers.some(b => browsersWithTransparentBug.has(b)) === false,
///   alphaHex:    isSupported('css-rrggbbaa', browsers),
///   name:        true,
/// };
/// return { ...defaults, ...options };
/// ```
///
/// The `caniuse_api::is_supported` call drives `alphaHex` — when no
/// IE 8/9 in target, `transparent` defaults true. `name` is always true.
/// User options override the defaults.
///
/// Returns the `MinifyOpts` shape that `minify_color::minify_color`
/// passes through to `colord::plugins::minify::minify`.
pub fn add_plugin_defaults(
    user_options: Option<&MinifyOpts>,
    resolved_browsers: &[String],
    browserslist_query: &str,
) -> MinifyOpts {
    let transparent_default = !resolved_browsers
        .iter()
        .any(|b| BROWSERS_WITH_TRANSPARENT_BUG.contains(b.as_str()));
    // Upstream `isSupported('css-rrggbbaa', browsers)` accepts a resolved
    // list directly; our `caniuse_api::is_supported` takes a query string
    // and resolves it internally. Caller passes the same query that
    // produced `resolved_browsers` so both paths see the same browser set.
    let alpha_hex_default = caniuse_api::is_supported("css-rrggbbaa", browserslist_query);
    // hex/rgb/hsl default true; `name`/`transparent`/`alphaHex` get computed
    // defaults that the user can override.
    let mut o = MinifyOpts {
        hex: true,
        rgb: true,
        hsl: true,
        name: true,
        transparent: transparent_default,
        alpha_hex: alpha_hex_default,
    };
    if let Some(u) = user_options {
        // Mirror `Object.assign({...defaults}, user)` — user keys override
        // defaults whenever explicitly set. We expose every key on
        // MinifyOpts so the override is unconditional. Future work: add
        // an Option<bool> wrapper if upstream's "key omitted from user
        // opts means keep default" semantics need preserving across more
        // call sites. For now `MinifyOpts` carries plain bools because
        // postcss-colormin always provides every key in its merge.
        o.hex = u.hex;
        o.rgb = u.rgb;
        o.hsl = u.hsl;
        o.name = u.name;
        o.transparent = u.transparent;
        o.alpha_hex = u.alpha_hex;
    }
    o
}

// Re-export the colord MinifyOpts shape so callers don't have to reach
// into the colord crate for it.
pub use ::colord::plugins::minify::MinifyOpts;
// Convenience alias matching upstream's module shape.
pub use minify_color::minify_color as minify_color_value;

/// Plugin entry — `pluginCreator(config = {}).prepare(result).OnceExit`.
///
/// **Not yet implemented.** Next session ports:
/// 1. `transform(value, options)` — postcss-value-parser walk that
///    rewrites every `rgb()/rgba()/hsl()/hsla()` function and every
///    bare-word color via `minify_color_value`. On any rewrite where
///    the next sibling is a word/function, splice a `' '` space token
///    so `rgb(...)blue` doesn't collapse to `redblue`.
/// 2. `walk` helper — `walk(parent, |node, idx, parent| -> bubble)`
///    descending into function children iff the callback didn't
///    return false.
/// 3. `OnceExit` body — `walkDecls(decl)`: skip via `SKIP_PROP_RE`,
///    JSON.stringify cache key (value + options + browsers), call
///    `transform`, store back to `decl.value`. Cache must be an
///    `IndexMap<String, String>` (insertion order doesn't reach output,
///    but using HashMap is banned per cardinal rules anyway).
/// 4. `browserslist::resolve(query, opts)` — passing through the
///    `result.opts.stats`/`env` if present (for parity-runner the
///    defaults suffice).
pub fn postcss_colormin(_root: &mut Root) -> PluginResult {
    unimplemented!(
        "Phase 6g — `postcss-colormin@5.3.1` plugin body. \
         Helper constants and `minifyColor` are ported; the `transform()` \
         walk, `OnceExit` cache+walkDecls, and browserslist resolution \
         remain. See module docs for the next-session checklist."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_prop_re_matches_anchored_set() {
        // composes — anchored at start.
        assert!(SKIP_PROP_RE.is_match("composes"));
        assert!(SKIP_PROP_RE.is_match("composes-from"));
        // font — prefix match (font, font-size, font-family all skipped).
        assert!(SKIP_PROP_RE.is_match("font"));
        assert!(SKIP_PROP_RE.is_match("font-size"));
        assert!(SKIP_PROP_RE.is_match("FONT-FAMILY")); // case-insensitive
        // src — only when bare `src`. `src$` end-anchor in upstream.
        assert!(SKIP_PROP_RE.is_match("src"));
        // `font-src` matches because of the `font` arm — upstream too.
        assert!(SKIP_PROP_RE.is_match("font-src"));
        // `mask-src` should NOT match — `font` doesn't prefix it and `src`
        // doesn't end-anchor at start of string.
        assert!(!SKIP_PROP_RE.is_match("mask-src"));
        // filter — prefix match.
        assert!(SKIP_PROP_RE.is_match("filter"));
        assert!(SKIP_PROP_RE.is_match("filter-mode"));
        // -webkit-tap-highlight-color — exact prefix.
        assert!(SKIP_PROP_RE.is_match("-webkit-tap-highlight-color"));
        // color — should NOT match (none of the alternatives prefix it).
        assert!(!SKIP_PROP_RE.is_match("color"));
        // background-color — should NOT match.
        assert!(!SKIP_PROP_RE.is_match("background-color"));
    }

    #[test]
    fn math_functions_set_membership() {
        assert!(is_math_function_name("calc"));
        assert!(is_math_function_name("min"));
        assert!(is_math_function_name("max"));
        assert!(is_math_function_name("clamp"));
        // case-insensitive (upstream: `node.value.toLowerCase()`).
        assert!(is_math_function_name("CALC"));
        assert!(is_math_function_name("Calc"));
        // Not a math function.
        assert!(!is_math_function_name("rgb"));
        assert!(!is_math_function_name("var"));
    }

    #[test]
    fn transparent_disabled_for_ie89() {
        let browsers = vec!["ie 8".to_string(), "chrome 100".to_string()];
        let opts = add_plugin_defaults(None, &browsers, "ie 8, chrome 100");
        assert!(!opts.transparent);
    }

    #[test]
    fn transparent_enabled_for_modern() {
        let browsers = vec!["chrome 100".to_string(), "firefox 100".to_string()];
        let opts = add_plugin_defaults(None, &browsers, "chrome 100, firefox 100");
        assert!(opts.transparent);
    }

    #[test]
    fn name_default_true() {
        let browsers = vec!["chrome 100".to_string()];
        let opts = add_plugin_defaults(None, &browsers, "chrome 100");
        assert!(opts.name);
    }

    #[test]
    fn user_options_override_defaults() {
        let browsers = vec!["chrome 100".to_string()];
        let user = MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: false,
            transparent: false,
            alpha_hex: false,
        };
        let opts = add_plugin_defaults(Some(&user), &browsers, "chrome 100");
        assert!(!opts.name);
        assert!(!opts.transparent);
        assert!(!opts.alpha_hex);
    }
}
