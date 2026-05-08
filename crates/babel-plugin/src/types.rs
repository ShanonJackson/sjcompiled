//! 1:1 port of `packages/babel-plugin/src/types.ts` — config-side
//! types only.
//!
//! Phase 2 §2.4 split: the encapsulation-sensitive `State` (and its
//! inner shapes — `CompiledImports`, `ImportedCompiledImports`,
//! `PragmaState`, `CleanupAction`, `CleanupKind`, `CacheSlot`) moved
//! to `state.rs` so PLAN.md §3.9.8's `pub(crate)` field-visibility
//! contract holds at the module boundary. This file keeps the
//! configuration / handler-call shapes that have no encapsulation
//! contract:
//!
//! * `PluginOptions` + `CacheMode` — userland plugin config (wire
//!   shape: camelCase; serde derive). Read-only by the visitor.
//! * `Tag` / `TagKind` — `(name, type)` describing a JSX/styled host.
//!   Built per-call from the AST; no shared state.
//! * `Metadata` / `MetadataContext` — the per-handler call shape
//!   carrying `&mut State` and traversal context.
//! * `TransformResult` — the return shape for in-process integration
//!   tests.
//!
//! Re-exports `State` (and its inner shapes) from `state.rs` so
//! external consumers see the full surface here. Internal modules
//! import directly from `crate::state`.
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
//!   indices, threaded through the visitor directly.
//!
//! Drift policy: when upstream `types.ts` changes, this file MUST be
//! updated in the same commit. Reviewers diff field-by-field.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ───────── Re-exports of encapsulated state shapes ─────────
//
// Internal callers (babel_plugin.rs, lib.rs) prefer `crate::state::*`
// for these. External callers see them here too, matching the
// original types.ts surface area.
pub use crate::state::{
    CacheSlot, CleanupAction, CleanupKind, CompiledImports, ImportedCompiledImports, PragmaState,
    State,
};

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
    /// `DEFAULT_IMPORT_SOURCES` from `@compiled/utils`).
    #[serde(default)]
    pub import_sources: Option<Vec<String>>,

    // `onIncludedFiles` is intentionally absent here — see module
    // docs. The contract is `included-files.json` sidecar.
    /// Run cssnano normalisation. Default `true`.
    #[serde(default)]
    pub optimize_css: Option<bool>,

    /// Consumer-supplied `resolver` config. Three accepted shapes:
    ///
    /// - **Object** — declarative JSON resolver per
    ///   `plugins/RESOLVER_SPEC.md` / `RESOLVER_SPEC_PART_TWO.md`.
    ///   Parsed into [`crate::resolver::config::ResolverConfig`] and
    ///   handed to [`crate::resolver::build_from_config`] in
    ///   `lib.rs::process`.
    /// - **String** — module path of a JS resolver
    ///   (e.g. `"@jira-dev/compiled-resolver"`). Cannot be honoured
    ///   inside the WASI plugin (PLAN.md §1 constraint 1: no JS
    ///   callbacks). Falls back to [`crate::resolver::build_default`];
    ///   the host wrapper is documented as the place that should swap
    ///   string-form `resolver` for the JSON object form before the
    ///   config reaches the plugin.
    /// - **Absent / null** — falls back to [`build_default`].
    ///
    /// Captured as a raw [`serde_json::Value`] so that mistyping any
    /// nested key (or sending a shape the plugin doesn't yet honour)
    /// cannot poison the whole [`PluginOptions`] deserialization. The
    /// pre-§5.4 typing (`Option<String>`) caused **every other**
    /// option to silently revert to its default whenever a consumer
    /// set `resolver: { ... }`, because serde failed the entire
    /// struct on the type mismatch — see `ct-afm-add-component-name-styled`
    /// for the reproduction.
    #[serde(default)]
    pub resolver: Option<serde_json::Value>,

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

    /// Project root used as the base for resolving relative
    /// `importSources` entries (e.g. `'./bar/stub-api'`) and for
    /// relative-path import-declaration matching. Mirrors upstream
    /// `state.opts.root ?? this.cwd` (`babel-plugin.ts:75`); Babel
    /// falls back to the babel cwd when `opts.root` is absent. The
    /// SWC plugin runs inside WASI with no `process.cwd()`, so the
    /// host wrapper (parity-harness `engines.ts`, production Parcel
    /// transformer) MUST thread the cwd through this field — there's
    /// no in-plugin fallback. When `None`, relative-path
    /// `importSources` entries are kept as-is (won't match relative
    /// userland imports), preserving the §2.3 deferral note's
    /// behaviour.
    #[serde(default)]
    pub root: Option<String>,

    /// **Optional perf / correctness knob — NOT part of the upstream
    /// `PluginOptions` surface.** Path under the WASI preopen
    /// (`/cwd/...`) to a postcard-encoded
    /// `cssnano_browserslist_snapshot::PrecomputedBrowserslist`
    /// produced by `precomputeBrowserslistDefault()` (NAPI). The
    /// plugin reads the file on each call and threads the decoded
    /// snapshot through `crates/css::TransformOpts::precomputed_browserslist_path`
    /// to `cssnano-preset-default`'s 5 browserslist-aware leaf
    /// plugins.
    ///
    /// **Required for correct WASI behaviour with non-default
    /// browserslist configs.** Absent this field, the leaf plugins
    /// fall back to `browserslist_shim::resolve("")` which inside
    /// WASI returns the wide `browserslist@4.24.2` defaults
    /// (including IE 11) — drift from the host's
    /// `.browserslistrc` resolution. See
    /// `DEFINITIVE_BROWSERSLIST_PLAN.md` for the bootstrap pattern
    /// (host writes the snapshot once, plugin reads on every call,
    /// OS page cache amortises the disk hit).
    ///
    /// **Inline-bytes delivery is intentionally NOT exposed here.**
    /// SWC's plugin config wire is `serde_json` → JSON (not
    /// `Buffer`), so the only viable delivery surface from the host
    /// to the WASI plugin is a path under the preopen. NAPI
    /// consumers (`@compiled/css-native`) get the inline-bytes
    /// surface via `TransformOpts::precomputedBrowserslist`
    /// (`Buffer`) instead.
    #[serde(default)]
    pub precomputed_browserslist_path: Option<String>,

    /// **Optional perf knob — NOT part of the upstream
    /// `PluginOptions` surface.** Path under the WASI preopen
    /// (`/cwd/...`) to a postcard-encoded autoprefixer prefix-tables
    /// snapshot produced by `precomputePrefixesDefault()` (NAPI).
    /// The plugin reads the file on each `transform_css` call and
    /// threads the bytes through
    /// `crates/css::TransformOpts::precomputed_prefixes_path` to
    /// the autoprefixer step, eliding the per-call
    /// `build_prefixes_default()` reconstruction (~6.6 ms/call on
    /// our bench host).
    ///
    /// **Byte-equality guarantee.** The snapshot path is
    /// equivalence-tested against the slow path by
    /// `crates/parity-runner --stage autoprefixer` over the 65-entry
    /// corpus and by `crates/css/examples/perf_precomputed.rs`'s
    /// sanity check. Threading it through here changes nothing
    /// about output bytes — only how the autoprefixer's `Prefixes`
    /// struct gets built.
    ///
    /// **Inline-bytes delivery is intentionally NOT exposed here**,
    /// for the same reason as
    /// [`Self::precomputed_browserslist_path`]: SWC's plugin config
    /// wire is JSON-only. NAPI consumers
    /// (`@compiled/css-native::transformCss`) get the inline-bytes
    /// surface via `TransformOpts::precomputedPrefixes` (`Buffer`).
    ///
    /// **WASI path translation.** As with
    /// [`Self::precomputed_browserslist_path`], the host wrapper
    /// passes a host-absolute path here; `lib.rs::process` rewrites
    /// it to `/cwd/<rel>` form via
    /// `compat::wasi_path::host_to_wasi` before downstream
    /// `std::fs::read` sees it. Native callers (`opts.root = None`)
    /// get a no-op translation, preserving unit-test behaviour.
    #[serde(default)]
    pub precomputed_prefixes_path: Option<String>,
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
    /// **§5.5 closure addition (claude-2026-05-05).** Per-call own-scope
    /// override read by the §5.6 evaluator's dispatch closure.
    ///
    /// JS upstream mutates `meta.ownPath = arrowFunctionExpressionPath`
    /// in `traverse-call-expression.ts:121` to swap the IIFE arrow's
    /// scope into the recursive `evaluateExpression(callee, meta)` call.
    /// The Rust port mirrors with this scoped, restorable field:
    /// `traverse_call_expression` sets it to `Some(iife_scope_id)`
    /// around the recursive evaluator call, then restores the prior
    /// value.
    ///
    /// **Read contract (§5.6):** when constructing the recursive
    /// `evaluate_expression` closure, the §5.6 evaluator reads
    /// `meta.own_scope_override` at each invocation. If `Some(id)`,
    /// the dispatch uses `id` as the `own_scope` parameter to leaves
    /// that take it (`traverse_identifier`, `evaluate_identifier`,
    /// `traverse_member_expression`, etc.). If `None`, the dispatch
    /// uses its environment-captured default.
    ///
    /// **Existing leaves are unaffected.** None of the §5.5 leaves
    /// read this field; they take `own_scope` as an explicit
    /// parameter. The override is dispatched-at-the-closure-boundary,
    /// not consumed by leaves.
    pub own_scope_override: Option<u32>,
    /// **§6.8f addition.** True when the current `extract_template_literal`
    /// invocation is processing a template literal that sits INSIDE a
    /// ConditionalExpression branch (i.e. the recursion entered via
    /// `extract_branch(Tpl)` → `build_css_inner(Tpl)` →
    /// `extract_template_literal(Tpl)`). Used by the optimization gate
    /// in `extract_template_literal` to skip per-interpolation
    /// `optimize_conditional_statement` when we're inside a branch —
    /// the inner optimization would split the branch into multiple
    /// CssItems, but `extract_branch::merged.len() > 1` rejects that.
    /// Mirrors upstream's `hasNestedTemplateLiteralsWithConditionalRules`
    /// case-1 detection (template-as-ternary-branch) without requiring
    /// the §5.6 parent-traversal index. See
    /// `crates/babel-plugin/src/utils/manipulate_template_literal.rs`
    /// for the corresponding gate documentation.
    pub in_conditional_branch: bool,
}

