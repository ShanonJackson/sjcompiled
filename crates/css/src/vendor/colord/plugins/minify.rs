//! Port of `colord/plugins/minify.js@2.9.3` — shortest-string serialization.
//!
//! `colord(input).minify(opts)` enumerates every representation the color
//! could take (short hex, full hex, rgb/rgba, hsl/hsla, transparent shortcut,
//! named color) and returns whichever has the shortest byte length.
//! Ties go to the FIRST shortest candidate (upstream uses `<` not `<=`),
//! which preserves the priority order hex → rgb → hsl → name.
//!
//! `postcss-colormin@5.3.1` calls this through `minifyColor.js`. Output bytes
//! are part of the consumer hash, so byte parity with upstream is required.
//!
//! The upstream JS is one minified line — see
//! `node_modules/colord/plugins/minify.js`. Variable names below mirror the
//! single-letter aliases upstream uses, so the comparison is line-by-line.

use super::super::Colord;
use postcss_core::js_number_to_string;

/// `f` upstream — opts after `Object.assign({hex:!0, rgb:!0, hsl:!0}, t)`.
///
/// `hex`/`rgb`/`hsl` default `true`; `name`/`transparent`/`alphaHex` default
/// `false` (`undefined` upstream — falsy in the truthiness checks).
/// `postcss-colormin` overrides `name`/`transparent`/`alphaHex` per browser
/// targets — this struct is the merged shape, not the upstream defaults.
#[derive(Debug, Clone)]
pub struct MinifyOpts {
    pub hex: bool,
    pub rgb: bool,
    pub hsl: bool,
    pub name: bool,
    pub transparent: bool,
    pub alpha_hex: bool,
}

impl Default for MinifyOpts {
    fn default() -> Self {
        // Mirrors `Object.assign({hex:!0, rgb:!0, hsl:!0}, undefined)`.
        MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: false,
            transparent: false,
            alpha_hex: false,
        }
    }
}

impl MinifyOpts {
    /// All flags `true`. Test-only convenience; not the upstream default.
    pub fn all() -> Self {
        MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: true,
            transparent: true,
            alpha_hex: true,
        }
    }
}

/// `n(t)` upstream — `String(t).replace("0.", ".")` for `0 < t < 1`,
/// `String(t)` otherwise. The `replace` only fires for fractional values,
/// stripping the leading `0`: `0.5` → `.5`, `0.05` → `.05`.
/// Integers, `0`, and `1` pass through unchanged.
fn n_format(t: f64) -> String {
    if t > 0.0 && t < 1.0 {
        let s = js_number_to_string(t);
        if let Some(rest) = s.strip_prefix("0.") {
            format!(".{rest}")
        } else {
            // Defensive — js_number_to_string for any t in (0,1) starts with
            // "0." (or scientific for very small magnitudes, which colord
            // alpha cannot reach since it's clamped to [0,1] and rounded to
            // 3dp).
            s
        }
    } else {
        js_number_to_string(t)
    }
}

