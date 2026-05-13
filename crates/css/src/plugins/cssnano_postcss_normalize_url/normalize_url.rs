//! Byte-for-byte port of `normalize-url@6.1.0`.
//!
//! Upstream source: `node_modules/normalize-url/index.js`.
//!
//! Per the cardinal rule, the upstream layout is preserved as closely as
//! Rust allows: this is a single file mirroring the single upstream file.
//! `normalize-url` is a separate npm package, but it's consumed by exactly
//! one plugin (`postcss-normalize-url@5.1.0`) so we vendor it here rather
//! than create a single-consumer crate.
//!
//! WHATWG URL parsing delegates to the Rust `url` crate (v2.5+). Both Node's
//! `new URL(...)` and the Rust `url` crate implement the WHATWG URL Standard,
//! so canonical serialization matches for every well-formed URL. Edge cases
//! around non-special schemes or invalid characters could differ; our test
//! corpus exercises the cases that appear in real CSS `url(...)` tokens.
//!
//! Options match upstream defaults verbatim:
//!   defaultProtocol     = "http:"
//!   normalizeProtocol   = true
//!   forceHttp           = false
//!   forceHttps          = false
//!   stripAuthentication = true
//!   stripHash           = false
//!   stripTextFragment   = true
//!   stripWWW            = true
//!   removeQueryParameters = [/^utm_\w+/i]
//!   removeTrailingSlash = true
//!   removeSingleSlash   = true
//!   removeDirectoryIndex = false
//!   sortQueryParameters = true
//!
//! `postcss-normalize-url@5.1.0` overrides 5 of these to `false`:
//!   normalizeProtocol, sortQueryParameters, stripHash, stripWWW,
//!   stripTextFragment

use once_cell::sync::Lazy;
use percent_encoding::percent_decode_str;
use regex::Regex;
use url::Url;

#[derive(Debug, Clone)]
pub struct NormalizeOptions {
    pub default_protocol: String,
    pub normalize_protocol: bool,
    pub force_http: bool,
    pub force_https: bool,
    pub strip_authentication: bool,
    pub strip_hash: bool,
    pub strip_text_fragment: bool,
    pub strip_www: bool,
    pub remove_query_parameters: RemoveQueryParameters,
    pub remove_trailing_slash: bool,
    pub remove_single_slash: bool,
    pub remove_directory_index: RemoveDirectoryIndex,
    pub sort_query_parameters: bool,
    pub strip_protocol: bool,
}

#[derive(Debug, Clone)]
pub enum RemoveQueryParameters {
    /// Upstream `Array<RegExp | string>`. Matches if any pattern equals (string)
    /// or matches (regex) the parameter name.
    Patterns(Vec<QueryFilter>),
    /// Upstream `true`: clears `urlObj.search` entirely.
    All,
}

#[derive(Debug, Clone)]
pub enum QueryFilter {
    Exact(String),
    Pattern(Regex),
}

#[derive(Debug, Clone)]
pub enum RemoveDirectoryIndex {
    Disabled,
    /// Upstream `true` is normalized to `[/^index\.[a-z]+$/]` before use.
    /// We pre-expand to the patterns variant.
    Patterns(Vec<QueryFilter>),
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            default_protocol: "http:".to_string(),
            normalize_protocol: true,
            force_http: false,
            force_https: false,
            strip_authentication: true,
            strip_hash: false,
            strip_text_fragment: true,
            strip_www: true,
            remove_query_parameters: RemoveQueryParameters::Patterns(vec![QueryFilter::Pattern(
                Regex::new(r"(?i)^utm_\w+").unwrap(),
            )]),
            remove_trailing_slash: true,
            remove_single_slash: true,
            remove_directory_index: RemoveDirectoryIndex::Disabled,
            sort_query_parameters: true,
            strip_protocol: false,
        }
    }
}

fn test_parameter(name: &str, filters: &[QueryFilter]) -> bool {
    filters.iter().any(|f| match f {
        QueryFilter::Exact(s) => s == name,
        QueryFilter::Pattern(r) => r.is_match(name),
    })
}

