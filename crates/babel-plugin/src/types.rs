//! 1:1 port of `packages/babel-plugin/src/types.ts`.
//!
//! Data-only at §2.1 — every field is mirrored, but the methods that
//! operate on this state live in their own modules (`state.rs` /
//! `mutation_recorder.rs`, landing in §2.4). The shapes here are
//! consumed by:
//!
//! * The dispatcher visitor (`babel_plugin.rs`, §2.3).
//! * The per-API handlers (Phase 6).
//! * The cache schema (`cache_schema.rs`, §5.3).
//!
//! ### What's intentionally omitted
//!
//! * `Resolver`'s `resolveSync` callback — JS callbacks aren't
//!   reachable from a WASI plugin (PLAN.md constraint 1). The Rust
//!   resolver lives in `utils/resolve_binding.rs` (§5.4) and uses
//!   `oxc_resolver` directly. `PluginOptions::resolver` retains its
//!   string variant only (the module-path form); host-side callback
//!   resolvers are dropped at the `babel-plugin` SWC plugin
//!   boundary by the host wrapper (`packages/parcel-transformer`).
//!
//! * `onIncludedFiles` callback — same constraint. The runtime
//!   contract is `included-files.json` sidecar (`SIDECAR_SCHEMA.md`
//!   §1, written in §5.7).
//!
//! * `t.Identifier` / `NodePath` / `PluginPass` types — Babel-specific.
//!   The Rust analogues are SWC AST node refs / `IndexMap`-keyed
//!   indices, threaded through the visitor directly. See `state.rs`.
//!
//! Drift policy: when upstream `types.ts` changes, this file MUST be
//! updated in the same commit. Reviewers diff field-by-field.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// `PluginOptions` — userland-facing configuration. Field names use
/// camelCase on the wire to match Babel's plugin-config convention;
/// the Rust struct uses snake_case via `#[serde(rename_all)]`.
///
/// Mirrors lines 12–115 of upstream `types.ts`. Every field is
/// `Option<_>` so the serde default of `None` matches Babel's
/// "missing key" semantics. Kept stable (don't reorder) so a diff
/// against the upstream file is line-comparable.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOptions {
    /// `true` | `"file-pass"` | `false`. Default `true`.
    /// Wire shape: `bool` OR the string `"file-pass"`. We model with
    /// the `CacheMode` enum and a custom serde tag — see below.
    #[serde(default)]
    pub cache: Option<CacheMode>,

    /// Defaults to `true`. When the host pipeline already runs
    /// `@babel/preset-react` with `runtime: 'automatic'`, set `false`.
    #[serde(default)]
    pub import_react: Option<bool>,

    /// `nonce` for inline `<style>` tags (CSP integration).
    #[serde(default)]
    pub nonce: Option<String>,

    /// Extra module origins to treat as Compiled (defaults to
    /// `DEFAULT_IMPORT_SOURCES` from `@sjcompiled/utils`).
    #[serde(default)]
    pub import_sources: Option<Vec<String>>,

    // `onIncludedFiles` is intentionally absent here — see module
    // docs. The contract is `included-files.json` sidecar.
    /// Run cssnano normalisation. Default `true`.
    #[serde(default)]
    pub optimize_css: Option<bool>,

    /// String form only — module path of a custom resolver. The
    /// object/callback variants are dropped at the host wrapper
    /// boundary (PLAN.md constraint 1). Phase 5 §5.4 wires this into
    /// the `oxc_resolver` config.
    #[serde(default)]
    pub resolver: Option<String>,

    /// File extensions the resolver considers as "code" (defaults to
    /// `DEFAULT_CODE_EXTENSIONS` in `constants.rs`).
    #[serde(default)]
    pub extensions: Option<Vec<String>>,

    /// Babel parser plugins for evaluated files. Mirrors upstream
    /// `parserBabelPlugins`. SWC has its own parser; this list maps
    /// to SWC's `JscConfig.parser` flags via the host wrapper. Stored
    /// here for parity / round-tripping.
    #[serde(default)]
    pub parser_babel_plugins: Option<Vec<String>>,

    /// Append the component name as a class for non-prod builds of
    /// `styled` components. Default `false`.
    #[serde(default)]
    pub add_component_name: Option<bool>,

    /// Compressed-classname mapping `{ atomicHash: shortName }`.
    /// Order-preserving — IndexMap so re-emit matches upstream.
    #[serde(default)]
    pub class_name_compression_map: Option<IndexMap<String, String>>,

    /// Default `true`. Disable when xcss isn't used in the codebase
    /// (some performance gain).
    #[serde(default)]
    pub process_xcss: Option<bool>,

    /// Default `false`. Adds `&` doubling to selectors to bump
    /// specificity during migration from another styling solution.
    #[serde(default)]
    pub increase_specificity: Option<bool>,

    /// Default `true`. Sort at-rules / media queries.
    #[serde(default)]
    pub sort_at_rules: Option<bool>,

    /// Hash prefix for generated atomic class names (micro-frontend
    /// isolation). Mixing with extraction is a documented footgun in
    /// the JS plugin; the Rust port preserves the same warning path.
    #[serde(default)]
    pub class_hash_prefix: Option<String>,
}

