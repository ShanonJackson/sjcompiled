//! crates/cssnano-postcss-reduce-initial
//! Byte-for-byte Rust port of `postcss-reduce-initial@5.1.2`.
//!
//! Folder/file mapping (1:1 with upstream):
//!   - `src/index.js`              -> `src/lib.rs` (this file).
//!   - `src/data/fromInitial.json` -> `src/data/fromInitial.json` (vendored verbatim).
//!   - `src/data/toInitial.json`   -> `src/data/toInitial.json` (vendored verbatim).
//!
//! All bugs of upstream 5.1.2 are intentionally preserved.
//!
//! ## Behavior (1:1 with upstream `prepare(result)` + `OnceExit(css)`)
//!
//! At plugin entry (mirrors upstream `prepare`):
//!   1. Resolve `browserslist` from defaults / env (upstream passes
//!      `path: __dirname` which finds no config in the install dir; AFM
//!      consumers reach the same end-state — see browserslist-shim docs).
//!   2. `initialSupport = isSupported('css-initial-value', browsers)`.
//!
//! Per declaration (mirrors upstream `OnceExit`):
//!   1. `lowerCasedProp = decl.prop.toLowerCase()`.
//!   2. If `lowerCasedProp ∈ defaultIgnoreProps ∪ opts.ignore`, skip.
//!      `defaultIgnoreProps = ['writing-mode', 'transform-box']`. Both
//!      have non-`initial` initial values that current browsers (incl.
//!      Chrome) do NOT actually default to — see upstream comment +
//!      cssnano#905.
//!   3. If `initialSupport && toInitial[lowerCasedProp] === decl.value.toLowerCase()`
//!      → set `decl.value = "initial"` and stop.
//!   4. Else if `decl.value.toLowerCase() !== "initial"` OR no
//!      `fromInitial` entry → skip.
//!   5. Else `decl.value = fromInitial[lowerCasedProp]`.
//!
//! Subtle upstream detail intentionally preserved:
//! - `toInitial` lookup gates on `Object.prototype.hasOwnProperty.call`
//!   (key existence). `fromInitial` lookup uses `!fromInitial[key]`
//!   (truthiness). All `fromInitial` values are non-empty strings, so
//!   the two predicates coincide on this data — but the difference is
//!   replicated in code shape.

use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;

use postcss_core::container::{walk_decls_mut, Mutation};
use postcss_core::node::NodeKind;
use postcss_core::{PluginResult, Root};

const INITIAL: &str = "initial";

/// Upstream `defaultIgnoreProps` (src/index.js:10):
/// > In most of the browser including chrome the initial for
/// > `writing-mode` is not `horizontal-tb`. Ref cssnano#905.
const DEFAULT_IGNORE_PROPS: &[&str] = &["writing-mode", "transform-box"];

static FROM_INITIAL: Lazy<IndexMap<String, String>> = Lazy::new(|| {
    serde_json::from_str(include_str!("data/fromInitial.json"))
        .expect("postcss-reduce-initial: fromInitial.json malformed")
});

static TO_INITIAL: Lazy<IndexMap<String, String>> = Lazy::new(|| {
    serde_json::from_str(include_str!("data/toInitial.json"))
        .expect("postcss-reduce-initial: toInitial.json malformed")
});

/// Mirrors upstream `result.opts` shape consumed by `prepare(result)`:
/// only `ignore` (a `string[]` of prop names to skip) is exposed by the
/// AFM consumer. `stats` and `env` are accepted but unused — upstream
/// passes them straight to `browserslist(null, { stats, path, env })`,
/// which finds no config from `__dirname` and falls through to defaults.
/// Same end-state for the Rust port (no path → no config lookup).
#[derive(Debug, Clone, Default)]
pub struct PostcssReduceInitialOpts {
    pub ignore: Vec<String>,
    pub env: Option<String>,
}