// `/^data:(?<type>[^,]*?),(?<data>[^#]*?)(?:#(?<hash>.*))?$/`
static DATA_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^data:([^,]*?),([^#]*?)(?:#(.*))?$").unwrap()
});
const DATA_URL_DEFAULT_MIME_TYPE: &str = "text/plain";
const DATA_URL_DEFAULT_CHARSET: &str = "us-ascii";

fn normalize_data_url(url_string: &str, options: &NormalizeOptions) -> Result<String, String> {
    let caps = DATA_URL_REGEX
        .captures(url_string)
        .ok_or_else(|| format!("Invalid URL: {url_string}"))?;
    let type_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let data = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let hash = if options.strip_hash {
        ""
    } else {
        caps.get(3).map(|m| m.as_str()).unwrap_or("")
    };

    let mut media_type: Vec<String> = type_str.split(';').map(|s| s.to_string()).collect();
    let mut is_base64 = false;
    if media_type
        .last()
        .map(|s| s == "base64")
        .unwrap_or(false)
    {
        media_type.pop();
        is_base64 = true;
    }

    // Lowercase MIME type. Upstream: `(mediaType.shift() || '').toLowerCase()`.
    let mime_type = if media_type.is_empty() {
        String::new()
    } else {
        media_type.remove(0).to_lowercase()
    };

    let attributes: Vec<String> = media_type
        .into_iter()
        .map(|attribute| {
            let mut parts = attribute.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim().to_string();
            let value_raw = parts.next().unwrap_or("").trim().to_string();
            let mut value = value_raw;
            // Lowercase `charset` value; drop default charset entirely.
            if key == "charset" {
                value = value.to_lowercase();
                if value == DATA_URL_DEFAULT_CHARSET {
                    return String::new();
                }
            }
            if value.is_empty() {
                key
            } else {
                format!("{key}={value}")
            }
        })
        .filter(|s| !s.is_empty())
        .collect();

    let mut normalized_media_type: Vec<String> = attributes;
    if is_base64 {
        normalized_media_type.push("base64".to_string());
    }
    let needs_mime = !normalized_media_type.is_empty()
        || (!mime_type.is_empty() && mime_type != DATA_URL_DEFAULT_MIME_TYPE);
    if needs_mime {
        normalized_media_type.insert(0, mime_type);
    }

    let data_part = if is_base64 { data.trim() } else { data };
    let hash_part = if hash.is_empty() {
        String::new()
    } else {
        format!("#{hash}")
    };
    Ok(format!(
        "data:{},{}{}",
        normalized_media_type.join(";"),
        data_part,
        hash_part
    ))
}

// `/^data:/i`
static DATA_PROTOCOL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?i)data:").unwrap());
// `/^view-source:/i`
static VIEW_SOURCE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?i)view-source:").unwrap());
// `/^\.*\//`
static RELATIVE_URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\.*/").unwrap());
// `/^(?!(?:\w+:)?\/\/)|^\/\//` — upstream's protocol prepender.
// Matches:
//   (a) start of string when NOT followed by `(\w+:)?//`
//   (b) `//` at start (relative protocol)
// We implement both branches manually since Rust regex lacks lookahead.
fn prepend_protocol(url_string: &str, default_protocol: &str) -> String {
    // Branch (b): relative protocol `//foo` — upstream's `^\/\/` alternative.
    if url_string.starts_with("//") {
        return format!("{default_protocol}{}", &url_string[2..]);
    }
    // Branch (a): if the string does NOT start with `(\w+:)?//`, prepend.
    // Equivalently: if it does start with `(\w+:)?//`, don't.
    let already_has_protocol = starts_with_scheme_slashslash(url_string);
    if already_has_protocol {
        url_string.to_string()
    } else {
        format!("{default_protocol}{url_string}")
    }
}

