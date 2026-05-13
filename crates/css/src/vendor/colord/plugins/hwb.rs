//! Port of `colord/plugins/hwb.js` — HWB color space.

use super::super::types::{HsvaColor, RgbaColor};
use super::super::helpers::{hsva_to_rgba, rgba_to_hsva};

#[derive(Debug, Clone, Copy)]
pub struct Hwba { pub h: f64, pub w: f64, pub b: f64, pub a: f64 }

pub fn hwba_to_rgba(c: Hwba) -> RgbaColor {
    let mut hsv = HsvaColor { h: c.h, s: 100.0, v: 100.0, a: c.a };
    if c.w + c.b >= 100.0 {
        let g = c.w / (c.w + c.b);
        return RgbaColor { r: g * 255.0, g: g * 255.0, b: g * 255.0, a: c.a };
    }
    hsv.s = ((1.0 - c.w / (100.0 - c.b)) * 100.0).max(0.0).min(100.0);
    hsv.v = (100.0 - c.b).max(0.0).min(100.0);
    hsva_to_rgba(hsv)
}

pub fn rgba_to_hwba(c: RgbaColor) -> Hwba {
    let hsv = rgba_to_hsva(c);
    let w = (1.0 - hsv.s / 100.0) * (hsv.v / 100.0) * 100.0;
    let b = (1.0 - hsv.v / 100.0) * 100.0;
    Hwba { h: hsv.h, w, b, a: hsv.a }
}
