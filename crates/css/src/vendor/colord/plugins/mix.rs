//! Port of `colord/plugins/mix.js`.
//!
//! Upstream uses LAB-space mixing: convert both colors to LAB, lerp by
//! `ratio`, convert back. We re-use the LAB conversion in `plugins/lab.rs`.

use super::super::types::RgbaColor;

/// `colord(c).mix(other, ratio=0.5)`.
pub fn mix(a: RgbaColor, b: RgbaColor, ratio: f64) -> RgbaColor {
    let r = ratio.clamp(0.0, 1.0);
    // Upstream linearly interpolates in LAB. We'll use LAB via plugins/lab.
    let la = super::lab::rgba_to_lab(a);
    let lb = super::lab::rgba_to_lab(b);
    let lab = super::lab::Lab {
        l: la.l + (lb.l - la.l) * r,
        a: la.a + (lb.a - la.a) * r,
        b: la.b + (lb.b - la.b) * r,
        alpha: la.alpha + (lb.alpha - la.alpha) * r,
    };
    super::lab::lab_to_rgba(lab)
}
