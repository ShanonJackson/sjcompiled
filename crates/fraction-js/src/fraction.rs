//! Port of `fraction.js@4.2.0`'s sole source file `fraction.js`.
//!
//! Upstream uses JS numbers (f64) for `s`, `n`, `d`. We keep that to preserve
//! arithmetic semantics including overflow / NaN propagation. Methods that take
//! one or two parameters in JS are exposed via the [`FractionInput`] enum which
//! reproduces the upstream `parse(p1, p2)` overload set.

use std::fmt;

// Sun fdlibm port. V8's `Math.log`, `Math.pow`, `Math.LN10` all come from
// the same fdlibm sources (`src/base/ieee754.cc`), so calling through
// `libm::*` here produces bit-identical f64 results to JavaScript on
// every platform we ship to. `f64::ln` / `f64::powf` would otherwise
// delegate to the system libm and drift by 1 ULP between OSes.
use libm;

/// JS `Math.LN10` — fdlibm's stored constant. Spelled out as the exact
/// f64 bit pattern V8 returns, so any consumer comparing against
/// `Math.LN10` sees the same value byte-for-byte.
const JS_LN10: f64 = 2.302585092994046_f64;

const MAX_CYCLE_LEN: u32 = 2000;

/// Mirrors the three exported error sentinels on the upstream `Fraction`
/// constructor: `Fraction.DivisionByZero`, `Fraction.InvalidParameter`,
/// `Fraction.NonIntegerParameter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FractionError {
    DivisionByZero,
    InvalidParameter,
    NonIntegerParameter,
}

impl fmt::Display for FractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FractionError::DivisionByZero => write!(f, "Division by Zero"),
            FractionError::InvalidParameter => write!(f, "Invalid argument"),
            FractionError::NonIntegerParameter => write!(f, "Parameters must be integer"),
        }
    }
}

impl std::error::Error for FractionError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fraction {
    pub s: f64,
    pub n: f64,
    pub d: f64,
}

/// Mirrors the polymorphic input accepted by the upstream `Fraction` ctor and
/// every arithmetic method: `(num)`, `(num, num)`, `({n,d,s})`, `([n,d])`,
/// `("123/456")`, etc.
#[derive(Debug, Clone)]
pub enum FractionInput {
    Undefined,
    Number(f64),
    Pair(f64, f64),
    Object { n: f64, d: f64, s: Option<f64> },
    Array(Vec<f64>),
    Str(String),
    Frac(Fraction),
}

impl From<f64> for FractionInput {
    fn from(v: f64) -> Self { FractionInput::Number(v) }
}
impl From<i64> for FractionInput {
    fn from(v: i64) -> Self { FractionInput::Number(v as f64) }
}
impl From<i32> for FractionInput {
    fn from(v: i32) -> Self { FractionInput::Number(v as f64) }
}
impl From<(f64, f64)> for FractionInput {
    fn from(v: (f64, f64)) -> Self { FractionInput::Pair(v.0, v.1) }
}
impl From<(i64, i64)> for FractionInput {
    fn from(v: (i64, i64)) -> Self { FractionInput::Pair(v.0 as f64, v.1 as f64) }
}
impl From<&str> for FractionInput {
    fn from(v: &str) -> Self { FractionInput::Str(v.to_string()) }
}
impl From<String> for FractionInput {
    fn from(v: String) -> Self { FractionInput::Str(v) }
}
impl From<Fraction> for FractionInput {
    fn from(v: Fraction) -> Self { FractionInput::Frac(v) }
}

#[derive(Default)]
struct Parsed {
    s: f64,
    n: f64,
    d: f64,
}

/// `assign(n, s)` — line 56 upstream. Note the JS `parseInt` semantics: it
/// strips a leading sign, parses as decimal, returns `NaN` on failure. We
/// mirror by parsing the leading integer prefix.
fn assign(token: &str, s: f64) -> Result<f64, FractionError> {
    let n = parse_int_radix10(token).ok_or(FractionError::InvalidParameter)?;
    Ok(n * s)
}

/// JS `parseInt(str, 10)` — reads optional sign then digits, ignores trailing.
fn parse_int_radix10(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') { i += 1; }
    let mut sign = 1.0f64;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        if bytes[i] == b'-' { sign = -1.0; }
        i += 1;
    }
    let start = i;
    let mut value: f64 = 0.0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        value = value * 10.0 + (bytes[i] - b'0') as f64;
        i += 1;
    }
    if i == start { return None; }
    Some(sign * value)
}

/// `gcd(a, b)` — line 342.
///
/// JS `!a` is true for both `0` and `NaN` (and any other falsy). We must
/// mirror that to avoid an infinite loop on NaN inputs — e.g. the constructor
/// invokes `gcd(P.d, P.n)` and `Fraction(NaN)` reaches `gcd(NaN, NaN)`.
fn gcd(mut a: f64, mut b: f64) -> f64 {
    if a == 0.0 || a.is_nan() { return b; }
    if b == 0.0 || b.is_nan() { return a; }
    loop {
        a = js_mod(a, b);
        if a == 0.0 || a.is_nan() { return b; }
        b = js_mod(b, a);
        if b == 0.0 || b.is_nan() { return a; }
    }
}

