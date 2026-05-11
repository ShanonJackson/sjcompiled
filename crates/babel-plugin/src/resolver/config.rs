//! Phase 5 §5.4b — Declarative `resolver: { ... }` JSON schema.
//!
//! 1:1 port of `plugins/RESOLVER_SPEC_PART_TWO.md` §2.1 (the
//! canonical schema, locked at the §5.4a architecture lock). Every
//! field in the spec maps to exactly one field on
//! [`ResolverConfig`]; unknown fields are rejected at parse time
//! via `#[serde(deny_unknown_fields)]`.
//!
//! ## What §5.4b ships
//!
//! - The full schema parses (deny-unknown-fields enforced today
//!   so consumers see typos at config-parse time, not at AFM-scale
//!   integration time).
//! - Strings/functions for the top-level `resolver` value are
//!   rejected with [`ResolverConfigError::Unsupported`] pointing at
//!   the spec.
//!
//! ## What §5.4c/§5.4d enable later
//!
//! - `package_json_transforms` — the 5-op transform engine (§5.4c).
//!   Today the field parses but is not yet honoured by the engine
//!   — the engine only consumes `extensions` per the §5.4b scope
//!   lock in `engine.rs`.
//! - `prefer_first` — the match-by-prefix dispatcher (§5.4d).
//!   Same parse-but-not-yet-honoured contract.
//! - `contexts.<name>.main_fields` + `default_context` — **honoured**
//!   as of the AFM `EditorContentContainer-compiled.tsx` SIGSEGV
//!   work. Engine wiring at `engine.rs::build_from_config` copies
//!   `contexts[default_context].main_fields` onto
//!   `oxc_resolver::ResolveOptions::main_fields`. Per-rule
//!   replacements still flow through `prefer_first`.
//! - `extra_main_fields` — **honoured** alongside `contexts` above.
//!   Prepended to whatever `main_fields` the active context resolves
//!   to. Replaces upstream's hard-coded `useModule2019MainField`.
//!
//! Each unhonoured field has a doc-comment pointing at the
//! checkpoint where it gets wired. Consumers see errors at
//! parse-time TODAY for typos / unknown fields; behavioural
//! divergences for the unhonoured fields surface at the §5.4c/d
//! gate corpora.

use indexmap::IndexMap;
use serde::Deserialize;

/// Top-level wrapper for the `resolver` config key. Discriminates
/// between the two valid shapes (object → schema; absent →
/// default-config) and explicitly rejects strings / functions
/// per PLAN.md §1 constraint 1.
///
/// Consumers parse `.compiledcssrc`'s `resolver` value via
/// [`ResolverConfig::parse_value`] which handles all three cases.
#[derive(Debug, Clone, Default)]
pub struct ResolverConfig {
    /// File-probing extensions in priority order. `None` = use
    /// [`crate::constants::DEFAULT_CODE_EXTENSIONS`].
    pub extensions: Option<Vec<String>>,

    /// Node-style exports resolution config. `None` = oxc_resolver
    /// bare default (`exports_fields: [["exports"]]`,
    /// `condition_names: []`).
    pub exports: Option<ExportsConfig>,

    /// Per-context resolver configurations. The key is the context
    /// name (e.g. `"browser"`, `"node"`); each value is the
    /// per-context settings. Key order is preserved via
    /// [`IndexMap`] — the spec doesn't lock iteration order, but
    /// future per-context dispatch may rely on the JSON order so
    /// preserving it is safer than relying on `HashMap`'s
    /// non-deterministic iteration.
    pub contexts: Option<IndexMap<String, ContextConfig>>,

    /// Default context name for non-relative imports. `None` =
    /// engine falls through to default-config behaviour.
    pub default_context: Option<String>,

    /// **§5.4c — parses today, NOT yet honoured by the engine.**
    /// The 5-op `packageJsonTransforms` engine lands as a separate
    /// checkpoint per the §5.4 sub-checkpoint split in STATUS.md.
    pub package_json_transforms: Option<Vec<PackageJsonTransform>>,

    /// **§5.4d — parses today, NOT yet honoured by the engine.**
    /// The `preferFirst` dispatcher lands as a separate checkpoint.
    pub prefer_first: Option<Vec<PreferFirstRule>>,

    /// Prepended to the active context's `main_fields`. Replaces
    /// upstream's hard-coded `useModule2019MainField` flag — a Jira
    /// consumer that wants `module:es2019` resolution sets
    /// `"extraMainFields": ["module:es2019"]` and `engine.rs`
    /// prepends it ahead of the context's own
    /// `main`/`module`/`browser` order. Per-rule `prefer_first`
    /// REPLACEMENTS bypass this prepend (spec §3.2).
    pub extra_main_fields: Option<Vec<String>>,
}

/// Errors surfaced at config-parse time.
#[derive(Debug)]
pub enum ResolverConfigError {
    /// Top-level `resolver` is a string (e.g. `"@jira-dev/compiled-resolver"`)
    /// or function-like value. Rejected per PLAN.md §1 constraint 1
    /// — the WASI plugin cannot load JS modules.
    Unsupported(String),
    /// Schema-level parse error (unknown field, type mismatch, etc.).
    /// Wraps `serde_json::Error` for line/column reporting.
    Schema(serde_json::Error),
}

