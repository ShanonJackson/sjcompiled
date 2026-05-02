//! crates/cssnano-postcss-normalize-url
//! Byte-for-byte Rust port of `postcss-normalize-url@5.1.0`.
//!
//! Folder/file mapping (1:1 with upstream + vendored deps):
//!   - `node_modules/postcss-normalize-url/src/index.js` -> `src/lib.rs`.
//!   - `node_modules/normalize-url/index.js`            -> `src/normalize_url.rs`.
//!   - subset of Node `lib/path.js` (`path.posix.normalize`) -> `src/path.rs`.
//!
//! The 5 options postcss-normalize-url@5.1.0 overrides on top of
//! normalize-url@6.1.0 defaults (see upstream `pluginCreator(opts)`):
//!
//! ```text
//! normalizeProtocol  : false   // keep `//foo` as `//foo`, not `http://foo`
//! sortQueryParameters: false   // preserve original parameter order
//! stripHash          : false   // keep `#frag`
//! stripWWW           : false   // keep `www.`
//! stripTextFragment  : false   // keep `#:~:text=...`
//! ```
//!
//! All other normalize-url defaults stand. Bug-for-bug rule applies; do
//! not "improve" any behavior beyond what upstream does.
//!
//! ## Plugin behavior (1:1 with upstream `OnceExit(css)`)
//!
//! `css.walk(node => ...)` — for every Decl, transform the value; for
//! every AtRule named `namespace` (case-insensitive), transform its params.
//!
//! ### `transformDecl(decl, opts)`
//! Walks the decl value via `postcss-value-parser`. For every `url(...)`
//! function:
//!   1. Reset `node.before = node.after = ''`.
//!   2. If empty: set inner quote to `''`, return.
//!   3. Trim+collapse `\\\r?\n` from inner value.
//!   4. If `data:`-prefixed: return.
//!   5. If `*-extension:/` (e.g. `chrome-extension://`, `moz-extension://`):
//!      skip the `convert()` step.
//!   6. Otherwise: `url.value = convert(url.value, opts)`.
//!   7. If escapeChars regex matches AND the inner is a String, try to
//!      escape; if shorter, swap to Word with escaped value. Otherwise
//!      always demote to Word.
//!
//! ### `transformNamespace(rule)`
//! Walks rule.params via `postcss-value-parser`. For every `url(...)`
//! function with children, mutates the function node IN-PLACE into a
//! string node (type='string', quote=child[0].quote or '"', value=child[0].value).
//! For every existing string node, trims its value.

mod normalize_url;
mod path;

use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::container::{walk_mut, Mutation};
use postcss_core::node::NodeKind as CssNodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, stringify as vp_stringify};

pub use normalize_url::{NormalizeOptions, QueryFilter, RemoveDirectoryIndex, RemoveQueryParameters};

#[derive(Debug, Clone)]
pub struct NormalizeUrlOpts {
    pub normalize: NormalizeOptions,
}

impl Default for NormalizeUrlOpts {
    fn default() -> Self {
        // Upstream `Object.assign({}, defaults, opts)` where defaults override
        // 5 fields off normalize-url's defaults.
        let mut opts = NormalizeOptions::default();
        opts.normalize_protocol = false;
        opts.sort_query_parameters = false;
        opts.strip_hash = false;
        opts.strip_www = false;
        opts.strip_text_fragment = false;
        Self { normalize: opts }
    }
}

// `/\\[\r\n]/`
static MULTILINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\[\r\n]").unwrap());
// `/([\s\(\)"'])/g`
static ESCAPE_CHARS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"([\s\(\)"'])"#).unwrap());
// `/^[a-zA-Z][a-zA-Z\d+\-.]*?:/`
static ABSOLUTE_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z\d+\-.]*?:").unwrap()
});
// `/^[a-zA-Z]:\\/` — Windows drive paths, NOT URLs.
static WINDOWS_PATH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z]:\\").unwrap());
// `/^data:(.*)?,/i`
static DATA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?i)data:(.*)?,").unwrap());
// `/^.+-extension:\//i`
static EXTENSION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?i).+-extension:/").unwrap());

