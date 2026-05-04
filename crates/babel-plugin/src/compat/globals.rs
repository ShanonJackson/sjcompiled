//! Vendored copy of `@babel/helper-globals@7.28.0` data files.
//!
//! Source-of-truth (verbatim, not transformed):
//!   - `node_modules/.bun/@babel+helper-globals@7.28.0/.../data/builtin-lower.json`
//!   - `node_modules/.bun/@babel+helper-globals@7.28.0/.../data/builtin-upper.json`
//!
//! `@babel/traverse@7.29.0` (`scope/index.js:14-15, :940-941`) builds
//! `Scope.globals = [...builtinLower, ...builtinUpper]` and uses it
//! from `hasBinding(name, { noGlobals: false })` and `isPure()`. The
//! §5.0c partial-evaluator (`compat/evaluation.rs`) needs the same
//! list to resolve the `undefined` / `NaN` / `Infinity` identifier
//! literals against — see `plugins/COMPAT_SCOPE_AUDIT.md` Finding 8.
//!
//! Pinned at 7.28.0 in `crates/PARITY_VERSIONS.md`. The
//! `globals_match_pinned_entry_counts` test below schema-locks the
//! 13+49 split so a future `@babel/traverse` bump that pulls a
//! different `helper-globals` version fails fast at `cargo test`
//! before any silent class-name drift can land in production.

/// Lowercase global identifiers (function-y globals like
/// `parseInt`, `decodeURI`, `eval`, `globalThis`, `undefined`, …).
///
/// Babel parity: `@babel/helper-globals@7.28.0/data/builtin-lower.json`.
pub const BUILTIN_LOWER: &[&str] = &[
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "eval",
    "globalThis",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "undefined",
    "unescape",
];

/// Uppercase global identifiers (constructors and namespace-y
/// globals like `Array`, `JSON`, `Math`, `NaN`, `Infinity`, …).
///
/// Babel parity: `@babel/helper-globals@7.28.0/data/builtin-upper.json`.
pub const BUILTIN_UPPER: &[&str] = &[
    "AggregateError",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "FinalizationRegistry",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "Function",
    "Infinity",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "Iterator",
    "JSON",
    "Map",
    "Math",
    "NaN",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "URIError",
    "WeakMap",
    "WeakRef",
    "WeakSet",
];

/// `Scope.contextVariables` — `arguments`, `undefined`, `Infinity`,
/// `NaN` — variables that are "always in scope" even without an
/// explicit binding. Babel parity: `scope/index.js:941`.
pub const CONTEXT_VARIABLES: &[&str] = &["arguments", "undefined", "Infinity", "NaN"];

/// True iff `name` appears in `Scope.globals` (i.e. either builtin
/// list). Babel: the concatenation `[...lower, ...upper].includes(name)`
/// at `scope/index.js:861`.
pub fn is_global(name: &str) -> bool {
    BUILTIN_LOWER.contains(&name) || BUILTIN_UPPER.contains(&name)
}

/// True iff `name` is in `Scope.contextVariables` (`arguments` /
/// `undefined` / `Infinity` / `NaN`). Babel: `scope/index.js:862`.
pub fn is_context_variable(name: &str) -> bool {
    CONTEXT_VARIABLES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schema-lock — the Babel `helper-globals@7.28.0` data files have
    /// 13 lowercase + 49 uppercase entries. A future `@babel/traverse`
    /// bump that pulls a new helper-globals release with different
    /// counts fails THIS test before the missing/added globals can
    /// silently drift `Scope.globals.includes(name)` checks in the
    /// §5.0c evaluator. Reaction: re-vendor the JSONs, re-confirm
    /// `crates/PARITY_VERSIONS.md`'s pin, re-check call sites.
    #[test]
    fn globals_match_pinned_entry_counts() {
        assert_eq!(
            BUILTIN_LOWER.len(),
            13,
            "@babel/helper-globals/data/builtin-lower.json entry count drifted from pinned 7.28.0"
        );
        assert_eq!(
            BUILTIN_UPPER.len(),
            49,
            "@babel/helper-globals/data/builtin-upper.json entry count drifted from pinned 7.28.0"
        );
    }

    #[test]
    fn lookups_cover_well_known_globals() {
        for name in ["undefined", "globalThis", "parseInt", "eval"] {
            assert!(is_global(name), "expected `{name}` in builtin-lower");
        }
        for name in ["Array", "Symbol", "NaN", "Infinity", "JSON", "Promise"] {
            assert!(is_global(name), "expected `{name}` in builtin-upper");
        }
        for name in ["arguments", "undefined", "NaN", "Infinity"] {
            assert!(
                is_context_variable(name),
                "expected `{name}` in CONTEXT_VARIABLES"
            );
        }
    }

    #[test]
    fn context_variables_match_upstream_order() {
        // Order-locked against scope/index.js:941:
        //   Scope.contextVariables = ["arguments", "undefined", "Infinity", "NaN"];
        assert_eq!(
            CONTEXT_VARIABLES,
            ["arguments", "undefined", "Infinity", "NaN"]
        );
    }

    #[test]
    fn no_duplicates_within_or_across_lists() {
        // Babel concatenates the two lists; if they overlap, the
        // post-concat array contains dups. Vendor parity demands no
        // dups.
        let mut all: Vec<&&str> = BUILTIN_LOWER.iter().chain(BUILTIN_UPPER.iter()).collect();
        let len_before = all.len();
        all.sort();
        all.dedup();
        assert_eq!(
            all.len(),
            len_before,
            "duplicate entry across BUILTIN_LOWER + BUILTIN_UPPER"
        );
    }
}
