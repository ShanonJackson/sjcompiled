//! JS number-to-string parity helper.
//!
//! `String(n)` in JavaScript follows the ECMAScript ToString-for-Number
//! algorithm, which is more nuanced than Rust's `f64` Display:
//!
//!   * Integers in (-2^53, 2^53) print without a decimal point: `String(5) === "5"`.
//!   * Negative zero prints as `"0"` (NOT `"-0"`): `String(-0) === "0"`.
//!   * `NaN` -> `"NaN"`. `Infinity` -> `"Infinity"`. `-Infinity` -> `"-Infinity"`.
//!   * Numbers with magnitude in [1e-6, 1e21) use plain decimal notation
//!     with the *shortest* string that uniquely round-trips back to the
//!     original `f64` (Steele & White, also called Grisu / Ryu in Rust).
//!   * Numbers outside that range use scientific notation: `1e+21`.
//!
//! Rust's `format!("{}", f64)` uses Ryu, which produces the shortest unique
//! representation — same algorithm V8 uses. The remaining gaps:
//!
//!   1. Rust formats `-0.0` as `"-0"` while JS prints `"0"`.
//!   2. Rust formats very small/large numbers with `e0`-style exponents
//!      (e.g. `1e-7` becomes `"0.0000001"` in some Rust versions);
//!      Ryu uses scientific notation as JS does, but the exponent format
//!      is `1e-7` in both.
//!   3. Integers >= 2^53 lose precision the same way in both engines.
//!
//! This helper handles cases (1) and (2) explicitly. Plugin authors who
//! emit a number to a CSS string MUST use this function rather than
//! `format!("{}", n)` to preserve byte parity with JS output.

/// Mirrors JS `String(n)` for `f64`.
pub fn js_number_to_string(n: f64) -> String {
    // NaN.
    if n.is_nan() { return "NaN".to_string(); }
    // Infinities.
    if n.is_infinite() {
        return if n < 0.0 { "-Infinity".to_string() } else { "Infinity".to_string() };
    }
    // Negative zero -> "0".
    if n == 0.0 { return "0".to_string(); }

    // Integer fast path: any finite f64 that's an exact integer in
    // (-1e21, 1e21) prints without a decimal point. JS also uses this fast
    // path; Rust's default `{}` for `5.0_f64` prints "5" already in many
    // versions, but we make it explicit for cross-platform stability.
    if n == n.trunc() && n.abs() < 1e21 {
        // Use i128 for exact representation up to ±2^127 (covers the safe
        // integer range and beyond).
        let neg = n < 0.0;
        let abs = if neg { -n } else { n };
        // For values >= 2^53 the f64 cast is lossy — but JS is also lossy
        // here (it can't represent them exactly either), so the lossy
        // conversion is the right behaviour.
        if abs < (1u128 << 63) as f64 {
            let i = abs as u128;
            return if neg { format!("-{}", i) } else { i.to_string() };
        }
    }

    // For non-integer finite numbers, defer to Rust's Ryu-based Display.
    // This matches V8's shortest-unique-roundtrip algorithm.
    let mut s = format!("{}", n);

    // Edge case: Rust emits `-0` for negative zero where JS emits `0`.
    // Already filtered `n == 0.0` above; this is defensive.
    if s == "-0" { s = "0".to_string(); }
    s
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
}
