//! Port of `colord/plugins/harmonies.js` — harmony color generation.

use super::super::types::HslaColor;
use super::super::helpers::{normalize_hue, rgba_to_hsla, hsla_to_rgba};
use super::super::types::RgbaColor;

#[derive(Debug, Clone, Copy)]
pub enum Harmony {
    Analogous,
    Complementary,
    DoubleSplitComplementary,
    Rectangle,
    SplitComplementary,
    Tetradic,
    Triadic,
}

/// Returns N variants of the input color in the given harmony.
pub fn harmonies(rgba: RgbaColor, kind: Harmony) -> Vec<RgbaColor> {
    let base = rgba_to_hsla(rgba);
    let offsets: Vec<f64> = match kind {
        Harmony::Analogous => vec![-30.0, 0.0, 30.0],
        Harmony::Complementary => vec![0.0, 180.0],
        Harmony::DoubleSplitComplementary => vec![-30.0, 0.0, 30.0, 150.0, 210.0],
        Harmony::Rectangle | Harmony::Tetradic => vec![0.0, 60.0, 180.0, 240.0],
        Harmony::SplitComplementary => vec![0.0, 150.0, 210.0],
        Harmony::Triadic => vec![0.0, 120.0, 240.0],
    };
    offsets.into_iter().map(|delta| {
        hsla_to_rgba(HslaColor {
            h: normalize_hue(base.h + delta),
            s: base.s,
            l: base.l,
            a: base.a,
        })
    }).collect()
}
