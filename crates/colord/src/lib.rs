//! crates/colord
//! Byte-for-byte Rust port of `colord@2.9.1`.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/colord/`):
//!   - `index.mjs`           -> `src/lib.rs` (re-exports)
//!   - `colord.js`           -> `src/colord.rs`
//!   - `helpers.js`          -> `src/helpers.rs`
//!   - `parse.js`            -> `src/parse.rs`
//!   - `random.js`           -> `src/random.rs`
//!   - `constants.js`        -> `src/constants.rs`
//!   - `types.d.ts`          -> `src/types.rs`
//!   - `plugins/names.js`    -> `src/names.rs`
//!   - `plugins/{a11y,harmonies,hwb,lab,minify,mix}.js` -> `src/plugins/`
//!
//! All bugs of the upstream version are intentionally preserved.

pub mod constants;
pub mod types;
pub mod helpers;
pub mod parse;
pub mod random;
pub mod colord;
pub mod names;
pub mod plugins;

pub use crate::colord::Colord;
pub use crate::types::{HslaColor, HsvaColor, RgbaColor};

/// Top-level `colord(input)` factory.
pub fn colord<I: Into<parse::ColordInput>>(input: I) -> Colord {
    Colord::new(input.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_short_uppercases() {
        let c = colord("#FFF");
        assert!(c.is_valid());
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn parse_hex_long() {
        let c = colord("#ff0000");
        assert_eq!(c.to_hex(), "#ff0000");
        assert_eq!(c.to_rgb_string(), "rgb(255, 0, 0)");
    }

    #[test]
    fn parse_hex_alpha() {
        let c = colord("#ff000080");
        assert!(c.alpha_value() > 0.0 && c.alpha_value() < 1.0);
    }

    #[test]
    fn parse_named_color() {
        let c = colord("red");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn parse_transparent() {
        let c = colord("transparent");
        assert_eq!(c.alpha_value(), 0.0);
    }

    #[test]
    fn parse_rgb_legacy() {
        let c = colord("rgb(255, 0, 0)");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn parse_rgb_modern() {
        let c = colord("rgb(255 0 0)");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn parse_rgba_alpha() {
        let c = colord("rgba(255, 0, 0, 0.5)");
        assert_eq!(c.to_hex(), "#ff000080");
    }

    #[test]
    fn parse_hsl() {
        let c = colord("hsl(0, 100%, 50%)");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn parse_hsl_modern_slash_alpha() {
        let c = colord("hsl(0deg 100% 50% / 0.5)");
        assert!(c.alpha_value() > 0.4 && c.alpha_value() < 0.6);
    }

    #[test]
    fn parse_hsl_with_grad() {
        let c = colord("hsl(100grad, 100%, 50%)");
        assert!(c.is_valid());
    }

    #[test]
    fn to_name_known() {
        let c = colord("#ff0000");
        assert_eq!(c.to_name(), Some("red"));
    }

    #[test]
    fn to_name_unknown_returns_none() {
        let c = colord("#123456");
        assert_eq!(c.to_name(), None);
    }

    #[test]
    fn closest_name_picks_red() {
        let c = colord("#fe0001");
        assert_eq!(c.to_name_closest(), "red");
    }

    #[test]
    fn invert() {
        let c = colord("#ff0000").invert();
        assert_eq!(c.to_hex(), "#00ffff");
    }

    #[test]
    fn lighten_darken() {
        let c = colord("#808080");
        assert_ne!(c.lighten(0.5).to_hex(), "#808080");
        assert_ne!(c.darken(0.5).to_hex(), "#808080");
    }

    #[test]
    fn brightness_red() {
        // Per WCAG-ish formula: (299*255 + 587*0 + 114*0) / 1000 / 255 = 0.299.
        let b = colord("#ff0000").brightness();
        assert!((b - 0.30).abs() < 0.005);
    }

    #[test]
    fn is_dark_light() {
        assert!(colord("#000000").is_dark());
        assert!(colord("#ffffff").is_light());
    }

    #[test]
    fn rotate_180_red_to_cyan() {
        let c = colord("#ff0000").rotate(180.0);
        assert_eq!(c.to_hex(), "#00ffff");
    }

    #[test]
    fn alpha_setter() {
        let c = colord("#ff0000").with_alpha(0.5);
        assert!(c.alpha_value() > 0.4 && c.alpha_value() < 0.6);
    }

    #[test]
    fn is_equal() {
        assert!(colord("#ff0000").is_equal(&colord("rgb(255, 0, 0)")));
        assert!(colord("#ff0000").is_equal(&colord("hsl(0, 100%, 50%)")));
    }

    #[test]
    fn invalid_input_isnt_valid() {
        assert!(!colord("not-a-color").is_valid());
    }

    #[test]
    fn rgb_percent_form() {
        let c = colord("rgb(100%, 0%, 0%)");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn rgb_percent_must_be_uniform() {
        // Mixing pct and non-pct is invalid in upstream.
        assert!(!colord("rgb(100%, 0, 0)").is_valid());
    }

    #[test]
    fn hex_alpha_rounding() {
        // #ff000080 -> alpha 0.5 (0x80 / 255).
        let c = colord("#ff000080");
        assert_eq!(c.to_hex(), "#ff000080");
    }

    #[test]
    fn saturate_full() {
        let c = colord("#808080").saturate(1.0);
        // Saturating gray gets a hue with full saturation.
        assert_ne!(c.to_hex(), "#808080");
    }

    #[test]
    fn grayscale() {
        let c = colord("#ff0000").grayscale();
        // Gray has equal r/g/b.
        let rgb = c.to_rgb();
        assert!((rgb.r - rgb.g).abs() < 1.5 && (rgb.g - rgb.b).abs() < 1.5);
    }

    #[test]
    fn names_table_size() {
        // 148 named colors total per upstream.
        assert_eq!(crate::names::NAME_TO_HEX.len(), 148);
    }

    #[test]
    fn hsl_to_hsl_round_trip() {
        let c = colord("hsl(120, 50%, 50%)");
        let s = c.to_hsl_string();
        let c2 = colord(s.as_str());
        // Round-trip via HSL is not strictly byte-exact (rounding), but the
        // hex form must agree.
        assert_eq!(c.to_hex(), c2.to_hex());
    }

    #[test]
    fn a11y_contrast_extremes() {
        use plugins::a11y::*;
        let black = colord("#000000").to_rgb();
        let white = colord("#ffffff").to_rgb();
        let c = contrast(black, white);
        // Black on white = 21.
        assert!((c - 21.0).abs() < 0.01);
    }

    #[test]
    fn harmony_complementary() {
        use plugins::harmonies::{harmonies, Harmony};
        let red = colord("#ff0000").to_rgb();
        let pair = harmonies(red, Harmony::Complementary);
        assert_eq!(pair.len(), 2);
    }

    #[test]
    fn minify_picks_shortest() {
        use plugins::minify::{minify, MinifyOpts};
        // `red` (3) beats `#ff0000` (7).
        let c = colord("#ff0000");
        let m = minify(&c, &MinifyOpts::all());
        assert_eq!(m, "red");
    }

    #[test]
    fn hsl_zero_saturation_is_gray() {
        let c = colord("hsl(0, 0%, 50%)");
        let rgb = c.to_rgb();
        assert!((rgb.r - rgb.g).abs() < 1.5 && (rgb.g - rgb.b).abs() < 1.5);
    }

    #[test]
    fn hsl_with_alpha_legacy_pct() {
        let c = colord("hsla(0, 100%, 50%, 50%)");
        assert!((c.alpha_value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn rgb_legacy_with_alpha() {
        let c = colord("rgba(255, 0, 0, 0.25)");
        assert!((c.alpha_value() - 0.25).abs() < 0.01);
    }

    #[test]
    fn hex_3_digit_with_alpha() {
        let c = colord("#f008");
        assert!(c.is_valid());
    }

    #[test]
    fn hex_short_components() {
        // `#abc` -> `#aabbcc`.
        let c = colord("#abc");
        assert_eq!(c.to_hex(), "#aabbcc");
    }

    #[test]
    fn case_insensitive_named() {
        let c = colord("RED");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn whitespace_trimmed() {
        let c = colord("   red   ");
        assert!(c.is_valid());
    }
}