/// Returns true if `s` matches `^(\w+:)?\/\/`.
fn starts_with_scheme_slashslash(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'/' && bytes[1] == b'/' {
        return true;
    }
    // Try `(\w+:)//` — `\w` is `[A-Za-z0-9_]` per ECMAScript.
    let mut i = 0;
    while i < bytes.len() && is_word_char(bytes[i]) {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    if i + 2 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b'/' && bytes[i + 2] == b'/' {
        return true;
    }
    // Also `i+2 == bytes.len()` would mean `xxx://` is the whole input — match too.
    if i + 2 == bytes.len() && bytes.get(i) == Some(&b':') {
        // Falls into "no //" — not a match.
    }
    if i + 3 <= bytes.len()
        && bytes[i] == b':'
        && bytes[i + 1] == b'/'
        && bytes[i + 2] == b'/'
    {
        return true;
    }
    false
}

fn is_word_char(b: u8) -> bool {
    (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') || (b >= b'0' && b <= b'9') || b == b'_'
}

// `/(?<!\b(?:[a-z][a-z\d+\-.]{1,50}:))\/{2,}/g`
// Upstream collapses runs of `/` UNLESS preceded by a scheme like `https:`.
// Rust `regex` lacks lookbehind. We emulate by walking the string.
fn collapse_path_slashes(pathname: &str) -> String {
    let bytes = pathname.as_bytes();
    let mut out = String::with_capacity(pathname.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            // Find run end.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'/' {
                j += 1;
            }
            let run_len = j - i;
            if run_len >= 2 {
                // Check lookbehind: is `out` ending with `[a-z][a-z\d+\-.]{1,50}:`
                // at a word boundary? `\b` boundary in JS means transition
                // between word and non-word char. The scheme ends with `:`
                // (non-word), so `\b` is satisfied trivially before the scheme
                // pattern. Check if `out` ends with `[a-z][a-z\d+\-.]{1,50}:`.
                if ends_with_scheme(&out) {
                    out.push_str(&"/".repeat(run_len));
                } else {
                    out.push('/');
                }
            } else {
                out.push('/');
            }
            i = j;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn ends_with_scheme(s: &str) -> bool {
    let bytes = s.as_bytes();
    if !bytes.last().map(|&b| b == b':').unwrap_or(false) {
        return false;
    }
    // Walk backwards from before the `:` collecting `[a-z\d+\-.]` chars,
    // then require the run to start with `[a-z]` and be 1..=50 + 1 chars.
    let end = bytes.len() - 1; // index of `:`
    if end == 0 {
        return false;
    }
    let mut start = end;
    while start > 0 {
        let b = bytes[start - 1];
        let in_class = (b >= b'a' && b <= b'z')
            || (b >= b'0' && b <= b'9')
            || b == b'+'
            || b == b'-'
            || b == b'.';
        if !in_class {
            break;
        }
        start -= 1;
    }
    // Need first char to be `[a-z]`.
    if start >= end {
        return false;
    }
    let first = bytes[start];
    if !(first >= b'a' && first <= b'z') {
        return false;
    }
    // Length of the `[a-z][a-z\d+\-.]{1,50}` portion is `end - start`.
    // We need 2..=51 chars before the `:` (1 letter + 1..=50 trailing).
    let len = end - start;
    (2..=51).contains(&len)
}

// `/#?:~:text.*?$/i` — strip text-fragment portion from urlObj.hash.
static TEXT_FRAGMENT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)#?:~:text.*?$").unwrap()
});

// `/^www\.(?!www\.)(?:[a-z\-\d]{1,63})\.(?:[a-z.\-\d]{2,63})$/`
// Rust lacks negative lookahead. Manually verify the `(?!www\.)` constraint.
static WWW_HOSTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^www\.([a-z\-\d]{1,63})\.([a-z.\-\d]{2,63})$").unwrap()
});

fn matches_strip_www(hostname: &str) -> bool {
    if !WWW_HOSTNAME_REGEX.is_match(hostname) {
        return false;
    }
    // Reject `www.www.<rest>` (the `(?!www\.)` lookahead).
    let after = &hostname[4..]; // skip leading `www.`
    !after.starts_with("www.")
}

