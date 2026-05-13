//! Port of `colord/types.d.ts`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor { pub r: f64, pub g: f64, pub b: f64, pub a: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HslaColor { pub h: f64, pub s: f64, pub l: f64, pub a: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HsvaColor { pub h: f64, pub s: f64, pub v: f64, pub a: f64 }

impl Default for RgbaColor { fn default() -> Self { RgbaColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } } }
impl Default for HslaColor { fn default() -> Self { HslaColor { h: 0.0, s: 0.0, l: 0.0, a: 1.0 } } }
impl Default for HsvaColor { fn default() -> Self { HsvaColor { h: 0.0, s: 0.0, v: 0.0, a: 1.0 } } }