impl std::fmt::Display for ResolverConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "{msg}"),
            Self::Schema(e) => write!(
                f,
                "resolver config schema error: {e}\n\
                 See plugins/RESOLVER_SPEC_PART_TWO.md §2.1 for the canonical \
                 declarative resolver schema."
            ),
        }
    }
}

impl std::error::Error for ResolverConfigError {}

impl ResolverConfig {
    /// Parse a `serde_json::Value` from the consumer's config under
    /// the `resolver` key. Returns:
    ///
    /// - `Ok(None)` if the value is `null` (treat as absent → caller
    ///   falls back to [`super::build_default`]).
    /// - `Ok(Some(cfg))` for a valid `{ ... }` object.
    /// - `Err(Unsupported)` for a string or function — the hard-fail
    ///   case PLAN.md §1 constraint 1 mandates.
    /// - `Err(Schema)` for a malformed object (unknown fields, type
    ///   mismatches, etc.).
    pub fn parse_value(
        v: &serde_json::Value,
    ) -> Result<Option<Self>, ResolverConfigError> {
        match v {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(_) => Err(ResolverConfigError::Unsupported(
                "resolver must be a JSON object — strings/functions are unsupported in the WASI plugin. \
                 See plugins/RESOLVER_SPEC_PART_TWO.md for the JSON shape."
                    .to_string(),
            )),
            // serde_json doesn't have a Function variant — JS functions
            // serialize to objects with non-standard keys when crossing
            // the WASI boundary. The host wrapper (Parcel transformer)
            // is responsible for stripping function-typed `resolver`
            // values BEFORE passing config to the plugin; if one slips
            // through it'll produce a Schema(Unknown field) error from
            // serde_json::from_value below — which is the right
            // outcome.
            serde_json::Value::Object(_) => {
                let cfg: Self = serde_json::from_value(v.clone())
                    .map_err(ResolverConfigError::Schema)?;
                Ok(Some(cfg))
            }
            _ => Err(ResolverConfigError::Unsupported(format!(
                "resolver must be a JSON object — got {} value. \
                 See plugins/RESOLVER_SPEC_PART_TWO.md for the JSON shape.",
                value_kind(v)
            ))),
        }
    }
}

fn value_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// Manual Deserialize so we can apply deny_unknown_fields + camelCase
// at the same time without losing the default-when-absent semantics
// for every Option<T> field.
impl<'de> serde::Deserialize<'de> for ResolverConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Raw {
            #[serde(default)]
            extensions: Option<Vec<String>>,
            #[serde(default)]
            exports: Option<ExportsConfig>,
            #[serde(default)]
            contexts: Option<IndexMap<String, ContextConfig>>,
            #[serde(default)]
            default_context: Option<String>,
            #[serde(default)]
            package_json_transforms: Option<Vec<PackageJsonTransform>>,
            #[serde(default)]
            prefer_first: Option<Vec<PreferFirstRule>>,
            #[serde(default)]
            extra_main_fields: Option<Vec<String>>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            extensions: raw.extensions,
            exports: raw.exports,
            contexts: raw.contexts,
            default_context: raw.default_context,
            package_json_transforms: raw.package_json_transforms,
            prefer_first: raw.prefer_first,
            extra_main_fields: raw.extra_main_fields,
        })
    }
}

/// Node-style exports resolution config — RESOLVER_SPEC_PART_TWO.md §2.1.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportsConfig {
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub conditions: Option<Vec<String>>,
}

/// Per-context resolver settings — `resolver.contexts.<name>`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextConfig {
    #[serde(default)]
    pub main_fields: Option<Vec<String>>,
}

/// One entry in `resolver.packageJsonTransforms[]`. Tagged on the
/// `op` field per RESOLVER_SPEC_PART_TWO.md §2.2.
///
/// **§5.4c will wire these into the resolver engine.** Today the
/// schema parses but the engine ignores transform entries — see
/// the doc-comment on [`ResolverConfig::package_json_transforms`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum PackageJsonTransform {
    /// `{ "op": "ensureObject", "key": "..." }`
    EnsureObject {
        key: String,
    },
    /// `{ "op": "renameKey", "from": "...", "to": "...", ... }`
    RenameKey {
        from: String,
        to: String,
        #[serde(default, rename = "ifTargetMissing")]
        if_target_missing: bool,
        #[serde(default)]
        wrap: Option<RenameKeyWrap>,
    },
    /// `{ "op": "renameMapEntry", "in": "...", "from": "...", "to": "...", ... }`
    RenameMapEntry {
        #[serde(rename = "in")]
        in_key: String,
        from: String,
        to: String,
        #[serde(default, rename = "ifTargetMissing")]
        if_target_missing: bool,
        #[serde(default, rename = "deleteSource")]
        delete_source: bool,
    },
    /// `{ "op": "setDefault", "in": "...", "entries": { ... } }`
    SetDefault {
        #[serde(rename = "in")]
        in_key: String,
        entries: IndexMap<String, serde_json::Value>,
    },
    /// `{ "op": "deleteKey", "key": "..." }`
    DeleteKey {
        key: String,
    },
}

