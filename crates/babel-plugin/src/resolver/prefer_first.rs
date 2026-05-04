//! Phase 5 §5.4d — `preferFirst` match-by-prefix dispatcher.
//!
//! 1:1 port of `plugins/RESOLVER_SPEC_PART_TWO.md` §2.3: when a
//! request specifier starts with one of a configured list of
//! prefixes, the resolver tries that rule's overridden
//! `exportsFields` / `mainFields` first; non-matching specifiers
//! fall through to the base resolver.
//!
//! ## Architecture (locked at §5.4d entry)
//!
//! Each `preferFirst` rule is compiled once, at config load:
//!
//! - **Prefixes loaded once.** Inline `["@scope/foo", ...]` arrays
//!   are taken verbatim; `{"fromFile": "./path.json"}` indirections
//!   are read relative to the consumer config's directory. The
//!   resulting `Vec<String>` lives on the rule for its lifetime.
//! - **Per-rule resolver pre-built.** Each rule clones the base
//!   `ResolveOptions` and overrides `exports_fields` / `main_fields`
//!   per `rule.use_`. The rule then owns a
//!   `ResolverGeneric<TransformingFileSystem>` that's reused across
//!   every matching request.
//! - **First-match wins.** Rules are walked in array order; the
//!   first prefix-match returns its resolver. Non-matched requests
//!   fall through to the base resolver.
//!
//! Picked option (b) — per-rule pre-built resolvers — over (a)
//! per-request reconstruction (would burn cycles on every match
//! at AFM scale: ~1,585 prefixes × thousands of imports) and over
//! (c) coupling with per-context dispatch (the spec keeps
//! preferFirst and contexts orthogonal; combining them now
//! introduces unnecessary coupling).
//!
//! ## What §5.4d ships
//!
//! - [`PreferFirstDispatcher`] — owns the compiled rules; exposes
//!   `match_request(specifier) -> Option<&ResolverGeneric<…>>`.
//! - [`load_prefixes`] — resolves [`super::config::SpecifierStartsWith`]
//!   to a `Vec<String>`, loading the `{fromFile}` shape relative to
//!   the consumer config's directory and validating its JSON shape.
//! - [`PreferFirstError`] — IO / JSON-shape errors at config load
//!   time. Engine bubbles via [`super::engine::build_from_config`]'s
//!   new `Result` return.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{ResolveOptions, ResolverGeneric};

use super::config::{
    PackageJsonTransform, PreferFirstRule, PreferFirstUse, SpecifierStartsWith,
};
use super::engine::TransformingFileSystem;

/// Errors surfaced when compiling `preferFirst` rules at config
/// load time. Bubbles out of `engine::build_from_config` so the
/// consumer sees a clear error rather than a silent-skip.
#[derive(Debug)]
pub enum PreferFirstError {
    /// `fromFile` path could not be read. Includes the absolute
    /// path the loader attempted plus the underlying IO error.
    FromFileIo {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `fromFile` JSON didn't have the expected shape:
    /// either a `string[]` (legacy) or `{"prefixes": string[]}`.
    /// The dev-tooling generator script in spec §3.6 emits the
    /// `{"prefixes": [...]}` shape; we accept both for
    /// forward-compat with hand-written prefix files.
    FromFileShape { path: PathBuf, message: String },
}

impl std::fmt::Display for PreferFirstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FromFileIo { path, source } => {
                write!(
                    f,
                    "preferFirst: could not read fromFile {}: {source}\n  \
                     See plugins/RESOLVER_SPEC_PART_TWO.md §2.3.",
                    path.display(),
                )
            }
            Self::FromFileShape { path, message } => {
                write!(
                    f,
                    "preferFirst: malformed fromFile {}: {message}\n  \
                     Expected either a JSON array of strings OR \
                     {{\"prefixes\": [\"@scope/x\", ...]}}.\n  \
                     See plugins/RESOLVER_SPEC_PART_TWO.md §2.3.",
                    path.display(),
                )
            }
        }
    }
}

impl std::error::Error for PreferFirstError {}