/// `cache: true | 'file-pass' | false`. Custom (de)serializer matches
/// the JS shape exactly: `bool` literal OR string `"file-pass"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    On,
    Off,
    FilePass,
}

impl<'de> Deserialize<'de> for CacheMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        // Accept both `true`/`false` and `"file-pass"`. Anything else
        // is a user-config typo we surface immediately.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Str(String),
        }
        match Raw::deserialize(d)? {
            Raw::Bool(true) => Ok(CacheMode::On),
            Raw::Bool(false) => Ok(CacheMode::Off),
            Raw::Str(s) if s == "file-pass" => Ok(CacheMode::FilePass),
            Raw::Str(s) => Err(D::Error::custom(format!(
                "invalid cache mode '{}': expected true | false | \"file-pass\"",
                s
            ))),
        }
    }
}

impl Serialize for CacheMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            CacheMode::On => s.serialize_bool(true),
            CacheMode::Off => s.serialize_bool(false),
            CacheMode::FilePass => s.serialize_str("file-pass"),
        }
    }
}

/// Per-file traversal state. Mirrors `State` in upstream `types.ts`
/// lines 117–211. The Babel `PluginPass` superclass fields (`file`,
/// `filename`, `cwd`, `opts`) are absorbed where used; the rest of
/// the state lives here verbatim.
///
/// Method-side mutation is forbidden outside `state.rs` /
/// `mutation_recorder.rs` (§2.4 hard rule per PLAN.md §3.9.8).
#[derive(Debug, Default)]
pub struct State {
    /// Set when the Compiled module import is found. Each known API
    /// gets its imported binding name(s) — the visitor uses these to
    /// match call sites.
    pub compiled_imports: Option<CompiledImports>,

    /// `true` if the module imports xcss from a Compiled origin.
    pub uses_xcss: Option<bool>,

    /// `importedCompiledImports.css` — set when `import { css } from
    /// '@sjcompiled/react'` is found AND a host-imported alias
    /// shadows it (rare but supported upstream).
    pub imported_compiled_imports: Option<ImportedCompiledImports>,

    /// Module origins recognised as Compiled. Resolved from
    /// `PluginOptions.import_sources` ∪ `DEFAULT_IMPORT_SOURCES`.
    pub import_sources: Vec<String>,

    /// Pragma state — JSX classic vs automatic, source override, etc.
    pub pragma: PragmaState,