/// `wrap` parameter on `renameKey` — when promoting a string-valued
/// field into an object-valued field, this describes the wrapping
/// shape. Currently only `{ "as": "object", "key": "..." }` is
/// defined by the spec; future shapes may extend.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenameKeyWrap {
    /// `"object"` — wrap the source value as
    /// `{ <key>: <source-value> }` in the destination key.
    #[serde(rename = "as")]
    pub as_kind: String,
    pub key: String,
}

/// One entry in `resolver.preferFirst[]` — RESOLVER_SPEC_PART_TWO.md §2.3.
///
/// **§5.4d will wire these into the resolver engine.** Today the
/// schema parses but the engine ignores preferFirst entries.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreferFirstRule {
    #[serde(rename = "match")]
    pub match_: PreferFirstMatch,
    #[serde(rename = "use")]
    pub use_: PreferFirstUse,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreferFirstMatch {
    /// Either an inline `["@af/...", "@atlaskit/..."]` list OR a
    /// `{"fromFile": "./local-platform-packages.json"}` indirection.
    /// Untagged enum so JSON consumers don't need a discriminator.
    pub specifier_starts_with: SpecifierStartsWith,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SpecifierStartsWith {
    Inline(Vec<String>),
    FromFile {
        #[serde(rename = "fromFile")]
        from_file: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreferFirstUse {
    #[serde(default)]
    pub exports_fields: Option<Vec<String>>,
    #[serde(default)]
    pub main_fields: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_absent_returns_none() {
        let v = serde_json::Value::Null;
        let cfg = ResolverConfig::parse_value(&v).unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn parse_string_rejected_with_spec_pointer() {
        let v = serde_json::json!("@jira-dev/compiled-resolver");
        let err = ResolverConfig::parse_value(&v).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("RESOLVER_SPEC_PART_TWO.md"), "got: {msg}");
        assert!(msg.contains("strings/functions"), "got: {msg}");
    }

    #[test]
    fn parse_array_rejected_with_value_kind() {
        let v = serde_json::json!([]);
        let err = ResolverConfig::parse_value(&v).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("array value"), "got: {msg}");
    }

    #[test]
    fn parse_unknown_field_rejected() {
        let v = serde_json::json!({ "unknownKey": 1 });
        let err = ResolverConfig::parse_value(&v).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "got: {msg}");
    }

    #[test]
    fn parse_minimal_object_succeeds() {
        let v = serde_json::json!({ "extensions": [".ts", ".js"] });
        let cfg = ResolverConfig::parse_value(&v).unwrap().unwrap();
        assert_eq!(
            cfg.extensions.as_deref(),
            Some(&[".ts".to_string(), ".js".to_string()][..])
        );
    }

    #[test]
    fn parse_full_jira_shape_succeeds() {
        // Equivalent of the .compiledcssrc shape from
        // RESOLVER_SPEC_PART_TWO.md §2.4.
        let v = serde_json::json!({
            "extensions": [".ts", ".tsx", ".mjs", ".js", ".jsx", ".cjs", ".json"],
            "exports": { "fields": ["exports"], "conditions": ["exports"] },
            "contexts": {
                "browser": { "mainFields": ["browser", "module", "main"] },
                "node":    { "mainFields": ["module", "main"] }
            },
            "defaultContext": "browser",
            "packageJsonTransforms": [
                { "op": "ensureObject", "key": "af:exports" },
                {
                    "op": "renameMapEntry",
                    "in": "af:exports",
                    "from": "./",
                    "to": ".",
                    "ifTargetMissing": true,
                    "deleteSource": true
                },
                {
                    "op": "renameKey",
                    "from": "atlaskit:src",
                    "to": "af:exports",
                    "ifTargetMissing": true,
                    "wrap": { "as": "object", "key": "." }
                },
                { "op": "deleteKey", "key": "atlaskit:src" }
            ],
            "preferFirst": [
                {
                    "match": { "specifierStartsWith": { "fromFile": "./platform-packages.json" } },
                    "use": { "exportsFields": ["af:exports", "exports"], "mainFields": [] }
                }
            ]
        });
        let cfg = ResolverConfig::parse_value(&v).unwrap().unwrap();
        assert_eq!(cfg.default_context.as_deref(), Some("browser"));
        assert_eq!(cfg.package_json_transforms.as_ref().unwrap().len(), 4);
        assert_eq!(cfg.prefer_first.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn prefer_first_inline_specifier_list_parses() {
        let v = serde_json::json!({
            "preferFirst": [
                {
                    "match": { "specifierStartsWith": ["@af/foo", "@atlaskit/bar"] },
                    "use": { "exportsFields": ["af:exports"] }
                }
            ]
        });
        let cfg = ResolverConfig::parse_value(&v).unwrap().unwrap();
        let first = &cfg.prefer_first.unwrap()[0];
        match &first.match_.specifier_starts_with {
            SpecifierStartsWith::Inline(list) => assert_eq!(list.len(), 2),
            _ => panic!("expected Inline"),
        }
    }
}