impl<'a> Metadata<'a> {
    /// Babel's `{ ...meta, context: ..., keyframe: ... }` reborrow.
    ///
    /// JS object spread shares the State reference and overrides
    /// fields. Rust requires an explicit reborrow because `State` is
    /// `&mut`-held — `&mut self` here lets us produce a fresh
    /// `Metadata` carrying a re-borrowed `&mut State` without
    /// running afoul of aliasing rules.
    ///
    /// Used by `utils::css_builders::extract_keyframes` to build the
    /// `MetadataContext::Keyframes { keyframe }` child meta the inner
    /// `build_css` walk needs.
    pub fn reborrow_with_context<'b>(&'b mut self, context: MetadataContext) -> Metadata<'b> {
        Metadata {
            state: &mut *self.state,
            parent_id: self.parent_id,
            own_id: self.own_id,
            context,
            own_scope_override: self.own_scope_override,
            in_conditional_branch: self.in_conditional_branch,
        }
    }

    /// Same shape, but keeps the existing context. Used at every call
    /// site where the JS spread is `{ ...meta }` (no overrides) and
    /// the Rust port needs to descend through a non-borrow-compatible
    /// signature.
    pub fn reborrow<'b>(&'b mut self) -> Metadata<'b> {
        Metadata {
            state: &mut *self.state,
            parent_id: self.parent_id,
            own_id: self.own_id,
            context: self.context.clone(),
            own_scope_override: self.own_scope_override,
            in_conditional_branch: self.in_conditional_branch,
        }
    }
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
