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
//! ## Drift fix landed in the foundation session
//!
//! `crates/colord/src/plugins/minify.rs` was a placeholder that bore no
//! resemblance to upstream `colord/plugins/minify.js@2.9.3`. Fixed
//! during the colormin commitment because `minifyColor.js` calls
//! straight through to it; without parity here every colormin output
//! would diverge. JS-parity vector test at
//! `crates/colord/tests/minify_parity.rs` (392 vectors) locks it in.

pub mod minify_color;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, stringify as vp_stringify};
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
pub use crate::vendor::colord::plugins::minify::MinifyOpts;
// Convenience alias matching upstream's module shape.
pub use minify_color::minify_color as minify_color_value;

// ---------------------------------------------------------------------------
// transform() — value-parser walk + minifyColor rewrites + space splice.
// ---------------------------------------------------------------------------

/// Upstream `RGB_HSL_RE = /^(rgb|hsl)a?$/i`. Matches the four legacy
/// color-function names. Modern syntax (`rgb(255 0 0)` modern w/o `a`) is
/// still spelled `rgb` — same name match.
static RGB_HSL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(rgb|hsl)a?$").expect("colormin RGB_HSL_RE compiles")
});

/// Mirrors upstream `walk(parent, callback)` from `index.js`:
///
/// ```js
/// function walk(parent, callback) {
///   parent.nodes.forEach((node, index) => {
///     const bubble = callback(node, index, parent);
///     if (node.type === 'function' && bubble !== false) {
///       walk(node, callback);
///     }
///   });
/// }
/// ```
///
/// Critical detail: `forEach` snapshots `length` at the start of iteration
/// (ECMA-262 §22.1.3.10 step 4). When the callback splices an element at
/// `index+1` (the rgb→word space splice in `transform()`), forEach still
/// iterates only `cached_len` times. Subsequent indices read shifted
/// elements, but the loop terminates at the original length. We mirror
/// the cached-length semantics with a snapshot taken before the loop.
///
/// Recursion check uses the POST-callback `node.type` so the rgb→word
/// rewrite (which mutates `kind` from Function to Word) correctly skips
/// recursion into the now-empty children — matching upstream
/// `if (node.type === 'function' && bubble !== false)`.
fn walk_with_parent<F>(parent_nodes: &mut Vec<VNode>, callback: &mut F)
where
    F: FnMut(usize, &mut Vec<VNode>) -> Option<bool>,
{
    let cached_len = parent_nodes.len();
    let mut k: usize = 0;
    while k < cached_len {
        if k >= parent_nodes.len() {
            // Defensive — a splice that REMOVED an element would shrink
            // length below cached_len. colormin only inserts, but mirror
            // upstream's `kPresent`-conditional callback skip.
            k += 1;
            continue;
        }
        let bubble = callback(k, parent_nodes);
        // Re-check kind AFTER the callback — rgb/hsl rewrites mutate kind
        // from Function to Word, in which case upstream skips recursion.
        let still_function = matches!(parent_nodes[k].kind, VKind::Function);
        if still_function && bubble != Some(false) {
            // Re-borrow children. `mem::take` swaps in an empty Vec so we
            // own the children mutably for the recursive walk; restore
            // afterwards. (Function nodes' children are independent of
            // parent_nodes structurally, so this is safe.)
            let mut children = std::mem::take(&mut parent_nodes[k].nodes);
            walk_with_parent(&mut children, callback);
            parent_nodes[k].nodes = children;
        }
        k += 1;
    }
}

/// Build a value-parser Space node — the splice insert when an rgb→word
/// rewrite is followed by another word/function and would otherwise
/// concatenate (e.g. `rgb(255,0,0)blue` → `redblue` without the splice).
///
/// Upstream JS: `{type: 'space', value: ' '}` — no other fields are
/// initialized (the `before`/`after`/`unclosed` defaults flow from
/// JS object semantics). We initialize empty/false to match.
fn make_space_node() -> VNode {
    VNode {
        kind: VKind::Space,
        value: " ".to_string(),
        before: String::new(),
        after: String::new(),
        quote: None,
        unclosed: false,
        nodes: Vec::new(),
        source_index: 0,
        source_end_index: 0,
    }
}