fn is_absolute(url: &str) -> bool {
    if WINDOWS_PATH_REGEX.is_match(url) {
        return false;
    }
    ABSOLUTE_URL_REGEX.is_match(url)
}

/// Mirrors upstream `convert(url, options)`.
fn convert(url: &str, options: &NormalizeOptions) -> String {
    if is_absolute(url) || url.starts_with("//") {
        // Catch any error from normalize-url; fall back to original on Err.
        match normalize_url::normalize_url(url, options) {
            Ok(s) => s,
            Err(_) => url.to_string(),
        }
    } else {
        // Upstream: `path.normalize(url).replace(new RegExp('\\' + path.sep, 'g'), '/')`.
        // Host-OS dependent — see `path::host_normalize_to_forward_slashes`.
        path::host_normalize_to_forward_slashes(url)
    }
}

/// Mirrors upstream `transformNamespace(rule)`. Rewrites `url(...)` in
/// `@namespace` params to a string literal.
fn transform_namespace(params: &str) -> String {
    let mut parsed = vp_parse(params);
    walk_inplace(&mut parsed, &mut |node| {
        if node.kind == VKind::Function
            && node.value.to_lowercase() == "url"
            && !node.nodes.is_empty()
        {
            let first = &node.nodes[0];
            let quote = if first.kind == VKind::String {
                first.quote.unwrap_or('"')
            } else {
                '"'
            };
            let new_value = first.value.clone();
            // Mutate in place: switch type to String, copy value & quote.
            node.kind = VKind::String;
            node.value = new_value;
            node.quote = Some(quote);
            // Children, before/after of the function are now stringifier-irrelevant
            // for a String node, but upstream leaves them attached. Keep as-is
            // — stringify_node for String only reads quote+value+unclosed.
        }
        if node.kind == VKind::String {
            node.value = node.value.trim().to_string();
        }
        false
    });
    vp_stringify(&parsed)
}

/// Mirrors upstream `transformDecl(decl, opts)`. Rewrites `url(...)` calls
/// inside a decl value.
fn transform_decl(value: &str, opts: &NormalizeOptions) -> String {
    let mut parsed = vp_parse(value);
    walk_inplace(&mut parsed, &mut |node| {
        if node.kind != VKind::Function || node.value.to_lowercase() != "url" {
            return false;
        }

        node.before = String::new();
        node.after = String::new();

        if node.nodes.is_empty() {
            return false;
        }
        // Operate on the first child only — upstream `node.nodes[0]`.
        let url = &mut node.nodes[0];

        // url.value = url.value.trim().replace(/\\[\r\n]/, '');
        // Upstream uses `replace(regex, '')` — JS String.prototype.replace
        // without `g` flag = first match only. Replicate verbatim.
        let trimmed = url.value.trim().to_string();
        let cleaned = MULTILINE_RE.replace(&trimmed, "").to_string();
        url.value = cleaned;

        // Empty URL: clear quote and stop. Empty url() means current sheet.
        if url.value.is_empty() {
            url.quote = Some('\u{0}'); // sentinel; will be erased below
            // Upstream sets `url.quote = ''`. Our parser models quote as
            // `Option<char>`; emulate empty by switching kind so stringify
            // doesn't emit any quote.
            url.kind = VKind::Word;
            url.quote = None;
            return false;
        }

        if DATA_REGEX.is_match(&url.value) {
            return false;
        }

        if !EXTENSION_REGEX.is_match(&url.value) {
            url.value = convert(&url.value, opts);
        }

        // ESCAPE_CHARS test on a NEW Regex instance each time = no `lastIndex`
        // pollution. Upstream uses a SHARED instance with `g` flag, which DOES
        // mutate lastIndex across `.test()` calls — but the immediately-following
        // `.replace(escapeChars, '\\$1')` is a fresh match anyway.
        // ⚠️ The shared regex has `g` flag — `.test()` advances lastIndex on
        // match, RESETS to 0 on no-match. Replicate by checking `is_match`
        // (stateless in Rust regex) — equivalent for the upstream usage where
        // url.value is fresh each call.
        if ESCAPE_CHARS_RE.is_match(&url.value) && url.kind == VKind::String {
            let escaped = ESCAPE_CHARS_RE
                .replace_all(&url.value, "\\$1")
                .to_string();
            // Upstream: `if (escaped.length < url.value.length + 2)` — i.e.
            // shorter than `"value"` (which adds 2 quote bytes).
            if escaped.len() < url.value.len() + 2 {
                url.value = escaped;
                url.kind = VKind::Word;
            }
        } else {
            url.kind = VKind::Word;
        }
        false
    });
    vp_stringify(&parsed)
}

