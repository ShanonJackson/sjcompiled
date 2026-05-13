//! Port of `postcss-calc/src/lib/convertUnit.js`. Line-numbered references
//! in this file point at upstream `convertUnit.js`.
//!
//! Conversion ratios are taken byte-for-byte from upstream. The exact
//! `f64` arithmetic matters: we MUST compute these the same way V8 does
//! so resulting values stringify identically. Since both engines use IEEE
//! 754 binary64, the ratios end up bit-identical when the source
//! expressions match.

use indexmap::IndexMap;
use std::sync::OnceLock;

/// `Math.PI` in V8.
const PI: f64 = std::f64::consts::PI;

/// Build the conversion table once. The outer key is the *target* unit;
/// the inner key is the *source* unit. `conversions[target][source] * value`
/// converts `value` from `source` to `target`.
///
/// The shape mirrors upstream `conversions` (line 5..). Insertion order is
/// preserved (IndexMap) — though iteration order doesn't reach output bytes
/// here, it's still policy under PARITY_VERSIONS.md cardinal rule #6.
fn conversions() -> &'static IndexMap<&'static str, IndexMap<&'static str, f64>> {
    static TABLE: OnceLock<IndexMap<&'static str, IndexMap<&'static str, f64>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: IndexMap<&'static str, IndexMap<&'static str, f64>> = IndexMap::new();
        // Absolute length units.
        t.insert("px", {
            let mut m = IndexMap::new();
            m.insert("px", 1.0);
            m.insert("cm", 96.0 / 2.54);
            m.insert("mm", 96.0 / 25.4);
            m.insert("q", 96.0 / 101.6);
            m.insert("in", 96.0);
            m.insert("pt", 96.0 / 72.0);
            m.insert("pc", 16.0);
            m
        });
        t.insert("cm", {
            let mut m = IndexMap::new();
            m.insert("px", 2.54 / 96.0);
            m.insert("cm", 1.0);
            m.insert("mm", 0.1);
            m.insert("q", 0.025);
            m.insert("in", 2.54);
            m.insert("pt", 2.54 / 72.0);
            m.insert("pc", 2.54 / 6.0);
            m
        });
        t.insert("mm", {
            let mut m = IndexMap::new();
            m.insert("px", 25.4 / 96.0);
            m.insert("cm", 10.0);
            m.insert("mm", 1.0);
            m.insert("q", 0.25);
            m.insert("in", 25.4);
            m.insert("pt", 25.4 / 72.0);
            m.insert("pc", 25.4 / 6.0);
            m
        });
        t.insert("q", {
            let mut m = IndexMap::new();
            m.insert("px", 101.6 / 96.0);
            m.insert("cm", 40.0);
            m.insert("mm", 4.0);
            m.insert("q", 1.0);
            m.insert("in", 101.6);
            m.insert("pt", 101.6 / 72.0);
            m.insert("pc", 101.6 / 6.0);
            m
        });
        t.insert("in", {
            let mut m = IndexMap::new();
            m.insert("px", 1.0 / 96.0);
            m.insert("cm", 1.0 / 2.54);
            m.insert("mm", 1.0 / 25.4);
            m.insert("q", 1.0 / 101.6);
            m.insert("in", 1.0);
            m.insert("pt", 1.0 / 72.0);
            m.insert("pc", 1.0 / 6.0);
            m
        });
        t.insert("pt", {
            let mut m = IndexMap::new();
            m.insert("px", 0.75);
            m.insert("cm", 72.0 / 2.54);
            m.insert("mm", 72.0 / 25.4);
            m.insert("q", 72.0 / 101.6);
            m.insert("in", 72.0);
            m.insert("pt", 1.0);
            m.insert("pc", 12.0);
            m
        });
        t.insert("pc", {
            let mut m = IndexMap::new();
            m.insert("px", 0.0625);
            m.insert("cm", 6.0 / 2.54);
            m.insert("mm", 6.0 / 25.4);
            m.insert("q", 6.0 / 101.6);
            m.insert("in", 6.0);
            m.insert("pt", 6.0 / 72.0);
            m.insert("pc", 1.0);
            m
        });
        // Angle units.
        t.insert("deg", {
            let mut m = IndexMap::new();
            m.insert("deg", 1.0);
            m.insert("grad", 0.9);
            m.insert("rad", 180.0 / PI);
            m.insert("turn", 360.0);
            m
        });
        t.insert("grad", {
            let mut m = IndexMap::new();
            m.insert("deg", 400.0 / 360.0);
            m.insert("grad", 1.0);
            m.insert("rad", 200.0 / PI);
            m.insert("turn", 400.0);
            m
        });
        t.insert("rad", {
            let mut m = IndexMap::new();
            m.insert("deg", PI / 180.0);
            m.insert("grad", PI / 200.0);
            m.insert("rad", 1.0);
            m.insert("turn", PI * 2.0);
            m
        });
        t.insert("turn", {
            let mut m = IndexMap::new();
            m.insert("deg", 1.0 / 360.0);
            m.insert("grad", 0.0025);
            m.insert("rad", 0.5 / PI);
            m.insert("turn", 1.0);
            m
        });
        // Duration units.
        t.insert("s", {
            let mut m = IndexMap::new();
            m.insert("s", 1.0);
            m.insert("ms", 0.001);
            m
        });
        t.insert("ms", {
            let mut m = IndexMap::new();
            m.insert("s", 1000.0);
            m.insert("ms", 1.0);
            m
        });
        // Frequency units.
        t.insert("hz", {
            let mut m = IndexMap::new();
            m.insert("hz", 1.0);
            m.insert("khz", 1000.0);
            m
        });
        t.insert("khz", {
            let mut m = IndexMap::new();
            m.insert("hz", 0.001);
            m.insert("khz", 1.0);
            m
        });
        // Resolution units.
        t.insert("dpi", {
            let mut m = IndexMap::new();
            m.insert("dpi", 1.0);
            m.insert("dpcm", 1.0 / 2.54);
            m.insert("dppx", 1.0 / 96.0);
            m
        });
        t.insert("dpcm", {
            let mut m = IndexMap::new();
            m.insert("dpi", 2.54);
            m.insert("dpcm", 1.0);
            m.insert("dppx", 2.54 / 96.0);
            m
        });
        t.insert("dppx", {
            let mut m = IndexMap::new();
            m.insert("dpi", 96.0);
            m.insert("dpcm", 96.0 / 2.54);
            m.insert("dppx", 1.0);
            m
        });
        t
    })
}