/// Upstream `transform(value, options)` from `src/index.js:47-83`.
///
/// 1. Parse `value` with postcss-value-parser.
/// 2. `walk(parsed, ...)`:
///    - Function with name matching `^(rgb|hsl)a?$/i`:
///       - `originalValue = node.value` (the function name).
///       - `node.value = minifyColor(valueParser.stringify(node), options)`.
///       - `node.type = 'word'`.
///       - If `node.value !== originalValue` AND next sibling exists AND
///         next sibling is word/function, splice a `Space{value: ' '}` at
///         `index + 1`.
///    - Math function (`isMathFunctionNode`): return `false` to skip
///      recursion (treat opaque).
///    - Word: `node.value = minifyColor(node.value, options)`.
/// 3. Stringify and return.
///
/// Borrow choreography: each match arm reads everything it needs out of
/// the current node before performing the parent-level splice, so the
/// `&mut Vec<VNode>` borrow doesn't conflict with re-borrows of
/// `parent[index]`.
pub fn transform(value: &str, options: &MinifyOpts) -> String {
    let mut parsed = vp_parse(value);

    walk_with_parent(&mut parsed, &mut |index, parent| {
        // Snapshot what we need from the current node up-front so we can
        // mutate it (and parent) without overlapping borrows.
        let kind = parent[index].kind.clone();
        let original_value = parent[index].value.clone();

        match kind {
            VKind::Function => {
                if RGB_HSL_RE.is_match(&original_value) {
                    // valueParser.stringify(node) — upstream stringifies the
                    // single function node. Our `vp_stringify` operates on
                    // a slice; wrap the borrow in a one-element slice via
                    // std::slice::from_ref. The borrow is released before
                    // any mutation.
                    let stringified = vp_stringify(std::slice::from_ref(&parent[index]));
                    let minified = minify_color::minify_color(&stringified, options);

                    let changed = minified != original_value;

                    // Mutate the function into a word.
                    parent[index].value = minified;
                    parent[index].kind = VKind::Word;
                    // No children for a Word — clear them so any future
                    // walk doesn't attempt to descend through stale data.
                    // (Upstream JS flips `node.type` only; the children
                    // array is left alone but never re-entered because of
                    // the post-callback `node.type === 'function'` gate.
                    // Our walk has the same gate; clearing is defensive,
                    // not load-bearing.)
                    parent[index].nodes.clear();

                    if changed {
                        let next_idx = index + 1;
                        let should_splice = parent
                            .get(next_idx)
                            .map(|n| matches!(n.kind, VKind::Word | VKind::Function))
                            .unwrap_or(false);
                        if should_splice {
                            parent.insert(next_idx, make_space_node());
                        }
                    }
                    None
                } else if is_math_function_name(&original_value) {
                    // Math functions are opaque — skip child recursion.
                    Some(false)
                } else {
                    // Other functions (var, env, url, etc.) — recurse normally.
                    None
                }
            }
            VKind::Word => {
                let new_val = minify_color::minify_color(&original_value, options);
                parent[index].value = new_val;
                None
            }
            _ => None,
        }
    });

    vp_stringify(&parsed)
}

// ---------------------------------------------------------------------------
// Plugin entry — postcss `OnceExit(css)` hook.
// ---------------------------------------------------------------------------

/// `postcss-colormin` plugin entry. Mirrors upstream
/// `pluginCreator(config = {}).prepare(result).OnceExit(css)`.
///
/// Invariants:
///   - The browserslist query and the resolved list passed in must come
///     from the SAME resolution pass (otherwise `add_plugin_defaults`'s
///     `caniuse_api::is_supported(query)` and the IE 8/9 detection on the
///     resolved list see different snapshots).
///   - Cache is an `IndexMap` (cardinal-rule check); iteration order
///     doesn't reach output bytes, but the rule applies to all output-
///     adjacent state out of paranoia.
///   - `decl.value` is set in place; `raws` is left alone so the
///     postcss-core stringifier's `raws.value.value === decl.value` raw
///     fallback fires correctly on no-op transforms (preserves trailing
///     comments). Same pattern as `cssnano-postcss-normalize-string`.
pub fn postcss_colormin_with_browsers(
    root: &mut Root,
    user_options: Option<&MinifyOpts>,
    resolved_browsers: &[String],
    browserslist_query: &str,
) -> PluginResult {
    let options = add_plugin_defaults(user_options, resolved_browsers, browserslist_query);
    let mut cache: IndexMap<String, String> = IndexMap::new();

    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        if let NodeKind::Declaration(decl) = &mut node.kind {
            // Skip-prop regex — `composes`, `font*`, bare `src`, `filter*`,
            // `-webkit-tap-highlight-color`.
            if SKIP_PROP_RE.is_match(&decl.prop) {
                return Mutation::Keep;
            }
            // Bail on empty value (upstream `if (!value) return;`).
            if decl.value.is_empty() {
                return Mutation::Keep;
            }

            // Cache key. Shape doesn't have to match upstream byte-for-byte
            // (cache is internal — collisions only matter if two distinct
            // (value, options, browsers) triples produce the same key, and
            // our shape is injective over the same axes as upstream). Use
            // a delimiter that can't appear in resolved-browser strings or
            // option booleans.
            let cache_key = build_cache_key(&decl.value, &options, resolved_browsers);

            if let Some(cached) = cache.get(&cache_key) {
                decl.value = cached.clone();
                return Mutation::Keep;
            }

            let new_value = transform(&decl.value, &options);
            decl.value = new_value.clone();
            cache.insert(cache_key, new_value);
        }
        Mutation::Keep
    });

    Ok(())
}

