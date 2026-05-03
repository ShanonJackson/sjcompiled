//! crates/cssnano-postcss-minify-gradients
//! Byte-for-byte Rust port of `postcss-minify-gradients@5.1.1`.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-minify-gradients/src/`):
//!   - `index.js`       -> `src/lib.rs` (this file).
//!   - `isColorStop.js` -> `src/is_color_stop.rs`.
//!
//! All bugs of upstream 5.1.1 are intentionally preserved. Phase 6g.
//!
//! ## Behaviour (1:1 with upstream `OnceExit(css)`)
//!
//! `css.walkDecls(optimise)`. For each decl:
//!   - bail if `value` empty.
//!   - bail if `value.toLowerCase()` contains `var(` or `env(`.
//!   - bail if `value.toLowerCase()` does not contain `gradient`.
//!   - else `decl.value = valueParser(value).walk(node => ...).toString()`.
//!
//! The walker only enters Function nodes and short-circuits on every branch
//! by returning `false` (postcss-value-parser convention: false means
//! "don't recurse into this Function's children"). The walker handles 4
//! function-name groups (case-insensitive):
//!   1. `linear-gradient` / `repeating-linear-gradient` /
//!      `-webkit-linear-gradient` / `-webkit-repeating-linear-gradient`.
//!   2. `radial-gradient` / `repeating-radial-gradient`.
//!   3. `-webkit-radial-gradient` / `-webkit-repeating-radial-gradient`.
//! Non-gradient functions are walked into (no early `false`); see
//! upstream behaviour where the function callback only returns `false`
//! for the gradient branches.

pub mod is_color_stop;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};
use postcss_value_parser::parse::{Node as VNode, NodeKind as VKind};
use postcss_value_parser::{parse as vp_parse, parse_unit, stringify as vp_stringify};

use crate::is_color_stop::is_color_stop;

// ---------------------------------------------------------------------------
// Constants — mirror upstream module-level `angles` map.
// ---------------------------------------------------------------------------

