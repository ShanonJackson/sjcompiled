//! Port of `postcss-convert-values/src/lib/convert.js`.
//!
//! Three insertion-ordered unit→multiplier maps and one entry point
//! (`convert(number, unit, opts)`). Picks the shortest stringification
//! across compatible units; ties go to the LATER candidate per
//! upstream's `reduce((a,b) => a.length < b.length ? a : b)`.

use postcss_core::js_number_to_string;

/// Insertion order matters — replicates JS `new Map([...])` literal order.
/// Length: in→96, px→1, pt→4/3, pc→16.
fn length_conv() -> &'static [(&'static str, f64)] {
    &[("in", 96.0), ("px", 1.0), ("pt", 4.0 / 3.0), ("pc", 16.0)]
}

/// Time: s→1000, ms→1.
fn time_conv() -> &'static [(&'static str, f64)] {
    &[("s", 1000.0), ("ms", 1.0)]
}

/// Angle: turn→360, deg→1.
fn angle_conv() -> &'static [(&'static str, f64)] {
    &[("turn", 360.0), ("deg", 1.0)]
}

fn map_get(map: &[(&str, f64)], key: &str) -> Option<f64> {
    map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn map_has(map: &[(&str, f64)], key: &str) -> bool {
    map.iter().any(|(k, _)| *k == key)
}

/// `dropLeadingZero(number)` — convert.js:22.
///
/// JS:
/// ```js
/// const value = String(number);
/// if (number % 1) {
///   if (value[0] === '0')                    return value.slice(1);
///   if (value[0] === '-' && value[1] === '0') return '-' + value.slice(2);
/// }
/// return value;
/// ```
///
/// `number % 1` is truthy iff non-zero remainder (i.e. number is non-
/// integer finite). NaN/Inf propagate to `false` because `NaN % 1` is
/// `NaN` (falsy) and `Infinity % 1` is `NaN`.
pub fn drop_leading_zero(number: f64) -> String {
    let value = js_number_to_string(number);
    let bytes = value.as_bytes();
    // `number % 1` truthiness: non-integer finite.
    let has_remainder = number.is_finite() && (number % 1.0) != 0.0;
    if has_remainder {
        if !bytes.is_empty() && bytes[0] == b'0' {
            return value[1..].to_string();
        }
        if bytes.len() >= 2 && bytes[0] == b'-' && bytes[1] == b'0' {
            let mut s = String::from("-");
            s.push_str(&value[2..]);
            return s;
        }
    }
    value
}

/// `transform(number, originalUnit, conversions)` — convert.js:43.
///
/// Generates candidate strings for every conversion unit other than
/// `original_unit`, picks the SHORTEST. JS reduce tie-break favors the
/// LATER candidate (because `a.length < b.length` is strict).
fn transform_internal(number: f64, original_unit: &str, conversions: &[(&str, f64)]) -> String {
    let conversion_units: Vec<&str> = conversions
        .iter()
        .map(|(k, _)| *k)
        .filter(|u| *u != original_unit)
        .collect();
    let base = number * map_get(conversions, original_unit).unwrap();
    let candidates: Vec<String> = conversion_units
        .iter()
        .map(|u| {
            let mult = map_get(conversions, u).unwrap();
            let mut s = drop_leading_zero(base / mult);
            s.push_str(u);
            s
        })
        .collect();
    // `reduce((a, b) => a.length < b.length ? a : b)` — tie → b.
    let mut iter = candidates.into_iter();
    let first = iter.next().expect("conversions has >1 entry");
    iter.fold(first, |a, b| if a.len() < b.len() { a } else { b })
}

/// `module.exports = function (number, unit, { time, length, angle })`
/// — convert.js:64. `time/length/angle` come from the plugin opts; only
/// the explicit `false` value disables a branch (undefined enables).
pub fn convert(number: f64, unit: &str, opts: &ConvertOpts) -> String {
    let mut value = drop_leading_zero(number);
    if !unit.is_empty() {
        value.push_str(unit);
    }
    let mut converted: Option<String> = None;
    let lc = unit.to_lowercase();
    if opts.length != Some(false) && map_has(length_conv(), &lc) {
        converted = Some(transform_internal(number, &lc, length_conv()));
    }
    if opts.time != Some(false) && map_has(time_conv(), &lc) {
        converted = Some(transform_internal(number, &lc, time_conv()));
    }
    if opts.angle != Some(false) && map_has(angle_conv(), &lc) {
        converted = Some(transform_internal(number, &lc, angle_conv()));
    }
    if let Some(c) = converted {
        if c.len() < value.len() {
            value = c;
        }
    }
    value
}

/// The three optional booleans the plugin's outer opts struct holds.
/// Only `Some(false)` disables a branch.
#[derive(Debug, Clone, Default)]
pub struct ConvertOpts {
    pub time: Option<bool>,
    pub length: Option<bool>,
    pub angle: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_leading_zero_integer() {
        assert_eq!(drop_leading_zero(0.0), "0");
        assert_eq!(drop_leading_zero(5.0), "5");
        assert_eq!(drop_leading_zero(-3.0), "-3");
    }

    #[test]
    fn drop_leading_zero_decimal() {
        assert_eq!(drop_leading_zero(0.5), ".5");
        assert_eq!(drop_leading_zero(-0.5), "-.5");
        assert_eq!(drop_leading_zero(0.125), ".125");
    }

    #[test]
    fn drop_leading_zero_keeps_nonzero_lead() {
        assert_eq!(drop_leading_zero(1.5), "1.5");
        assert_eq!(drop_leading_zero(-1.5), "-1.5");
    }

    #[test]
    fn transform_picks_shortest_with_tie_to_later() {
        // 96px (filter px out): in=1in(3), pt=72pt(4), pc=6pc(3).
        // reduce: "1in" vs "72pt" → 3<4 keep "1in"; "1in" vs "6pc" →
        // 3<3 false → take b "6pc". Tie favors LATER candidate.
        let c = transform_internal(96.0, "px", length_conv());
        assert_eq!(c, "6pc");
    }

    #[test]
    fn transform_pc_picks_pt_after_ties() {
        // 1pc → base=16. Filter pc out. Candidates:
        //   in:  16/96 = 0.1666… → ".16666666666666666in" (long)
        //   px:  16    → "16px" (4)
        //   pt:  16/(4/3) = 12 → "12pt" (4)
        // reduce: long vs "16px" (4 < long) → "16px"; "16px" vs "12pt"
        // (4<4 false) → "12pt".
        let c = transform_internal(1.0, "pc", length_conv());
        assert_eq!(c, "12pt");
    }

    #[test]
    fn convert_enables_by_default() {
        // 1000ms → "1s" (2 < "1000ms" 6).
        let c = convert(1000.0, "ms", &ConvertOpts::default());
        assert_eq!(c, "1s");
    }

    #[test]
    fn convert_disables_when_false() {
        // time: false → keep "1000ms".
        let c = convert(
            1000.0,
            "ms",
            &ConvertOpts { time: Some(false), ..Default::default() },
        );
        assert_eq!(c, "1000ms");
    }

    #[test]
    fn convert_no_match_passthrough() {
        // em is not in any conv table → original.
        let c = convert(1.0, "em", &ConvertOpts::default());
        assert_eq!(c, "1em");
    }

    #[test]
    fn convert_zero_unitless() {
        let c = convert(0.0, "", &ConvertOpts::default());
        assert_eq!(c, "0");
    }

    #[test]
    fn convert_keeps_original_when_no_shorter() {
        // 1px → candidates: 1/96 in, 1/(4/3) pt, 1/16 pc — none shorter
        // than "1px".
        let c = convert(1.0, "px", &ConvertOpts::default());
        assert_eq!(c, "1px");
    }

    #[test]
    fn convert_case_insensitive_lookup() {
        let c = convert(1000.0, "MS", &ConvertOpts::default());
        // ms branch matches via lowercased lookup; converted shorter than
        // value.length — but value uses unit verbatim ("MS"). So converted
        // is "1s" (2), value "1000MS" (6) → take converted.
        assert_eq!(c, "1s");
    }

    #[test]
    fn drop_leading_zero_negative_zero() {
        // String(-0) === "0" in JS (per js_number_to_string).
        assert_eq!(drop_leading_zero(-0.0), "0");
    }
}