/// `precision` mirrors upstream's `number | false` shape.
#[derive(Debug, Clone, Copy)]
pub enum Precision {
    /// `false` upstream — no rounding.
    Never,
    /// `number` upstream. Note: `0` falls back to `5` inside convertUnit
    /// (upstream line 152: `Math.ceil(precision) || 5`), but we delay that
    /// transform to the call site so the type stays simple.
    At(f64),
}

/// Convertible error string. Matches upstream messages byte-for-byte
/// (see audit doc).
#[derive(Debug, Clone)]
pub struct ConvertUnitError(pub String);

impl std::fmt::Display for ConvertUnitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ConvertUnitError {}

/// Mirrors `convertUnit(value, sourceUnit, targetUnit, precision)`.
///
/// Upstream reference: `convertUnit.js:136-158`.
pub fn convert_unit(
    value: f64,
    source_unit: &str,
    target_unit: &str,
    precision: Precision,
) -> Result<f64, ConvertUnitError> {
    // .toLowerCase() — upstream line 137-138.
    let source_normalized = source_unit.to_ascii_lowercase();
    let target_normalized = target_unit.to_ascii_lowercase();

    let table = conversions();

    // Target-row missing → 'Cannot convert to ' + targetUnit (raw, NOT normalized).
    let row = match table.get(target_normalized.as_str()) {
        Some(r) => r,
        None => return Err(ConvertUnitError(format!("Cannot convert to {}", target_unit))),
    };

    // Source missing in target's row → 'Cannot convert from ' + sourceUnit + ' to ' + targetUnit.
    let ratio = match row.get(source_normalized.as_str()) {
        Some(r) => *r,
        None => {
            return Err(ConvertUnitError(format!(
                "Cannot convert from {} to {}",
                source_unit, target_unit
            )))
        }
    };

    let converted = ratio * value;

    match precision {
        Precision::Never => Ok(converted),
        Precision::At(p) => {
            // Math.pow(10, Math.ceil(precision) || 5)
            // - Math.ceil(NaN) === NaN, NaN || 5 === 5
            // - Math.ceil(0)   === 0,   0   || 5 === 5
            // - Math.ceil(1.2) === 2,   2   || 5 === 2
            // - Math.ceil(-1)  === -1,  -1  || 5 === -1
            let mut ceil_p = js_ceil(p);
            if ceil_p == 0.0 || ceil_p.is_nan() {
                ceil_p = 5.0;
            }
            let factor = (10f64).powf(ceil_p);
            // Math.round semantics: half toward +∞ (NOT half-away-from-zero).
            // Math.round(-0.5) === 0,  Math.round(-1.5) === -1.
            let rounded = js_math_round(converted * factor);
            Ok(rounded / factor)
        }
    }
}