/// Convenience wrapper that resolves `browserslist` internally given a
/// query string. Matches the shape `cssnano-preset-default` will eventually
/// invoke (zero-config call site).
pub fn postcss_colormin_with_query(
    root: &mut Root,
    user_options: Option<&MinifyOpts>,
    browserslist_query: &str,
) -> PluginResult {
    let resolved = browserslist_shim::resolve(browserslist_query, true);
    postcss_colormin_with_browsers(root, user_options, &resolved, browserslist_query)
}

/// Default-options entry — what cssnano-preset-default invokes when it
/// instantiates the plugin via `creator()` (no user options). Resolves
/// browsers via the workspace browserslist defaults.
pub fn postcss_colormin(root: &mut Root) -> PluginResult {
    // Empty query → browserslist's default query (locked in
    // `crates/browserslist-shim` to `4.24.2` defaults). This matches
    // upstream `browserslist(null, opts)` when no `.browserslistrc` is
    // present — the default query is built into browserslist itself.
    postcss_colormin_with_query(root, None, "")
}

/// Snapshot-aware variant. When `snapshot` is `Some`, both the
/// `transparent_default` membership probe (against
/// `BROWSERS_WITH_TRANSPARENT_BUG`) and the
/// `caniuse_api::is_supported("css-rrggbbaa", ...)` decision drive
/// off the host-resolved browserslist via the snapshot — no FS / env
/// reads required. When `None`, byte-equivalent to
/// [`postcss_colormin`] (resolves via in-process shim).
///
/// Used by `cssnano-preset-default::apply_postcss_colormin` when the
/// SWC babel-plugin host has provided a snapshot via
/// [`PresetOpts::browserslist_snapshot`].
pub fn postcss_colormin_with_snapshot(
    root: &mut Root,
    user_options: Option<&MinifyOpts>,
    snapshot: Option<&::cssnano_browserslist_snapshot::PrecomputedBrowserslist>,
) -> PluginResult {
    match snapshot {
        Some(snap) => {
            // Hand the snapshot's pre-resolved list AND its joined query
            // to `_with_browsers` directly. Critical invariant (line 322
            // of this file): "The browserslist query and the resolved
            // list passed in must come from the SAME resolution pass."
            // The snapshot satisfies this by construction —
            // `joined_query == selected.join(", ")`, and the schema gate
            // (`cssnano-browserslist-snapshot::tests::joined_query_resolves_back_to_selected_via_shim`)
            // pins that `resolve(&joined_query) == selected` for the AFM
            // canonical list.
            postcss_colormin_with_browsers(
                root,
                user_options,
                snap.selected.as_slice(),
                snap.joined_query.as_str(),
            )
        }
        None => postcss_colormin_with_query(root, user_options, ""),
    }
}

