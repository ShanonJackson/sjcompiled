//! Phase 5 §5.4b/c — Generic Node-style resolver engine.
//!
//! Wraps `oxc_resolver`. The `Resolver` struct shape supports growth
//! without rewriting call sites — `resolve_sync` is the public
//! surface; everything inside is implementation detail.
//!
//! ## What §5.4b shipped
//!
//! Default-config path: a stock `oxc_resolver::Resolver` configured
//! only with `extensions`. Byte-parity-locked against
//! `enhanced-resolve@5.18.3` per
//! `crates/babel-plugin/RESOLVER_MATRIX.md`.
//!
//! ## What §5.4c adds (this checkpoint)
//!
//! [`TransformingFileSystem`] — a `FileSystem` adapter that wraps
//! `oxc_resolver::FileSystemOs` and intercepts every
//! `package.json` `read()` call: the bytes are parsed as JSON, the
//! configured `packageJsonTransforms` are applied in array order, and
//! the mutated bytes are returned to oxc_resolver. This is the
//! WASI-safe path the §5.4c architecture lock chose: NO on-disk
//! mutation, the transforms run at the read site, matching spec
//! §2.2 wording ("operations are applied in array order, after
//! reading and before exports resolution").
//!
//! When the config has no transforms, `build_from_config` returns
//! a stock `oxc_resolver::Resolver` (no wrapper) — zero overhead
//! for the default-config path. When the config has transforms,
//! `build_from_config` returns a `Resolver` backed by
//! `ResolverGeneric<TransformingFileSystem>` instead.
//!
//! ## What §5.4d enables next
//!
//! `preferFirst` per-request dispatch + per-context `mainFields` —
//! the spec §2.3 surface. The `Resolver` enum below already supports
//! holding per-context resolvers; §5.4d wires them in.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{
    FileMetadata, FileSystem, FileSystemOs, ResolveError, ResolveOptions,
    Resolver as DefaultResolver, ResolverGeneric,
};

use super::config::{PackageJsonTransform, ResolverConfig};
use super::default;
use super::prefer_first::{PreferFirstDispatcher, PreferFirstError};
use super::transforms::apply_transforms;

/// The runtime resolver. Built once on `Program::enter` (per the
/// §5.4 caching lock — no cross-call cache); dropped on
/// `Program::exit`. Cheap to construct.
///
/// `resolve_sync(from, request)` is the only call shape the
/// consumer (`utils/resolve_binding.rs`, §5.4e) needs. The Babel
/// production wrapper's `resolveSync(context, request)` is the
/// 1:1 model: `context` is an absolute file path; the resolver
/// uses `dirname(context)` as the resolution root and walks
/// `request` from there.
///
/// Two backing variants:
///
/// - [`ResolverInner::Default`] — stock `oxc_resolver` over
///   `FileSystemOs`. Zero overhead. Used when the consumer config
///   has no `packageJsonTransforms`.
/// - [`ResolverInner::Transforming`] — `oxc_resolver` over
///   [`TransformingFileSystem`], applying the configured transforms
///   to every `package.json` read. Used when the consumer config
///   has at least one transform.
pub struct Resolver {
    inner: ResolverInner,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `oxc_resolver`'s types don't implement Debug. We only need
        // the variant name for diagnostics; State's Debug print is
        // the only consumer.
        let kind = match &self.inner {
            ResolverInner::Default(_) => "Default",
            ResolverInner::Transforming(_) => "Transforming",
            ResolverInner::PreferFirst { .. } => "PreferFirst",
        };
        f.debug_struct("Resolver").field("inner", &kind).finish()
    }
}

enum ResolverInner {
    Default(DefaultResolver),
    Transforming(ResolverGeneric<TransformingFileSystem>),
    /// `preferFirst` rules in play (§5.4d). The dispatcher walks
    /// rules in array order; the first prefix-match returns its
    /// pre-built resolver. Non-matching requests fall through to
    /// `base`. `base` is itself a `ResolverGeneric<TransformingFileSystem>`
    /// so it inherits the same transform list as the rules — keeps
    /// transforms applied uniformly across both matched and
    /// non-matched requests.
    PreferFirst {
        base: ResolverGeneric<TransformingFileSystem>,
        dispatcher: PreferFirstDispatcher,
    },
}

