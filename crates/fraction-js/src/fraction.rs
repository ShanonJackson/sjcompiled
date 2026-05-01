//! Port of `fraction.js@4.2.0`'s sole source file `fraction.js`.
//!
//! Upstream uses JS numbers (f64) for `s`, `n`, `d`. We keep that to preserve
//! arithmetic semantics including overflow / NaN propagation. Methods that take
//! one or two parameters in JS are exposed via the [`FractionInput`] enum which
//! reproduces the upstream `parse(p1, p2)` overload set.

use std::fmt;

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
fn gcd(mut a: f64, mut b: f64) -> f64 {
    if a == 0.0 { return b; }
    if b == 0.0 { return a; }
    loop {
        a = js_mod(a, b);
        if a == 0.0 { return b; }
        b = js_mod(b, a);
        if b == 0.0 { return a; }
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
            out.push(s[i..i + ch_len].to_string());
            i += ch_len;
        }
    }
    out
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
                    z = (10.0_f64).powf((1.0 + (p1.ln() / std::f64::consts::LN_10)).floor());
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
            } else if (a + 1 < b.len() && b[a + 1] == ".") || b[a] == "." {
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
                        y = (10.0_f64).powi(tok.len() as i32);
                        a += 1;
                    }
                }
                if a + 2 < b.len() && ((b[a] == "(" && b[a + 2] == ")") || (b[a] == "'" && b[a + 2] == "'")) {
                    let tok = b[a + 1].clone();
                    x = assign(&tok, s)?;
                    z = (10.0_f64).powi(tok.len() as i32) - 1.0;
                    a += 3;
                }
            } else if a + 1 < b.len() && (b[a + 1] == "/" || b[a + 1] == ":") {
                let tok = b[a].clone();
                w = assign(&tok, s)?;
                let tok2 = b[a + 2].clone();
                y = assign(&tok2, 1.0)?;
                a += 3;
            } else if a + 3 < b.len() && b[a + 3] == "/" && b[a + 1] == " " {
                let tok = b[a].clone();
                v = assign(&tok, s)?;
                let tok2 = b[a + 2].clone();
                w = assign(&tok2, s)?;
                let tok3 = b[a + 4].clone();
                y = assign(&tok3, 1.0)?;
                a += 5;
            }

            if b.len() <= a {
                d = y * z;
                n = x + d * v + z * w;
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
        let places = (10.0_f64).powi(places.unwrap_or(0));
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        new_fraction((places * self.s * self.n / self.d).ceil(), places)
    }

    pub fn floor(&self, places: Option<i32>) -> Result<Self, FractionError> {
        let places = (10.0_f64).powi(places.unwrap_or(0));
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        new_fraction((places * self.s * self.n / self.d).floor(), places)
    }

    pub fn round(&self, places: Option<i32>) -> Result<Self, FractionError> {
        let places = (10.0_f64).powi(places.unwrap_or(0));
        if self.n.is_nan() || self.d.is_nan() {
            return Ok(Fraction { s: 1.0, n: f64::NAN, d: f64::NAN });
        }
        // JS Math.round rounds half away from zero for positives, half-up.
        // Rust f64::round rounds half away from zero, matching Math.round for positives;
        // but Math.round in JS rounds .5 toward +∞ (e.g. -0.5 -> 0). We'll use the JS rule.
        new_fraction(js_math_round(places * self.s * self.n / self.d), places)
    }

    pub fn inverse(&self) -> Result<Self, FractionError> {
        new_fraction(self.s * self.d, self.n)
    }

    pub fn pow<I: Into<FractionInput>>(&self, other: I) -> Result<Option<Self>, FractionError> {
        let p = parse_input(&other.into())?;
        if p.d == 1.0 {
            if p.s < 0.0 {
                return new_fraction(
                    (self.s * self.d).powf(p.n),
                    self.n.powf(p.n),
                ).map(Some);
            } else {
                return new_fraction(
                    (self.s * self.n).powf(p.n),
                    self.d.powf(p.n),
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
            n_acc *= k.powf(*val);
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
            d_acc *= k.powf(*val);
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
        let dec = dec.unwrap_or(15);
        let cyc_len = cycle_len(n_val, d_val);
        let cyc_off = cycle_start(n_val, d_val, cyc_len);
        let mut str_out = if self.s < 0.0 { "-".to_string() } else { String::new() };
        // `N / D | 0` — JS bitwise OR coerces to int32.
        str_out.push_str(&js_number_to_string((n_val / d_val).trunc()));
        n_val = js_mod(n_val, d_val);
        n_val *= 10.0;
        if n_val != 0.0 { str_out.push('.'); }
        if cyc_len > 0 {
            for _ in 0..cyc_off {
                str_out.push_str(&js_number_to_string((n_val / d_val).trunc()));
                n_val = js_mod(n_val, d_val);
                n_val *= 10.0;
            }
            str_out.push('(');
            for _ in 0..cyc_len {
                str_out.push_str(&js_number_to_string((n_val / d_val).trunc()));
                n_val = js_mod(n_val, d_val);
                n_val *= 10.0;
            }
            str_out.push(')');
        } else {
            for _ in 0..dec {
                if n_val == 0.0 { break; }
                str_out.push_str(&js_number_to_string((n_val / d_val).trunc()));
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
}