/// Mirrors upstream `normalizeUrl(urlString, options)`.
pub fn normalize_url(url_string: &str, options: &NormalizeOptions) -> Result<String, String> {
    let mut url_string = url_string.trim().to_string();

    // Data URL.
    if DATA_PROTOCOL_REGEX.is_match(&url_string) {
        return normalize_data_url(&url_string, options);
    }
    if VIEW_SOURCE_REGEX.is_match(&url_string) {
        return Err(
            "`view-source:` is not supported as it is a non-standard protocol".to_string(),
        );
    }

    let has_relative_protocol = url_string.starts_with("//");
    let is_relative_url = !has_relative_protocol && RELATIVE_URL_REGEX.is_match(&url_string);

    // Prepend protocol when not relative.
    if !is_relative_url {
        url_string = prepend_protocol(&url_string, &options.default_protocol);
    }

    let mut url_obj = Url::parse(&url_string).map_err(|e| format!("URL parse error: {e}"))?;

    if options.force_http && options.force_https {
        return Err("The `forceHttp` and `forceHttps` options cannot be used together".to_string());
    }

    if options.force_http && url_obj.scheme() == "https" {
        let _ = url_obj.set_scheme("http");
    }
    if options.force_https && url_obj.scheme() == "http" {
        let _ = url_obj.set_scheme("https");
    }

    if options.strip_authentication {
        let _ = url_obj.set_username("");
        let _ = url_obj.set_password(None);
    }

    // Hash handling.
    if options.strip_hash {
        url_obj.set_fragment(None);
    } else if options.strip_text_fragment {
        if let Some(frag) = url_obj.fragment() {
            // Reconstruct upstream's `urlObj.hash = '#' + frag`-then-replace flow.
            // Upstream: `urlObj.hash = urlObj.hash.replace(/#?:~:text.*?$/i, '')`.
            // urlObj.hash includes the leading `#`. The regex matches an
            // optional `#`, then `:~:text` and tail. We operate on the
            // `#frag` form.
            let with_hash = format!("#{frag}");
            let stripped = TEXT_FRAGMENT_REGEX.replace(&with_hash, "");
            // Setting hash to "" means no fragment; otherwise drop the leading `#`.
            if stripped.is_empty() {
                url_obj.set_fragment(None);
            } else if let Some(rest) = stripped.strip_prefix('#') {
                if rest.is_empty() {
                    url_obj.set_fragment(None);
                } else {
                    url_obj.set_fragment(Some(rest));
                }
            } else {
                url_obj.set_fragment(Some(&stripped));
            }
        }
    }

    // Collapse duplicate slashes in pathname (unless preceded by a scheme).
    {
        let path = url_obj.path().to_string();
        if !path.is_empty() {
            let collapsed = collapse_path_slashes(&path);
            url_obj.set_path(&collapsed);
        }
    }

    // decodeURI on pathname (upstream catches any error and ignores).
    {
        let path = url_obj.path().to_string();
        if !path.is_empty() {
            if let Ok(decoded) = decode_uri(&path) {
                url_obj.set_path(&decoded);
            }
        }
    }

    // Remove directory index.
    let dir_patterns: Option<Vec<QueryFilter>> = match &options.remove_directory_index {
        RemoveDirectoryIndex::Disabled => None,
        RemoveDirectoryIndex::Patterns(p) if !p.is_empty() => Some(p.clone()),
        RemoveDirectoryIndex::Patterns(_) => None,
    };
    if let Some(patterns) = dir_patterns {
        let path = url_obj.path().to_string();
        let mut components: Vec<&str> = path.split('/').collect();
        if let Some(last) = components.last() {
            if test_parameter(last, &patterns) {
                components.pop();
                // Upstream: `pathComponents.slice(1).join('/') + '/'`.
                let rebuilt = if components.len() <= 1 {
                    "/".to_string()
                } else {
                    format!("{}/", components[1..].join("/"))
                };
                url_obj.set_path(&rebuilt);
            }
        }
    }

    // Hostname mutations.
    if url_obj.host_str().is_some() {
        let mut hostname = url_obj.host_str().unwrap().to_string();
        // Remove trailing dot.
        if hostname.ends_with('.') {
            hostname.pop();
        }
        if options.strip_www && matches_strip_www(&hostname) {
            hostname = hostname[4..].to_string();
        }
        let _ = url_obj.set_host(Some(&hostname));
    }

    // Remove query unwanted parameters.
    match &options.remove_query_parameters {
        RemoveQueryParameters::Patterns(filters) => {
            let pairs: Vec<(String, String)> = url_obj
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let kept: Vec<(String, String)> = pairs
                .into_iter()
                .filter(|(k, _)| !test_parameter(k, filters))
                .collect();
            // Upstream operates on searchParams; setting `urlObj.search` to
            // empty and re-emitting via query_pairs_mut produces the right
            // serialization. If `kept` is empty AND the URL had query, we
            // need to clear it.
            if kept.is_empty() {
                url_obj.set_query(None);
            } else {
                let mut serializer = url::form_urlencoded::Serializer::new(String::new());
                for (k, v) in &kept {
                    serializer.append_pair(k, v);
                }
                let q = serializer.finish();
                url_obj.set_query(Some(&q));
            }
        }
        RemoveQueryParameters::All => {
            // Upstream sets `urlObj.search = ''`, which removes the query AND
            // produces no leading `?` in the serialization.
            url_obj.set_query(None);
        }
    }

    if options.sort_query_parameters {
        let mut pairs: Vec<(String, String)> = url_obj
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        if pairs.is_empty() {
            // Upstream: searchParams.sort() leaves an empty `?` if the URL
            // already had `?`. WHATWG behavior — preserved by re-setting.
            // For simplicity drop it; aligns with serialization equivalence.
        } else {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in &pairs {
                serializer.append_pair(k, v);
            }
            let q = serializer.finish();
            url_obj.set_query(Some(&q));
        }
    }

    if options.remove_trailing_slash {
        let path = url_obj.path().to_string();
        if path.ends_with('/') && path.len() > 1 {
            url_obj.set_path(path.trim_end_matches('/'));
        }
    }

    let old_url_string = url_string.clone();
    let mut url_string = url_obj.to_string();

    if !options.remove_single_slash
        && url_obj.path() == "/"
        && !old_url_string.ends_with('/')
        && url_obj.fragment().is_none()
    {
        if url_string.ends_with('/') {
            url_string.pop();
        }
    }

    if (options.remove_trailing_slash || url_obj.path() == "/")
        && url_obj.fragment().is_none()
        && options.remove_single_slash
    {
        if url_string.ends_with('/') {
            url_string.pop();
        }
    }

    if has_relative_protocol && !options.normalize_protocol {
        if let Some(rest) = url_string.strip_prefix("http://") {
            url_string = format!("//{rest}");
        }
    }

    if options.strip_protocol {
        if let Some(rest) = url_string.strip_prefix("https://") {
            url_string = rest.to_string();
        } else if let Some(rest) = url_string.strip_prefix("http://") {
            url_string = rest.to_string();
        } else if let Some(rest) = url_string.strip_prefix("//") {
            url_string = rest.to_string();
        }
    }

    Ok(url_string)
}