impl Resolver {
    /// Resolve `request` against the directory containing
    /// `from_file`. `from_file` MUST be an absolute path; the
    /// resolver uses its parent directory as the resolution root,
    /// matching `createDefaultResolver`'s
    /// `resolver.resolveSync({}, dirname(context), request)` shape.
    ///
    /// Returns the absolute resolved path or an error from
    /// `oxc_resolver` (re-exported as [`super::ResolveError`]).
    pub fn resolve_sync(
        &self,
        from_file: &Path,
        request: &str,
    ) -> Result<PathBuf, ResolveError> {
        let dir = from_file
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let resolution = match &self.inner {
            ResolverInner::Default(r) => r.resolve(dir, request)?,
            ResolverInner::Transforming(r) => r.resolve(dir, request)?,
            ResolverInner::PreferFirst { base, dispatcher } => {
                // Walk rules in array order; first match wins.
                // Non-match falls through to base. Per spec §2.3
                // the matched rule's overrides REPLACE the base
                // exports/main fields — that's encoded in the
                // pre-built rule resolver, so the dispatch is just
                // a one-resolver pick.
                if let Some(rule_resolver) = dispatcher.match_request(request) {
                    rule_resolver.resolve(dir, request)?
                } else {
                    base.resolve(dir, request)?
                }
            }
        };
        Ok(resolution.full_path())
    }

    /// Construct from a pre-built default `oxc_resolver`. Internal;
    /// consumers should call [`super::build_default`] or
    /// [`build_from_config`].
    pub(crate) fn from_oxc(inner: DefaultResolver) -> Self {
        Self {
            inner: ResolverInner::Default(inner),
        }
    }

    /// Construct from a pre-built transforming `ResolverGeneric`.
    /// Internal; only [`build_from_config`] reaches here.
    pub(crate) fn from_transforming(
        inner: ResolverGeneric<TransformingFileSystem>,
    ) -> Self {
        Self {
            inner: ResolverInner::Transforming(inner),
        }
    }

    /// Construct with a preferFirst dispatcher in play.
    /// Internal; only [`build_from_config`] reaches here.
    pub(crate) fn from_prefer_first(
        base: ResolverGeneric<TransformingFileSystem>,
        dispatcher: PreferFirstDispatcher,
    ) -> Self {
        Self {
            inner: ResolverInner::PreferFirst { base, dispatcher },
        }
    }
}

