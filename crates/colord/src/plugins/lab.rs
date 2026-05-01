//! Port of `colord/plugins/lab.js`.
//!
//! CIE LAB conversion via D50-referenced XYZ — matches upstream's bundled
//! constants exactly.

use crate::types::RgbaColor;

#[derive(Debug, Clone, Copy)]
pub struct Lab { pub l: f64, pub a: f64, pub b: f64, pub alpha: f64 }

// D50 reference white.
const D50_X: f64 = 96.422;
const D50_Y: f64 = 100.0;
const D50_Z: f64 = 82.521;

const E: f64 = 216.0 / 24389.0;
const K: f64 = 24389.0 / 27.0;

fn srgb_to_linear(c: f64) -> f64 {
    let c = c / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f64) -> f64 {
    let v = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    v * 255.0
}

fn rgb_to_xyz(rgba: RgbaColor) -> (f64, f64, f64) {
    let r = srgb_to_linear(rgba.r);
    let g = srgb_to_linear(rgba.g);
    let b = srgb_to_linear(rgba.b);
    // sRGB -> XYZ D65 -> Bradford-adapted to D50 (matches upstream constants).
    let x = (r * 0.4360747 + g * 0.3850649 + b * 0.1430804) * 100.0;
    let y = (r * 0.2225045 + g * 0.7168786 + b * 0.0606169) * 100.0;
    let z = (r * 0.0139322 + g * 0.0971045 + b * 0.7141733) * 100.0;
    (x, y, z)
}

fn xyz_to_rgb(x: f64, y: f64, z: f64) -> RgbaColor {
    let x = x / 100.0;
    let y = y / 100.0;
    let z = z / 100.0;
    let r = x * 3.1338561 + y * -1.6168667 + z * -0.4906146;
    let g = x * -0.9787684 + y * 1.9161415 + z * 0.0334540;
    let b = x * 0.0719453 + y * -0.2289914 + z * 1.4052427;
    RgbaColor { r: linear_to_srgb(r), g: linear_to_srgb(g), b: linear_to_srgb(b), a: 1.0 }
}

fn pivot(t: f64) -> f64 {
    if t > E { t.cbrt() } else { (K * t + 16.0) / 116.0 }
}

fn unpivot(t: f64) -> f64 {
    let t3 = t.powi(3);
    if t3 > E { t3 } else { (116.0 * t - 16.0) / K }
}

pub fn rgba_to_lab(rgba: RgbaColor) -> Lab {
    let (x, y, z) = rgb_to_xyz(rgba);
    let fx = pivot(x / D50_X);
    let fy = pivot(y / D50_Y);
    let fz = pivot(z / D50_Z);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
        alpha: rgba.a,
    }
}

pub fn lab_to_rgba(lab: Lab) -> RgbaColor {
    let fy = (lab.l + 16.0) / 116.0;
    let fx = lab.a / 500.0 + fy;
    let fz = fy - lab.b / 200.0;
    let x = unpivot(fx) * D50_X;
    let y = unpivot(fy) * D50_Y;
    let z = unpivot(fz) * D50_Z;
    let mut out = xyz_to_rgb(x, y, z);
    out.a = lab.alpha;
    out
}