    /// `pathsToCleanup` is a Babel-NodePath construct. The Rust port
    /// records these as deferred mutations on the `MutationRecorder`
    /// (§2.4); this field stays as a marker for parity but the
    /// concrete representation moves to that recorder. See PLAN.md
    /// §3.9.8 `StateDiff::CleanupPath`.
    pub paths_to_cleanup: Vec<CleanupAction>,

    /// User-supplied options. Owned here so handlers don't thread an
    /// extra param.
    pub opts: PluginOptions,

    /// Hoisted style sheets — `name → identifier`. Order preserved
    /// (insertion order matches Babel; the AST emit order depends on
    /// it). Babel stores `t.Identifier`; we store the symbol name and
    /// reconstruct the SWC `Ident` on emit (Phase 6).
    pub sheets: IndexMap<String, String>,

    /// Cache for evaluated paths. The concrete cache type lands in
    /// §5.3 (`utils::cache::Cache`); this is a placeholder slot.
    pub cache: CacheSlot,

    /// Files included in this transformation pass. Drained at
    /// `Program::exit` and serialised to `included-files.json`
    /// sidecar (§5.7 / SIDECAR_SCHEMA.md §1).
    pub included_files: Vec<String>,

    /// Evaluated `cssMap()` outputs — `localName → vec of css rules`.
    /// Order preserved (visitor walks in source order).
    pub css_map: IndexMap<String, Vec<String>>,

    /// MemberExpression names to skip — populated when a binding is
    /// known not to be a Compiled API (avoids re-resolving across
    /// many references). Mirrors `state.ignoreMemberExpressions`.
    pub ignore_member_expressions: IndexMap<String, bool>,

    // `resolver` is omitted: object form isn't reachable, string
    // form is in `opts`. `transformCache` (Babel WeakMap on
    // NodePath) is a Babel-only construct — the Rust visitor's
    // single-pass design (PLAN.md §3.5) eliminates the re-visit
    // problem the WeakMap was guarding against.
}

/// `state.compiledImports` — which Compiled API names are bound in
/// this module, and under which local identifiers. Empty vec means
/// "imported but no aliases" / "imported with default name".
#[derive(Debug, Default)]
pub struct CompiledImports {
    pub class_names: Option<Vec<String>>,
    pub css: Option<Vec<String>>,
    pub keyframes: Option<Vec<String>>,
    pub styled: Option<Vec<String>>,
    pub css_map: Option<Vec<String>>,
}

/// `state.importedCompiledImports` — kept narrow because upstream
/// only ever sets `css` here. Adding a new variant requires bumping
/// the cache schema (§5.3 `schema_hash`).
#[derive(Debug, Default)]
pub struct ImportedCompiledImports {
    pub css: Option<String>,
}

/// `state.pragma` — JSX-pragma awareness used by the css-prop and
/// classnames handlers (Phase 6). Defaults are all `false` / `None`.
#[derive(Debug, Default)]
pub struct PragmaState {
    /// `/** @jsx ... */` is set on this file.
    pub jsx: Option<bool>,
    /// `/** @jsxImportSource ... */` is set on this file.
    pub jsx_import_source: Option<bool>,
    /// Classic-pragma name resolves to a Compiled binding.
    pub classic_jsx_pragma_is_compiled: Option<bool>,
    /// Local name of the classic-pragma identifier (after rename).
    pub classic_jsx_pragma_local_name: Option<String>,
}

/// One entry of `state.pathsToCleanup`. Babel's `NodePath` reference
/// is replaced with an opaque ID the Phase 5 mutation recorder
/// knows how to resolve to a concrete AST mutation.
///
/// `id` here corresponds to a `MutationRecorder`-issued handle
/// (§2.4); the recorder owns the actual AST identity (BytePos /
/// node-index).
#[derive(Debug, Clone, Copy)]
pub struct CleanupAction {
    pub action: CleanupKind,
    pub id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    Replace,
    Remove,
}

/// Placeholder for the Phase 5 `Cache`. Today this is empty so
/// dispatcher code can hold a `&mut State.cache` without a type
/// dependency on the unported cache module.
#[derive(Debug, Default)]
pub struct CacheSlot;

