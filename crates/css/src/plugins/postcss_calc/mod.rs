//! Byte-for-byte Rust port of `postcss-calc@8.2.4`.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-calc/src/`):
//!   - `index.js`               -> `src/lib.rs` (this file — the postcss plugin entry)
//!   - `parser.js` / `parser.jison` -> `src/parser.rs` (hand-rolled to match the jison grammar 1:1)
//!   - `lib/transform.js`       -> `src/lib/transform.rs`
//!   - `lib/convertUnit.js`     -> `src/lib/convert_unit.rs`
//!   - `lib/reducer.js`         -> `src/lib/reducer.rs`
//!   - `lib/stringifier.js`     -> `src/lib/stringifier.rs`
//!
//! All bugs of the upstream version are intentionally preserved. See
//! `crates/_vendor/POSTCSS_CALC_8.2.4_REAUDIT.md` for the audit doc.

pub mod parser;

#[allow(clippy::module_inception)]
pub mod lib {
    pub mod convert_unit;
    pub mod reducer;
    pub mod stringifier;
    pub mod transform;
}

use postcss_core::container::{walk_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};

pub use lib::convert_unit::Precision;
pub use lib::transform::{Property, TransformOutcome, Warning};

/// Plugin options. Mirrors the `PostCssCalcOptions` typedef at upstream
/// `src/index.js:5-9`. Defaults match upstream `src/index.js:18-25`.
#[derive(Debug, Clone)]
pub struct Options {
    /// `precision: number | false`. Default `5`.
    pub precision: Precision,
    /// `preserve: boolean`. Default `false`.
    pub preserve: bool,
    /// `warnWhenCannotResolve: boolean`. Default `false`.
    pub warn_when_cannot_resolve: bool,
    /// `mediaQueries: boolean`. Default `false`.
    pub media_queries: bool,
    /// `selectors: boolean`. Default `false`.
    pub selectors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            precision: Precision::At(5.0),
            preserve: false,
            warn_when_cannot_resolve: false,
            media_queries: false,
            selectors: false,
        }
    }
}

/// `pluginCreator` upstream (`src/index.js:16-47`). The single public entry
/// the parity-runner exercises.
///
/// Mirrors `OnceExit(css, { result })`: walks every node in `css`. For
/// `decl`, transforms `value`. For `atrule` when `mediaQueries: true`,
/// transforms `params`. For `rule` when `selectors: true`, transforms
/// `selector`.
///
/// Returned warnings are collected into `out_warnings` so the caller can
/// register them on the postcss `Result`. Bridged through to the JS side
/// in tests.
pub fn postcss_calc(root: &mut Root, opts: &Options) -> PluginResult {
    postcss_calc_with_warnings(root, opts, &mut Vec::new())
}

/// As `postcss_calc`, but also returns the warnings that would be emitted
/// via `result.warn(...)`. Used by the parity-runner stage.
pub fn postcss_calc_with_warnings(
    root: &mut Root,
    opts: &Options,
    out_warnings: &mut Vec<Warning>,
) -> PluginResult {
    walk_mut(&mut root.root, &mut |node, _ctx| {
        // Compute the outcome FIRST (immutable borrow of decl.value/etc.)
        // and snapshot the original node for preserve mode, BEFORE the
        // mutable borrow of node.kind that writes back the new value.
        let (property, current_value): (Property, String) = match &node.kind {
            NodeKind::Declaration(d) => (Property::Value, d.value.clone()),
            NodeKind::AtRule(a) if opts.media_queries => (Property::Params, a.params.clone()),
            NodeKind::Rule(r) if opts.selectors => (Property::Selector, r.selector.clone()),
            _ => return Mutation::Keep,
        };

        let outcome = lib::transform::transform_node_property(&current_value, opts, property);
        out_warnings.extend(outcome.warnings.iter().cloned());

        // Per `transform.js:102-107`:
        //   if (options.preserve && node[property] !== value) {
        //     const clone = node.clone();   // clone is a DEEP copy of the original
        //     clone[property] = value;      // NEW (transformed) value goes on the clone
        //     node.parent.insertBefore(node, clone);  // clone inserted BEFORE node
        //   } else {
        //     node[property] = value;       // overwrite original in place
        //   }
        //
        // Result for `bar:calc(1rem * 1.5)` with preserve=true:
        //   `bar:1.5rem;bar:calc(1rem * 1.5)`
        //   (clone with 1.5rem before original calc)
        if opts.preserve && outcome.changed {
            // Build the clone with the NEW value; leave the original
            // unchanged. Insert the clone before the current node.
            let mut clone = node.clone();
            match (&mut clone.kind, property) {
                (NodeKind::Declaration(d), Property::Value) => d.value = outcome.new_value,
                (NodeKind::AtRule(a), Property::Params) => a.params = outcome.new_value,
                (NodeKind::Rule(r), Property::Selector) => r.selector = outcome.new_value,
                _ => {}
            }
            return Mutation::InsertBefore(vec![clone]);
        }

        // No preserve (or no change): overwrite in place.
        match (&mut node.kind, property) {
            (NodeKind::Declaration(d), Property::Value) => d.value = outcome.new_value,
            (NodeKind::AtRule(a), Property::Params) => a.params = outcome.new_value,
            (NodeKind::Rule(r), Property::Selector) => r.selector = outcome.new_value,
            _ => {}
        }
        Mutation::Keep
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(input: &str) -> String {
        run_with(input, &Options::default())
    }
    fn run_with(input: &str, opts: &Options) -> String {
        let mut root = parse(input).unwrap();
        postcss_calc(&mut root, opts).unwrap();
        stringify(&root)
    }

    #[test]
    fn simple_decl() {
        assert_eq!(run("foo{bar:calc(1px + 1px)}"), "foo{bar:2px}");
    }

    #[test]
    fn preserve_mode() {
        let opts = Options { preserve: true, ..Options::default() };
        assert_eq!(
            run_with("foo{bar:calc(1rem * 1.5)}", &opts),
            "foo{bar:1.5rem;bar:calc(1rem * 1.5)}"
        );
    }

    #[test]
    fn no_calc_passthrough() {
        assert_eq!(run("foo{bar:16px}"), "foo{bar:16px}");
    }

    #[test]
    fn at_rule_default_skips_media() {
        // mediaQueries defaults false → @media params stay raw.
        assert_eq!(
            run("@media (min-width:calc(10px+10px)){}"),
            "@media (min-width:calc(10px+10px)){}"
        );
    }

    #[test]
    fn at_rule_media_queries_enabled() {
        let opts = Options { media_queries: true, ..Options::default() };
        assert_eq!(
            run_with("@media (min-width:calc(10px+10px)){}", &opts),
            "@media (min-width:20px){}"
        );
    }

    #[test]
    fn divide_by_zero_keeps_original() {
        assert_eq!(run("foo{bar:calc(500px/0)}"), "foo{bar:calc(500px/0)}");
    }

    #[test]
    fn calc_in_custom_property_passes_through_via_var() {
        // calc(var(--bar)/8) — Function preserved, division kept.
        let r = run(":root { --foo: calc(var(--bar) / 8); }");
        assert_eq!(r, ":root { --foo: calc(var(--bar)/8); }");
    }
}
