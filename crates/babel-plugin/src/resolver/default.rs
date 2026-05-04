//! Phase 5 §5.4b — Default-config resolver factory.
//!
//! Mirrors what `createDefaultResolver(config)` produces when the
//! consumer's `.compiledcssrc` does NOT contain a `resolver` key —
//! i.e. the production path that runs in production today for every
//! AFM consumer that hasn't customised resolution.
//!
//! The JS reference (from `plugins/PARCEL_USAGE_EXAMPLE.md` and the
//! AFM-supplied `createDefaultResolver` source the user shared at
//! §5.4b architecture lock):
//!
//! ```js
//! ResolverFactory.createResolver({
//!   fileSystem: new CachedInputFileSystem(fs, 4000),
//!   ...(config.extensions && { extensions: config.extensions }),
//!   ...(config.resolve ?? {}),                  // empty for the default path
//!   useSyncFileSystemCalls: true,
//! });
//! ```
//!
//! With `config.resolve = {}` (the no-config case the §5.4a corpus
//! exercises), the only configured field is `extensions` —
//! everything else is enhanced-resolve's bare default. The Rust
//! analogue is `oxc_resolver::ResolveOptions { extensions, ..Default::default() }`.
//!
//! ## What's intentionally NOT replicated
//!
//! - **`CachedInputFileSystem(fs, 4000)`.** The 4-second fs cache in
//!   the host wrapper is unsound for WASI: SWC tears down the WASI
//!   instance between `transformSync` calls (PLAN.md §3.9.4), so any
//!   cross-call cache is unreachable anyway. `oxc_resolver`'s
//!   per-instance package.json caching during a single transform is
//!   sufficient — we re-instantiate the resolver on `Program::enter`
//!   and drop on `Program::exit`. The byte-parity contract in
//!   `crates/babel-plugin/RESOLVER_MATRIX.md` confirms identical
//!   resolved paths regardless of the cache layer.
//! - **`useSyncFileSystemCalls: true`.** In `oxc_resolver`, all
//!   resolution is sync by default — there's no async surface to
//!   opt out of.

use oxc_resolver::{ResolveOptions, Resolver as OxcResolver};

use crate::constants::DEFAULT_CODE_EXTENSIONS;

use super::engine::Resolver;

/// Convenience: clone `DEFAULT_CODE_EXTENSIONS` into the
/// `Vec<String>` shape `oxc_resolver::ResolveOptions::extensions`
/// expects. `oxc_resolver` doesn't accept `&'static [&str]`
/// directly.
pub(crate) fn default_code_extensions() -> Vec<String> {
    DEFAULT_CODE_EXTENSIONS.iter().map(|s| (*s).to_string()).collect()
}

/// Build a [`Resolver`] for the no-config case.
///
/// `extensions` corresponds to the consumer's `config.extensions`
/// option: pass `Some(...)` if the consumer set it, `None` otherwise
/// (in which case [`DEFAULT_CODE_EXTENSIONS`] is used). This
/// preserves the JS spread semantics:
/// `config.extensions ? { extensions: config.extensions } : {}`,
/// which means "if the consumer didn't set extensions, fall through
/// to enhanced-resolve's default" — but enhanced-resolve's default
/// is `['.js', '.json']`, NOT what Compiled wants. The JS plugin
/// falls back to `DEFAULT_CODE_EXTENSIONS` at the call site
/// (`resolve-binding.ts:299` — `meta.state.opts.extensions ?? DEFAULT_CODE_EXTENSIONS`).
/// We mirror that here so the resolver always sees a complete
/// extensions list before `oxc_resolver` runs.
pub fn build_default(extensions: Option<&[String]>) -> Resolver {
    let exts = extensions
        .map(|s| s.to_vec())
        .unwrap_or_else(default_code_extensions);
    let opts = ResolveOptions {
        extensions: exts,
        ..Default::default()
    };
    Resolver::from_oxc(OxcResolver::new(opts))
}
