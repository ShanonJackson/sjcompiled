//! Port of `colord/constants.js` — angle unit conversion factors.
//!
//! Upstream `r = { grad: 0.9, turn: 360, rad: 360 / (2 * Math.PI) }`.

pub fn angle_factor(unit: &str) -> Option<f64> {
    match unit {
        "grad" => Some(0.9),
        "turn" => Some(360.0),
        "rad" => Some(360.0 / (2.0 * std::f64::consts::PI)),
        "deg" | "" => Some(1.0),
        _ => None,
    }
}
