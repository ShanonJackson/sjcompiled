//! crates/cssnano-postcss-convert-values
//! Byte-for-byte Rust port of `postcss-convert-values@5.1.3`.
//! See `crates/_vendor/POSTCSS_CONVERT_VALUES_5.1.3_REAUDIT.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-convert-values/src/`):
//!   - `index.js`        -> `src/lib.rs` (this file — plugin entry)
//!   - `lib/convert.js`  -> `src/lib/convert.rs`
//!
//! Browserslist-aware. `pluginCreator` resolves
//! `browsers = browserslist(null, { stats, path: __dirname, env })` once;
//! the result is consumed only via `.includes('ie 11')` inside
//! `shouldKeepZeroUnit`. Under the workspace's locked `browserslist@4.24.2`
//! defaults the resolved list does NOT contain `'ie 11'`, so that branch
//! never fires in practice — we still compute and pass `browsers` for
//! parity completeness.
//!
//! `OnceExit`-only — single pass, no `Once` body.

#[allow(clippy::module_inception)]
pub mod lib {
    pub mod convert;
}

use postcss_core::container::{walk_decls_mut_with_parent, DeferredMutation};
use postcss_core::node::NodeKind;
use postcss_core::{
    js_number_to_string, node_at_path, parent_path, PluginResult, Root,
};
use postcss_value_parser::parse::NodeKind as VpKind;
use postcss_value_parser::{parse as value_parse, stringify as value_stringify, walk};

use lib::convert::{convert as convert_unit, ConvertOpts};

/// `LENGTH_UNITS` — index.js:6.
const LENGTH_UNITS: &[&str] = &[
    "em", "ex", "ch", "rem", "vw", "vh", "vmin", "vmax", "cm", "mm", "q",
    "in", "pt", "pc", "px",
];

fn is_length_unit(u_lower: &str) -> bool {
    LENGTH_UNITS.iter().any(|x| *x == u_lower)
}

/// `notALength` — index.js:25. Properties that only accept percentages.
fn is_not_a_length(prop_lower: &str) -> bool {
    matches!(
        prop_lower,
        "descent-override"
            | "ascent-override"
            | "font-stretch"
            | "size-adjust"
            | "line-gap-override"
    )
}

/// `keepWhenZero` — index.js:34.
fn is_keep_when_zero(prop_lower: &str) -> bool {
    matches!(
        prop_lower,
        "stroke-dashoffset" | "stroke-width" | "line-height"
    )
}

/// `keepZeroPercent` — index.js:41.
fn is_keep_zero_percent(prop_lower: &str) -> bool {
    matches!(prop_lower, "max-height" | "height" | "min-width")
}

/// `stripLeadingDot(item)` — index.js:51.
/// Drops a leading `.` (handles invalid `.5px`-style inputs that
/// value-parser lumps into the unit field).
fn strip_leading_dot(item: &str) -> &str {
    if item.as_bytes().first() == Some(&b'.') {
        &item[1..]
    } else {
        item
    }
}

/// Plugin opts — `Options` typedef in index.d.ts. Browserslist
/// `stats`/`env` go through the resolution shim; we keep them as opaque
/// strings (currently unused by the consumer; default `creator()` call
/// from cssnano-preset-default leaves them undefined).
#[derive(Debug, Clone, Default)]
pub struct ConvertValuesOpts {
    /// `precision: boolean | number` — `Some(n)` for the numeric form
    /// (px-precision rounding), `None` for `false`/undefined.
    pub precision: Option<i32>,
    pub angle: Option<bool>,
    pub time: Option<bool>,
    pub length: Option<bool>,
}

impl From<&ConvertValuesOpts> for ConvertOpts {
    fn from(o: &ConvertValuesOpts) -> Self {
        ConvertOpts {
            time: o.time,
            length: o.length,
            angle: o.angle,
        }
    }
}

/// Plugin entry. Mirrors `pluginCreator(opts).OnceExit(css)`.
pub fn postcss_convert_values(root: &mut Root, opts: &ConvertValuesOpts) -> PluginResult {
    // Resolve browserslist once, mirroring upstream's `pluginCreator`
    // path. The pure shim makes timing irrelevant for output bytes.
    let browsers = browserslist_shim::resolve("", true);
    postcss_convert_values_with_browsers(root, opts, &browsers)
}