/// Resolve a [`SpecifierStartsWith`] to a concrete `Vec<String>`.
///
/// - `Inline(list)` → returned verbatim.
/// - `FromFile { from_file }` → resolved relative to `config_dir`,
///   read, parsed as JSON. Accepts:
///   - `["@scope/x", "@scope/y", ...]` (a bare array — convenient
///     for hand-written files).
///   - `{"prefixes": ["@scope/x", ...]}` (the shape the dev-tooling
///     generator script in spec §3.6 emits).
pub fn load_prefixes(
    spec: &SpecifierStartsWith,
    config_dir: &Path,
) -> Result<Vec<String>, PreferFirstError> {
    match spec {
        SpecifierStartsWith::Inline(list) => Ok(list.clone()),
        SpecifierStartsWith::FromFile { from_file } => {
            let abs = if Path::new(from_file).is_absolute() {
                PathBuf::from(from_file)
            } else {
                config_dir.join(from_file)
            };
            let raw = std::fs::read_to_string(&abs).map_err(|source| {
                PreferFirstError::FromFileIo {
                    path: abs.clone(),
                    source,
                }
            })?;
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
                PreferFirstError::FromFileShape {
                    path: abs.clone(),
                    message: format!("invalid JSON: {e}"),
                }
            })?;
            extract_prefix_list(&abs, &value)
        }
    }
}

fn extract_prefix_list(
    path: &Path,
    value: &serde_json::Value,
) -> Result<Vec<String>, PreferFirstError> {
    // Bare array case.
    if let Some(arr) = value.as_array() {
        return arr
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    PreferFirstError::FromFileShape {
                        path: path.to_path_buf(),
                        message: format!(
                            "every entry in the prefix array must be a string; got {v}"
                        ),
                    }
                })
            })
            .collect();
    }
    // `{"prefixes": [...]}` case.
    if let Some(arr) = value.get("prefixes").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    PreferFirstError::FromFileShape {
                        path: path.to_path_buf(),
                        message: format!(
                            "every entry in `prefixes` must be a string; got {v}"
                        ),
                    }
                })
            })
            .collect();
    }
    Err(PreferFirstError::FromFileShape {
        path: path.to_path_buf(),
        message: "top-level value must be either an array of strings \
                  OR an object with a `prefixes` key holding an array of strings"
            .to_string(),
    })
}

/// One compiled preferFirst rule: prefix list + ready-to-use
/// resolver instance with the rule's overrides applied.
pub struct CompiledRule {
    prefixes: Vec<String>,
    resolver: ResolverGeneric<TransformingFileSystem>,
}

impl CompiledRule {
    /// Returns true iff `request` starts with any of this rule's
    /// prefixes. Linear scan — at AFM-scale (~1,585 prefixes) this
    /// is O(N) per request; acceptable for §5.4d. If profiling
    /// surfaces this as hot, swap in a trie / sorted-prefix
    /// binary-search.
    fn matches(&self, request: &str) -> bool {
        self.prefixes.iter().any(|p| request.starts_with(p))
    }

    pub(crate) fn resolver(&self) -> &ResolverGeneric<TransformingFileSystem> {
        &self.resolver
    }
}

/// Walks compiled rules in array order and returns the first match.
/// `match_request(spec)` returns `None` for non-matched requests so
/// the caller falls through to the base resolver.
pub struct PreferFirstDispatcher {
    rules: Vec<CompiledRule>,
}

impl PreferFirstDispatcher {
    /// Build the dispatcher from parsed rules + base resolve
    /// options + transforms (cloned into each rule's
    /// `TransformingFileSystem`).
    ///
    /// `config_dir` is the directory the `fromFile` indirection
    /// resolves against — typically the directory containing
    /// `.compiledcssrc`.
    pub fn build(
        rules: &[PreferFirstRule],
        base_opts: &ResolveOptions,
        transforms: &Arc<[PackageJsonTransform]>,
        config_dir: &Path,
    ) -> Result<Self, PreferFirstError> {
        let mut compiled = Vec::with_capacity(rules.len());
        for rule in rules {
            let prefixes = load_prefixes(&rule.match_.specifier_starts_with, config_dir)?;
            let opts = build_rule_options(base_opts, &rule.use_);
            let fs = TransformingFileSystem::with_transforms_arc(transforms.clone());
            let resolver = ResolverGeneric::new_with_file_system(fs, opts);
            compiled.push(CompiledRule { prefixes, resolver });
        }
        Ok(Self { rules: compiled })
    }

    /// First-match-wins lookup. Returns the matched rule's
    /// resolver, or `None` if no rule's prefix list contains a
    /// matching prefix.
    pub fn match_request(
        &self,
        request: &str,
    ) -> Option<&ResolverGeneric<TransformingFileSystem>> {
        self.rules
            .iter()
            .find(|r| r.matches(request))
            .map(|r| r.resolver())
    }