/// Build a [`Resolver`] from a parsed [`ResolverConfig`].
///
/// `config_dir` is the directory the consumer's config file
/// (`.compiledcssrc`) lives in. It's used to resolve `fromFile`
/// indirections in `preferFirst[].match.specifierStartsWith`. For
/// configs with no `preferFirst` rules using `fromFile`, the value
/// is unused — pass any path (including a placeholder).
///
/// **Honoured fields:**
/// - `extensions` — passed through to `oxc_resolver::ResolveOptions`.
/// - `exports.fields` (§5.4d) — wired into `ResolveOptions::exports_fields`.
/// - `package_json_transforms` (§5.4c) — when non-empty, wires the
///   resolver through [`TransformingFileSystem`]; transforms apply
///   to every `package.json` read before exports / mainFields
///   resolution sees the bytes.
/// - `prefer_first` (§5.4d) — when non-empty, builds a
///   [`PreferFirstDispatcher`] alongside the base resolver; matched
///   requests route to per-rule pre-built resolvers with
///   `exports.fields` / `main.fields` overridden per rule.
///
/// **Parses-but-not-yet-honoured (future):**
/// - `contexts`, `default_context`, `extra_main_fields`,
///   `exports.conditions`. Schema validates today
///   (deny-unknown-fields in `config.rs`); engine wiring lands as
///   those checkpoints open. Each unhonoured field is documented
///   inline in `config.rs`.
///
/// # Errors
///
/// Returns [`PreferFirstError`] if a `preferFirst` rule's
/// `fromFile` indirection can't be read or has the wrong shape.
pub fn build_from_config(
    cfg: &ResolverConfig,
    config_dir: &Path,
) -> Result<Resolver, PreferFirstError> {
    let extensions = cfg
        .extensions
        .clone()
        .unwrap_or_else(default::default_code_extensions);

    let mut opts = ResolveOptions {
        extensions,
        // Build-time mode dispatch — see `default::build_default`
        // for the full rationale. WASI gets `false` (avoids
        // canonicalisation hang on symlinked entries); native
        // gets `true` to match Node's `realpathSync` semantics
        // and align `imported_filename` strings with upstream
        // Babel's `resolve.sync` output.
        symlinks: !cfg!(target_arch = "wasm32"),
        ..Default::default()
    };
    // §5.4d — `cfg.exports.fields` wiring. The schema models this
    // as `Option<Vec<String>>` (each entry a top-level field name).
    // oxc_resolver's `exports_fields` is `Vec<Vec<String>>` (each
    // inner Vec is a path-into-package-json, e.g.
    // `["af:exports"]` or `["nested", "key"]`). Wrap each top-level
    // name as a single-element path. Same shape conversion as
    // `prefer_first::build_rule_options`.
    if let Some(exports) = &cfg.exports {
        if let Some(fields) = &exports.fields {
            opts.exports_fields = fields.iter().map(|f| vec![f.clone()]).collect();
        }
        // `exports.conditions` (→ oxc_resolver's `condition_names`)
        // parses today but isn't yet wired. The §5.4d corpus doesn't
        // exercise non-default conditions (the Jira shape uses
        // `conditions: ["exports"]` which is a no-op against
        // oxc_resolver's empty default). When the first non-default
        // conditions fixture lands, wire it here.
    }

    let transforms_arc: Arc<[PackageJsonTransform]> = match &cfg.package_json_transforms {
        Some(transforms) if !transforms.is_empty() => Arc::from(transforms.clone()),
        _ => Arc::from(Vec::new()),
    };

    let prefer_first_active = cfg
        .prefer_first
        .as_ref()
        .map(|r| !r.is_empty())
        .unwrap_or(false);

    if prefer_first_active {
        // §5.4d path: dispatcher + transform-aware base.
        let dispatcher = PreferFirstDispatcher::build(
            cfg.prefer_first.as_deref().unwrap_or(&[]),
            &opts,
            &transforms_arc,
            config_dir,
        )?;
        let base_fs = TransformingFileSystem::with_transforms_arc(transforms_arc.clone());
        let base = ResolverGeneric::new_with_file_system(base_fs, opts);
        return Ok(Resolver::from_prefer_first(base, dispatcher));
    }

    if !transforms_arc.is_empty() {
        // §5.4c path: transforms only.
        let fs = TransformingFileSystem::with_transforms_arc(transforms_arc);
        return Ok(Resolver::from_transforming(
            ResolverGeneric::new_with_file_system(fs, opts),
        ));
    }

    // §5.4b path: stock default-config resolver.
    Ok(Resolver::from_oxc(DefaultResolver::new(opts)))
}

/// `FileSystem` adapter that wraps `oxc_resolver::FileSystemOs` and
/// applies [`PackageJsonTransform`]s to every `package.json`
/// `read()` call before returning the bytes.
///
/// **Why a `read()`-only intercept:** `oxc_resolver` reads
/// `package.json` via `self.fs.read(&package_json_path)` (cache_impl.rs
/// line 161 in 11.19.1) and feeds the bytes directly to its JSON
/// parser. `read_to_string` is used for `tsconfig.json`, which is
/// out of scope for §5.4c — the transforms apply to package.json
/// only per spec §2.2.
///
/// **What is NOT cached here.** The transforms re-run on every
/// `package.json` read. `oxc_resolver`'s internal package.json cache
/// (per-instance, lifetime of the resolver) sits ABOVE this layer —
/// it caches the parsed `PackageJson` struct, not the raw bytes —
/// so the transform cost is paid once per (path, resolver instance).
/// Per the §5.4 caching lock, no cross-call caching.
pub struct TransformingFileSystem {
    inner: FileSystemOs,
    transforms: Arc<[PackageJsonTransform]>,
}

impl TransformingFileSystem {
    /// Construct with a shared transform list. Internal — used by
    /// [`build_from_config`] when the parsed config has a non-empty
    /// `packageJsonTransforms` array. The §5.4d `preferFirst`
    /// dispatcher builds N+1 resolvers (base + one per rule) that
    /// all share the same transform list — using a single `Arc<[..]>`
    /// means one transform-list allocation per `build_from_config`
    /// call, not N+1.
    pub(crate) fn with_transforms_arc(
        transforms: Arc<[PackageJsonTransform]>,
    ) -> Self {
        Self {
            inner: <FileSystemOs as FileSystem>::new(),
            transforms,
        }
    }

