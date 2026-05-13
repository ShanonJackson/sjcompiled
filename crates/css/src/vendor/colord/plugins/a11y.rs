//! Port of `colord/plugins/a11y.js` — WCAG contrast ratio + luminance.

use super::super::types::RgbaColor;

fn srgb_to_linear(c: f64) -> f64 {
    let c = c / 255.0;
    if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// `luminance(rgba)` — relative luminance per WCAG 2.0.
pub fn luminance(c: RgbaColor) -> f64 {
    let r = srgb_to_linear(c.r);
    let g = srgb_to_linear(c.g);
    let b = srgb_to_linear(c.b);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// `contrast(a, b)` — `(L1 + 0.05) / (L2 + 0.05)`.
pub fn contrast(a: RgbaColor, b: RgbaColor) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    let (l1, l2) = if la > lb { (la, lb) } else { (lb, la) };
    (l1 + 0.05) / (l2 + 0.05)
}

/// `isReadable(a, b, opts)` — WCAG AA/AAA thresholds.
pub fn is_readable(a: RgbaColor, b: RgbaColor, level: WcagLevel, size: WcagSize) -> bool {
    let c = contrast(a, b);
    let threshold = match (level, size) {
        (WcagLevel::AA, WcagSize::Normal) => 4.5,
        (WcagLevel::AA, WcagSize::Large) => 3.0,
        (WcagLevel::AAA, WcagSize::Normal) => 7.0,
        (WcagLevel::AAA, WcagSize::Large) => 4.5,
    };
    c >= threshold
}

#[derive(Debug, Clone, Copy)]
pub enum WcagLevel { AA, AAA }
#[derive(Debug, Clone, Copy)]
pub enum WcagSize { Normal, Large }
