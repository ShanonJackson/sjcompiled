//! Port of `postcss-calc/src/lib/transform.js`. Line-numbered references
//! point at upstream `transform.js`.
//!
//! This file glues the calc-parser, reducer, and stringifier together with
//! a value-parser walk over a CSS string. The core entry point
//! `transform_value` mirrors `transformValue(value, options, result, item)`
//! upstream — it walks the value-parser AST looking for `(-vendor-)?calc(...)`
//! function nodes, reduces each, and writes the result back.

use postcss_value_parser as vp;

use crate::lib::reducer::reduce;
use crate::lib::stringifier::{stringify_calc, ShouldPrintCalc};
use crate::parser::parse as calc_parse;
use crate::Options;

/// One warning to emit on the postcss `Result`. The transform layer collects
/// these and the plugin's caller delivers them via `result.warn(...)`.
#[derive(Debug, Clone)]
pub struct Warning {
    pub text: String,
}

/// `MATCH_CALC` upstream (line 10): `/((?:-(moz|webkit)-)?calc)/i`.
/// Note: this is a `.test()` (substring) match upstream, not a full anchor —
/// but every value-parser Function `value` is the *bare* function name
/// without parens, so substring vs anchor is equivalent here. The regex
/// gates ALL calc-prefixed functions: `calc`, `-webkit-calc`, `-moz-calc`,
/// case-insensitive.
fn matches_calc(name: &str) -> bool {
    let name_lc = name.to_ascii_lowercase();
    if name_lc == "calc" { return true; }
    if let Some(rest) = name_lc.strip_prefix('-') {
        if let Some(rest) = rest.strip_prefix("webkit-") {
            return rest == "calc";
        }
        if let Some(rest) = rest.strip_prefix("moz-") {
            return rest == "calc";
        }
    }
    false
}

/// `transformValue(value, options, result, item)` upstream (lines 18-48).
///
/// Walks the value-parser AST, transforms every calc() function node, and
/// returns the re-stringified value. Warnings from
/// `warnWhenCannotResolve` mode are collected into `warnings`.
///
/// IMPORTANT — error semantics: when `parser.parse(contents)` throws (lexical
/// or parse error from the calc parser), upstream lets the error propagate
/// out of `transformValue`. The caller in `module.exports` (lines 81-95)
/// catches it, calls `result.warn(error.message, ...)`, and returns without
/// writing back. Since we can't unwind from inside the value-parser walk
/// cleanly, we capture the first error and surface it via `Err`.
pub fn transform_value(
    value: &str,
    options: &Options,
    warnings: &mut Vec<Warning>,
) -> Result<String, String> {
    let mut nodes = vp::parse(value);

    // We can't early-return out of `vp::walk` cleanly. Instead we set a
    // flag inside the closure and check after each visit.
    let mut error_msg: Option<String> = None;

    vp::walk(
        &mut nodes,
        |node, _idx| {
            if error_msg.is_some() {
                // Stop descending.
                return Some(false);
            }
            // skip anything which isn't a calc() function
            if node.kind != vp::NodeKind::Function || !matches_calc(&node.value) {
                return Some(true); // continue walking
            }
            // stringify calc inner contents and produce an AST.
            let contents = vp::stringify(&node.nodes);
            let ast = match calc_parse(&contents) {
                Ok(a) => a,
                Err(e) => {
                    error_msg = Some(e.0);
                    return Some(false);
                }
            };
            // reduce
            let reduced_ast = match reduce(ast, options.precision) {
                Ok(a) => a,
                Err(e) => {
                    error_msg = Some(e.0);
                    return Some(false);
                }
            };

            // Stringify and write back.
            let (out_str, should_print_calc) =
                stringify_calc(&node.value, &reduced_ast, options.precision);

            // warnWhenCannotResolve: emit a warning when re-wrapped as calc().
            // upstream emits per-calc, and the warning text uses the ORIGINAL
            // value (the full decl-value), not the inner contents.
            if options.warn_when_cannot_resolve
                && matches!(should_print_calc, ShouldPrintCalc::Yes)
            {
                warnings.push(Warning {
                    text: format!("Could not reduce expression: {}", value),
                });
            }

            // node.type = 'word'; node.value = out_str.
            node.kind = vp::NodeKind::Word;
            node.value = out_str;
            // Per upstream `return false;` — don't descend into this calc.
            Some(false)
        },
        false,
    );

    if let Some(msg) = error_msg {
        return Err(msg);
    }
    Ok(vp::stringify(&nodes))
}