fn angle_for(side: &str) -> Option<&'static str> {
    match side {
        "top" => Some("0deg"),
        "right" => Some("90deg"),
        "bottom" => Some("180deg"),
        "left" => Some("270deg"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Helpers — mirror upstream `isLessThan` and `getArguments`.
// ---------------------------------------------------------------------------

/// Mirrors upstream `isLessThan(a, b)`:
/// `a.unit.toLowerCase() === b.unit.toLowerCase() && parseFloat(a.number) >= parseFloat(b.number)`.
/// (Yes, the function name says "isLessThan" but the body returns ≥. Upstream
/// bug; replicated verbatim — call site interprets "true" as
/// "lastStop is at or past thisStop, normalize thisStop to 0".)
fn is_less_than(a: &ParsedDim, b: &ParsedDim) -> bool {
    if a.unit.to_lowercase() != b.unit.to_lowercase() {
        return false;
    }
    let a_num: f64 = a.number.parse().unwrap_or(f64::NAN);
    let b_num: f64 = b.number.parse().unwrap_or(f64::NAN);
    // JS `>=` with NaN is always false; Rust `>=` with NaN is also false.
    a_num >= b_num
}

#[derive(Clone, Debug)]
struct ParsedDim {
    number: String,
    unit: String,
}

/// `valueParser.unit(s)` — falsy when the input doesn't begin with a number.
/// Returns `Some(ParsedDim)` for "0", "10px", "50%", etc.
fn value_parser_unit(s: &str) -> Option<ParsedDim> {
    parse_unit(s).map(|p| ParsedDim { number: p.number, unit: p.unit })
}

/// Port of `cssnano-utils::getArguments(node)` specialised for value-parser
/// children. Splits on top-level commas (`Div` with value ","). The
/// `cssnano_utils::get_arguments` generic helper is used elsewhere; this
/// thin wrapper keeps the call sites aligned with upstream's shape (the
/// returned `Vec<Vec<&mut VNode>>`-style slicing isn't borrow-safe, so we
/// instead carry index ranges and mutate by index).
#[derive(Clone, Debug)]
struct ArgRange {
    /// Indices into the function's `nodes` Vec, in upstream `arg` order.
    /// `arg.length` upstream == `indices.len()` here.
    indices: Vec<usize>,
}

fn get_arguments_indices(func_nodes: &[VNode]) -> Vec<ArgRange> {
    let mut out: Vec<ArgRange> = vec![ArgRange { indices: Vec::new() }];
    for (i, child) in func_nodes.iter().enumerate() {
        let is_div = child.kind == VKind::Div && child.value == ",";
        if is_div {
            out.push(ArgRange { indices: Vec::new() });
        } else {
            out.last_mut().unwrap().indices.push(i);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// optimise(decl) — 1:1 with upstream `optimise(decl)`.
// ---------------------------------------------------------------------------

fn optimise(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let normalized = value.to_lowercase();
    if normalized.contains("var(") || normalized.contains("env(") {
        return None;
    }
    if !normalized.contains("gradient") {
        return None;
    }

    let mut parsed = vp_parse(value);
    walk_top_functions(&mut parsed);
    Some(vp_stringify(&parsed))
}

/// postcss-value-parser `.walk(cb)` mirror, but specialised: upstream's
/// callback only ever returns `false` for Function nodes (so children are
/// never descended into) — and only acts on Function nodes in the first
/// place (returns `false` immediately for non-function or empty function).
/// That means a flat top-level scan is observably equivalent for the
/// gradient short-circuit branches.
///
/// However, for non-gradient functions the upstream callback returns
/// `undefined` (NOT `false`), so the walker recurses into the function's
/// children. To preserve that behaviour bit-for-bit (in case a nested
/// gradient sits inside a non-gradient function — `image()` /
/// `cross-fade()` etc. wrap gradients), we recurse manually.
fn walk_top_functions(nodes: &mut [VNode]) {
    for n in nodes.iter_mut() {
        if n.kind != VKind::Function {
            continue;
        }
        if n.nodes.is_empty() {
            // Upstream: `if (node.type !== 'function' || !node.nodes.length) return false;`
            continue;
        }
        let lower = n.value.to_lowercase();
        let handled = handle_gradient_function(n, &lower);
        if !handled {
            // Upstream returns implicit `undefined` for non-gradient
            // functions → walker recurses into children.
            walk_top_functions(&mut n.nodes);
        }
    }
}

/// Returns true iff this function name was a gradient and was processed.
/// Mirrors the `linear` / `radial` / `-webkit-radial` branches in
/// upstream `optimise`.
fn handle_gradient_function(n: &mut VNode, lower: &str) -> bool {
    match lower {
        "linear-gradient"
        | "repeating-linear-gradient"
        | "-webkit-linear-gradient"
        | "-webkit-repeating-linear-gradient" => {
            handle_linear(n);
            true
        }
        "radial-gradient" | "repeating-radial-gradient" => {
            handle_radial(n);
            true
        }
        "-webkit-radial-gradient" | "-webkit-repeating-radial-gradient" => {
            handle_webkit_radial(n);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Linear-gradient branch.
// ---------------------------------------------------------------------------

fn handle_linear(n: &mut VNode) {
    // Mirrors upstream:
    //   let args = getArguments(node);
    //   if (node.nodes[0].value.toLowerCase() === 'to' && args[0].length === 3) {
    //       node.nodes = node.nodes.slice(2);
    //       node.nodes[0].value = angles[node.nodes[0].value.toLowerCase()];
    //   }
    //
    // **Load-bearing detail**: `args` is computed BEFORE the slice. After
    // `node.nodes = node.nodes.slice(2)`, the JS array literal contains
    // node references; the OLD `args` array still holds them, including
    // (a) two now-orphaned references to the dropped "to" Word + Space,
    // and (b) the third reference (`args[0][2]`) which IS the angle node
    // (now living at `node.nodes[0]` and mutated to the angle string).
    // The forEach loop below reads `arg[2].value` (the angle), which is
    // the same memory.
    //
    // Our index-based port mimics this by:
    //   1. Snapshotting `args` before the slice.
    //   2. After draining 2 entries, shifting every retained index by
    //      -2 (saturating at 0). The first arg's first two indices
    //      collapse to 0 — they become aliases of the angle node — but
    //      they're never read in the forEach (only arg[1].value and
    //      arg[2].value are ever read or written, and the
    //      leading-zero-strip gate `number == "0" && unit != "deg"`
    //      can never fire on an angle (top→"0deg" satisfies number but
    //      not unit; right/bottom/left have number != "0")).
    let mut args = get_arguments_indices(&n.nodes);
    let first_value_lower = n.nodes.first().map(|c| c.value.to_lowercase()).unwrap_or_default();
    let to_rewrite =
        first_value_lower == "to" && args.first().map(|r| r.indices.len()) == Some(3);
    if to_rewrite {
        // node.nodes = node.nodes.slice(2)
        n.nodes.drain(0..2);
        // node.nodes[0].value = angles[node.nodes[0].value.toLowerCase()];
        // angles only has top/right/bottom/left → undefined for any other
        // side; JS assigning undefined produces the literal string
        // "undefined", and a downstream `.length` on the value throws
        // (an upstream crash bug — replicated only insofar as we leave
        // the literal "undefined" in the value, since panicking inside
        // the plugin would block valid inputs in the same Decl pass).
        let side_lower = n.nodes[0].value.to_lowercase();
        n.nodes[0].value = match angle_for(&side_lower) {
            Some(angle) => angle.to_string(),
            None => "undefined".to_string(),
        };
        // Shift every retained index by -2.
        for arg in args.iter_mut() {
            for idx in arg.indices.iter_mut() {
                *idx = idx.saturating_sub(2);
            }
        }
    }

    let last_arg_idx = args.len().saturating_sub(1);

    // Tracks `lastStop` across the forEach; JS assigns thisStop into it
    // even if `parse_unit` was None (false-y assignment); we mirror by
    // wrapping in an enum.
    #[derive(Clone, Debug)]
    enum LastStop {
        /// `lastStop === undefined` upstream — never assigned yet.
        Undefined,
        /// Truthy ParsedDim (assigned and was a real unit).
        Some(ParsedDim),
        /// Assigned-but-falsy (JS `valueParser.unit(...)` returned false).
        FalsyAssigned,
    }

    let mut last_stop = LastStop::Undefined;

    for (index, arg) in args.iter().enumerate() {
        // `arg.length !== 3` short-circuit upstream.
        if arg.indices.len() != 3 {
            continue;
        }
        let is_final = index == last_arg_idx;
        // arg[2].value at parse time
        let stop_idx = arg.indices[2];
        let mid_idx = arg.indices[1];

        let stop_value_now = n.nodes[stop_idx].value.clone();
        let this_stop = value_parser_unit(&stop_value_now);

        match &last_stop {
            LastStop::Undefined => {
                // First-encountered 3-token arg.
                last_stop = match &this_stop {
                    Some(p) => LastStop::Some(p.clone()),
                    None => LastStop::FalsyAssigned,
                };
                if !is_final {
                    if let LastStop::Some(p) = &last_stop {
                        // Strip leading `0<unit>` (any non-deg unit).
                        // arg[1].value = arg[2].value = '';
                        if p.number == "0" && p.unit.to_lowercase() != "deg" {
                            n.nodes[mid_idx].value = String::new();
                            n.nodes[stop_idx].value = String::new();
                        }
                    }
                }
                continue;
            }
            LastStop::Some(last_p) => {
                if let Some(this_p) = &this_stop {
                    if is_less_than(last_p, this_p) {
                        n.nodes[stop_idx].value = "0".to_string();
                    }
                }
            }
            LastStop::FalsyAssigned => {
                // `lastStop` is falsy → outer `if (lastStop && thisStop && ...)`
                // is false; skip rewrite.
            }
        }

        // Update lastStop AFTER the comparison.
        last_stop = match &this_stop {
            Some(p) => LastStop::Some(p.clone()),
            None => LastStop::FalsyAssigned,
        };

        // Final-stop 100% strip — `arg[1].value = arg[2].value = '';`
        // Re-read arg[2].value AFTER the potential rewrite above so
        // that the "100%" check sees the post-mutation string. Upstream
        // does exactly that (the `arg[2].value === '100%'` check fires
        // after the `arg[2].value = '0'` assignment).
        if is_final && n.nodes[stop_idx].value == "100%" {
            n.nodes[mid_idx].value = String::new();
            n.nodes[stop_idx].value = String::new();
        }
    }
}

// ---------------------------------------------------------------------------
// Radial-gradient branch.
// ---------------------------------------------------------------------------

fn handle_radial(n: &mut VNode) {
    let args = get_arguments_indices(&n.nodes);

    // hasAt = args[0].find(n => n.value.toLowerCase() === 'at')
    let has_at = args
        .first()
        .map(|r| r.indices.iter().any(|i| n.nodes[*i].value.to_lowercase() == "at"))
        .unwrap_or(false);

    let mut last_stop: Option<ParsedDim> = None;

    for (index, arg) in args.iter().enumerate() {
        // `if (!arg[2] || (!index && hasAt)) return;`
        if arg.indices.len() < 3 {
            continue;
        }
        if index == 0 && has_at {
            continue;
        }

        let stop_idx = arg.indices[2];
        let stop_val = n.nodes[stop_idx].value.clone();
        let this_stop = value_parser_unit(&stop_val);

        match &last_stop {
            None => {
                last_stop = this_stop.clone();
                continue;
            }
            Some(last_p) => {
                if let Some(this_p) = &this_stop {
                    if is_less_than(last_p, this_p) {
                        n.nodes[stop_idx].value = "0".to_string();
                    }
                }
            }
        }

        last_stop = this_stop;
    }
}

// ---------------------------------------------------------------------------
// -webkit-radial-gradient branch.
// ---------------------------------------------------------------------------

fn handle_webkit_radial(n: &mut VNode) {
    let args = get_arguments_indices(&n.nodes);

    let mut last_stop: Option<ParsedDim> = None;

    for arg in args.iter() {
        // Upstream computes `color` and `stop` strings for the predicate;
        // tracks two cases: arg[2] defined vs undefined.
        let color_str: String;
        let stop_str: Option<String>;

        if arg.indices.len() >= 3 {
            // arg[2] !== undefined branch.
            let c0 = &n.nodes[arg.indices[0]];
            color_str = if c0.kind == VKind::Function {
                format!("{}({})", c0.value, vp_stringify(&c0.nodes))
            } else {
                c0.value.clone()
            };
            let c2 = &n.nodes[arg.indices[2]];
            stop_str = Some(if c2.kind == VKind::Function {
                format!("{}({})", c2.value, vp_stringify(&c2.nodes))
            } else {
                c2.value.clone()
            });
        } else if !arg.indices.is_empty() {
            // arg[2] === undefined; upstream:
            //   if (arg[0].type === 'function') {
            //       color = `${arg[0].value}(${valueParser.stringify(arg[0].nodes)})`;
            //   }
            //   color = arg[0].value;     <-- unconditional reassignment, dropping
            //                                 the function-stringified form.
            // Replicate the bug verbatim.
            let c0 = &n.nodes[arg.indices[0]];
            // The conditional assignment is dead; only the final assignment
            // takes effect. But preserving the JS line keeps the intent
            // visible even though the result is `arg[0].value`.
            let _maybe_func = if c0.kind == VKind::Function {
                Some(format!("{}({})", c0.value, vp_stringify(&c0.nodes)))
            } else {
                None
            };
            color_str = c0.value.clone();
            stop_str = None;
        } else {
            // Empty arg (back-to-back commas etc.) — nothing to inspect.
            continue;
        }

        let color_lower = color_str.to_lowercase();
        let stop_lower = stop_str.as_ref().map(|s| s.to_lowercase());
        let is_cs = is_color_stop(&color_lower, stop_lower.as_deref());

        // `if (!colorStop || !arg[2]) return;`
        if !is_cs || arg.indices.len() < 3 {
            continue;
        }

        let stop_idx = arg.indices[2];
        let stop_val = n.nodes[stop_idx].value.clone();
        let this_stop = value_parser_unit(&stop_val);

        match &last_stop {
            None => {
                last_stop = this_stop.clone();
                continue;
            }
            Some(last_p) => {
                if let Some(this_p) = &this_stop {
                    if is_less_than(last_p, this_p) {
                        n.nodes[stop_idx].value = "0".to_string();
                    }
                }
            }
        }
        last_stop = this_stop;
    }
}

// ---------------------------------------------------------------------------
// Plugin entry — 1:1 with upstream `pluginCreator().OnceExit(css)`.
// ---------------------------------------------------------------------------

pub fn postcss_minify_gradients(root: &mut Root) -> PluginResult {
    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        let decl = match &mut node.kind {
            NodeKind::Declaration(d) => d,
            _ => return Mutation::Keep,
        };
        let v = decl.value.clone();
        if let Some(new_value) = optimise(&v) {
            decl.value = new_value;
        }
        Mutation::Keep
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — minimal coverage; corpus parity is the load-bearing gate.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_minify_gradients(&mut root).unwrap();
        stringify(&root)
    }

    #[test]
    fn no_op_blank() {
        assert_eq!(run(""), "");
    }

    #[test]
    fn ignores_non_gradient() {
        let css = "a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn var_bailout() {
        let css = "a { background: linear-gradient(var(--c) 0%, blue 100%); }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn env_bailout() {
        let css = "a { background: linear-gradient(red 0%, env(--x) 100%); }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn linear_to_top_rewrites_to_zero_deg() {
        let out = run("a { background: linear-gradient(to top, red, blue); }");
        assert!(out.contains("0deg"), "got: {out:?}");
        assert!(!out.contains("to top"), "got: {out:?}");
    }

    #[test]
    fn linear_to_right_rewrites_to_90deg() {
        let out = run("a { background: linear-gradient(to right, red, blue); }");
        assert!(out.contains("90deg"), "got: {out:?}");
    }

    #[test]
    fn linear_zero_pct_first_stripped() {
        // `red 0%, blue 100%` -> first arg has 3 tokens (red, ' ', 0%);
        // first stop `0%` !== 'deg' AND not final → strip.
        let out = run("a { background: linear-gradient(red 0%, blue 100%); }");
        assert!(out.contains("linear-gradient(red,blue)") || out.contains("linear-gradient(red, blue)"),
            "got: {out:?}");
    }

    #[test]
    fn linear_final_100pct_stripped() {
        // Need at least TWO 3-token args; the FIRST 3-token arg always
        // takes the `lastStop === undefined` early-return branch upstream,
        // so the final-stop 100% strip only fires from arg index >= 2.
        let out = run("a { background: linear-gradient(red 0%, green 50%, blue 100%); }");
        // First 3-token arg `red 0%` triggers the leading-zero strip,
        // last 3-token arg `blue 100%` triggers the trailing-100% strip.
        assert!(out.contains("red,"), "leading `red 0%` should collapse to `red`; got: {out:?}");
        assert!(!out.contains("100%"), "trailing 100% should be stripped; got: {out:?}");
        assert!(out.contains("50%"), "middle 50% should be preserved; got: {out:?}");
        assert!(out.contains("blue)") || out.contains("blue;"), "blue should sit at end; got: {out:?}");
    }

    #[test]
    fn radial_no_at_first_arg_processed() {
        // No `at` in arg[0] → first stop participates in lastStop tracking,
        // but doesn't get rewritten on its own; second stop with smaller
        // unit gets normalized to 0.
        let out = run("a { background: radial-gradient(red 50%, blue 25%); }");
        assert!(out.contains("blue 0"), "got: {out:?}");
    }
}