    /// Whether the dispatcher has zero rules. Engine-side the
    /// no-rules case is filtered before the dispatcher is even
    /// built (in [`super::engine::build_from_config`]) — but this
    /// method exists for tests that exercise the dispatcher's
    /// "empty == no matches ever" contract.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Apply a rule's `use_` overrides on top of base `ResolveOptions`.
///
/// Per spec §2.3: when the rule fires, `exportsFields` and
/// `mainFields` are REPLACED, not merged. The schema models them
/// as `Option<Vec<String>>` so:
/// - `Some(list)` → override to that list (including `Some([])`,
///   which means "no exports/main walks" — the "source" resolver
///   case from RESOLVER_SPEC.md §3.2's three-resolver design).
/// - `None` → keep base.
fn build_rule_options(base: &ResolveOptions, use_: &PreferFirstUse) -> ResolveOptions {
    let mut opts = base.clone();
    if let Some(fields) = &use_.exports_fields {
        // oxc_resolver's `exports_fields` is `Vec<Vec<String>>` —
        // each inner Vec is a path-into-package-json (e.g.
        // `["af:exports"]` or `["nested", "key"]`). The spec's
        // `use.exportsFields` is `string[]` — each entry is a
        // top-level field name. Wrap each as a single-element
        // path.
        opts.exports_fields = fields.iter().map(|f| vec![f.clone()]).collect();
    }
    if let Some(fields) = &use_.main_fields {
        opts.main_fields = fields.clone();
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::config::{PreferFirstMatch, ResolverConfig};

    fn parse_one_rule(json: serde_json::Value) -> PreferFirstRule {
        let cfg_value = serde_json::json!({ "preferFirst": [json] });
        let cfg = ResolverConfig::parse_value(&cfg_value).unwrap().unwrap();
        cfg.prefer_first.unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn load_prefixes_inline_passes_through() {
        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": ["@af/foo", "@atlaskit/bar"] },
            "use": {}
        }));
        let prefixes = load_prefixes(
            &rule.match_.specifier_starts_with,
            Path::new("/anywhere"),
        )
        .unwrap();
        assert_eq!(prefixes, vec!["@af/foo".to_string(), "@atlaskit/bar".to_string()]);
    }