/// Variant exposed for tests / parity-runner that need to pin the
/// browserslist resolution to a specific set (e.g. force `ie 11` in for
/// the `keepZeroPercent` branch).
pub fn postcss_convert_values_with_browsers(
    root: &mut Root,
    opts: &ConvertValuesOpts,
    browsers: &[String],
) -> PluginResult {
    let convert_opts = ConvertOpts::from(opts);
    let precision = opts.precision;
    let browsers_has_ie11 = browsers.iter().any(|b| b == "ie 11");

    walk_decls_mut_with_parent(&mut root.root, |root_ref, path, _ctx| {
        // Read decl info up front (clone the strings we need).
        let (prop, value) = match node_at_path(root_ref, path).map(|n| &n.kind) {
            Some(NodeKind::Declaration(d)) => (d.prop.clone(), d.value.clone()),
            _ => return DeferredMutation::Keep,
        };
        let lower_prop = prop.to_lowercase();

        // index.js:139-145 — bail on flex / custom prop / not-a-length.
        if lower_prop.contains("flex")
            || lower_prop.starts_with("--")
            || is_not_a_length(&lower_prop)
        {
            return DeferredMutation::Keep;
        }

        // `shouldKeepZeroUnit(decl, browsers)` — index.js:114.
        let keep_zero_unit = should_keep_zero_unit(
            root_ref,
            path,
            &lower_prop,
            &value,
            browsers_has_ie11,
        );

        // Walk the value-parser tree and rewrite words/functions.
        let new_value = transform_value(
            &value,
            &convert_opts,
            precision,
            &lower_prop,
            keep_zero_unit,
        );

        if new_value != value {
            // Mutably write the new value via a fresh borrow.
            if let Some(parent_node) =
                postcss_core::node_at_path_mut(root_ref, parent_path(path))
            {
                if let Some(children) = parent_node.nodes_mut() {
                    let idx = postcss_core::parent_index_of(path);
                    if let Some(target) = children.get_mut(idx) {
                        if let NodeKind::Declaration(d) = &mut target.kind {
                            d.value = new_value;
                        }
                    }
                }
            }
        }
        DeferredMutation::Keep
    });
    Ok(())
}

/// `shouldKeepZeroUnit(decl, browsers)` — index.js:114.
///
/// Returns true when ANY of:
///   1. value contains `%` AND lower_prop ∈ keepZeroPercent AND browsers
///      includes `ie 11`.
///   2. parent.parent is an at-rule whose name (lowercased) is exactly
///      `keyframes` (vendor-prefixed names like `-webkit-keyframes` do
///      NOT match — replicated verbatim) AND lower_prop is
///      `stroke-dasharray`.
///   3. lower_prop ∈ keepWhenZero.
fn should_keep_zero_unit(
    root: &postcss_core::Node,
    path: &[usize],
    lower_prop: &str,
    value: &str,
    browsers_has_ie11: bool,
) -> bool {
    // Branch 1.
    if value.contains('%') && is_keep_zero_percent(lower_prop) && browsers_has_ie11 {
        return true;
    }
    // Branch 2 — decl.parent is the rule (path[..-1]); decl.parent.parent
    // is path[..-2]. We need at least 2 levels above the decl.
    if path.len() >= 2 && lower_prop == "stroke-dasharray" {
        let pp = &path[..path.len() - 2];
        if let Some(grand) = node_at_path(root, pp) {
            if let NodeKind::AtRule(at) = &grand.kind {
                // JS: `parent.parent.name.toLowerCase() === 'keyframes'`
                // — strict-equal, no vendor-prefix tolerance.
                if at.name.to_lowercase() == "keyframes" {
                    return true;
                }
            }
        }
    }
    // Branch 3.
    is_keep_when_zero(lower_prop)
}