/// Cache key builder. Internal-only — see `postcss_colormin_with_browsers`
/// for why the shape doesn't have to mirror JS's `JSON.stringify`.
fn build_cache_key(value: &str, opts: &MinifyOpts, browsers: &[String]) -> String {
    // U+001F is information separator one — never appears in CSS values
    // or browser names.
    let sep = '\u{1f}';
    format!(
        "{value}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}",
        opts.hex as u8,
        opts.rgb as u8,
        opts.hsl as u8,
        opts.name as u8,
        opts.transparent as u8,
        opts.alpha_hex as u8,
        browsers.join(",")
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

    fn modern_opts() -> MinifyOpts {
        // Defaults that postcss-colormin would produce on a modern target
        // (no IE 8/9, css-rrggbbaa supported).
        MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: true,
            transparent: true,
            alpha_hex: true,
        }
    }

    #[test]
    fn transform_rgb_to_name() {
        // `rgb(255,0,0)` → minify produces several candidates incl. "red".
        // input "rgb(255,0,0)" length 12; minified "red" length 3 < 12 →
        // returns "red".
        assert_eq!(transform("rgb(255,0,0)", &modern_opts()), "red");
    }

    #[test]
    fn transform_hex_collapse() {
        // `#aabbcc` (7) → `#abc` (4).
        assert_eq!(transform("#aabbcc", &modern_opts()), "#abc");
    }

    #[test]
    fn transform_word_passthrough_when_not_shorter() {
        // `red` is already shortest — input "red" length 3.
        // minified candidates: hex_short "#f00" (4), rgb (12), hsl (15),
        // name "red" (3). Min = "red". 3 < 3 false → fall back to
        // input.toLowerCase() = "red". Round-trip stable.
        assert_eq!(transform("red", &modern_opts()), "red");
    }

    #[test]
    fn transform_uppercase_word_lowercased() {
        // `RED` → minify produces "red" (3ch). 3 < 3 false → fall back to
        // input.toLowerCase() = "red".
        assert_eq!(transform("RED", &modern_opts()), "red");
    }

    #[test]
    fn transform_math_function_opaque() {
        // `calc(...)` is a math function — bail; do NOT recurse into args
        // or rewrite anything inside. Output must round-trip.
        let input = "calc(1px + 2px)";
        assert_eq!(transform(input, &modern_opts()), input);
    }

    #[test]
    fn transform_math_function_with_color_arg_unchanged() {
        // Math functions are opaque even if a color hides inside (e.g.
        // someone wrote `calc(red)` — invalid but must round-trip).
        let input = "calc(rgb(255,0,0))";
        assert_eq!(transform(input, &modern_opts()), input);
    }

    #[test]
    fn transform_splices_space_when_next_is_word() {
        // `rgb(255,0,0)blue` — rgb→word produces "red" (changed); next
        // sibling "blue" is a Word → splice " " to produce "red blue".
        let out = transform("rgb(255,0,0)blue", &modern_opts());
        assert_eq!(out, "red blue");
    }

    #[test]
    fn transform_does_not_splice_when_next_is_space() {
        // `rgb(255,0,0) blue` (with existing space) — next sibling is
        // Space (not Word/Function) → no splice.
        let out = transform("rgb(255,0,0) blue", &modern_opts());
        assert_eq!(out, "red blue");
    }

    #[test]
    fn transform_no_splice_when_no_change() {
        // If minified == originalValue (the function NAME), no splice.
        // For `rgb(255,0,0)` the function name "rgb" vs minified "red"
        // are different — splice fires. Hard to find a real case where
        // they're equal because minify always returns a non-name color
        // representation. Test the negative: a value that reaches the
        // word branch only and has no rewrite (e.g. var name).
        let out = transform("foo bar", &modern_opts());
        // "foo" is not a valid color → minify_color returns it unchanged.
        // Same for "bar". No splice anywhere; whitespace preserved.
        assert_eq!(out, "foo bar");
    }

    #[test]
    fn transform_inside_var_function_recurses() {
        // var() is NOT a math function — recurse into its args. The named
        // color `red` inside should pass through (already shortest).
        let out = transform("var(--x, red)", &modern_opts());
        assert_eq!(out, "var(--x, red)");
    }

    #[test]
    fn transform_inside_var_with_collapsible_hex() {
        // var() recurse → #aabbcc → #abc.
        let out = transform("var(--x, #aabbcc)", &modern_opts());
        assert_eq!(out, "var(--x, #abc)");
    }

    #[test]
    fn transform_skips_word_when_alpha_hex_off() {
        // `#aabbcccc` — alpha pair "cc" round-trips through 2dp (204/255*100=80).
        // With alpha_hex enabled the hex collapses to "#abcc" (5ch).
        let mut opts = modern_opts();
        opts.alpha_hex = true;
        assert_eq!(transform("#aabbcccc", &opts), "#abcc");
        // With alpha_hex OFF and alpha != 1, hex_short never produces a
        // candidate. Falls back to rgba/hsla which are far longer than
        // the input → input.toLowerCase().
        opts.alpha_hex = false;
        assert_eq!(transform("#aabbcccc", &opts), "#aabbcccc");
    }

    // --- Plugin-entry tests -------------------------------------------------

    fn run_with_query(css: &str, query: &str) -> String {
        let mut root = postcss_core::parse(css).unwrap();
        postcss_colormin_with_query(&mut root, None, query).unwrap();
        postcss_core::stringify(&root)
    }

    #[test]
    fn plugin_collapses_hex_in_decl_value() {
        let out = run_with_query("a { color: #ff0000; }", "chrome 100");
        assert!(out.contains("color: red"), "got: {out:?}");
    }

    #[test]
    fn plugin_skips_composes_prop() {
        // composes property is in the skip list — value left unchanged
        // (even though `#ff0000` would otherwise minify to `red`).
        let out = run_with_query("a { composes: #ff0000; }", "chrome 100");
        assert!(out.contains("composes: #ff0000"), "got: {out:?}");
    }

    #[test]
    fn plugin_skips_font_prop() {
        // font shorthand also skipped — colormin doesn't tangle with it.
        let out = run_with_query("a { font: 12px/1 #ff0000; }", "chrome 100");
        assert!(out.contains("font: 12px/1 #ff0000"), "got: {out:?}");
    }

    #[test]
    fn plugin_skips_filter_prop() {
        let out = run_with_query("a { filter: drop-shadow(0 0 1px #ff0000); }", "chrome 100");
        assert!(out.contains("#ff0000"), "got: {out:?}");
    }

    #[test]
    fn plugin_handles_empty_value() {
        // Empty decl value — `if (!value) return;`. No-op.
        let out = run_with_query("a { color: ; }", "chrome 100");
        // Round-trip stable. (postcss may normalize differently on parse;
        // just assert it doesn't crash and `color:` is still present.)
        assert!(out.contains("color"), "got: {out:?}");
    }

    #[test]
    fn plugin_caches_repeated_value() {
        // Two decls with the same color value — cache hit on the second.
        // Output should be identical and stable.
        let out = run_with_query(
            "a { color: #ff0000; } b { color: #ff0000; }",
            "chrome 100",
        );
        let count = out.matches("red").count();
        assert_eq!(count, 2, "got: {out:?}");
    }

    #[test]
    fn plugin_modern_target_keeps_transparent() {
        // Modern target — `transparent` shortcut fires for rgba(0,0,0,0).
        // Modern target also has alpha_hex enabled → "#0000" (5ch) beats
        // "transparent" (11ch) on length. So output is "#0000".
        let out = run_with_query("a { color: rgba(0,0,0,0); }", "chrome 100");
        assert!(out.contains("#0000"), "got: {out:?}");
    }

    // -------------------------------------------------------------------
    // Phase B / E5 — snapshot-aware entry-point parity tests.
    // -------------------------------------------------------------------

    use ::cssnano_browserslist_snapshot::{
        PrecomputedBrowserslist, PRECOMPUTED_FORMAT_VERSION,
    };

    fn snap(selected: &[&str]) -> PrecomputedBrowserslist {
        let owned: Vec<String> = selected.iter().map(|s| (*s).to_string()).collect();
        let joined = owned.join(", ");
        PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: owned,
            joined_query: joined,
        }
    }

    fn run_with_snap(
        css: &str,
        snapshot: Option<&PrecomputedBrowserslist>,
    ) -> String {
        let mut root = postcss_core::parse(css).unwrap();
        postcss_colormin_with_snapshot(&mut root, None, snapshot).unwrap();
        postcss_core::stringify(&root)
    }

    /// E5.a — `None` snapshot is byte-equivalent to `postcss_colormin`.
    #[test]
    fn snapshot_none_byte_equivalent_to_default_entry() {
        let cases = [
            "a { color: #ff0000; }",
            "a { color: rgba(255,255,255,1); }",
            "a { color: rgba(0,0,0,0); }",
            "a { background: #aabbcc; }",
            "a { color: hsl(0, 100%, 50%); }",
        ];
        for src in cases {
            let mut r1 = postcss_core::parse(src).unwrap();
            postcss_colormin(&mut r1).unwrap();
            let from_default = postcss_core::stringify(&r1);
            let from_snap_none = run_with_snap(src, None);
            assert_eq!(
                from_default, from_snap_none,
                "snapshot=None drifted from default entry on input {src:?}",
            );
        }
    }

    /// E5.b — modern snapshot enables `transparent` rewrite (no IE 8/9
    /// in selected list → `transparent_default = true`).
    #[test]
    fn snapshot_modern_enables_transparent_default() {
        let modern = snap(&["chrome 144", "firefox 147"]);
        // rgba(0,0,0,0) on modern — alpha-hex enabled, "#0000" beats
        // "transparent" on length, same as `plugin_modern_target_keeps_transparent`.
        let out = run_with_snap("a { color: rgba(0,0,0,0); }", Some(&modern));
        assert!(out.contains("#0000"), "got: {out:?}");
    }

    /// E5.c — legacy snapshot containing `ie 8` disables
    /// `transparent_default` (mirrors `transparent_disabled_for_ie89`).
    #[test]
    fn snapshot_legacy_ie8_disables_transparent_default() {
        let legacy = snap(&["ie 8", "chrome 100"]);
        let opts = add_plugin_defaults(None, &legacy.selected, &legacy.joined_query);
        assert!(!opts.transparent);
    }
}