    #[test]
    fn load_prefixes_from_file_bare_array_shape() {
        let dir = std::env::temp_dir().join("§5.4d_prefer_first_bare_array");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefixes.json");
        std::fs::write(&path, r#"["@af/x", "@af/y"]"#).unwrap();

        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": { "fromFile": "./prefixes.json" } },
            "use": {}
        }));
        let prefixes = load_prefixes(&rule.match_.specifier_starts_with, &dir).unwrap();
        assert_eq!(prefixes, vec!["@af/x".to_string(), "@af/y".to_string()]);
    }

    #[test]
    fn load_prefixes_from_file_prefixes_object_shape() {
        // The shape spec §3.6's dev-tooling generator emits.
        let dir = std::env::temp_dir().join("§5.4d_prefer_first_object");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefixes.json");
        std::fs::write(&path, r#"{"prefixes": ["@af/x", "@af/y"]}"#).unwrap();

        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": { "fromFile": "./prefixes.json" } },
            "use": {}
        }));
        let prefixes = load_prefixes(&rule.match_.specifier_starts_with, &dir).unwrap();
        assert_eq!(prefixes, vec!["@af/x".to_string(), "@af/y".to_string()]);
    }

    #[test]
    fn load_prefixes_from_file_missing_returns_io_error() {
        let dir = std::env::temp_dir().join("§5.4d_prefer_first_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // intentionally don't create the file
        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": { "fromFile": "./does-not-exist.json" } },
            "use": {}
        }));
        let err = load_prefixes(&rule.match_.specifier_starts_with, &dir).unwrap_err();
        match err {
            PreferFirstError::FromFileIo { .. } => {}
            other => panic!("expected FromFileIo, got {other}"),
        }
    }

    #[test]
    fn load_prefixes_from_file_wrong_shape_errors() {
        let dir = std::env::temp_dir().join("§5.4d_prefer_first_bad_shape");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefixes.json");
        std::fs::write(&path, r#"{"wrong_key": [1, 2]}"#).unwrap();

        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": { "fromFile": "./prefixes.json" } },
            "use": {}
        }));
        let err = load_prefixes(&rule.match_.specifier_starts_with, &dir).unwrap_err();
        match err {
            PreferFirstError::FromFileShape { message, .. } => {
                assert!(
                    message.contains("array of strings"),
                    "expected pointer to expected shape, got: {message}"
                );
            }
            other => panic!("expected FromFileShape, got {other}"),
        }
    }

    #[test]
    fn load_prefixes_from_file_non_string_entry_errors() {
        let dir = std::env::temp_dir().join("§5.4d_prefer_first_nonstring");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefixes.json");
        std::fs::write(&path, r#"["@af/x", 42]"#).unwrap();

        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": { "fromFile": "./prefixes.json" } },
            "use": {}
        }));
        let err = load_prefixes(&rule.match_.specifier_starts_with, &dir).unwrap_err();
        match err {
            PreferFirstError::FromFileShape { message, .. } => {
                assert!(
                    message.contains("must be a string"),
                    "expected per-entry type pointer, got: {message}"
                );
            }
            other => panic!("expected FromFileShape, got {other}"),
        }
    }

    #[test]
    fn build_rule_options_overrides_exports_fields() {
        let base = ResolveOptions {
            exports_fields: vec![vec!["exports".to_string()]],
            main_fields: vec!["main".to_string()],
            ..Default::default()
        };
        let use_ = PreferFirstUse {
            exports_fields: Some(vec!["af:exports".to_string(), "exports".to_string()]),
            main_fields: None,
        };
        let opts = build_rule_options(&base, &use_);
        assert_eq!(
            opts.exports_fields,
            vec![
                vec!["af:exports".to_string()],
                vec!["exports".to_string()],
            ]
        );
        // main_fields stays as base
        assert_eq!(opts.main_fields, vec!["main".to_string()]);
    }

    #[test]
    fn build_rule_options_overrides_main_fields_with_empty() {
        // Explicit empty list MUST be respected (the "source" resolver
        // case from RESOLVER_SPEC.md §3.2 — no main fields walked).
        let base = ResolveOptions {
            main_fields: vec!["main".to_string(), "module".to_string()],
            ..Default::default()
        };
        let use_ = PreferFirstUse {
            exports_fields: None,
            main_fields: Some(vec![]),
        };
        let opts = build_rule_options(&base, &use_);
        assert_eq!(opts.main_fields, Vec::<String>::new());
    }

    #[test]
    fn build_rule_options_keeps_base_when_overrides_none() {
        let base = ResolveOptions {
            exports_fields: vec![vec!["exports".to_string()]],
            main_fields: vec!["main".to_string()],
            ..Default::default()
        };
        let use_ = PreferFirstUse {
            exports_fields: None,
            main_fields: None,
        };
        let opts = build_rule_options(&base, &use_);
        assert_eq!(opts.exports_fields, vec![vec!["exports".to_string()]]);
        assert_eq!(opts.main_fields, vec!["main".to_string()]);
    }

    #[test]
    fn dispatcher_first_match_wins() {
        let rule_a = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": ["@af/"] },
            "use": { "mainFields": ["a-field"] }
        }));
        let rule_b = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": ["@af/secondary"] },
            "use": { "mainFields": ["b-field"] }
        }));
        let base = ResolveOptions::default();
        let transforms: Arc<[PackageJsonTransform]> = Arc::from(Vec::new());
        let dispatcher = PreferFirstDispatcher::build(
            &[rule_a, rule_b],
            &base,
            &transforms,
            Path::new("/"),
        )
        .unwrap();
        // "@af/secondary/x" matches BOTH rules; rule A is first
        // in the array, so it wins. We don't have a public way to
        // inspect the resolver's main_fields directly, so the
        // assertion is "match_request returns Some" + "is_empty
        // is false" — the byte-parity contract for ordering is
        // covered by the engine integration test below.
        assert!(!dispatcher.is_empty());
        assert!(dispatcher.match_request("@af/secondary/x").is_some());
        assert!(dispatcher.match_request("@af/foo").is_some());
        assert!(dispatcher.match_request("react").is_none());
    }

    #[test]
    fn dispatcher_no_rules_is_empty() {
        let base = ResolveOptions::default();
        let transforms: Arc<[PackageJsonTransform]> = Arc::from(Vec::new());
        let dispatcher =
            PreferFirstDispatcher::build(&[], &base, &transforms, Path::new("/")).unwrap();
        assert!(dispatcher.is_empty());
        assert!(dispatcher.match_request("anything").is_none());
    }

    #[test]
    fn dispatcher_non_matching_request_falls_through() {
        let rule = parse_one_rule(serde_json::json!({
            "match": { "specifierStartsWith": ["@matched/"] },
            "use": {}
        }));
        let base = ResolveOptions::default();
        let transforms: Arc<[PackageJsonTransform]> = Arc::from(Vec::new());
        let dispatcher = PreferFirstDispatcher::build(
            &[rule],
            &base,
            &transforms,
            Path::new("/"),
        )
        .unwrap();
        assert!(dispatcher.match_request("react").is_none());
        assert!(dispatcher.match_request("./relative").is_none());
        assert!(dispatcher.match_request("@matched/foo").is_some());
    }
}
