//! Phase 5 §5.4b — In-plugin module resolver.
//!
//! ## Why this module exists (PLAN.md constraint-4 exception)
//!
//! `plugins/PLAN.md` constraint 4 mandates 1:1 file mapping between
//! `packages/babel-plugin/src/<path>.ts` and
//! `crates/babel-plugin/src/<path>.rs`. **This module is an explicit
//! exception** — there is no JS analogue to port.
//!
//! In production (today), the host wrapper (Parcel transformer) builds
//! a resolver via `createDefaultResolver(config)` (see
//! `plugins/PARCEL_USAGE_EXAMPLE.md`) wrapping `enhanced-resolve@5.x`,
//! and injects it into the plugin via `state.resolver`. The Rust SWC
//! plugin runs inside a WASI sandbox and **cannot accept a JS
//! callback** (PLAN.md §1 constraint 1) — so the host wrapper's role
//! moves *into* the plugin. PLAN.md §1 constraint 2 explicitly
//! authorises this:
//!
//! > "Module resolution is in-plugin via `oxc_resolver`. `oxc_resolver`
//! > is the Rust analogue of webpack's `enhanced-resolve` and covers
//! > the same surface."
//!
//! The §5.4a entry-gate (`crates/babel-plugin/RESOLVER_MATRIX.md`)
//! locks the byte-parity contract: every `(fromFile, request,
//! extensions)` triple this module returns must match what
//! `enhanced-resolve@5.18.3` produces, byte-for-byte. The
//! seed corpus at `parity-harness/resolver-matrix/` is the gate.
//!
//! ## Public surface
//!
//! - [`Resolver`] — the runtime resolver. Built via
//!   [`build_default`] (no `resolver` key in `.compiledcssrc`) or
//!   [`build_from_config`] (declarative `resolver: { ... }` JSON).
//! - [`ResolverConfig`] — the canonical declarative JSON schema
//!   from `plugins/RESOLVER_SPEC_PART_TWO.md` §2.1.
//! - [`ResolveError`] — re-exported from `oxc_resolver`.
//!
//! Two consumer modes (per the §5.4a architectural lock):
//!
//! 1. `resolver` absent → the plugin calls [`build_default`]. Defaults
//!    match `createDefaultResolver(config)` with empty `config.resolve`:
//!    just `extensions = config.extensions ?? DEFAULT_CODE_EXTENSIONS`,
//!    everything else inherits oxc_resolver's bare defaults.
//! 2. `resolver: { ... }` JSON object → the plugin calls
//!    [`build_from_config`] with the parsed schema. **Strings and
//!    functions are rejected at config-parse time** (PLAN.md §1
//!    constraint 1) with a hard error pointing at
//!    `plugins/RESOLVER_SPEC_PART_TWO.md`.
//!
//! ## What §5.4b ships
//!
//! Default-config path only. The §5.4a corpus (4 seed fixtures across
//! 4 axes) is byte-clean against `enhanced-resolve@5.18.3`. The 5-op
//! `packageJsonTransforms` engine (§5.4c) and the `preferFirst`
//! dispatcher (§5.4d) land in subsequent checkpoints. `config.rs`
//! parses the full schema today so consumers see deny-unknown-fields
//! errors at parse time, but the transforms/preferFirst arrays are
//! not yet honoured by the engine — they're staged for the next
//! checkpoints.
//!
//! ## What §5.4b does NOT ship
//!
//! - No caching layer. The host's `CachedInputFileSystem(fs, 4000)`
//!   is intentionally NOT replicated — WASI tears down the instance
//!   between `transformSync` calls (PLAN.md §3.9.4), so any
//!   cross-call cache is unsound. `oxc_resolver`'s per-instance
//!   in-memory caching during a single transform is sufficient.
//! - No host-injected resolver. PLAN.md §1 constraint 1 forbids JS
//!   callbacks; the JS host's `state.resolver.resolveSync` path is
//!   replaced by this module.
//! - No `tsconfig` paths. Default-config path matches enhanced-resolve
//!   *without* `TsconfigPathsPlugin` (the production wrapper doesn't
//!   load it for the no-config case). Adding tsconfig support is a
//!   §5.4c/d-or-later concern.

mod config;
mod default;
mod engine;
mod prefer_first;
mod transforms;

pub use config::{ResolverConfig, ResolverConfigError};
pub use default::build_default;
pub use engine::{build_from_config, Resolver};
pub use oxc_resolver::ResolveError;
pub use prefer_first::PreferFirstError;
pub use transforms::apply_transforms;
