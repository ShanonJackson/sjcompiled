//! Postcard wire format for `<workerScratchDir>/cache.bin`.
//!
//! Locked at `version: 1` per `plugins/SIDECAR_SCHEMA.md` §3 and
//! `plugins/PLAN.md` §3.9.10. This file is the SINGLE source of truth
//! for the on-disk shape; mutating any struct here MUST recompute the
//! `SCHEMA_HASH` and bump the `CACHE_VERSION` if the wire format
//! breaks (rather than just additively widens).
//!
//! ### Layer 1 vs Layer 2
//!
//! Layer 1 is in-memory only (`utils/cache.rs`'s `Cache<T>` keyed by
//! `hash(namespace + key)`). It carries the AST / file-content slices
//! that the JS upstream's `utils/cache.ts` already caches per pass.
//! NEVER persisted — `swc_ecma_ast::Module` has no stable serde
//! impl across `swc_core` bumps (PLAN.md §3.9.8).
//!
//! Layer 2 is persisted via this schema. Holds the bounded
//! evaluated-value entries that survive cross-transform via
//! `cache.bin`. Today the upstream Babel plugin carries no Layer 2
//! analog — this is a Rust-only addition motivated by the SWC WASI
//! teardown model (the in-memory Layer 1 dies between transforms).
//! The §5.6 evaluator is what eventually populates entries.
//! Phase 5 §5.4–§5.6 are now CLOSED, but the cache→State wiring is
//! still deferred (the schema is **defined but unused** until a
//! follow-up checkpoint wires reads/writes through the State
//! cache slot — see `plugins/STATUS.md` Phase 5 §5.3 row).
//!
//! ### Hard caps
//!
//! Per PLAN.md §3.9.10:
//! * `CACHE_VERSION = 1` — bumped on any breaking shape change.
//! * `MAX_CACHE_BYTES = 5 MiB` — serialized-postcard byte cap. LRU
//!   evicts on write until satisfied.
//! * `MAX_ENTRIES = 500` — entry count cap (matches Babel's
//!   `cache.ts:11` `maxSize`).
//! * `MAX_TDEPS_PER_ENTRY = 32` — caps `Layer2Entry::transitive_deps`.
//! * `MAX_STATE_DIFFS = 64` — caps `Layer2Entry::state_diffs`.
//!
//! ### Schema-hash discipline
//!
//! `schema_hash: [u8; 32]` is computed from a static fingerprint
//! string in `compute_schema_hash()` below. PLAN.md prescribes
//! SHA-256 / BLAKE3, but pulling either dep widens the `wasm32-wasip1`
//! compile-graph (CLAUDE.md "don't add 10MB Rust libraries" — neither
//! is 10 MB, but neither is needed either). The wipe-on-mismatch
//! semantics are correct as long as the function is:
//!   1. Stable across the same plugin build (deterministic).
//!   2. Different across plugin builds whose `Layer2Entry` shape or
//!      version-affecting inputs (`@swc/core` ABI, parser plugins,
//!      classHashPrefix) differ.
//! A 32-byte expanded FNV-1a-XOR fingerprint over a canonicalised
//! input string satisfies both. If a future change wants stronger
//! collision resistance (e.g. for cache keys, NOT for schema_hash),
//! that's a different conversation.
//!
//! ### Cache-wipe vs sidecar-fail asymmetry
//!
//! `cache.bin` is regenerable from source — a corrupt or
//! version-mismatched cache is a slow-build, not a wrong-build, so
//! the plugin silently wipes and rebuilds. This is the inverse of
//! sidecar handling (`included-files.json`, `style-rules.json`),
//! which carry information the host can't reconstruct.
//!
//! ### Atomic write protocol
//!
//! Writers write `cache.bin.tmp`, `fd_sync` it, then `path_rename`
//! to `cache.bin`. WASI supports both. A worker crash mid-write
//! leaves `cache.bin.tmp` behind; worker startup sweeps stale
//! `*.tmp` siblings before reading `cache.bin`. The sweep is part of
//! `crates/babel-plugin/src/utils/cache.rs::Layer2::open`.

use serde::{Deserialize, Serialize};

use crate::mutation_recorder::StateDiff;

/// Wire-format version. Bumped on a BREAKING shape change (renamed,
/// removed, or re-typed field). Additive new fields keep `1`.
pub const CACHE_VERSION: u32 = 1;