/// `decl.value = valueParser(decl.value).walk(cb).toString()` — the
/// outer transformation. Mirrors index.js:147-180.
fn transform_value(
    value: &str,
    convert_opts: &ConvertOpts,
    precision: Option<i32>,
    lower_prop: &str,
    keep_zero_unit: bool,
) -> String {
    let mut nodes = value_parse(value);
    walk(
        &mut nodes,
        |node, _idx| {
            // `lowerCasedValue = node.value.toLowerCase()`.
            let lower_value = node.value.to_lowercase();
            match node.kind {
                VpKind::Word => {
                    parse_word(node, convert_opts, precision, keep_zero_unit);
                    if lower_prop == "opacity" || lower_prop == "shape-image-threshold" {
                        clamp_opacity(node);
                    }
                    Some(true)
                }
                VpKind::Function => {
                    if matches!(
                        lower_value.as_str(),
                        "calc" | "min" | "max" | "clamp" | "hsl" | "hsla"
                    ) {
                        // Inner walk with keepZeroUnit = true; every word
                        // inside passes parseWord(n, opts, true). Then
                        // RETURN FALSE to prevent the outer walk from
                        // descending again.
                        walk(
                            &mut node.nodes,
                            |n, _| {
                                if n.kind == VpKind::Word {
                                    parse_word(n, convert_opts, precision, true);
                                }
                                Some(true)
                            },
                            false,
                        );
                        Some(false)
                    } else if lower_value == "url" {
                        // Don't touch URL arguments.
                        Some(false)
                    } else {
                        // Descend normally.
                        Some(true)
                    }
                }
                _ => Some(true),
            }
        },
        false,
    );
    value_stringify(&nodes)
}

/// `parseWord(node, opts, keepZeroUnit)` — index.js:65.
fn parse_word(
    node: &mut postcss_value_parser::Node,
    convert_opts: &ConvertOpts,
    precision: Option<i32>,
    keep_zero_unit: bool,
) {
    let pair = postcss_value_parser::parse_unit(&node.value);
    let Some(pair) = pair else { return };
    // `Number(pair.number)` — JS double parse. `pair.number` from
    // value-parser is always a complete numeric token (including any
    // exponent), so `Number` and Rust `f64::from_str` agree.
    let num: f64 = pair.number.parse().unwrap_or(f64::NAN);
    let u = strip_leading_dot(&pair.unit).to_string();

    if num == 0.0 {
        // `0 + ((keepZeroUnit || (!LENGTH_UNITS.has(u.toLowerCase()) && u !== '%')) ? u : '')`
        let lower_u = u.to_lowercase();
        let keep_unit = keep_zero_unit || (!is_length_unit(&lower_u) && u != "%");
        let mut out = String::from("0");
        if keep_unit {
            out.push_str(&u);
        }
        node.value = out;
    } else {
        let mut new_value = convert_unit(num, &u, convert_opts);
        // px-precision branch — index.js:79-87.
        if let Some(prec_int) = precision {
            if u.to_lowercase() == "px" && pair.number.contains('.') {
                let precision_mult = 10f64.powi(prec_int);
                // `parseFloat(node.value)` — node.value is `new_value`.
                // JS parseFloat extracts the leading numeric prefix; Rust
                // doesn't have an equivalent built-in, so split off the
                // unit suffix using parse_unit.
                let parsed_num = match postcss_value_parser::parse_unit(&new_value) {
                    Some(p) => p.number.parse::<f64>().unwrap_or(f64::NAN),
                    None => f64::NAN,
                };
                let rounded = js_math_round(parsed_num * precision_mult) / precision_mult;
                new_value = js_number_to_string(rounded);
                new_value.push_str(&u);
            }
        }
        node.value = new_value;
    }
}

/// `Math.round(n)` — half-toward-+∞ (NOT half-away-from-zero like Rust
/// `f64::round`). Diverges on `-0.5`/`-1.5`/`-2.5`/etc.
fn js_math_round(n: f64) -> f64 {
    if n.is_nan() || n.is_infinite() {
        return n;
    }
    (n + 0.5).floor()
}

