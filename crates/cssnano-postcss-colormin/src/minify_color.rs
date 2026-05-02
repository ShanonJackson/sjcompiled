//! Port of `postcss-colormin@5.3.1/src/minifyColor.js`.
//!
//! Wraps `colord(input).minify(options)` with a length-fallback: when the
//! minified form is **not strictly shorter** than the input, the upstream
//! returns `input.toLowerCase()` (note: `<`, not `<=` — equal-length
//! candidates fall back to lowercased input). Invalid inputs pass through
//! unchanged.
//!
//! Upstream calls `extend([namesPlugin, minifierPlugin])` once at module
//! load. In our port the colord crate always exposes both — see
//! `crates/colord/src/plugins/mod.rs`.

use ::colord as colord_crate;
use colord_crate::colord;
use colord_crate::plugins::minify::{minify, MinifyOpts};

pub fn minify_color(input: &str, options: &MinifyOpts) -> String {
    let instance = colord(input);
    if instance.is_valid() {
        let minified = minify(&instance, options);
        if minified.len() < input.len() {
            minified
        } else {
            // `input.toLowerCase()` upstream. CSS color values are ASCII, so
            // lowercased bytes are identical to JS UTF-16 lowercasing for
            // every input the upstream parser accepts.
            input.to_lowercase()
        }
    } else {
        // Invalid input — pass through unchanged (matches upstream `else`).
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> MinifyOpts {
        MinifyOpts::default()
    }

    #[test]
    fn shortens_collapsible_hex() {
        // `#aabbcc` (7 chars) -> `#abc` (4 chars) -> shorter, pick minified.
        assert_eq!(minify_color("#aabbcc", &opts()), "#abc");
    }

    #[test]
    fn equal_length_falls_back_to_lowercase() {
        // `red` is 3 chars; minify produces `#f00` (4) or `red` if name opt.
        // With default opts (name=false), candidates are hex `#f00`(4),
        // rgb (12), hsl (15). Min = #f00 (4). Input "red" is 3 chars, so
        // 4 < 3 is false -> fall back to "red".toLowerCase() = "red".
        assert_eq!(minify_color("red", &opts()), "red");
    }

    #[test]
    fn lowercases_when_no_strict_shortening() {
        // `RED` -> shortest minified is `#f00` (4ch) — input "RED" is 3ch,
        // 4 < 3 false -> lowercase("RED") = "red".
        assert_eq!(minify_color("RED", &opts()), "red");
    }

    #[test]
    fn invalid_passthrough() {
        // Invalid input — return unchanged (case preserved).
        assert_eq!(minify_color("not-a-color", &opts()), "not-a-color");
    }

    #[test]
    fn name_option_picks_red_over_hex() {
        let mut o = opts();
        o.name = true;
        // `#ff0000` (7ch) -> minify produces `#f00`(4), `red`(3) [via name].
        // `red` < 7 chars -> pick "red".
        assert_eq!(minify_color("#ff0000", &o), "red");
    }
}
