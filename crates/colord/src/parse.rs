//! Port of `colord/parse.js` — input parsing.
//!
//! Upstream parsers map (`y`):
//!   - `string`: hex / rgb(a) / hsl(a)  (legacy and modern syntax both)
//!   - `object`: rgb / hsl / hsv (and per-plugin: hwb, lab, lch, xyz, cmyk)
//!
//! Plugin parsers register themselves through [`Parsers::register_*`]. The
//! base parsers below match upstream byte-for-byte.

use crate::constants::angle_factor;
use crate::helpers::{clamp_rgba, clamp_hsla, hsla_to_rgba, round};
use crate::types::{HslaColor, HsvaColor, RgbaColor};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub enum ColordInput {
    Str(String),
    Rgba(RgbaColor),
    Hsla(HslaColor),
    Hsva(HsvaColor),
    /// Object form mapped from a `serde_json::Value`-like shape. The key set
    /// is matched against r/g/b/h/s/l/v/a so plugins can inject parsing.
    Object(ObjectInput),
}

#[derive(Debug, Clone, Default)]
pub struct ObjectInput {
    pub r: Option<f64>,
    pub g: Option<f64>,
    pub b: Option<f64>,
    pub h: Option<f64>,
    pub s: Option<f64>,
    pub l: Option<f64>,
    pub v: Option<f64>,
    pub a: Option<f64>,
}

impl From<&str> for ColordInput { fn from(s: &str) -> Self { ColordInput::Str(s.to_string()) } }
impl From<String> for ColordInput { fn from(s: String) -> Self { ColordInput::Str(s) } }
impl From<RgbaColor> for ColordInput { fn from(c: RgbaColor) -> Self { ColordInput::Rgba(c) } }
impl From<HslaColor> for ColordInput { fn from(c: HslaColor) -> Self { ColordInput::Hsla(c) } }
impl From<HsvaColor> for ColordInput { fn from(c: HsvaColor) -> Self { ColordInput::Hsva(c) } }

// Upstream regex literals (matching `y` parsers).
//   `i = /^#([0-9a-f]{3,8})$/i`
static HEX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^#([0-9a-f]{3,8})$").unwrap());
//   `l = /^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s*,\s*([+-]?\d*\.?\d+)%\s*,\s*([+-]?\d*\.?\d+)%\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i`
static HSL_LEGACY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?i)^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s*,\s*([+-]?\d*\.?\d+)%\s*,\s*([+-]?\d*\.?\d+)%\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$"
).unwrap());
//   `p = /^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s+([+-]?\d*\.?\d+)%\s+([+-]?\d*\.?\d+)%\s*(?:\/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i`
static HSL_MODERN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?i)^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s+([+-]?\d*\.?\d+)%\s+([+-]?\d*\.?\d+)%\s*(?:/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$"
).unwrap());
//   `v = /^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i`
static RGB_LEGACY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?i)^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$"
).unwrap());
//   `m = /^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s*(?:\/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i`
static RGB_MODERN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?i)^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s*(?:/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$"
).unwrap());

/// Public parse entry — mirrors upstream `x(r)`. Returns `(rgba, format_name)`.
pub fn parse(input: &ColordInput) -> Option<(RgbaColor, &'static str)> {
    match input {
        ColordInput::Rgba(c) => Some((*c, "rgb")),
        ColordInput::Hsla(c) => Some((hsla_to_rgba(clamp_hsla(*c)), "hsl")),
        ColordInput::Hsva(c) => Some((crate::helpers::hsva_to_rgba(*c), "hsv")),
        ColordInput::Str(s) => parse_string(s.trim()),
        ColordInput::Object(obj) => parse_object(obj),
    }
}

fn parse_string(s: &str) -> Option<(RgbaColor, &'static str)> {
    if let Some(c) = parse_hex(s) { return Some((c, "hex")); }
    if let Some(c) = parse_rgb(s) { return Some((c, "rgb")); }
    if let Some(c) = parse_hsl(s) { return Some((c, "hsl")); }
    if let Some(c) = crate::names::lookup_name(s) { return Some((c, "name")); }
    None
}