/// `Metadata`'s `context` discriminator from upstream lines 232–246.
/// Each handler call gets a `Metadata` struct describing the parent
/// node + traversal context. The Rust port keeps this as a tagged
/// enum so the visitor can match on context without string
/// comparison.
#[derive(Debug, Clone)]
pub enum MetadataContext {
    Root,
    Fragment,
    Keyframes { keyframe: String },
}

/// Common metadata fields. `parent_id` / `own_id` correspond to
/// MutationRecorder-issued handles; the recorder resolves these to
/// the concrete AST nodes when mutating.
#[derive(Debug)]
pub struct Metadata<'a> {
    pub state: &'a mut State,
    pub parent_id: u32,
    pub own_id: Option<u32>,
    pub context: MetadataContext,
}

/// `Tag` from upstream lines 248–259 — `(name, type)` describing a
/// JSX/styled host (`'div'` / `'MyComponent'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub kind: TagKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    InBuiltComponent,
    UserDefinedComponent,
}

/// `TransformResult` — the public return shape the host wrapper
/// surfaces. SWC's plugin contract returns the rewritten Program;
/// this struct is for the workspace integration tests that drive
/// the visitor in-process and want a tuple of (code, includedFiles).
#[derive(Debug, Default)]
pub struct TransformResult {
    pub included_files: Vec<String>,
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_mode_deserializes_from_bool_and_string() {
        let on: CacheMode = serde_json::from_str("true").unwrap();
        assert_eq!(on, CacheMode::On);
        let off: CacheMode = serde_json::from_str("false").unwrap();
        assert_eq!(off, CacheMode::Off);
        let fp: CacheMode = serde_json::from_str("\"file-pass\"").unwrap();
        assert_eq!(fp, CacheMode::FilePass);
    }

    #[test]
    fn cache_mode_rejects_unknown_strings() {
        let err = serde_json::from_str::<CacheMode>("\"sometimes\"").unwrap_err();
        assert!(err.to_string().contains("invalid cache mode"));
    }

    #[test]
    fn plugin_options_camelcase_wire_shape() {
        let json = r#"{
            "importReact": false,
            "optimizeCss": true,
            "classHashPrefix": "abc",
            "sortAtRules": true,
            "classNameCompressionMap": { "_aaaabbbb": "a" },
            "cache": "file-pass"
        }"#;
        let opts: PluginOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.import_react, Some(false));
        assert_eq!(opts.optimize_css, Some(true));
        assert_eq!(opts.class_hash_prefix.as_deref(), Some("abc"));
        assert_eq!(opts.sort_at_rules, Some(true));
        assert_eq!(opts.cache, Some(CacheMode::FilePass));
        let map = opts.class_name_compression_map.unwrap();
        assert_eq!(map.get("_aaaabbbb").map(|s| s.as_str()), Some("a"));
    }

    #[test]
    fn plugin_options_default_is_all_none() {
        let opts: PluginOptions = serde_json::from_str("{}").unwrap();
        assert!(opts.import_react.is_none());
        assert!(opts.cache.is_none());
        assert!(opts.import_sources.is_none());
        assert!(opts.class_name_compression_map.is_none());
    }

    #[test]
    fn plugin_options_round_trip() {
        let original = PluginOptions {
            cache: Some(CacheMode::FilePass),
            import_react: Some(false),
            sort_at_rules: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&original).unwrap();
        // Surface key check — must be camelCase over the wire.
        assert!(json.contains("\"importReact\""));
        assert!(json.contains("\"sortAtRules\""));
        assert!(json.contains("\"file-pass\""));
        let back: PluginOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cache, Some(CacheMode::FilePass));
        assert_eq!(back.import_react, Some(false));
        assert_eq!(back.sort_at_rules, Some(true));
    }
}