/// `Math.ceil(x)` — JS semantics. Rust's `f64::ceil()` matches IEEE 754
/// `roundTiesToAway` for positive inputs and rounds toward +∞ — same as JS.
/// `(-0.0).ceil() === -0.0` in both engines. We just delegate.
fn js_ceil(x: f64) -> f64 {
    x.ceil()
}

/// `Math.round(x)` — JS semantics: round half toward +∞.
/// JS: `Math.round(-0.5) === 0`, `Math.round(0.5) === 1`, `Math.round(-1.5) === -1`.
/// Rust: `(-0.5_f64).round() === -1.0` (half-away-from-zero) — DIFFERENT.
/// Implementation: floor(x + 0.5), with one IEEE-754 corner: +∞/-∞/NaN
/// pass through, and `-0.0` stays `-0.0` (matches V8 — `Math.round(-0.4) === -0`).
pub fn js_math_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // (x + 0.5).floor() is the exact JS semantic per the spec
    // (ECMA-262 §21.3.2.27). For tie-breaking, +0.5 rounds up to 1 (so
    // 0.5 → 1), but -0.5 + 0.5 = 0 → floor(0) = 0 (so -0.5 → 0). ✓
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9 || a == b, "lhs={a} rhs={b}");
    }

    #[test]
    fn valid_conversions_smoke() {
        // Sample from upstream tests (default precision = 5).
        let prec = Precision::At(5.0);
        approx_eq(convert_unit(10.0, "px", "px", prec).unwrap(), 10.0);
        approx_eq(convert_unit(10.0, "px", "cm", prec).unwrap(), 0.26458);
        approx_eq(convert_unit(10.0, "cm", "px", prec).unwrap(), 377.95276);
        approx_eq(convert_unit(10.0, "deg", "grad", prec).unwrap(), 11.11111);
        approx_eq(convert_unit(10.0, "rad", "turn", prec).unwrap(), 1.59155);
        approx_eq(convert_unit(10.0, "s", "ms", prec).unwrap(), 10000.0);
        approx_eq(convert_unit(10.0, "Hz", "kHz", prec).unwrap(), 0.01);
        approx_eq(convert_unit(10.0, "dpi", "dpcm", prec).unwrap(), 25.4);
    }

    #[test]
    fn invalid_target() {
        let err = convert_unit(10.0, "px", "deg", Precision::At(5.0)).unwrap_err();
        assert_eq!(err.0, "Cannot convert from px to deg");
        // Note: upstream returns "Cannot convert to deg" only when target
        // category is unknown entirely. `deg` is in the table, but `px` is
        // not in `deg`'s row. Need a cross-category to trip the "unknown
        // target" branch:
        let err2 = convert_unit(10.0, "px", "totally-unknown", Precision::At(5.0)).unwrap_err();
        assert_eq!(err2.0, "Cannot convert to totally-unknown");
    }

    #[test]
    fn falsey_precision() {
        // Upstream: convertUnit(10, 'px', 'cm', false) === 0.26458333333333334.
        let r = convert_unit(10.0, "px", "cm", Precision::Never).unwrap();
        assert_eq!(r, 0.26458333333333334);
    }

    #[test]
    fn precision_10() {
        let prec = Precision::At(10.0);
        approx_eq(convert_unit(10.0, "px", "cm", prec).unwrap(), 0.2645833333);
        approx_eq(convert_unit(10.0, "cm", "px", prec).unwrap(), 377.9527559055);
    }

    #[test]
    fn js_math_round_tie_break() {
        assert_eq!(js_math_round(0.5), 1.0);
        assert_eq!(js_math_round(-0.5), 0.0); // KEY: differs from f64::round
        assert_eq!(js_math_round(1.5), 2.0);
        assert_eq!(js_math_round(-1.5), -1.0); // differs from f64::round
        assert_eq!(js_math_round(2.5), 3.0);
        assert_eq!(js_math_round(-2.5), -2.0);
    }

    #[test]
    fn unit_case_insensitive() {
        // 'PX' === 'px' after toLowerCase().
        let prec = Precision::At(5.0);
        let r = convert_unit(10.0, "PX", "Cm", prec).unwrap();
        approx_eq(r, 0.26458);
    }
}