pub fn postcss_reduce_initial(root: &mut Root, opts: &PostcssReduceInitialOpts) -> PluginResult {
    // `prepare(result)` upstream — resolved once at plugin instantiation.
    // The Rust port computes once per `postcss_reduce_initial` call;
    // identical end-state since neither caches across calls. `opts.env`
    // is dormant on this code path (no config file is reachable from the
    // shim's `path: None` resolution); kept on the struct for API parity.
    let initial_support = caniuse_api::is_supported("css-initial-value", "");

    // `new Set(defaultIgnoreProps.concat(resultOpts.ignore || []))` —
    // built once outside the walk callback (upstream rebuilds inside the
    // callback per-decl; both produce the same set per call).
    let mut ignore_set: IndexSet<String> = IndexSet::new();
    for p in DEFAULT_IGNORE_PROPS {
        ignore_set.insert((*p).to_string());
    }
    for p in &opts.ignore {
        ignore_set.insert(p.clone());
    }

    walk_decls_mut(&mut root.root, &mut |node, _ctx| {
        let decl = match &mut node.kind {
            NodeKind::Declaration(d) => d,
            _ => return Mutation::Keep,
        };

        let lower_prop = decl.prop.to_lowercase();
        if ignore_set.contains(&lower_prop) {
            return Mutation::Keep;
        }

        let lower_value = decl.value.to_lowercase();

        // toInitial branch: gated on `initialSupport` AND key existence
        // AND value equality (case-insensitive). Mirrors:
        //   if (initialSupport && hasOwnProperty(toInitial, k)
        //       && decl.value.toLowerCase() === toInitial[k]) { ... }
        if initial_support {
            if let Some(initial_for_prop) = TO_INITIAL.get(&lower_prop) {
                if &lower_value == initial_for_prop {
                    decl.value = INITIAL.to_string();
                    return Mutation::Keep;
                }
            }
        }

        // fromInitial branch: only fires when value is exactly "initial"
        // (case-insensitive) AND prop has a `fromInitial` entry.
        if lower_value != INITIAL {
            return Mutation::Keep;
        }
        if let Some(replacement) = FROM_INITIAL.get(&lower_prop) {
            decl.value = replacement.clone();
        }
        Mutation::Keep
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_reduce_initial(&mut root, &PostcssReduceInitialOpts::default()).unwrap();
        stringify(&root)
    }

    fn run_with(css: &str, opts: PostcssReduceInitialOpts) -> String {
        let mut root = parse(css).unwrap();
        postcss_reduce_initial(&mut root, &opts).unwrap();
        stringify(&root)
    }

    #[test]
    fn data_tables_load_in_order() {
        // Insertion-order sanity — first key of each.
        assert_eq!(FROM_INITIAL.keys().next().map(String::as_str), Some("-webkit-line-clamp"));
        assert_eq!(TO_INITIAL.keys().next().map(String::as_str), Some("background-clip"));
        // Spot-check a known pair.
        assert_eq!(FROM_INITIAL.get("white-space"), Some(&"normal".to_string()));
        assert_eq!(TO_INITIAL.get("color"), Some(&"canvastext".to_string()));
    }

    #[test]
    fn from_initial_rewrites_initial_keyword() {
        // `min-width: initial` → `min-width: auto` (fromInitial has it).
        assert_eq!(run("a { min-width: initial }"), "a { min-width: auto }");
    }

    #[test]
    fn from_initial_uppercase_value() {
        // Case-insensitive value match. Property is rewritten in-place so
        // prop bytes are preserved; value rewritten lowercase.
        assert_eq!(run("a { min-width: INITIAL }"), "a { min-width: auto }");
    }

    #[test]
    fn from_initial_uppercase_prop() {
        // Case-insensitive prop match. Prop bytes preserved as-written.
        assert_eq!(run("a { MIN-WIDTH: initial }"), "a { MIN-WIDTH: auto }");
    }

    #[test]
    fn from_initial_unknown_prop_skipped() {
        // No `fromInitial` entry for `foo` — left untouched.
        assert_eq!(run("a { foo: initial }"), "a { foo: initial }");
    }

    #[test]
    fn to_initial_branch_gated_on_caniuse() {
        // `border-collapse: separate` is `toInitial`'s entry. Whether it
        // rewrites to `initial` depends on browserslist defaults; test
        // documents the observable transform either way.
        let out = run("a { border-collapse: separate }");
        let supports = caniuse_api::is_supported("css-initial-value", "");
        if supports {
            assert_eq!(out, "a { border-collapse: initial }");
        } else {
            assert_eq!(out, "a { border-collapse: separate }");
        }
    }

    #[test]
    fn ignore_writing_mode_default() {
        // `writing-mode` is in defaultIgnoreProps — never rewritten.
        assert_eq!(run("a { writing-mode: horizontal-tb }"), "a { writing-mode: horizontal-tb }");
        assert_eq!(run("a { writing-mode: initial }"), "a { writing-mode: initial }");
    }

    #[test]
    fn ignore_transform_box_default() {
        assert_eq!(run("a { transform-box: view-box }"), "a { transform-box: view-box }");
    }

    #[test]
    fn ignore_opt_extends_defaults() {
        // User-supplied ignore list is unioned with defaults.
        let out = run_with(
            "a { color: initial; min-width: initial; }",
            PostcssReduceInitialOpts { ignore: vec!["min-width".to_string()], env: None },
        );
        // `min-width` skipped (user-ignored). `color` falls through:
        // `fromInitial` has no entry for `color` (it's in `toInitial`),
        // so the `initial` keyword stays.
        assert_eq!(out, "a { color: initial; min-width: initial; }");
    }

    #[test]
    fn vendor_prefixed_in_from_initial() {
        // `-webkit-line-clamp: initial` → `-webkit-line-clamp: none`.
        assert_eq!(run("a { -webkit-line-clamp: initial }"), "a { -webkit-line-clamp: none }");
    }

    #[test]
    fn important_preserved() {
        // !important survives the value rewrite (raws.important untouched).
        assert_eq!(
            run("a { min-width: initial !important }"),
            "a { min-width: auto !important }",
        );
    }

    #[test]
    fn no_op_round_trips_byte_identical() {
        // Plugin should be a no-op on input it doesn't touch — exact
        // round-trip including raws.between bytes.
        let src = "a { color: red;\n  background:  blue;  }\n\n@media (min-width: 0) { b { display: block; } }";
        assert_eq!(run(src), src);
    }
}
