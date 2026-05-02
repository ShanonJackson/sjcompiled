//! JS number-to-string parity helper.
//!
//! `String(n)` in JavaScript follows the ECMAScript ToString-for-Number
//! algorithm (ECMA-262 §6.1.6.1.13), which is more nuanced than Rust's
//! `f64` Display:
//!
//!   * Integers in (-2^53, 2^53) print without a decimal point: `String(5) === "5"`.
//!   * Negative zero prints as `"0"` (NOT `"-0"`): `String(-0) === "0"`.
//!   * `NaN` -> `"NaN"`. `Infinity` -> `"Infinity"`. `-Infinity` -> `"-Infinity"`.
//!   * Numbers with magnitude in [1e-6, 1e21) use plain decimal notation
//!     with the *shortest* string that uniquely round-trips back to the
//!     original `f64` (Steele & White, also called Grisu / Ryu in Rust).
//!   * Numbers with `|n| < 1e-6` or `|n| >= 1e21` use scientific notation:
//!     `1e-7`, `1.5e-8`, `1e+21`, `2.5e+25`. The exponent always carries an
//!     explicit sign (`+` or `-`); Rust's `{:e}` omits the `+` on positive
//!     exponents, so we patch it.
//!
//! Rust's `format!("{}", f64)` uses Ryu, which produces the shortest unique
//! representation — same algorithm V8 uses. The remaining gaps:
//!
//!   1. Rust formats `-0.0` as `"-0"` while JS prints `"0"`.
//!   2. Rust's default `{}` formatter never switches to scientific notation
//!      (e.g. `format!("{}", 1e-7_f64)` yields `"0.0000001"`, not `"1e-7"`).
//!      JS does, at the thresholds above. We use `{:e}` for the scientific
//!      range and patch the exponent sign.
//!   3. Integers >= 2^53 lose precision the same way in both engines.
//!
//! Plugin authors who emit a number to a CSS string MUST use this function
//! rather than `format!("{}", n)` to preserve byte parity with JS output.

/// Mirrors JS `String(n)` for `f64`.
pub fn js_number_to_string(n: f64) -> String {
    // NaN.
    if n.is_nan() { return "NaN".to_string(); }
    // Infinities.
    if n.is_infinite() {
        return if n < 0.0 { "-Infinity".to_string() } else { "Infinity".to_string() };
    }
    // Negative zero -> "0". (`n == 0.0` matches both +0.0 and -0.0.)
    if n == 0.0 { return "0".to_string(); }

    let abs = n.abs();

    // ECMA-262 §6.1.6.1.13: scientific notation when |n| < 1e-6 or |n| >= 1e21.
    // The boundary value `1e-6` itself stays decimal (`String(1e-6) === "0.000001"`),
    // hence the strict `<`. The upper boundary `1e21` is scientific
    // (`String(1e21) === "1e+21"`), hence the `>=`.
    if abs >= 1e21 || abs < 1e-6 {
        return format_js_scientific(n);
    }

    // Integer fast path: any finite f64 that's an exact integer in
    // (-1e21, 1e21) prints without a decimal point. JS also uses this fast
    // path; Rust's default `{}` for `5.0_f64` prints "5" already in many
    // versions, but we make it explicit for cross-platform stability.
    if n == n.trunc() {
        // Use i128 for exact representation up to ±2^127 (covers the safe
        // integer range and beyond).
        let neg = n < 0.0;
        let abs_v = if neg { -n } else { n };
        // For values >= 2^53 the f64 cast is lossy — but JS is also lossy
        // here (it can't represent them exactly either), so the lossy
        // conversion is the right behaviour.
        if abs_v < (1u128 << 63) as f64 {
            let i = abs_v as u128;
            return if neg { format!("-{}", i) } else { i.to_string() };
        }
    }

    // Non-integer in [1e-6, 1e21): defer to Rust's Ryu-based Display.
    // Matches V8's shortest-unique-roundtrip algorithm in this range.
    let mut s = format!("{}", n);

    // Edge case: Rust emits `-0` for negative zero where JS emits `0`.
    // Already filtered `n == 0.0` above; this is defensive.
    if s == "-0" { s = "0".to_string(); }
    s
}