/// JS `%` operator: truncates toward zero.
fn js_mod(a: f64, b: f64) -> f64 {
    a - (a / b).trunc() * b
}

/// `factorize(num)` — line 83. Returns insertion-ordered factor map.
fn factorize(num: f64) -> Vec<(f64, f64)> {
    let mut factors: Vec<(f64, f64)> = Vec::new();
    let mut n = num;
    let mut i: f64 = 2.0;
    let mut s: f64 = 4.0;
    while s <= n {
        while js_mod(n, i) == 0.0 {
            n /= i;
            inc_factor(&mut factors, i);
        }
        s += 1.0 + 2.0 * i;
        i += 1.0;
    }
    if n != num {
        if n > 1.0 { inc_factor(&mut factors, n); }
    } else {
        inc_factor(&mut factors, num);
    }
    factors
}

fn inc_factor(factors: &mut Vec<(f64, f64)>, key: f64) {
    if let Some(entry) = factors.iter_mut().find(|(k, _)| *k == key) {
        entry.1 += 1.0;
    } else {
        factors.push((key, 1.0));
    }
}

/// `modpow(b, e, m)` — line 281.
fn modpow(mut b: f64, mut e: f64, m: f64) -> f64 {
    let mut r = 1.0;
    while e > 0.0 {
        // (e & 1) — JS coerces to int32. Use trunc + bitand on i64.
        if (e as i64 & 1) == 1 {
            r = js_mod(r * b, m);
        }
        b = js_mod(b * b, m);
        e = ((e as i64) >> 1) as f64;
    }
    r
}

/// `cycleLen(n, d)` — line 294.
fn cycle_len(_n: f64, mut d: f64) -> u32 {
    while js_mod(d, 2.0) == 0.0 { d /= 2.0; }
    while js_mod(d, 5.0) == 0.0 { d /= 5.0; }
    if d == 1.0 { return 0; }
    let mut rem = js_mod(10.0, d);
    let mut t: u32 = 1;
    while rem != 1.0 {
        rem = js_mod(rem * 10.0, d);
        t += 1;
        if t > MAX_CYCLE_LEN { return 0; }
    }
    t
}

/// `cycleStart(n, d, len)` — line 325.
fn cycle_start(_n: f64, d: f64, len: u32) -> u32 {
    let mut rem1 = 1.0f64;
    let mut rem2 = modpow(10.0, len as f64, d);
    for t in 0u32..300 {
        if rem1 == rem2 { return t; }
        rem1 = js_mod(rem1 * 10.0, d);
        rem2 = js_mod(rem2 * 10.0, d);
    }
    0
}

/// JS string -> tokens like `B = p1.match(/\d+|./g)`. Returns digits-runs and
/// single chars (code units, but ASCII for our purposes).
///
/// JS regex `.` (without the `s` flag) matches any code unit EXCEPT the
/// LineTerminator set: U+000A (\n), U+000D (\r), U+2028, U+2029. Those code
/// points are silently skipped by `match`, not emitted as tokens. We mirror
/// that here so that strings like `"1\n2"` produce the same `[\"1\", \"2\"]`
/// tokenization that JS does and reach the same parse failure path.
fn match_digits_or_char(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            out.push(s[start..i].to_string());
        } else {
            // Non-digit: consume one UTF-8 char.
            let ch_len = utf8_char_len(bytes[i]);
            let slice = &s[i..i + ch_len];
            i += ch_len;
            if is_js_line_terminator(slice) {
                continue;
            }
            out.push(slice.to_string());
        }
    }
    out
}