/// `transformSelector(value, options, result, item)` upstream (lines 55-73).
///
/// Selector mode runs only when `options.selectors === true`. Our integration
/// pipeline never enables this option (cssnano-preset-default invokes
/// `postcssCalc()` with default options — `selectors: false`). We provide a
/// real port for parity-completeness, but the path is exercised only in
/// dedicated tests.
///
/// Walks the parsed selector tree; for every `attribute` node with a value,
/// runs that value through `transform_value`. For every `tag` node, ditto.
pub fn transform_selector(
    value: &str,
    options: &Options,
    warnings: &mut Vec<Warning>,
) -> Result<String, String> {
    use postcss_selector_parser::nodes::{walk_all, NodeKind};
    use postcss_selector_parser::Processor;

    // The closure can't easily return errors from inside walk_all; we
    // capture them and raise after.
    let mut deferred_error: Option<String> = None;
    let mut deferred_warnings: Vec<Warning> = Vec::new();

    let result = Processor::new().process(value, |root| {
        // walk_all visits every descendant. The closure receives (parent, idx).
        let mut visit = |parent: &mut postcss_selector_parser::nodes::Node, idx: usize| {
            if deferred_error.is_some() { return; }
            let child = match parent.nodes.get_mut(idx) {
                Some(c) => c,
                None => return,
            };
            match child.kind {
                NodeKind::Attribute => {
                    if !child.value.is_empty() {
                        let new_v = match transform_value(&child.value, options, &mut deferred_warnings) {
                            Ok(v) => v,
                            Err(e) => { deferred_error = Some(e); return; }
                        };
                        // setValue equivalent: write the typed payload's
                        // `value` (without quotes), mark payload dirty so
                        // the stringifier rebuilds the bracket text.
                        if let Some(p) = child.attribute.as_mut() {
                            p.value = Some(new_v.clone());
                            p.dirty = true;
                        }
                        child.set_value(new_v);
                    }
                }
                NodeKind::Tag => {
                    let new_v = match transform_value(&child.value, options, &mut deferred_warnings) {
                        Ok(v) => v,
                        Err(e) => { deferred_error = Some(e); return; }
                    };
                    child.value = new_v;
                    child.raw_value = None;
                }
                _ => {}
            }
        };
        walk_all(root, &mut visit);
    });

    if let Some(msg) = deferred_error { return Err(msg); }
    let out = result.map_err(|e| format!("{:?}", e))?;
    warnings.extend(deferred_warnings);
    Ok(out)
}

/// Property kind selector mirroring `transform.js:81`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Value,    // declaration `value`
    Params,   // atrule `params`
    Selector, // rule `selector`
}

/// The shared transform that backs `module.exports = (node, property, options, result) => ...`.
///
/// On error: emits a `Warning` and leaves the property unchanged (matches
/// the `try/catch` at upstream lines 84-96).
/// On success: returns the new value plus a `changed` bool the caller uses
/// to decide whether to clone-and-insert (preserve) or in-place-overwrite.
#[derive(Debug, Clone)]
pub struct TransformOutcome {
    pub new_value: String,
    pub changed: bool,
    pub warnings: Vec<Warning>,
}

pub fn transform_node_property(
    current_value: &str,
    options: &Options,
    property: Property,
) -> TransformOutcome {
    let mut warnings: Vec<Warning> = Vec::new();
    let result = if property == Property::Selector {
        transform_selector(current_value, options, &mut warnings)
    } else {
        transform_value(current_value, options, &mut warnings)
    };
    match result {
        Ok(new_value) => {
            let changed = new_value != current_value;
            TransformOutcome { new_value, changed, warnings }
        }
        Err(message) => {
            // Caught error → result.warn(message). `transform.js:91-94`.
            warnings.push(Warning { text: message });
            TransformOutcome {
                new_value: current_value.to_string(),
                changed: false,
                warnings,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::convert_unit::Precision;

    fn opts() -> Options {
        Options {
            precision: Precision::At(5.0),
            preserve: false,
            warn_when_cannot_resolve: false,
            media_queries: false,
            selectors: false,
        }
    }

    #[test]
    fn calc_unwrapped() {
        let mut warns = Vec::new();
        let r = transform_value("calc(1px + 1px)", &opts(), &mut warns).unwrap();
        assert_eq!(r, "2px");
        assert!(warns.is_empty());
    }

    #[test]
    fn calc_with_surroundings() {
        let mut warns = Vec::new();
        let r = transform_value("a calc(1px + 1px) b", &opts(), &mut warns).unwrap();
        assert_eq!(r, "a 2px b");
    }

    #[test]
    fn no_calc_passthrough() {
        let mut warns = Vec::new();
        let r = transform_value("16px", &opts(), &mut warns).unwrap();
        assert_eq!(r, "16px");
    }

    #[test]
    fn vendor_calc() {
        let mut warns = Vec::new();
        let r = transform_value("-webkit-calc(1px + 1px)", &opts(), &mut warns).unwrap();
        assert_eq!(r, "2px");
    }

    #[test]
    fn calc_with_var_preserves() {
        let mut warns = Vec::new();
        let r = transform_value("calc(var(--mouseX) * 1px)", &opts(), &mut warns).unwrap();
        assert_eq!(r, "calc(var(--mouseX)*1px)");
    }

    #[test]
    fn divide_by_zero_emits_warning_via_outcome() {
        let outcome = transform_node_property("calc(500px/0)", &opts(), Property::Value);
        assert_eq!(outcome.new_value, "calc(500px/0)");
        assert!(!outcome.changed);
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].text, "Cannot divide by zero");
    }
}