/// Format `n` in JS-compatible scientific notation: `<mantissa>e<sign><exp>`
/// where the sign is always `+` or `-` and the mantissa has no trailing zero
/// (`1e+21`, not `1.0e+21`). Rust's `{:e}` omits the `+` on positive exponents
/// (`"1e21"`); JS requires it (`"1e+21"`). Mantissa shape matches between
/// Rust's Ryu-shortest and V8's shortest-roundtrip.
fn format_js_scientific(n: f64) -> String {
    // `{:e}` for f64 in Rust 1.55+ produces the Ryu-shortest mantissa with a
    // signed exponent (sign elided on positive). Examples produced:
    //   `1e-7`, `1.5e-7`, `1e21`, `1.5e21`, `-1e-7`, `5e-324`.
    let raw = format!("{:e}", n);
    let e_pos = raw.find('e').expect("LowerExp f64 always contains 'e'");
    let (mantissa, exp_with_e) = raw.split_at(e_pos);
    // Skip the literal 'e'.
    let exp = &exp_with_e[1..];

    if let Some(rest) = exp.strip_prefix('-') {
        format!("{}e-{}", mantissa, rest)
    } else {
        format!("{}e+{}", mantissa, exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_no_decimal() {
        assert_eq!(js_number_to_string(0.0), "0");
        assert_eq!(js_number_to_string(1.0), "1");
        assert_eq!(js_number_to_string(-1.0), "-1");
        assert_eq!(js_number_to_string(255.0), "255");
        assert_eq!(js_number_to_string(1000.0), "1000");
    }

    #[test]
    fn negative_zero_normalizes() {
        assert_eq!(js_number_to_string(-0.0), "0");
    }

    #[test]
    fn nan_and_infinity() {
        assert_eq!(js_number_to_string(f64::NAN), "NaN");
        assert_eq!(js_number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(js_number_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn fractions_match_js_shortest() {
        // `String(0.5)` -> "0.5"
        assert_eq!(js_number_to_string(0.5), "0.5");
        // `String(0.1)` -> "0.1"
        assert_eq!(js_number_to_string(0.1), "0.1");
        // The infamous: `String(0.1 + 0.2)` -> "0.30000000000000004"
        assert_eq!(js_number_to_string(0.1 + 0.2), "0.30000000000000004");
    }

    #[test]
    fn large_integer() {
        assert_eq!(js_number_to_string(1_000_000.0), "1000000");
    }

    #[test]
    fn scientific_small_threshold() {
        // `String(1e-6) === "0.000001"` — boundary stays decimal.
        assert_eq!(js_number_to_string(1e-6), "0.000001");
        // `String(1e-7) === "1e-7"` — first scientific value.
        assert_eq!(js_number_to_string(1e-7), "1e-7");
        // `String(1.5e-7) === "1.5e-7"`.
        assert_eq!(js_number_to_string(1.5e-7), "1.5e-7");
        // `String(-1e-7) === "-1e-7"`.
        assert_eq!(js_number_to_string(-1e-7), "-1e-7");
        // `String(5e-324) === "5e-324"` — smallest positive subnormal.
        assert_eq!(js_number_to_string(5e-324_f64), "5e-324");
    }

    #[test]
    fn scientific_large_threshold() {
        // `String(1e20) === "100000000000000000000"` — boundary stays decimal.
        assert_eq!(js_number_to_string(1e20), "100000000000000000000");
        // `String(1e21) === "1e+21"` — first scientific value.
        assert_eq!(js_number_to_string(1e21), "1e+21");
        // `String(1.5e21) === "1.5e+21"`.
        assert_eq!(js_number_to_string(1.5e21), "1.5e+21");
        // `String(-1e21) === "-1e+21"`.
        assert_eq!(js_number_to_string(-1e21), "-1e+21");
        // `String(2.5e25) === "2.5e+25"`.
        assert_eq!(js_number_to_string(2.5e25), "2.5e+25");
    }

    #[test]
    fn scientific_postcss_calc_cases() {
        // Concrete failing inputs from the postcss-calc port's drift
        // report. Both must round-trip JS-equivalently.
        // calc(1e-2 / 1e5) = 1e-7
        assert_eq!(js_number_to_string(1e-2_f64 / 1e5_f64), "1e-7");
        // calc(1e+10 * 1e+11) = 1e21
        assert_eq!(js_number_to_string(1e10_f64 * 1e11_f64), "1e+21");
    }

    #[test]
    fn boundary_values_stay_decimal() {
        // Just above 1e-6: decimal.
        assert_eq!(js_number_to_string(2e-6), "0.000002");
        // Just below 1e21: decimal.
        assert_eq!(js_number_to_string(9e20), "900000000000000000000");
    }
}