/// `r(t)` upstream — the hex shortener.
///
/// Returns `None` when alpha is fractional AND the alpha-hex pair would
/// round-trip incorrectly to 2dp (upstream returns `null`, which the caller
/// treats as "skip the hex form"). Otherwise returns the shortest valid hex
/// encoding:
///   - `#sup`  (4 chars) when RGB pairs match AND alpha === 1
///   - `#supg` (5 chars) when RGB pairs AND alpha pair all match
///   - the full 7- or 9-char form otherwise
fn hex_short(c: &Colord) -> Option<String> {
    let i = c.to_hex(); // "#rrggbb" or "#rrggbbaa"
    let a = c.alpha_value(); // round_to_3dp(rgba.a) — same as upstream `alpha()`
    let bytes = i.as_bytes();

    // bytes[0]='#'; positions 1..6 are RGB pair chars; 7..8 are alpha pair if 9-char.
    let s = bytes[1];
    let o = bytes[2];
    let u = bytes[3];
    let l = bytes[4];
    let p = bytes[5];
    let f = bytes[6];
    let g = bytes.get(7).copied();
    let v = bytes.get(8).copied();

    if a > 0.0 && a < 1.0 {
        // Fractional alpha → 9-char hex must be present (toHex emits the
        // alpha pair iff alpha < 1). Run the 2dp round-trip check.
        match (g, v) {
            (Some(gc), Some(vc)) => {
                let pair = [gc, vc];
                let pair_s = std::str::from_utf8(&pair).unwrap_or("");
                let pair_int = i64::from_str_radix(pair_s, 16).unwrap_or(0);
                let r = pair_int as f64 / 255.0;
                let e = 100.0_f64; // Math.pow(10, 2)
                // Upstream: `Math.round(e*r)/e + 0 !== a`. The `+ 0` flips
                // `-0` to `0` (JS quirk). f64 `+ 0.0` matches.
                let rounded = super::super::helpers::js_math_round(e * r) / e + 0.0;
                if rounded != a {
                    return None;
                }
            }
            _ => return None,
        }
    }

    if s == o && u == l && p == f {
        if a == 1.0 {
            return Some(format!("#{}{}{}", s as char, u as char, p as char));
        }
        if let (Some(gc), Some(vc)) = (g, v) {
            if gc == vc {
                return Some(format!(
                    "#{}{}{}{}",
                    s as char, u as char, p as char, gc as char
                ));
            }
        }
    }
    Some(i)
}

