//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/cross-fade.js`.
//!
//! ```js
//! let list = require('postcss').list
//! let Value = require('../value')
//!
//! class CrossFade extends Value {
//!   replace(string, prefix) {
//!     return list
//!       .space(string)
//!       .map(value => {
//!         if (value.slice(0, +this.name.length + 1) !== this.name + '(') {
//!           return value
//!         }
//!         let close = value.lastIndexOf(')')
//!         let after = value.slice(close + 1)
//!         let args = value.slice(this.name.length + 1, close)
//!         if (prefix === '-webkit-') {
//!           let match = args.match(/\d*.?\d+%?/)
//!           if (match) {
//!             args = args.slice(match[0].length).trim()
//!             args += `, ${match[0]}`
//!           } else {
//!             args += ', 0.5'
//!           }
//!         }
//!         return prefix + this.name + '(' + args + ')' + after
//!       })
//!       .join(' ')
//!   }
//! }
//!
//! CrossFade.names = ['cross-fade']
//! ```
//!
//! Subclass of `Value`. Only `replace` is overridden — `add`, `check`,
//! `value`, `regexp`, and `old` come from `ValueBase` unchanged.

use crate::value::ValueBase;
use postcss_core::list;

#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
pub struct CrossFade {
    pub base: ValueBase,
}

impl CrossFade {
    pub const NAMES: &'static [&'static str] = &["cross-fade"];
    pub const CLASS_NAME: &'static str = "CrossFade";

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            base: ValueBase::new(name, prefixes, all_id),
        }
    }

    /// JS `replace(string, prefix)`. Walks each space-separated token; if a
    /// token starts with `<name>(`, peels off the matching `)` and rewrites
    /// the head as `<prefix><name>(...)`. The `-webkit-` form additionally
    /// rotates the leading percentage to the trailing position because the
    /// upstream `cross-fade()` syntax differs between specs (final spec puts
    /// the percent first, the WebKit prefixed form puts it last).
    pub fn replace(&self, string: &str, prefix: &str) -> String {
        let name = &self.base.prefixer.name;
        let head = format!("{name}(");
        let head_len = head.len();

        let parts: Vec<String> = list::space(string)
            .into_iter()
            .map(|value| {
                if value.len() < head_len || &value[..head_len] != head.as_str() {
                    return value;
                }
                // Mirror JS `value.lastIndexOf(')')` — operates on UTF-16
                // code units, but the only chars in a CSS value path that
                // matter here are ASCII; byte index equals char index.
                let close = match value.rfind(')') {
                    Some(i) => i,
                    None => return value,
                };
                let after = value[close + 1..].to_string();
                let mut args = value[head_len..close].to_string();

                if prefix == "-webkit-" {
                    if let Some(m) = PERCENT_RE.find(&args) {
                        let matched = m.as_str().to_string();
                        let matched_len = matched.len();
                        // JS: `args.slice(match[0].length)` — drops the
                        // matched leading number/percent off the front,
                        // regardless of whether it was actually positioned
                        // at the start. JS `String.prototype.slice(n)`
                        // takes from index `n`, so this DOES start from
                        // the matched length, not from the match index.
                        // Replicate verbatim.
                        let tail = if matched_len <= args.len() {
                            args[matched_len..].trim_start().trim_end().to_string()
                        } else {
                            String::new()
                        };
                        args = format!("{tail}, {matched}");
                    } else {
                        args.push_str(", 0.5");
                    }
                }
                format!("{prefix}{name}({args}){after}")
            })
            .collect();

        parts.join(" ")
    }
}

// JS regex literal `/\d*.?\d+%?/` — note the unescaped `.` matches any
// char (not just literal dot). Translated literally so byte-equal inputs
// produce byte-equal results.
static PERCENT_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"\d*.?\d+%?").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    fn cf() -> CrossFade {
        CrossFade::new("cross-fade".into(), vec!["-webkit-".into()], 0)
    }

    #[test]
    fn replace_passes_through_non_crossfade_tokens() {
        let h = cf();
        // Token doesn't start with `cross-fade(` → untouched.
        assert_eq!(h.replace("url(a.png)", "-webkit-"), "url(a.png)");
        assert_eq!(h.replace("none", "-webkit-"), "none");
    }

    #[test]
    fn replace_webkit_rotates_percent_to_end() {
        let h = cf();
        let input = "cross-fade(50% url(a.png), url(b.png))";
        let out = h.replace(input, "-webkit-");
        // The leading `50%` (matched by /\d*.?\d+%?/) is dropped from the
        // front via `.slice(match[0].length)` and re-emitted at the end.
        assert!(out.starts_with("-webkit-cross-fade("));
        assert!(out.ends_with(", 50%)"));
    }

    #[test]
    fn replace_webkit_inserts_default_when_no_percent() {
        let h = cf();
        let input = "cross-fade(url(a.png), url(b.png))";
        let out = h.replace(input, "-webkit-");
        assert!(out.contains("-webkit-cross-fade("));
        assert!(out.ends_with(", 0.5)"));
    }

    #[test]
    fn replace_non_webkit_preserves_args_unchanged() {
        let h = cf();
        let input = "cross-fade(50% url(a.png), url(b.png))";
        // Any non-webkit prefix (real autoprefixer never asks for one
        // here, but the code path exists) skips the rotation.
        let out = h.replace(input, "-moz-");
        assert_eq!(out, "-moz-cross-fade(50% url(a.png), url(b.png))");
    }

    #[test]
    fn replace_preserves_after_close_paren() {
        let h = cf();
        // Trailing characters after `)` (e.g. ` no-repeat`) must survive.
        let input = "cross-fade(url(a.png), url(b.png))/4 4";
        let out = h.replace(input, "-webkit-");
        assert!(out.contains(")/4"));
    }
}