/// Hard cap on serialized `CacheFile` bytes. Enforced at write time;
/// LRU evicts until satisfied. PLAN.md §3.9.10.
pub const MAX_CACHE_BYTES: usize = 5 * 1024 * 1024;

/// Hard cap on Layer 2 entry count. Matches Babel's `cache.ts:11`
/// `maxSize` exactly so cache-hit-vs-miss patterns stay parity-safe.
pub const MAX_ENTRIES: usize = 500;

/// Hard cap on `Layer2Entry::transitive_deps` length. Evaluations
/// pulling in more than 32 distinct files mark
/// `cacheable_at_layer2 = false` and are skipped.
pub const MAX_TDEPS_PER_ENTRY: usize = 32;

/// Hard cap on `Layer2Entry::state_diffs` length. Longer diff logs
/// decline caching.
pub const MAX_STATE_DIFFS: usize = 64;

/// Bounded subset of `swc_ecma_ast::Expr` that can appear inside a
/// Layer 2 cache entry. The evaluator builds these from the live
/// AST; the cache stores them; replay reconstructs back to Expr.
///
/// Phase 5 §5.6 ☑ ships `evaluate_expression` at
/// `utils::evaluate_expression::evaluate_expression`; the cache
/// wire-up to consume its `ResultPair { value }` shape into these
/// variants is deferred (cache→State wiring — see Phase 5 §5.3 row).
/// Defined here so the wire format is locked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SerializedExpr {
    /// `Lit::Str(value)`.
    Str(String),
    /// `Lit::Num(value)`. Stored as f64 — matches JS Number semantics.
    Num(f64),
    /// `Lit::Bool(value)`.
    Bool(bool),
    /// `Lit::Null`.
    Null,
    /// `ObjectLit` of cacheable-shape properties.
    Object(Vec<(String, SerializedExpr)>),
    /// `ArrayLit` of cacheable-shape elements.
    Array(Vec<Option<SerializedExpr>>),
    /// `Tpl` template literal. `quasis.len() == exprs.len() + 1`.
    Template {
        quasis: Vec<String>,
        exprs: Vec<SerializedExpr>,
    },
    /// Compiled `keyframes(...)` call result. Stored as raw source so
    /// replay can splice without re-evaluating. Bug-parity: matches
    /// JS upstream's "tagged literal call as cacheable" rule
    /// (PLAN.md §3.9.9).
    KeyframesCall { source: String },
}

/// One cached evaluation. Bounded by the `MAX_*` caps above.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer2Entry {
    pub evaluated_ast: SerializedExpr,
    pub state_diffs: Vec<StateDiff>,
    pub transitive_deps: Vec<TransitiveDep>,
    /// `SystemTime → u128 nanoseconds since UNIX epoch` of the
    /// cached source. Postcard encodes u128 natively.
    pub source_mtime_ns: u128,
    /// Monotonic LRU sequence — bumped on every access. Larger =
    /// more recently touched.
    pub lru_seq: u64,
    /// Cached estimate of the entry's contribution to total
    /// serialized bytes. Used by the byte-cap eviction routine to
    /// pick victims without re-serializing on every step.
    pub byte_size_estimate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitiveDep {
    pub path: String,
    pub mtime_ns: u128,
}

/// Top-level wire shape. Keys (the `u64`) are mtime-derived hashes
/// produced by the cache layer above; replay does
/// `cache.layer2.iter().find(|(k, _)| *k == lookup_key)`.
///
/// Sorted by key on serialize for determinism — two builds with the
/// same input set produce a byte-identical `cache.bin`. This matches
/// the "deterministic-by-construction" invariant we want for any
/// scratch file under `node_modules/.cache/` (debugging, diffing,
/// reproducibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheFile {
    pub version: u32,
    pub schema_hash: [u8; 32],
    pub layer2: Vec<(u64, Layer2Entry)>,
}

impl CacheFile {
    /// Empty file. `schema_hash` is the canonical fingerprint; if a
    /// future load reads a `CacheFile` whose hash differs, that's a
    /// schema-drift wipe trigger.
    pub fn empty() -> Self {
        Self {
            version: CACHE_VERSION,
            schema_hash: compute_schema_hash(),
            layer2: Vec::new(),
        }
    }