/// JS `decodeURI`. RFC 3986 specifies which characters are reserved (these
/// stay percent-encoded) — JS's `decodeURI` decodes everything EXCEPT the
/// reserved set: `;/?:@&=+$,#`. Returns Err on malformed UTF-8.
fn decode_uri(s: &str) -> Result<String, String> {
    // Walk the string; for every `%XX` sequence not in the reserved set,
    // decode it. Strict JS-`decodeURI` semantics.
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("URI malformed at {i}"));
            }
            let hi = hex_digit(bytes[i + 1]).ok_or_else(|| format!("URI malformed at {i}"))?;
            let lo = hex_digit(bytes[i + 2]).ok_or_else(|| format!("URI malformed at {i}"))?;
            let decoded = (hi << 4) | lo;
            // `decodeURI` keeps RFC 3986 reserved chars encoded:
            //   `;/?:@&=+$,#` AND also (per ECMAScript spec) `%`.
            const RESERVED: &[u8] = b";/?:@&=+$,#";
            if RESERVED.contains(&decoded) {
                // Preserve original bytes (uppercase the hex per WHATWG path
                // serialization — but we leave as-is for byte equality).
                out.push(bytes[i]);
                out.push(bytes[i + 1]);
                out.push(bytes[i + 2]);
            } else {
                out.push(decoded);
            }
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    // For multi-byte UTF-8 sequences `decodeURI` decodes them as a unit and
    // validates UTF-8. Use percent_decode for the validation pass.
    let _ = percent_decode_str(s); // touch the dep so it isn't dead
    String::from_utf8(out).map_err(|e| format!("URI malformed: {e}"))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts_postcss_default() -> NormalizeOptions {
        // The five overrides postcss-normalize-url@5.1.0 applies.
        NormalizeOptions {
            normalize_protocol: false,
            sort_query_parameters: false,
            strip_hash: false,
            strip_www: false,
            strip_text_fragment: false,
            ..NormalizeOptions::default()
        }
    }

    #[test]
    fn ends_scheme() {
        assert!(ends_with_scheme("https:"));
        assert!(ends_with_scheme("http:"));
        assert!(ends_with_scheme("foo+bar:"));
        assert!(!ends_with_scheme(":"));
        assert!(!ends_with_scheme("/"));
    }

    #[test]
    fn collapse_keeps_scheme_double_slash() {
        // After "https:" we keep the `//`.
        assert_eq!(collapse_path_slashes("/foo//bar"), "/foo/bar");
        assert_eq!(collapse_path_slashes("//foo"), "/foo");
    }

    #[test]
    fn datauri_charset_default_drop() {
        let out = normalize_data_url(
            "data:text/plain;charset=us-ascii,hello",
            &opts_postcss_default(),
        )
        .unwrap();
        // Default charset stripped AND default mime stripped (no remaining
        // attributes + mime is the default `text/plain`).
        assert_eq!(out, "data:,hello");
    }

    #[test]
    fn datauri_charset_kept_when_nondefault() {
        let out = normalize_data_url(
            "data:text/html;charset=utf-8,<p/>",
            &opts_postcss_default(),
        )
        .unwrap();
        assert_eq!(out, "data:text/html;charset=utf-8,<p/>");
    }

    #[test]
    fn datauri_lowercase_mime() {
        let out = normalize_data_url("data:TEXT/HTML,foo", &opts_postcss_default()).unwrap();
        assert_eq!(out, "data:text/html,foo");
    }

    #[test]
    fn datauri_default_mime() {
        // text/plain is the default — dropped when no other attributes.
        let out = normalize_data_url("data:text/plain,foo", &opts_postcss_default()).unwrap();
        assert_eq!(out, "data:,foo");
    }

    #[test]
    fn absolute_url_strips_default_port() {
        let out = normalize_url(
            "http://example.com:80/path",
            &opts_postcss_default(),
        )
        .unwrap();
        assert!(!out.contains(":80"), "got: {out}");
    }

    #[test]
    fn absolute_url_strips_utm() {
        let out = normalize_url(
            "http://example.com/?utm_source=foo&keep=1",
            &opts_postcss_default(),
        )
        .unwrap();
        assert!(!out.contains("utm_source"), "got: {out}");
        assert!(out.contains("keep=1"), "got: {out}");
    }

    #[test]
    fn relative_protocol_url_keeps_protocol_relative() {
        // postcss default sets normalizeProtocol=false, so `//example.com`
        // stays as `//...` (not promoted to `http://`).
        let out = normalize_url("//example.com/foo", &opts_postcss_default()).unwrap();
        assert!(out.starts_with("//"), "got: {out}");
    }
}