    /// Apply the configured transforms to JSON bytes if `path` is a
    /// `package.json` and the bytes parse as a JSON object. Returns
    /// the (possibly mutated) bytes either way; never panics or
    /// returns an error caused by the transform layer alone — a
    /// malformed package.json passes through unchanged so
    /// `oxc_resolver`'s own JSON parser surfaces the parse error
    /// upstream.
    fn maybe_transform_package_json(&self, path: &Path, raw: Vec<u8>) -> Vec<u8> {
        if self.transforms.is_empty() {
            return raw;
        }
        let is_pkg_json = path
            .file_name()
            .map(|n| n == "package.json")
            .unwrap_or(false);
        if !is_pkg_json {
            return raw;
        }
        let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&raw) else {
            return raw;
        };
        if !value.is_object() {
            return raw;
        }
        apply_transforms(&mut value, &self.transforms);
        match serde_json::to_vec(&value) {
            Ok(bytes) => bytes,
            // Defensive: if re-serialization fails (which shouldn't
            // happen — every Value is serializable — but defend
            // against future serde_json regressions), return the
            // original bytes so resolution proceeds against the
            // pre-transform shape rather than failing the whole
            // resolve.
            Err(_) => raw,
        }
    }
}

impl FileSystem for TransformingFileSystem {
    // `oxc_resolver`'s `FileSystem::new` signature is cfg-gated on
    // its own `yarn_pnp` feature: `fn new()` without yarn_pnp,
    // `fn new(yarn_pnp: bool)` with. We pin `oxc_resolver` with
    // `default-features = false` (no `yarn_pnp`) — see
    // `crates/Cargo.toml` workspace pin — so only the no-arg
    // signature is reachable. If a future bump enables yarn_pnp,
    // this impl needs updating in lockstep.
    fn new() -> Self {
        Self {
            inner: <FileSystemOs as FileSystem>::new(),
            transforms: Arc::from(Vec::new()),
        }
    }

    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let raw = self.inner.read(path)?;
        Ok(self.maybe_transform_package_json(path, raw))
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        // tsconfig path — pass through verbatim. §5.4c does not
        // transform tsconfig.
        self.inner.read_to_string(path)
    }

    fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        self.inner.metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        self.inner.symlink_metadata(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        self.inner.read_link(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// End-to-end: build a config-driven resolver with one
    /// `renameKey` transform that promotes a non-existent
    /// `atlaskit:src` field — verifies the wiring (read-intercept
    /// + transform engine + oxc_resolver pipeline) doesn't blow up
    /// on default fixtures.
    #[test]
    fn build_from_config_with_transforms_doesnt_break_default_resolution() {
        let cfg = ResolverConfig {
            extensions: Some(vec![
                ".js".into(),
                ".jsx".into(),
                ".ts".into(),
                ".tsx".into(),
            ]),
            package_json_transforms: Some(vec![PackageJsonTransform::DeleteKey {
                key: "nonexistent".into(),
            }]),
            ..Default::default()
        };
        let resolver = build_from_config(&cfg, std::path::Path::new("/")).unwrap();

        // Resolve against the existing axis-1 seed fixture. The
        // transform is a no-op for that package.json (no
        // "nonexistent" key), so resolution must produce the
        // same path the §5.4b default-config gate produces.
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let consumer = repo_root
            .join("parity-harness/resolver-matrix/fixtures-source")
            .join("axis-1-pkg-main/main-only/consumer.js");
        let expected = repo_root
            .join("parity-harness/resolver-matrix/fixtures-source")
            .join("axis-1-pkg-main/main-only/node_modules/parity-pkg-main-only/lib/entry.js");

        // Skip if the seed fixture isn't present (defensive — the
        // §5.4a entry-gate landed it but a future repo move could
        // shift paths).
        if !consumer.exists() {
            return;
        }
        let resolved = resolver
            .resolve_sync(&consumer, "parity-pkg-main-only")
            .expect("noop transform should not break default resolution");

        // Canonicalize both for symlink tolerance.
        let resolved = fs::canonicalize(&resolved).unwrap_or(resolved);
        let expected = fs::canonicalize(&expected).unwrap_or(expected);
        assert_eq!(resolved, expected);
    }
}