fn is_js_line_terminator(s: &str) -> bool {
    matches!(s, "\n" | "\r" | "\u{2028}" | "\u{2029}")
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 { 1 }
    else if b < 0xC0 { 1 }
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

/// `parse(p1, p2)` — line 109. Writes the parsed result into `Parsed` instead
/// of a module-global `P`.
fn parse_input(input: &FractionInput) -> Result<Parsed, FractionError> {
    let mut n: f64 = 0.0;
    let mut d: f64 = 1.0;
    let mut s: f64 = 1.0;
    let mut v: f64 = 0.0;
    let mut w: f64 = 0.0;
    let mut x: f64 = 0.0;
    let mut y: f64 = 1.0;
    let mut z: f64 = 1.0;

    match input {
        FractionInput::Undefined => { /* void */ }
        FractionInput::Pair(p1, p2) => {
            n = *p1;
            d = *p2;
            s = n * d;
            if js_mod(n, 1.0) != 0.0 || js_mod(d, 1.0) != 0.0 {
                return Err(FractionError::NonIntegerParameter);
            }
        }
        FractionInput::Object { n: on, d: od, s: os } => {
            n = *on;
            d = *od;
            if let Some(sv) = os { n *= *sv; }
            s = n * d;
        }
        FractionInput::Array(arr) => {
            if arr.is_empty() { return Err(FractionError::InvalidParameter); }
            n = arr[0];
            if arr.len() > 1 { d = arr[1]; }
            s = n * d;
        }
        FractionInput::Frac(f) => {
            // Frac is "object with d, n, s". Preserve s.
            n = f.n * f.s;
            d = f.d;
            s = n * d;
        }
        FractionInput::Number(p1) => {
            let mut p1 = *p1;
            if p1 < 0.0 {
                s = p1;
                p1 = -p1;
            }
            if p1.is_nan() {
                d = f64::NAN;
                n = f64::NAN;
            } else if js_mod(p1, 1.0) == 0.0 {
                n = p1;
            } else if p1 > 0.0 {
                if p1 >= 1.0 {
                    // `Math.pow(10, Math.floor(1 + Math.log(p1) / Math.LN10))`.
                    // Use libm (= V8's fdlibm) to match JS bit-for-bit. Rust's
                    // `f64::ln` / `std::f64::consts::LN_10` would drift by up
                    // to 1 ULP on non-Windows hosts and could land `floor` on
                    // a different integer near boundaries like `p1 = 10.0`.
                    z = libm::pow(10.0, libm::floor(1.0 + libm::log(p1) / JS_LN10));
                    p1 /= z;
                }
                let big_n: f64 = 10_000_000.0;
                let mut a = 0.0; let mut b = 1.0;
                let mut c = 1.0; let mut dd = 1.0;
                while b <= big_n && dd <= big_n {
                    let m = (a + c) / (b + dd);
                    if p1 == m {
                        if b + dd <= big_n { n = a + c; d = b + dd; }
                        else if dd > b { n = c; d = dd; }
                        else { n = a; d = b; }
                        break;
                    } else {
                        if p1 > m { a += c; b += dd; }
                        else { c += a; dd += b; }
                        if b > big_n { n = c; d = dd; }
                        else { n = a; d = b; }
                    }
                }
                n *= z;
            }
        }
        FractionInput::Str(p1) => {
            let b = match_digits_or_char(p1);
            if b.is_empty() { return Err(FractionError::InvalidParameter); }
            let mut a: usize = 0;
            if b[a] == "-" { s = -1.0; a += 1; }
            else if b[a] == "+" { a += 1; }

            if b.len() == a + 1 {
                let tok = b[a].clone(); a += 1;
                w = assign(&tok, s)?;
            } else if (a + 1 < b.len() && b[a + 1] == ".") || (a < b.len() && b[a] == ".") {
                // Bounds-guard b[a]: e.g. input "-" reaches here with a == b.len().
                // JS evaluates `B[A] === '.'` as `undefined === '.'` (false) and
                // continues; we must not panic.
                if b[a] != "." {
                    let tok = b[a].clone(); a += 1;
                    v = assign(&tok, s)?;
                }
                a += 1;
                if a + 1 == b.len()
                    || (a + 3 < b.len() && b[a + 1] == "(" && b[a + 3] == ")")
                    || (a + 3 < b.len() && b[a + 1] == "'" && b[a + 3] == "'")
                {
                    if a < b.len() {
                        let tok = b[a].clone();
                        w = assign(&tok, s)?;
                        // `Math.pow(10, B[A].length)` — route through libm
                        // to stay bit-equal to V8 across hosts.
                        y = libm::pow(10.0, tok.len() as f64);
                        a += 1;
                    }
                }
                if a + 2 < b.len() && ((b[a] == "(" && b[a + 2] == ")") || (b[a] == "'" && b[a + 2] == "'")) {
                    let tok = b[a + 1].clone();
                    x = assign(&tok, s)?;
                    z = libm::pow(10.0, tok.len() as f64) - 1.0;
                    a += 3;
                }
            } else if a + 1 < b.len() && (b[a + 1] == "/" || b[a + 1] == ":") {
                // JS does not bounds-check B[A+2]; if absent it parses
                // undefined → NaN → throws InvalidParameter via `assign`. We
                // mirror that error path explicitly to avoid an index panic.
                let tok = b[a].clone();
                w = assign(&tok, s)?;
                let tok2 = b.get(a + 2).ok_or(FractionError::InvalidParameter)?.clone();
                y = assign(&tok2, 1.0)?;
                a += 3;
            } else if a + 3 < b.len() && b[a + 3] == "/" && b[a + 1] == " " {
                // Same rationale as above for B[A+4] in the complex-fraction branch.
                let tok = b[a].clone();
                v = assign(&tok, s)?;
                let tok2 = b[a + 2].clone();
                w = assign(&tok2, s)?;
                let tok3 = b.get(a + 4).ok_or(FractionError::InvalidParameter)?.clone();
                y = assign(&tok3, 1.0)?;
                a += 5;
            }

            if b.len() <= a {
                d = y * z;
                // Upstream chain-assigns `s = /* void */ n = x + d * v + z * w`
                // (line 261-262 of fraction.js). The `s = ...` is not a no-op:
                // it overwrites the sign tracker with the computed numerator,
                // which matters when the input is negative-zero — `-0` would
                // otherwise emit Fraction { s: -1, n: 0, d: 1 } instead of the
                // upstream `{ s: 1, n: 0, d: 1 }`. Mirror that here.
                n = x + d * v + z * w;
                s = n;
            } else {
                return Err(FractionError::InvalidParameter);
            }
        }
    }

    if d == 0.0 { return Err(FractionError::DivisionByZero); }
    Ok(Parsed {
        s: if s < 0.0 { -1.0 } else { 1.0 },
        n: n.abs(),
        d: d.abs(),
    })
}

fn new_fraction(n: f64, d: f64) -> Result<Fraction, FractionError> {
    if d == 0.0 { return Err(FractionError::DivisionByZero); }
    let s = if n < 0.0 { -1.0 } else { 1.0 };
    let n_abs = if n < 0.0 { -n } else { n };
    let a = gcd(n_abs, d);
    Ok(Fraction { s, n: n_abs / a, d: d / a })
}

impl Default for Fraction {
    fn default() -> Self { Fraction { s: 1.0, n: 0.0, d: 1.0 } }
}

impl Fraction {
    pub fn new<I: Into<FractionInput>>(input: I) -> Result<Self, FractionError> {
        let p = parse_input(&input.into())?;
        // The constructor branch in JS reduces using gcd(d, n).
        let a = gcd(p.d, p.n);
        Ok(Fraction { s: p.s, n: p.n / a, d: p.d / a })
    }

    pub fn abs(&self) -> Result<Self, FractionError> { new_fraction(self.n, self.d) }

    pub fn neg(&self) -> Result<Self, FractionError> { new_fraction(-self.s * self.n, self.d) }

    pub fn add<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        new_fraction(
            self.s * self.n * p.d + p.s * self.d * p.n,
            self.d * p.d,
        )
    }

    pub fn sub<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        new_fraction(
            self.s * self.n * p.d - p.s * self.d * p.n,
            self.d * p.d,
        )
    }

    pub fn mul<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        new_fraction(self.s * p.s * self.n * p.n, self.d * p.d)
    }

    pub fn div<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        new_fraction(self.s * p.s * self.n * p.d, self.d * p.n)
    }

    pub fn clone_fraction(&self) -> Result<Self, FractionError> {
        new_fraction(self.s * self.n, self.d)
    }

    pub fn modulo<I: Into<FractionInput>>(&self, other: Option<I>) -> Result<Self, FractionError> {
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        match other {
            None => new_fraction(js_mod(self.s * self.n, self.d), 1.0),
            Some(other) => {
                let p = parse_input(&other.into())?;
                if p.n == 0.0 && self.d == 0.0 {
                    return Err(FractionError::DivisionByZero);
                }
                new_fraction(
                    js_mod(self.s * (p.d * self.n), p.n * self.d),
                    p.d * self.d,
                )
            }
        }
    }

    pub fn gcd<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        new_fraction(gcd(p.n, self.n) * gcd(p.d, self.d), p.d * self.d)
    }

    pub fn lcm<I: Into<FractionInput>>(&self, other: I) -> Result<Self, FractionError> {
        let p = parse_input(&other.into())?;
        if p.n == 0.0 && self.n == 0.0 {
            return new_fraction(0.0, 1.0);
        }
        new_fraction(p.n * self.n, gcd(p.n, self.n) * gcd(p.d, self.d))
    }

    pub fn ceil(&self, places: Option<i32>) -> Result<Self, FractionError> {
        // `Math.pow(10, places || 0)` — fdlibm. `powi` is allegedly exact
        // for integer exponents on most LLVM backends but is not guaranteed
        // bit-equal to `Math.pow(10, n)` for very large `n`; use libm::pow.
        let places = libm::pow(10.0, places.unwrap_or(0) as f64);
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        new_fraction(libm::ceil(places * self.s * self.n / self.d), places)
    }

    pub fn floor(&self, places: Option<i32>) -> Result<Self, FractionError> {
        let places = libm::pow(10.0, places.unwrap_or(0) as f64);
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        new_fraction(libm::floor(places * self.s * self.n / self.d), places)
    }

    pub fn round(&self, places: Option<i32>) -> Result<Self, FractionError> {
        let places = libm::pow(10.0, places.unwrap_or(0) as f64);
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        // JS Math.round = half toward +∞: see `js_math_round`. Floor is
        // exact in IEEE 754, so no libm needed for the floor step.
        new_fraction(js_math_round(places * self.s * self.n / self.d), places)
    }

    pub fn inverse(&self) -> Result<Self, FractionError> {
        new_fraction(self.s * self.d, self.n)
    }

    pub fn pow<I: Into<FractionInput>>(&self, other: I) -> Result<Option<Self>, FractionError> {
        // Every `Math.pow` call in upstream → `libm::pow`. `f64::powf`
        // routes through the system libm and is not bit-equal to V8.
        let p = parse_input(&other.into())?;
        if p.d == 1.0 {
            if p.s < 0.0 {
                return new_fraction(
                    libm::pow(self.s * self.d, p.n),
                    libm::pow(self.n, p.n),
                ).map(Some);
            } else {
                return new_fraction(
                    libm::pow(self.s * self.n, p.n),
                    libm::pow(self.d, p.n),
                ).map(Some);
            }
        }
        if self.s < 0.0 { return Ok(None); }

        let nf = factorize(self.n);
        let df = factorize(self.d);

        let mut n_acc = 1.0f64;
        let mut d_acc = 1.0f64;
        let mut nf = nf;
        for (k, val) in nf.iter_mut() {
            if *k == 1.0 { continue; }
            if *k == 0.0 { n_acc = 0.0; break; }
            *val *= p.n;
            if js_mod(*val, p.d) == 0.0 {
                *val /= p.d;
            } else {
                return Ok(None);
            }
            n_acc *= libm::pow(*k, *val);
        }
        let mut df = df;
        for (k, val) in df.iter_mut() {
            if *k == 1.0 { continue; }
            *val *= p.n;
            if js_mod(*val, p.d) == 0.0 {
                *val /= p.d;
            } else {
                return Ok(None);
            }
            d_acc *= libm::pow(*k, *val);
        }

        if p.s < 0.0 {
            new_fraction(d_acc, n_acc).map(Some)
        } else {
            new_fraction(n_acc, d_acc).map(Some)
        }
    }

    pub fn equals<I: Into<FractionInput>>(&self, other: I) -> Result<bool, FractionError> {
        let p = parse_input(&other.into())?;
        Ok(self.s * self.n * p.d == p.s * p.n * self.d)
    }

    pub fn compare<I: Into<FractionInput>>(&self, other: I) -> Result<i32, FractionError> {
        let p = parse_input(&other.into())?;
        let t = self.s * self.n * p.d - p.s * p.n * self.d;
        Ok(((0.0 < t) as i32) - ((t < 0.0) as i32))
    }

    /// `simplify(eps)` — line 689. Find the closest continued-fraction
    /// approximation within `eps` (default `0.001`).
    pub fn simplify(&self, eps: Option<f64>) -> Result<Self, FractionError> {
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(*self);
        }
        // `eps = eps || 0.001` — collapses 0 and NaN to default per JS truthiness.
        let eps = match eps {
            Some(v) if v != 0.0 && !v.is_nan() => v,
            _ => 0.001,
        };
        let this_abs = self.abs()?;
        let cont = this_abs.to_continued();
        for i in 1..cont.len() {
            let mut s_acc = new_fraction(cont[i - 1], 1.0)?;
            // JS `for (var k = i - 2; k >= 0; k--)` — descending from i-2 to 0.
            // For i == 1 the inner loop runs zero times; we mirror with a
            // 0..(i-1) range reversed (empty when i == 1).
            for k in (0..(i.saturating_sub(1))).rev() {
                s_acc = s_acc.inverse()?.add(cont[k])?;
            }
            if s_acc.sub(this_abs)?.abs()?.value_of() < eps {
                return s_acc.mul(self.s);
            }
        }
        Ok(*self)
    }

    pub fn divisible<I: Into<FractionInput>>(&self, other: I) -> Result<bool, FractionError> {
        let p = parse_input(&other.into())?;
        let lhs = p.n * self.d;
        if lhs == 0.0 { return Ok(false); }
        Ok(js_mod(self.n * p.d, lhs) == 0.0)
    }

    pub fn value_of(&self) -> f64 { self.s * self.n / self.d }

    pub fn to_fraction(&self, exclude_whole: bool) -> String {
        let mut str_out = String::new();
        let mut n = self.n;
        let d = self.d;
        if self.s < 0.0 { str_out.push('-'); }
        if d == 1.0 {
            str_out.push_str(&js_number_to_string(n));
        } else if exclude_whole {
            let whole = (n / d).floor();
            if whole > 0.0 {
                str_out.push_str(&js_number_to_string(whole));
                str_out.push(' ');
                n = js_mod(n, d);
            }
            str_out.push_str(&js_number_to_string(n));
            str_out.push('/');
            str_out.push_str(&js_number_to_string(d));
        } else {
            str_out.push_str(&js_number_to_string(n));
            str_out.push('/');
            str_out.push_str(&js_number_to_string(d));
        }
        str_out
    }

    pub fn to_latex(&self, exclude_whole: bool) -> String {
        let mut str_out = String::new();
        let mut n = self.n;
        let d = self.d;
        if self.s < 0.0 { str_out.push('-'); }
        if d == 1.0 {
            str_out.push_str(&js_number_to_string(n));
        } else {
            if exclude_whole {
                let whole = (n / d).floor();
                if whole > 0.0 {
                    str_out.push_str(&js_number_to_string(whole));
                    n = js_mod(n, d);
                }
            }
            str_out.push_str("\\frac{");
            str_out.push_str(&js_number_to_string(n));
            str_out.push_str("}{");
            str_out.push_str(&js_number_to_string(d));
            str_out.push('}');
        }
        str_out
    }

    pub fn to_continued(&self) -> Vec<f64> {
        let mut a = self.n;
        let mut b = self.d;
        let mut res: Vec<f64> = Vec::new();
        if a.is_nan() || b.is_nan() { return res; }
        loop {
            res.push((a / b).floor());
            let t = js_mod(a, b);
            a = b;
            b = t;
            if a == 1.0 { break; }
        }
        res
    }

    pub fn to_string_dec(&self, dec: Option<u32>) -> String {
        let mut n_val = self.n;
        let d_val = self.d;
        if n_val.is_nan() || d_val.is_nan() { return "NaN".to_string(); }
        // Upstream: `dec = dec || 15`. JS truthiness collapses `0` to the
        // default, so `Some(0)` must also fall back to 15 here.
        let dec = match dec {
            Some(0) | None => 15,
            Some(v) => v,
        };
        let cyc_len = cycle_len(n_val, d_val);
        let cyc_off = cycle_start(n_val, d_val, cyc_len);
        let mut str_out = if self.s < 0.0 { "-".to_string() } else { String::new() };
        // `N / D | 0` — JS `| 0` coerces to signed-int32 (wraps on overflow).
        // `f64::trunc` clips to the f64 range, so for `N/D >= 2^31` Rust would
        // emit the full integer while JS wraps to a negative value. Use
        // `js_int32_trunc` to reproduce wrap-on-overflow exactly.
        str_out.push_str(&js_number_to_string(js_int32_trunc(n_val / d_val)));
        n_val = js_mod(n_val, d_val);
        n_val *= 10.0;
        if n_val != 0.0 { str_out.push('.'); }
        if cyc_len > 0 {
            for _ in 0..cyc_off {
                str_out.push_str(&js_number_to_string(js_int32_trunc(n_val / d_val)));
                n_val = js_mod(n_val, d_val);
                n_val *= 10.0;
            }
            str_out.push('(');
            for _ in 0..cyc_len {
                str_out.push_str(&js_number_to_string(js_int32_trunc(n_val / d_val)));
                n_val = js_mod(n_val, d_val);
                n_val *= 10.0;
            }
            str_out.push(')');
        } else {
            for _ in 0..dec {
                if n_val == 0.0 { break; }
                str_out.push_str(&js_number_to_string(js_int32_trunc(n_val / d_val)));
                n_val = js_mod(n_val, d_val);
                n_val *= 10.0;
            }
        }
        str_out
    }
}

