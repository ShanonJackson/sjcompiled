//! Port of `colord/helpers.js` (upstream functions `n`/`e`/`u`/`a`/`o`).
//!
//! Upstream uses single-letter aliases after minification:
//!   - `n(r, t=0, n=10**t)` -> [`round`]: `Math.round(n*r)/n + 0`. The `+0`
//!     turns `-0` into `0` (JS quirk we preserve for byte parity).
//!   - `e(r, t=0, n=1)` -> [`clamp`]
//!   - `u(r)` -> [`normalize_hue`]: `(r % 360)` with negatives wrapped, NaN/Inf -> 0.
//!   - `a(r)` -> [`clamp_rgba`]
//!   - `o(r)` -> [`round_rgba`]

use super::types::{HslaColor, HsvaColor, RgbaColor};

/// Mirrors upstream `n(r, t=0)` — `Math.round(n*r)/n + 0`.
/// The `+ 0` is the JS sign-flip from `-0` to `0`. In Rust f64, `-0.0 + 0.0` is `0.0`.
pub fn round(r: f64, digits: i32) -> f64 {
    let n = (10.0_f64).powi(digits);
    js_math_round(n * r) / n + 0.0
}

/// JS `Math.round` rounds half-toward-+∞: `Math.round(-0.5) === 0`,
/// `Math.round(0.5) === 1`. Rust `f64::round` uses banker's-style on .5 in
/// some toolchains. We implement the JS rule explicitly.
pub fn js_math_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() { return x; }
    (x + 0.5).floor()
}

/// Mirrors upstream `e(r, t=0, n=1)` — `r > n ? n : r > t ? r : t`.
/// Note: the JS chain handles NaN by returning `t` (since `NaN > x` is always false).
pub fn clamp(r: f64, t: f64, n: f64) -> f64 {
    if r > n { n } else if r > t { r } else { t }
}

/// Mirrors upstream `u(r)`. NaN/non-finite → 0; negatives wrap by adding 360.
pub fn normalize_hue(r: f64) -> f64 {
    let r = if r.is_finite() { r % 360.0 } else { 0.0 };
    if r > 0.0 { r } else { r + 360.0 }
}

/// Mirrors upstream `a(r)` — clamp r/g/b to [0,255], a to [0,1].
pub fn clamp_rgba(r: RgbaColor) -> RgbaColor {
    RgbaColor {
        r: clamp(r.r, 0.0, 255.0),
        g: clamp(r.g, 0.0, 255.0),
        b: clamp(r.b, 0.0, 255.0),
        a: clamp(r.a, 0.0, 1.0),
    }
}

/// Mirrors upstream `o(r)` — round r/g/b to int, alpha to 3 digits.
pub fn round_rgba(r: RgbaColor) -> RgbaColor {
    RgbaColor {
        r: round(r.r, 0),
        g: round(r.g, 0),
        b: round(r.b, 0),
        a: round(r.a, 3),
    }
}

/// Mirrors upstream `g(r)` — clamp HSLA.
pub fn clamp_hsla(r: HslaColor) -> HslaColor {
    HslaColor {
        h: normalize_hue(r.h),
        s: clamp(r.s, 0.0, 100.0),
        l: clamp(r.l, 0.0, 100.0),
        a: clamp(r.a, 0.0, 1.0),
    }
}

/// Mirrors upstream `d(r)` — round HSLA.
pub fn round_hsla(r: HslaColor) -> HslaColor {
    HslaColor {
        h: round(r.h, 0),
        s: round(r.s, 0),
        l: round(r.l, 0),
        a: round(r.a, 3),
    }
}

/// Mirrors upstream `clampHsva` (inline in the bundle): clamp HSVA.
pub fn clamp_hsva(r: HsvaColor) -> HsvaColor {
    HsvaColor {
        h: normalize_hue(r.h),
        s: clamp(r.s, 0.0, 100.0),
        v: clamp(r.v, 0.0, 100.0),
        a: clamp(r.a, 0.0, 1.0),
    }
}

