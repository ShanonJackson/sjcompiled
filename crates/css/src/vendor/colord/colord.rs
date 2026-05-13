//! Port of `colord/colord.js` — the [`Colord`] class methods.

use super::helpers::{
    brightness, hex_pair_str, hsva_to_rgba, lighten, normalize_hue,
    rgba_to_hsla, rgba_to_hsva, round, round_hsla, round_rgba, saturate,
};
use super::names::{closest_name, rgba_to_name};
use super::parse::{parse, ColordInput};
use super::types::{HslaColor, HsvaColor, RgbaColor};

#[derive(Debug, Clone)]
pub struct Colord {
    pub rgba: RgbaColor,
    pub parsed: bool,
}

impl Colord {
    pub fn new(input: ColordInput) -> Self {
        match parse(&input) {
            Some((rgba, _)) => Colord { rgba, parsed: true },
            None => Colord { rgba: RgbaColor::default(), parsed: false },
        }
    }

    /// `isValid()` upstream.
    pub fn is_valid(&self) -> bool { self.parsed }

    /// `brightness()` upstream — round to 2 digits.
    pub fn brightness(&self) -> f64 { round(brightness(self.rgba), 2) }

    /// `isDark()` upstream — `< 0.5`.
    pub fn is_dark(&self) -> bool { brightness(self.rgba) < 0.5 }

    /// `isLight()` upstream — `>= 0.5`.
    pub fn is_light(&self) -> bool { brightness(self.rgba) >= 0.5 }

    /// `toHex()` upstream.
    pub fn to_hex(&self) -> String {
        let r = round_rgba(self.rgba);
        let alpha_suffix = if r.a < 1.0 {
            hex_pair_str(round(255.0 * r.a, 0) as u32)
        } else { String::new() };
        format!(
            "#{}{}{}{}",
            hex_pair_str(r.r as u32),
            hex_pair_str(r.g as u32),
            hex_pair_str(r.b as u32),
            alpha_suffix,
        )
    }

    /// `toRgb()` upstream — returns rounded RGBA.
    pub fn to_rgb(&self) -> RgbaColor { round_rgba(self.rgba) }

    /// `toRgbString()` upstream.
    pub fn to_rgb_string(&self) -> String {
        let r = round_rgba(self.rgba);
        let r_i = r.r as i64;
        let g_i = r.g as i64;
        let b_i = r.b as i64;
        if r.a < 1.0 {
            format!("rgba({}, {}, {}, {})", r_i, g_i, b_i, format_alpha(r.a))
        } else {
            format!("rgb({}, {}, {})", r_i, g_i, b_i)
        }
    }

    /// `toHsl()` upstream.
    pub fn to_hsl(&self) -> HslaColor { round_hsla(rgba_to_hsla(self.rgba)) }

    /// `toHslString()` upstream.
    pub fn to_hsl_string(&self) -> String {
        let h = round_hsla(rgba_to_hsla(self.rgba));
        let h_i = h.h as i64;
        let s_i = h.s as i64;
        let l_i = h.l as i64;
        if h.a < 1.0 {
            format!("hsla({}, {}%, {}%, {})", h_i, s_i, l_i, format_alpha(h.a))
        } else {
            format!("hsl({}, {}%, {}%)", h_i, s_i, l_i)
        }
    }

    /// `toHsv()` upstream.
    pub fn to_hsv(&self) -> HsvaColor {
        let r = rgba_to_hsva(self.rgba);
        HsvaColor {
            h: round(r.h, 0),
            s: round(r.s, 0),
            v: round(r.v, 0),
            a: round(r.a, 3),
        }
    }

    /// `invert()` upstream.
    pub fn invert(&self) -> Colord {
        let r = self.rgba;
        Colord::from_rgba(RgbaColor { r: 255.0 - r.r, g: 255.0 - r.g, b: 255.0 - r.b, a: r.a })
    }

    /// `saturate(amount=0.1)` upstream.
    pub fn saturate(&self, amount: f64) -> Colord {
        Colord::from_hsla(saturate(self.rgba, amount))
    }

    /// `desaturate(amount=0.1)` upstream.
    pub fn desaturate(&self, amount: f64) -> Colord {
        Colord::from_hsla(saturate(self.rgba, -amount))
    }

    /// `grayscale()` upstream — desaturate by 100%.
    pub fn grayscale(&self) -> Colord {
        Colord::from_hsla(saturate(self.rgba, -1.0))
    }

    /// `lighten(amount=0.1)` upstream.
    pub fn lighten(&self, amount: f64) -> Colord {
        Colord::from_hsla(lighten(self.rgba, amount))
    }

    /// `darken(amount=0.1)` upstream.
    pub fn darken(&self, amount: f64) -> Colord {
        Colord::from_hsla(lighten(self.rgba, -amount))
    }

    /// `rotate(degrees=15)` upstream.
    pub fn rotate(&self, degrees: f64) -> Colord {
        let h = self.hue() + degrees;
        let mut hsla = rgba_to_hsla(self.rgba);
        hsla.h = normalize_hue(h);
        Colord::from_hsla(hsla)
    }

    /// `alpha()` getter — round to 3 digits.
    pub fn alpha_value(&self) -> f64 { round(self.rgba.a, 3) }

    /// `alpha(value)` setter.
    pub fn with_alpha(&self, value: f64) -> Colord {
        let mut rgba = self.rgba;
        rgba.a = value;
        Colord::from_rgba(rgba)
    }

    /// `hue()` getter — round to integer.
    pub fn hue(&self) -> f64 { round(rgba_to_hsla(self.rgba).h, 0) }

    /// `hue(value)` setter.
    pub fn with_hue(&self, value: f64) -> Colord {
        let mut hsla = rgba_to_hsla(self.rgba);
        hsla.h = value;
        Colord::from_hsla(hsla)
    }

    /// `isEqual(other)` upstream — compare hex strings.
    pub fn is_equal(&self, other: &Colord) -> bool {
        self.to_hex() == other.to_hex()
    }

    /// `toName()` from the names plugin (no `closest` flag).
    pub fn to_name(&self) -> Option<&'static str> { rgba_to_name(self.rgba) }

    /// `toName({ closest: true })` from the names plugin.
    pub fn to_name_closest(&self) -> &'static str { closest_name(self.rgba) }

    // Internal helpers that are not part of the upstream surface.

    fn from_rgba(rgba: RgbaColor) -> Colord {
        Colord { rgba, parsed: true }
    }

    fn from_hsla(hsla: HslaColor) -> Colord {
        Colord { rgba: super::helpers::hsla_to_rgba(hsla), parsed: true }
    }
}

/// Format alpha for `toRgbString` / `toHslString` — upstream stringifies via
/// JS template literals so trailing-zero behaviour matches `String(0.5)`.
fn format_alpha(a: f64) -> String {
    if a == a.trunc() && a.abs() < 1e21 {
        return (a as i64).to_string();
    }
    // JS `String(0.5)` -> "0.5" (no trailing zeros). Rust `{}` prints "0.5"
    // for clean floats. For irrational results we approximate by trimming.
    let s = format!("{}", a);
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else { s }
}