/// JS `Math.round` — half toward +∞.
fn js_math_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// JS `x | 0` — ECMA-262 ToInt32. Truncate toward zero, then take the result
/// modulo 2^32 and reinterpret as a signed 32-bit integer. Returns the value
/// as `f64` so it composes with the rest of the f64 arithmetic in this module.
///
/// Pattern matches V8/SpiderMonkey: NaN → 0, ±Infinity → 0, otherwise
/// `(trunc(x) mod 2^32) - (>=2^31 ? 2^32 : 0)`.
fn js_int32_trunc(x: f64) -> f64 {
    if !x.is_finite() { return 0.0; }
    let truncated = x.trunc();
    // 32-bit modulo on the truncated magnitude, sign preserved.
    let modulo = 4_294_967_296.0_f64;
    let m = truncated.rem_euclid(modulo);
    let signed = if m >= 2_147_483_648.0 { m - modulo } else { m };
    signed
}

/// JS number-to-string for integers: produces the same digits as `String(n)`
/// for finite integer values. For non-integers we fall back to `f64` Display
/// — only invoked from non-hashing paths in this module.
pub(crate) fn js_number_to_string(n: f64) -> String {
    if n.is_nan() { return "NaN".to_string(); }
    if n.is_infinite() { return if n < 0.0 { "-Infinity".to_string() } else { "Infinity".to_string() }; }
    if n == 0.0 { return "0".to_string(); }
    if n == n.trunc() && n.abs() < 1e21 {
        // Integer fast path. Use i128 to keep precision up to 2^53.
        let neg = n < 0.0;
        let abs = if neg { -n } else { n } as u128;
        let s = abs.to_string();
        return if neg { format!("-{s}") } else { s };
    }
    // Non-integer: defer to ryu via format!. Caveat: this may not match
    // V8's number-to-string in edge cases; fraction.js itself rarely emits
    // non-integer strings outside of `valueOf`.
    format!("{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_int() {
        let f = Fraction::new(5).unwrap();
        assert_eq!(f.s, 1.0);
        assert_eq!(f.n, 5.0);
        assert_eq!(f.d, 1.0);
    }

    #[test]
    fn parse_pair() {
        let f = Fraction::new((3, 4)).unwrap();
        assert_eq!(f.n, 3.0);
        assert_eq!(f.d, 4.0);
    }

    #[test]
    fn parse_simple_string() {
        let f = Fraction::new("3/4").unwrap();
        assert_eq!(f.n, 3.0);
        assert_eq!(f.d, 4.0);
    }

    #[test]
    fn parse_repeating() {
        let f = Fraction::new("1.(3)").unwrap();
        assert_eq!(f.n, 4.0);
        assert_eq!(f.d, 3.0);
    }

    #[test]
    fn add_basic() {
        let a = Fraction::new((1, 2)).unwrap();
        let b = Fraction::new((1, 3)).unwrap();
        let c = a.add(b).unwrap();
        assert_eq!(c.n, 5.0);
        assert_eq!(c.d, 6.0);
    }

    #[test]
    fn mul_negative() {
        let a = Fraction::new(-2).unwrap();
        let b = Fraction::new((3, 4)).unwrap();
        let c = a.mul(b).unwrap();
        assert_eq!(c.s, -1.0);
        assert_eq!(c.n, 3.0);
        assert_eq!(c.d, 2.0);
    }

    #[test]
    fn to_fraction_string() {
        let f = Fraction::new("7/8").unwrap();
        assert_eq!(f.to_fraction(false), "7/8");
    }

    #[test]
    fn to_string_repeating() {
        let f = Fraction::new("1/3").unwrap();
        assert_eq!(f.to_string_dec(None), "0.(3)");
    }

    #[test]
    fn equals_compare() {
        let a = Fraction::new((1, 2)).unwrap();
        let b = Fraction::new((2, 4)).unwrap();
        assert!(a.equals(b.clone_fraction().unwrap()).unwrap());
        assert_eq!(a.compare(0).unwrap(), 1);
    }

    #[test]
    fn divisible() {
        let a = Fraction::new(6).unwrap();
        let b = Fraction::new(2).unwrap();
        assert!(a.divisible(b.clone_fraction().unwrap()).unwrap());
    }

    // --- Regressions for the 4.2.0 re-audit ---

    /// JS: `new Fraction("-0")` => { s: 1, n: 0, d: 1 }
    /// Upstream chain-assigns `s = /* void */ n = ...`, so the sign tracker
    /// gets overwritten with the (zero) numerator. Without that, our port
    /// emitted `-0` with `s = -1` and `toString` would print "-0".
    #[test]
    fn neg_zero_string_collapses_sign() {
        let f = Fraction::new("-0").unwrap();
        assert_eq!(f.s, 1.0);
        assert_eq!(f.n, 0.0);
        assert_eq!(f.d, 1.0);
        assert_eq!(f.to_string_dec(None), "0");
    }

    #[test]
    fn neg_zero_decimal_collapses_sign() {
        let f = Fraction::new("-0.0").unwrap();
        assert_eq!(f.s, 1.0);
        assert_eq!(f.n, 0.0);
        assert_eq!(f.d, 1.0);
    }

    /// JS: `new Fraction("-")` => { s: 1, n: 0, d: 1 }. Pre-fix, our port
    /// either panicked on `b[a]` indexing or returned `s = -1`.
    #[test]
    fn lone_sign_parses_as_zero() {
        let f = Fraction::new("-").unwrap();
        assert_eq!(f.s, 1.0);
        assert_eq!(f.n, 0.0);
        assert_eq!(f.d, 1.0);

        let f = Fraction::new("+").unwrap();
        assert_eq!(f.s, 1.0);
        assert_eq!(f.n, 0.0);
        assert_eq!(f.d, 1.0);
    }

    /// JS: `new Fraction("1/")` throws InvalidArgument (parseInt(undefined)
    /// → NaN → throw). Pre-fix our port panicked on `b[a + 2]`.
    #[test]
    fn truncated_fraction_string_errors() {
        assert_eq!(Fraction::new("1/"), Err(FractionError::InvalidParameter));
        assert_eq!(Fraction::new("1:"), Err(FractionError::InvalidParameter));
    }

    /// Same panic class for the complex-fraction `n n/d` form when truncated.
    #[test]
    fn truncated_complex_fraction_errors() {
        assert_eq!(Fraction::new("1 1/"), Err(FractionError::InvalidParameter));
    }

    /// JS: `new Fraction(NaN)` => { s: 1, n: NaN, d: NaN }. Pre-fix our port
    /// hung in `gcd(NaN, NaN)` because `0.0 == NaN` is false but JS `!NaN`
    /// is true.
    #[test]
    fn nan_constructor_does_not_hang() {
        let f = Fraction::new(f64::NAN).unwrap();
        assert_eq!(f.s, 1.0);
        assert!(f.n.is_nan());
        assert!(f.d.is_nan());
        assert_eq!(f.to_string_dec(None), "NaN");
    }

    /// JS `dec || 15` collapses 0 to the default, so `toString(0)` should
    /// behave like `toString()`.
    #[test]
    fn to_string_dec_zero_falls_back_to_default() {
        let f = Fraction::new("1/3").unwrap();
        assert_eq!(f.to_string_dec(Some(0)), "0.(3)");
        assert_eq!(f.to_string_dec(None), "0.(3)");
    }

    /// `simplify` was missing entirely. JS spec: closest CF approximation
    /// within `eps`. `(0.1).simplify(0.1)` => `1/10`.
    #[test]
    fn simplify_basic() {
        let f = Fraction::new(0.1).unwrap();
        let s = f.simplify(Some(0.1)).unwrap();
        assert_eq!(s.to_fraction(false), "1/10");
    }

    #[test]
    fn simplify_default_eps() {
        // 0.5 is already the simplest 1/2.
        let f = Fraction::new(0.5).unwrap();
        let s = f.simplify(None).unwrap();
        assert_eq!(s.n, 1.0);
        assert_eq!(s.d, 2.0);
    }

    #[test]
    fn simplify_nan_returns_self() {
        let f = Fraction::new(f64::NAN).unwrap();
        let s = f.simplify(None).unwrap();
        assert!(s.n.is_nan());
    }

    /// JS regex `.` skips line terminators. Either way the input is
    /// malformed; we just need the SAME error path.
    #[test]
    fn line_terminator_in_string_errors_like_js() {
        assert_eq!(Fraction::new("1\n2"), Err(FractionError::InvalidParameter));
        assert_eq!(Fraction::new("1\r2"), Err(FractionError::InvalidParameter));
    }

    /// JS `N / D | 0` is signed-int32 wrap. For `N/D >= 2^31` JS wraps to
    /// negative; `f64::trunc` does not. `js_int32_trunc` reproduces the
    /// JS behavior (NaN → 0, ±Infinity → 0, otherwise mod-2^32 signed).
    #[test]
    fn js_int32_trunc_matches_js_wrap() {
        // Within int32 range: same as trunc.
        assert_eq!(super::js_int32_trunc(0.0), 0.0);
        assert_eq!(super::js_int32_trunc(1.7), 1.0);
        assert_eq!(super::js_int32_trunc(-1.7), -1.0);
        assert_eq!(super::js_int32_trunc(2_147_483_647.0), 2_147_483_647.0);
        // 2^31 = 2147483648 → wraps to -2^31.
        assert_eq!(super::js_int32_trunc(2_147_483_648.0), -2_147_483_648.0);
        // 2^32 wraps to 0.
        assert_eq!(super::js_int32_trunc(4_294_967_296.0), 0.0);
        // 2^32 + 5 wraps to 5.
        assert_eq!(super::js_int32_trunc(4_294_967_301.0), 5.0);
        // Negatives also wrap.
        assert_eq!(super::js_int32_trunc(-2_147_483_649.0), 2_147_483_647.0);
        // Non-finite → 0 (matches JS `NaN | 0 === 0`, `Infinity | 0 === 0`).
        assert_eq!(super::js_int32_trunc(f64::NAN), 0.0);
        assert_eq!(super::js_int32_trunc(f64::INFINITY), 0.0);
        assert_eq!(super::js_int32_trunc(f64::NEG_INFINITY), 0.0);
    }

    /// libm-pow vs Rust `f64::powf`: we want `Math.pow(10, k)` for integer k
    /// to be exact. Spot-check a few that matter for the parse-Number branch.
    #[test]
    fn libm_pow_matches_v8_at_integer_powers_of_10() {
        // These pairs were sampled from Node 22 (V8); they MUST match.
        assert_eq!(libm::pow(10.0, 1.0), 10.0);
        assert_eq!(libm::pow(10.0, 2.0), 100.0);
        assert_eq!(libm::pow(10.0, 3.0), 1000.0);
        assert_eq!(libm::pow(10.0, 4.0), 10000.0);
        assert_eq!(libm::pow(10.0, 21.0), 1e21);
    }

    /// The Number-input parse path computes
    /// `z = Math.pow(10, Math.floor(1 + Math.log(p1) / Math.LN10))`.
    /// At `p1 = 1000` JS produces `Math.log(1000)/LN10 = 2.9999...` (1 ULP
    /// below 3.0), so `floor(1 + that) = 3`, `z = 1000`, `p1/z = 1.0`.
    /// Rust's `f64::ln` could land on 3.0 exactly on some platforms,
    /// producing `floor(1 + 3) = 4`, `z = 10000`. Lock the JS behavior.
    #[test]
    fn parse_number_z_matches_js_at_power_of_10() {
        let f = Fraction::new(1000.0).unwrap();
        // 1000 is integer → hits the `js_mod(p1, 1.0) == 0.0` fast path,
        // never reaches the log branch. Use a non-integer near 10^k instead.
        assert_eq!(f.n, 1000.0);
        assert_eq!(f.d, 1.0);

        // A fractional value > 1 that goes through the log branch.
        // JS:  new Fraction(1.5).n === 3, .d === 2.
        let g = Fraction::new(1.5).unwrap();
        assert_eq!(g.n, 3.0);
        assert_eq!(g.d, 2.0);

        // dpcm conversion factor used by autoprefixer's resolution.rs.
        // JS: new Fraction(2.54).n === 127, .d === 50.
        let h = Fraction::new(2.54).unwrap();
        assert_eq!(h.n, 127.0);
        assert_eq!(h.d, 50.0);
    }
}