/// Mirrors upstream `h(r)` — RGBA -> HSVA.
pub fn rgba_to_hsva(r: RgbaColor) -> HsvaColor {
    let max = r.r.max(r.g).max(r.b);
    let min = r.r.min(r.g).min(r.b);
    let o = max - min;
    let i = if o == 0.0 { 0.0 } else if max == r.r { (r.g - r.b) / o }
        else if max == r.g { 2.0 + (r.b - r.r) / o }
        else { 4.0 + (r.r - r.g) / o };
    HsvaColor {
        h: 60.0 * (if i < 0.0 { i + 6.0 } else { i }),
        s: if max == 0.0 { 0.0 } else { (o / max) * 100.0 },
        v: max / 255.0 * 100.0,
        a: r.a,
    }
}

/// Mirrors upstream `b(r)` — HSVA -> RGBA.
pub fn hsva_to_rgba(r: HsvaColor) -> RgbaColor {
    let h = r.h / 360.0 * 6.0;
    let s = r.s / 100.0;
    let v = r.v / 100.0;
    let a = h.floor();
    let o = v * (1.0 - s);
    let i = v * (1.0 - (h - a) * s);
    let s_val = v * (1.0 - (1.0 - h + a) * s);
    let h_idx = (a as i64).rem_euclid(6) as usize;
    let r_arr = [v, i, o, o, s_val, v];
    let g_arr = [s_val, v, v, i, o, o];
    let b_arr = [o, o, s_val, v, v, i];
    RgbaColor {
        r: 255.0 * r_arr[h_idx],
        g: 255.0 * g_arr[h_idx],
        b: 255.0 * b_arr[h_idx],
        a: r.a,
    }
}

/// Mirrors upstream `f(r)` — HSLA -> RGBA via HSVA intermediate.
pub fn hsla_to_rgba(t: HslaColor) -> RgbaColor {
    let l = t.l;
    let mut n = t.s;
    let scale = if l < 50.0 { l } else { 100.0 - l };
    n *= scale / 100.0;
    let s = if n > 0.0 { 2.0 * n / (l + n) * 100.0 } else { 0.0 };
    hsva_to_rgba(HsvaColor { h: t.h, s, v: l + n, a: t.a })
}

/// Mirrors upstream `c(r)` — RGBA -> HSLA via HSVA intermediate.
pub fn rgba_to_hsla(r: RgbaColor) -> HslaColor {
    let t = rgba_to_hsva(r);
    let n = t.s;
    let e = t.v;
    let u = (200.0 - n) * e / 100.0;
    let l = u / 2.0;
    let s = if u > 0.0 && u < 200.0 {
        n * e / 100.0 / (if u <= 100.0 { u } else { 200.0 - u }) * 100.0
    } else { 0.0 };
    HslaColor { h: t.h, s, l, a: t.a }
}

/// `t(r)` upstream — `isPresent`. Strings are present iff non-empty,
/// numbers are always present (NaN included — upstream uses `typeof === 'number'`).
pub fn is_present_string(s: &str) -> bool { !s.is_empty() }

/// Hex char string with two-char padding — upstream `s(r)`.
pub fn hex_pair_str(byte_value: u32) -> String {
    let s = format!("{:x}", byte_value);
    if s.len() < 2 { format!("0{s}") } else { s }
}

/// Brightness — upstream `H(r)`: `(299r + 587g + 114b) / 1000 / 255`.
pub fn brightness(r: RgbaColor) -> f64 {
    (299.0 * r.r + 587.0 * r.g + 114.0 * r.b) / 1000.0 / 255.0
}

/// Saturate helper — upstream `M(r, t)`.
pub fn saturate(r: RgbaColor, t: f64) -> HslaColor {
    let n = rgba_to_hsla(r);
    HslaColor {
        h: n.h,
        s: clamp(n.s + 100.0 * t, 0.0, 100.0),
        l: n.l,
        a: n.a,
    }
}

/// Lighten helper — upstream `$(r, t)`.
pub fn lighten(r: RgbaColor, t: f64) -> HslaColor {
    let n = rgba_to_hsla(r);
    HslaColor {
        h: n.h,
        s: n.s,
        l: clamp(n.l + 100.0 * t, 0.0, 100.0),
        a: n.a,
    }
}