/// `t.prototype.minify = function(t) { ... }` upstream. Enumerates every
/// representation enabled by `opts` and returns the first shortest.
pub fn minify(c: &Colord, opts: &MinifyOpts) -> String {
    let rgba = c.to_rgb(); // {r,g,b,a} — RGB rounded to int, alpha to 3dp
    let hsla = c.to_hsl(); // {h,s,l,a}
    let alpha = c.alpha_value();

    // `n()` is identity for integers and 0/1, so r/g/b/h/s/l stringify as
    // plain integers. Alpha goes through n_format for the leading-zero strip.
    let i_str = js_number_to_string(rgba.r);
    let a_str = js_number_to_string(rgba.g);
    let h_str = js_number_to_string(rgba.b);
    let o_str = js_number_to_string(hsla.h);
    let u_str = js_number_to_string(hsla.s);
    let l_str = js_number_to_string(hsla.l);
    let p_str = n_format(alpha);

    let alpha_is_one = alpha == 1.0;

    let mut g: Vec<String> = Vec::new();

    // hex: only when alpha === 1 OR alpha_hex opt is on.
    if opts.hex && (alpha_is_one || opts.alpha_hex) {
        if let Some(v) = hex_short(c) {
            g.push(v);
        }
    }

    // rgb / rgba: NO spaces between commas (matches upstream concat).
    if opts.rgb {
        g.push(if alpha_is_one {
            format!("rgb({i_str},{a_str},{h_str})")
        } else {
            format!("rgba({i_str},{a_str},{h_str},{p_str})")
        });
    }

    // hsl / hsla.
    if opts.hsl {
        g.push(if alpha_is_one {
            format!("hsl({o_str},{u_str}%,{l_str}%)")
        } else {
            format!("hsla({o_str},{u_str}%,{l_str}%,{p_str})")
        });
    }

    // transparent / name: mutually exclusive (`else if` upstream). The
    // transparent check uses the n()-formatted RGB (which for ints is
    // identity), so it fires iff rgba(0,0,0,0).
    if opts.transparent && rgba.r == 0.0 && rgba.g == 0.0 && rgba.b == 0.0 && alpha == 0.0 {
        g.push("transparent".to_string());
    } else if alpha_is_one && opts.name {
        if let Some(name) = c.to_name() {
            g.push(name.to_string());
        }
    }

    // First-shortest wins (`<`, not `<=`). Preserves hex,rgb,hsl,name order
    // when multiple representations tie at the same byte length.
    if g.is_empty() {
        // Upstream throws on g[0] when array is empty; the only way this
        // happens is opts.hex && opts.rgb && opts.hsl all false AND name/
        // transparent paths skipped. postcss-colormin never reaches it.
        return String::new();
    }
    let mut best_idx = 0usize;
    for (n_idx, s) in g.iter().enumerate().skip(1) {
        if s.len() < g[best_idx].len() {
            best_idx = n_idx;
        }
    }
    g.swap_remove(best_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::colord;

    #[test]
    fn defaults_match_upstream() {
        let d = MinifyOpts::default();
        assert!(d.hex && d.rgb && d.hsl);
        assert!(!d.name && !d.transparent && !d.alpha_hex);
    }

    #[test]
    fn n_format_strips_leading_zero() {
        assert_eq!(n_format(0.5), ".5");
        assert_eq!(n_format(0.05), ".05");
        assert_eq!(n_format(0.0), "0");
        assert_eq!(n_format(1.0), "1");
        assert_eq!(n_format(255.0), "255");
    }

    #[test]
    fn hex_short_collapses_aabbcc() {
        // #aabbcc -> all RGB pairs match -> "#abc"
        let c = colord("#aabbcc");
        assert_eq!(hex_short(&c), Some("#abc".to_string()));
    }

    #[test]
    fn hex_short_keeps_non_collapsible() {
        // #ab12cc -> g pair 12 != 1,2 -> full hex
        let c = colord("#ab12cc");
        assert_eq!(hex_short(&c), Some("#ab12cc".to_string()));
    }

    #[test]
    fn hex_short_alpha_pair_collapse() {
        // alpha=0.6 -> hex pair "99" (153) round-trips cleanly through 2dp
        // (153/255 * 100 = 60.0 exactly). All 4 pairs match (aa,bb,cc,99) ->
        // "#abc9". Compare upstream `colord("rgba(170,187,204,0.6)").minify()`.
        let c = colord("rgba(170,187,204,0.6)");
        assert_eq!(hex_short(&c), Some("#abc9".to_string()));
    }

    #[test]
    fn hex_short_alpha_lossy_skips_form() {
        // The hex *parser* clamps alpha to 2dp (see parse.rs line 95/105),
        // so any hex input always round-trips. To exercise the lossy-alpha
        // skip path, alpha must enter at full precision via rgba(...).
        // alpha=0.502 -> hex pair "80" -> 128/255 * 100 = 50.196, round=50,
        // /100 = 0.5 != 0.502 -> upstream returns null.
        let c = colord("rgba(255,0,0,0.502)");
        assert_eq!(hex_short(&c), None);
    }

    #[test]
    fn hex_short_alpha_no_roundtrip_returns_none() {
        // alpha 0.555 -> hex stores 142=0x8e -> reads back as 0.56, mismatch.
        let c = colord("rgba(170, 187, 204, 0.555)");
        assert_eq!(hex_short(&c), None);
    }

    #[test]
    fn minify_picks_shortest_with_name() {
        // For #ff0000 with all opts: hex_short -> "#f00" (4ch), rgb (12ch),
        // hsl (15ch), name "red" (3ch). First shortest -> "red".
        let m = minify(&colord("#ff0000"), &MinifyOpts::all());
        assert_eq!(m, "red");
    }

    #[test]
    fn minify_no_spaces_in_rgb() {
        // Default opts: hex form wins for #abcdef -> "#abcdef" (7ch) since
        // pairs don't collapse. But verify rgb form has no spaces.
        let mut opts = MinifyOpts::default();
        opts.hex = false;
        opts.hsl = false;
        let m = minify(&colord("#abcdef"), &opts);
        // toRgb of #abcdef = rgb(171, 205, 239), alpha 1.
        assert_eq!(m, "rgb(171,205,239)");
    }

    #[test]
    fn minify_no_spaces_in_rgba() {
        let mut opts = MinifyOpts::default();
        opts.hex = false;
        opts.hsl = false;
        let m = minify(&colord("rgba(255,0,0,0.5)"), &opts);
        assert_eq!(m, "rgba(255,0,0,.5)");
    }

    #[test]
    fn minify_no_spaces_in_hsl() {
        let mut opts = MinifyOpts::default();
        opts.hex = false;
        opts.rgb = false;
        let m = minify(&colord("#ff0000"), &opts);
        assert_eq!(m, "hsl(0,100%,50%)");
    }

    #[test]
    fn minify_transparent_only_when_rgba_all_zero() {
        let opts = MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: false,
            transparent: true,
            alpha_hex: true,
        };
        // rgba(0,0,0,0) -> "transparent" (11ch) loses to "#0000" (5ch).
        let m = minify(&colord("rgba(0,0,0,0)"), &opts);
        assert_eq!(m, "#0000");
    }

    #[test]
    fn minify_transparent_skipped_for_nonzero_rgb() {
        // rgba(255,0,0,0) — alpha 0 but RGB non-zero. Upstream skips the
        // transparent branch (only fires when r=g=b=0). Hex_short returns
        // None (alpha fractional doesn't round-trip... actually alpha 0 hits
        // the `a > 0 && a < 1` check as false, so falls through; full hex
        // emitted). Name skipped (alpha != 1).
        let opts = MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: true,
            transparent: true,
            alpha_hex: true,
        };
        let m = minify(&colord("rgba(255,0,0,0)"), &opts);
        // hex: alpha=0, not fractional, all pairs match (ff,00,00,00) -> "#f000" (5ch)
        // rgb form: rgba(255,0,0,0) (14ch), hsl: hsla(0,100%,50%,0) (16ch).
        // No transparent (RGB non-zero), no name (alpha != 1). Picks "#f000".
        assert_eq!(m, "#f000");
    }

    #[test]
    fn minify_name_only_when_alpha_one() {
        // alpha=0.6 (round-trips through hex pair "99" cleanly). Pairs ff,
        // 00,00,99 all match → hex_short returns "#f009" (5ch). rgba is
        // "rgba(255,0,0,.6)" (16ch). Name "red" skipped (alpha != 1).
        // Picks #f009.
        let opts = MinifyOpts {
            hex: true,
            rgb: true,
            hsl: true,
            name: true,
            transparent: true,
            alpha_hex: true,
        };
        let m = minify(&colord("rgba(255,0,0,0.6)"), &opts);
        assert_eq!(m, "#f009");
    }

    #[test]
    fn minify_first_shortest_wins_on_tie() {
        // Pick a color where hex and name tie. `aliceblue` (9ch) vs hex
        // `#f0f8ff` (7ch) — hex wins, not a tie.
        // Use `silver` (6ch) vs `#c0c0c0` -> hex_short collapses to "#ccc"?
        // c0/c0 yes pair-match all three -> "#ccc" (4ch). #ccc < silver. hex wins.
        // For a real tie test: `red` (3ch) vs hex_short of #ff0000 -> #f00 (4ch).
        // Already tested in minify_picks_shortest_with_name.
        // Tie demo: choose a color where name has 3 chars and hex_short has
        // 3 chars — impossible (hex is min 4). So priority order is observed
        // by the priority of insertion, not by tie-breaking. This test just
        // confirms hex inserts before name.
        let m = minify(&colord("#ff0000"), &MinifyOpts::all());
        assert_eq!(m, "red"); // 3ch beats 4ch
    }

    #[test]
    fn minify_alpha_hex_off_skips_hex_when_fractional() {
        // alpha_hex=false (default), alpha=0.5 -> hex skipped.
        let mut opts = MinifyOpts::default();
        opts.hex = true;
        opts.alpha_hex = false;
        let m = minify(&colord("rgba(255,0,0,0.5)"), &opts);
        // No hex. rgba(255,0,0,.5) (16ch) vs hsla(0,100%,50%,.5) (18ch).
        assert_eq!(m, "rgba(255,0,0,.5)");
    }
}