/// Walk that bubbles `false` from cb to skip recursion (upstream signal).
fn walk_inplace<F: FnMut(&mut VNode) -> bool>(nodes: &mut [VNode], cb: &mut F) {
    for node in nodes.iter_mut() {
        let recurse = cb(node);
        if recurse && node.kind == VKind::Function {
            walk_inplace(&mut node.nodes, cb);
        }
    }
}

/// Plugin entry — `OnceExit(css)`.
pub fn postcss_normalize_url(root: &mut Root, opts: &NormalizeUrlOpts) -> PluginResult {
    walk_mut(&mut root.root, &mut |node, _ctx| {
        match &mut node.kind {
            CssNodeKind::Declaration(d) => {
                let new_value = transform_decl(&d.value, &opts.normalize);
                if new_value != d.value {
                    d.value = new_value;
                    node.raws.value = None;
                }
            }
            CssNodeKind::AtRule(a) => {
                if a.name.to_lowercase() == "namespace" {
                    let new_params = transform_namespace(&a.params);
                    if new_params != a.params {
                        a.params = new_params;
                        node.raws.params = None;
                    }
                }
            }
            _ => {}
        }
        Mutation::Keep
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_normalize_url(&mut root, &NormalizeUrlOpts::default()).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_blank() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn no_op_no_url() {
        let css = "a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn relative_path_normalizes() {
        let out = run("a { background: url(./foo/../bar.png); }");
        assert!(out.contains("url(bar.png)"), "got: {out}");
    }

    #[test]
    fn unquoted_relative() {
        let out = run("a { background: url(foo.png); }");
        assert!(out.contains("url(foo.png)"), "got: {out}");
    }

    #[test]
    fn absolute_url_strips_default_port() {
        let out = run("a { background: url(http://example.com:80/path); }");
        assert!(!out.contains(":80"), "got: {out}");
    }

    #[test]
    fn data_url_passthrough() {
        let css = "a { background: url(data:image/svg+xml;utf8,<svg/>); }";
        let out = run(css);
        // Data URIs are not modified by convert().
        assert!(out.contains("data:image/svg+xml"), "got: {out}");
    }

    #[test]
    fn extension_url_preserved() {
        let css = "a { background: url(chrome-extension://abc/foo.png); }";
        let out = run(css);
        // Extension URLs short-circuit convert().
        assert!(out.contains("chrome-extension://"), "got: {out}");
    }

    #[test]
    fn empty_url_kept() {
        let css = "a { background: url(); }";
        let out = run(css);
        // Empty url() is preserved.
        assert!(out.contains("url("), "got: {out}");
    }

    #[test]
    fn at_namespace_strips_url_wrap() {
        let css = "@namespace svg url(\"http://www.w3.org/2000/svg\");";
        let out = run(css);
        assert!(!out.contains("url("), "got: {out}");
        assert!(out.contains("\"http://www.w3.org/2000/svg\""), "got: {out}");
    }
}