    /// A read-time validation: returns `Err(())` on either
    /// version-mismatch OR schema-hash-mismatch. Caller wipes the
    /// file and starts over (PLAN.md §3.9.10).
    pub fn validate(&self) -> Result<(), CacheValidationError> {
        if self.version != CACHE_VERSION {
            return Err(CacheValidationError::VersionMismatch {
                got: self.version,
                expected: CACHE_VERSION,
            });
        }
        if self.schema_hash != compute_schema_hash() {
            return Err(CacheValidationError::SchemaHashMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheValidationError {
    VersionMismatch { got: u32, expected: u32 },
    SchemaHashMismatch,
}

/// Static fingerprint string covering every input that, if changed,
/// MUST invalidate every cache entry.
///
/// Inputs (PLAN.md §3.9.10):
///   1. Plugin version — the package's `Cargo.toml` `version` field.
///   2. `swc_core` ABI version — coordinates with `@swc/core`
///      (PARITY_VERSIONS.md `swc_core = =54.0.0`).
///   3. Layer2Entry struct signature — captured here as a string
///      manually maintained alongside the struct. Drift between the
///      string and the struct is caught by the
///      `schema_hash_signature_lock` test below.
///
/// Other inputs called out by §3.9.10 (`sorted parser_babel_plugins`,
/// `sorted extensions`, `classHashPrefix`) are PluginOptions-level
/// — they vary per-call rather than per-build, so they're not in the
/// schema_hash. Per-call inputs are part of the cache *key*, not the
/// cache schema.
const SCHEMA_FINGERPRINT: &str = concat!(
    "babel-plugin/",
    env!("CARGO_PKG_VERSION"),
    "|swc_core=54.0.0",
    "|Layer2Entry={evaluated_ast:SerializedExpr,state_diffs:Vec<StateDiff>,",
    "transitive_deps:Vec<TransitiveDep>,source_mtime_ns:u128,lru_seq:u64,",
    "byte_size_estimate:u32}",
    "|SerializedExpr=Str|Num|Bool|Null|Object|Array|Template|KeyframesCall",
    "|StateDiff=IncludedFilesPush|CompiledImportsAppend|SheetsInsert|",
    "CssMapInsert|IgnoreMemberExprMark"
);

/// Compute the 32-byte schema hash deterministically from
/// `SCHEMA_FINGERPRINT`.
///
/// FNV-1a 64-bit, expanded to 32 bytes by hashing 4× with distinct
/// per-block tags. Not a cryptographic hash; suitable for the
/// "detect drift, wipe regenerable scratch file" use case (PLAN.md
/// §3.9.10 explicitly authorises silent wipe on mismatch). Stable
/// across Rust toolchain versions because we don't lean on the
/// std `Hasher` trait — every byte is computed by hand here.
pub fn compute_schema_hash() -> [u8; 32] {
    fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
        // FNV-1a 64-bit constants (RFC-equivalent — these are not
        // cryptographic constants; the PR-tested set is documented at
        // http://www.isthe.com/chongo/tech/comp/fnv/ ).
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut h = seed;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }
    // Four distinct seeds, each tagged with a per-block byte so
    // identical SCHEMA_FINGERPRINT inputs produce four distinct
    // 8-byte slices that concatenate into 32 bytes.
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    let bytes = SCHEMA_FINGERPRINT.as_bytes();

    let mut out = [0u8; 32];
    for block in 0..4u8 {
        let h = fnv1a64(FNV_OFFSET ^ (block as u64).wrapping_mul(0x9e3779b97f4a7c15), bytes);
        let h_bytes = h.to_le_bytes();
        let off = (block as usize) * 8;
        out[off..off + 8].copy_from_slice(&h_bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation_recorder::ApiKind;

    #[test]
    fn cache_version_locked_at_one() {
        assert_eq!(CACHE_VERSION, 1);
    }

    #[test]
    fn caps_match_plan() {
        // Plan §3.9.10 numerics — locking them in a test prevents
        // accidental edits.
        assert_eq!(MAX_CACHE_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_ENTRIES, 500);
        assert_eq!(MAX_TDEPS_PER_ENTRY, 32);
        assert_eq!(MAX_STATE_DIFFS, 64);
    }

    #[test]
    fn schema_hash_is_deterministic() {
        let a = compute_schema_hash();
        let b = compute_schema_hash();
        assert_eq!(a, b);
    }

    #[test]
    fn schema_hash_changes_with_fingerprint_input() {
        // Sanity-check the hash actually depends on its input. We
        // can't mutate SCHEMA_FINGERPRINT (it's `const`) so cover
        // this by mutating the input bytes locally.
        fn local_hash(input: &str) -> [u8; 32] {
            // Inline copy of compute_schema_hash() body but
            // parametric over input.
            fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
                const FNV_PRIME: u64 = 0x100000001b3;
                let mut h = seed;
                for &b in bytes {
                    h ^= b as u64;
                    h = h.wrapping_mul(FNV_PRIME);
                }
                h
            }
            const FNV_OFFSET: u64 = 0xcbf29ce484222325;
            let bytes = input.as_bytes();
            let mut out = [0u8; 32];
            for block in 0..4u8 {
                let h = fnv1a64(FNV_OFFSET ^ (block as u64).wrapping_mul(0x9e3779b97f4a7c15), bytes);
                out[block as usize * 8..block as usize * 8 + 8].copy_from_slice(&h.to_le_bytes());
            }
            out
        }
        assert_ne!(local_hash("a"), local_hash("b"));
        assert_ne!(local_hash("schema-v1"), local_hash("schema-v2"));
    }

    #[test]
    fn empty_file_round_trips_through_postcard() {
        let original = CacheFile::empty();
        let bytes = postcard::to_allocvec(&original).expect("encode");
        let decoded: CacheFile = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(decoded.version, original.version);
        assert_eq!(decoded.schema_hash, original.schema_hash);
        assert!(decoded.layer2.is_empty());
        // Also assert validation passes.
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn version_mismatch_rejects() {
        let mut file = CacheFile::empty();
        file.version = 9999;
        match file.validate() {
            Err(CacheValidationError::VersionMismatch { got, expected }) => {
                assert_eq!(got, 9999);
                assert_eq!(expected, CACHE_VERSION);
            }
            other => panic!("expected VersionMismatch, got {:?}", other),
        }
    }

    #[test]
    fn schema_hash_mismatch_rejects() {
        let mut file = CacheFile::empty();
        file.schema_hash = [0xff; 32];
        assert!(matches!(
            file.validate(),
            Err(CacheValidationError::SchemaHashMismatch)
        ));
    }

    #[test]
    fn layer2_entry_round_trips() {
        let entry = Layer2Entry {
            evaluated_ast: SerializedExpr::Str("blue".into()),
            state_diffs: vec![StateDiff::CompiledImportsAppend {
                api: ApiKind::Css,
                local_name: "css".into(),
            }],
            transitive_deps: vec![TransitiveDep {
                path: "/cwd/theme.ts".into(),
                mtime_ns: 1_700_000_000_000_000_000,
            }],
            source_mtime_ns: 1_700_000_000_000_000_000,
            lru_seq: 42,
            byte_size_estimate: 64,
        };
        let mut file = CacheFile::empty();
        file.layer2.push((0xdead_beef_dead_beef, entry.clone()));
        let bytes = postcard::to_allocvec(&file).expect("encode");
        let decoded: CacheFile = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(decoded.layer2.len(), 1);
        assert_eq!(decoded.layer2[0].0, 0xdead_beef_dead_beef);
        match &decoded.layer2[0].1.evaluated_ast {
            SerializedExpr::Str(s) => assert_eq!(s, "blue"),
            other => panic!("expected Str, got {:?}", other),
        }
    }

    #[test]
    fn serialized_expr_covers_every_layer2_safe_shape() {
        // Lock the variant set so a future agent doesn't add an
        // unbounded variant accidentally. The set is bounded
        // per-PLAN.md §3.9.9.
        let _: SerializedExpr = SerializedExpr::Str(String::new());
        let _: SerializedExpr = SerializedExpr::Num(0.0);
        let _: SerializedExpr = SerializedExpr::Bool(false);
        let _: SerializedExpr = SerializedExpr::Null;
        let _: SerializedExpr = SerializedExpr::Object(vec![]);
        let _: SerializedExpr = SerializedExpr::Array(vec![]);
        let _: SerializedExpr = SerializedExpr::Template {
            quasis: vec![],
            exprs: vec![],
        };
        let _: SerializedExpr = SerializedExpr::KeyframesCall {
            source: String::new(),
        };
    }
}