/// `clampOpacity(node)` — index.js:96.
fn clamp_opacity(node: &mut postcss_value_parser::Node) {
    let pair = postcss_value_parser::parse_unit(&node.value);
    let Some(pair) = pair else { return };
    let num: f64 = pair.number.parse().unwrap_or(f64::NAN);
    if num > 1.0 {
        // `pair.unit === '%' ? num + pair.unit : 1 + pair.unit`.
        let mut out = if pair.unit == "%" {
            js_number_to_string(num)
        } else {
            String::from("1")
        };
        out.push_str(&pair.unit);
        node.value = out;
    } else if num < 0.0 {
        let mut out = String::from("0");
        out.push_str(&pair.unit);
        node.value = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_convert_values(&mut root, &ConvertValuesOpts::default()).unwrap();
        stringify(&root)
    }

    fn run_with(css: &str, opts: ConvertValuesOpts) -> String {
        let mut root = parse(css).unwrap();
        postcss_convert_values(&mut root, &opts).unwrap();
        stringify(&root)
    }

    #[test]
    fn ms_to_s() {
        let out = run("a { transition: 1000ms }");
        assert!(out.contains(" 1s "), "got: {out}");
    }

    #[test]
    fn pc_to_pt() {
        // 1pc = 16px = 12pt → "12pt" (4) tied with "16px" (4); reduce ties
        // to b, so "12pt".
        let out = run("a { width: 1pc }");
        assert!(out.contains(" 12pt"), "got: {out}");
    }

    #[test]
    fn zero_strips_length_unit() {
        let out = run("a { margin: 0px }");
        assert!(out.contains(" 0 "), "got: {out}");
    }

    #[test]
    fn zero_keeps_unit_for_unknown() {
        let out = run("a { grid-template-columns: 0fr }");
        assert!(out.contains(" 0fr"), "got: {out}");
    }

    #[test]
    fn zero_strips_percent() {
        let out = run("a { width: 0% }");
        assert!(out.contains(" 0 "), "got: {out}");
    }

    #[test]
    fn keep_when_zero_line_height() {
        let out = run("a { line-height: 0px }");
        assert!(out.contains(" 0px"), "got: {out}");
    }

    #[test]
    fn flex_prop_skipped() {
        let out = run("a { flex-basis: 1000ms }");
        assert!(out.contains(" 1000ms"), "got: {out}");
    }

    #[test]
    fn custom_prop_skipped() {
        let out = run("a { --foo: 1000ms }");
        assert!(out.contains(" 1000ms"), "got: {out}");
    }

    #[test]
    fn calc_inner_words_processed() {
        let out = run("a { width: calc(1000ms + 1pc) }");
        assert!(out.contains("1s"), "got: {out}");
        assert!(out.contains("12pt"), "got: {out}");
    }

    #[test]
    fn url_argument_skipped() {
        let out = run("a { background: url(0px) }");
        assert!(out.contains("url(0px)"), "got: {out}");
    }

    #[test]
    fn opacity_clamps_above_one() {
        let out = run("a { opacity: 1.5 }");
        assert!(out.contains(" 1 "), "got: {out}");
    }

    #[test]
    fn opacity_clamps_below_zero() {
        let out = run("a { opacity: -.3 }");
        assert!(out.contains(" 0 "), "got: {out}");
    }

    #[test]
    fn opacity_percent_keeps_value() {
        let out = run("a { opacity: 150% }");
        assert!(out.contains(" 150% "), "got: {out}");
    }

    #[test]
    fn precision_rounding_px() {
        let out = run_with(
            "a { width: 1.111px }",
            ConvertValuesOpts {
                precision: Some(2),
                ..Default::default()
            },
        );
        assert!(out.contains(" 1.11px"), "got: {out}");
    }

    #[test]
    fn precision_skipped_for_non_px() {
        let out = run_with(
            "a { width: 1.111em }",
            ConvertValuesOpts {
                precision: Some(2),
                ..Default::default()
            },
        );
        assert!(out.contains(" 1.111em"), "got: {out}");
    }

    #[test]
    fn precision_no_dot_skipped() {
        let out = run_with(
            "a { width: 1px }",
            ConvertValuesOpts {
                precision: Some(2),
                ..Default::default()
            },
        );
        assert!(out.contains(" 1px"), "got: {out}");
    }

    #[test]
    fn keyframes_stroke_dasharray_keeps_unit() {
        let css = "@keyframes foo { 0% { stroke-dasharray: 0px } }";
        let out = run(css);
        assert!(out.contains("0px"), "got: {out}");
    }

    #[test]
    fn vendor_keyframes_does_not_match() {
        // `-webkit-keyframes` does NOT lowercase to `keyframes`, so the
        // strict-equal compare fails → 0px is stripped to 0.
        let css = "@-webkit-keyframes foo { 0% { stroke-dasharray: 0px } }";
        let out = run(css);
        assert!(out.contains(" 0 "), "got: {out}");
    }

    #[test]
    fn font_stretch_skipped() {
        let out = run("@font-face { font-stretch: 1000ms }");
        assert!(out.contains(" 1000ms"), "got: {out}");
    }

    #[test]
    fn leading_zero_strip() {
        let out = run("a { width: 0.5em }");
        assert!(out.contains(" .5em"), "got: {out}");
    }

    #[test]
    fn negative_leading_zero_strip() {
        let out = run("a { margin: -0.5em }");
        assert!(out.contains(" -.5em"), "got: {out}");
    }
}