fn parse_hex(s: &str) -> Option<RgbaColor> {
    let caps = HEX_RE.captures(s)?;
    let body = caps.get(1).unwrap().as_str();
    let chars: Vec<char> = body.chars().collect();
    let (r, g, b, a) = match chars.len() {
        3 | 4 => {
            let r = u8::from_str_radix(&format!("{}{}", chars[0], chars[0]), 16).ok()?;
            let g = u8::from_str_radix(&format!("{}{}", chars[1], chars[1]), 16).ok()?;
            let b = u8::from_str_radix(&format!("{}{}", chars[2], chars[2]), 16).ok()?;
            let a = if chars.len() == 4 {
                let av = u8::from_str_radix(&format!("{}{}", chars[3], chars[3]), 16).ok()?;
                round(av as f64 / 255.0, 2)
            } else { 1.0 };
            (r, g, b, a)
        }
        6 | 8 => {
            let r = u8::from_str_radix(&body[0..2], 16).ok()?;
            let g = u8::from_str_radix(&body[2..4], 16).ok()?;
            let b = u8::from_str_radix(&body[4..6], 16).ok()?;
            let a = if chars.len() == 8 {
                let av = u8::from_str_radix(&body[6..8], 16).ok()?;
                round(av as f64 / 255.0, 2)
            } else { 1.0 };
            (r, g, b, a)
        }
        _ => return None,
    };
    Some(RgbaColor { r: r as f64, g: g as f64, b: b as f64, a })
}

fn parse_rgb(s: &str) -> Option<RgbaColor> {
    let caps = RGB_LEGACY_RE.captures(s).or_else(|| RGB_MODERN_RE.captures(s))?;
    // All three RGB components must agree on `%` flag — upstream `t[2]!==t[4]||t[4]!==t[6]?null`.
    let p2 = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let p4 = caps.get(4).map(|m| m.as_str()).unwrap_or("");
    let p6 = caps.get(6).map(|m| m.as_str()).unwrap_or("");
    if p2 != p4 || p4 != p6 { return None; }
    let n1: f64 = caps.get(1)?.as_str().parse().ok()?;
    let n3: f64 = caps.get(3)?.as_str().parse().ok()?;
    let n5: f64 = caps.get(5)?.as_str().parse().ok()?;
    let pct_factor = if !p2.is_empty() { 100.0 / 255.0 } else { 1.0 };
    let a: f64 = match caps.get(7) {
        None => 1.0,
        Some(m) => {
            let v: f64 = m.as_str().parse().ok()?;
            let pct8 = caps.get(8).is_some();
            v / (if pct8 { 100.0 } else { 1.0 })
        }
    };
    Some(clamp_rgba(RgbaColor {
        r: n1 / pct_factor,
        g: n3 / pct_factor,
        b: n5 / pct_factor,
        a,
    }))
}

fn parse_hsl(s: &str) -> Option<RgbaColor> {
    let caps = HSL_LEGACY_RE.captures(s).or_else(|| HSL_MODERN_RE.captures(s))?;
    let n1: f64 = caps.get(1)?.as_str().parse().ok()?;
    let unit = caps.get(2).map(|m| m.as_str()).unwrap_or("deg");
    let factor = angle_factor(unit).unwrap_or(1.0);
    let s_v: f64 = caps.get(3)?.as_str().parse().ok()?;
    let l_v: f64 = caps.get(4)?.as_str().parse().ok()?;
    let a: f64 = match caps.get(5) {
        None => 1.0,
        Some(m) => {
            let v: f64 = m.as_str().parse().ok()?;
            let pct = caps.get(6).is_some();
            v / (if pct { 100.0 } else { 1.0 })
        }
    };
    let h = n1 * factor;
    Some(hsla_to_rgba(clamp_hsla(HslaColor { h, s: s_v, l: l_v, a })))
}

fn parse_object(o: &ObjectInput) -> Option<(RgbaColor, &'static str)> {
    if o.r.is_some() && o.g.is_some() && o.b.is_some() {
        let a = o.a.unwrap_or(1.0);
        return Some((clamp_rgba(RgbaColor {
            r: o.r.unwrap(), g: o.g.unwrap(), b: o.b.unwrap(), a,
        }), "rgb"));
    }
    if o.h.is_some() && o.s.is_some() && o.l.is_some() {
        let a = o.a.unwrap_or(1.0);
        let hsl = clamp_hsla(HslaColor { h: o.h.unwrap(), s: o.s.unwrap(), l: o.l.unwrap(), a });
        return Some((hsla_to_rgba(hsl), "hsl"));
    }
    if o.h.is_some() && o.s.is_some() && o.v.is_some() {
        let a = o.a.unwrap_or(1.0);
        let hsva = crate::helpers::clamp_hsva(HsvaColor { h: o.h.unwrap(), s: o.s.unwrap(), v: o.v.unwrap(), a });
        return Some((crate::helpers::hsva_to_rgba(hsva), "hsv"));
    }
    None
}

/// `getFormat(input)` upstream — returns the format name without the rgba.
pub fn get_format(input: &ColordInput) -> Option<&'static str> {
    parse(input).map(|(_, fmt)| fmt)
}
