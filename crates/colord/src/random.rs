//! Port of `colord/random.js`.
//!
//! Upstream `E = function(){ return new j({ r: 255*Math.random(), g: 255*Math.random(), b: 255*Math.random() }); }`.
//!
//! Random colors are not on our hashing path. We expose the same shape but
//! mark it `#[cfg(feature = "random")]` so the parity build can't reach it
//! by accident — non-deterministic output would break hash parity.

#[cfg(feature = "random")]
pub fn random() -> crate::Colord {
    use crate::types::RgbaColor;
    use crate::parse::ColordInput;
    let r = (rand::random::<f64>() * 255.0).into();
    let g = (rand::random::<f64>() * 255.0).into();
    let b = (rand::random::<f64>() * 255.0).into();
    crate::Colord::new(ColordInput::Rgba(RgbaColor { r, g, b, a: 1.0 }))
}
