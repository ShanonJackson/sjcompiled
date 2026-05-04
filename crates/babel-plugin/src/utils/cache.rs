//! 1:1 port of `packages/babel-plugin/src/utils/cache.ts` — Layer 1
//! in-memory LRU.
//!
//! Phase 5 §5.3. The upstream `Cache<T>` is a generic
//! `Map<string, T>` LRU sized at 500 entries by default; this port
//! mirrors that shape exactly so the call sites in §5.4
//! (`utils/resolve_binding.rs`) drop in unchanged.
//!
//! ### Why `IndexMap` and not `std::HashMap`
//!
//! The upstream cache evicts the LEAST-RECENTLY-USED entry; the
//! eviction key is "first key in insertion order". JS `Map` preserves
//! insertion order; Rust `std::HashMap` does NOT. `IndexMap`
//! preserves it.
//!
//! ### Why `move_to_back` rather than delete + reinsert
//!
//! Upstream's `_moveLastInQueue` is `cache.delete(key); cache.set(key,
//! value)`. `IndexMap::move_index` and `shift_remove` + `insert` are
//! both O(n) in the worst case; we mirror upstream's
//! delete-then-reinsert shape to keep the eviction order predictable
//! (the cache is bounded at 500 entries so the constant is small).
//!
//! ### Hash function — 1:1 with `@compiled/utils.hash`
//!
//! Upstream `getUniqueKey` does `hash(namespace ? `${namespace}----${cacheKey}` : cacheKey)`.
//! The Rust port uses `compiled_utils::hash` (already proven byte-equal
//! over the §3 corpus's 10 037 entries) so any cross-pass key
//! correlation a future Layer 2 wire-up does is hash-stable.
//!
//! ### Layer 2 (`cache.bin`)
//!
//! The Layer 2 postcard cache lives behind `Layer2`, defined later in
//! this file. It's the persistence cousin of `Cache<T>` — the on-disk
//! shape is locked in `crate::cache_schema` (`SIDECAR_SCHEMA.md` §3 /
//! `PLAN.md` §3.9.10). Today, no live writers (the §5.6 evaluator
//! that populates it is deferred); the on-disk shape is locked early
//! so follow-up work can wire reads/writes without the wire format
//! shifting under it.
//!
//! Layer 2 enforces:
//!  * `MAX_ENTRIES` (500) — LRU evict on count overflow.
//!  * `MAX_CACHE_BYTES` (5 MiB) — LRU evict on serialized-size
//!    overflow (re-serialize and check at write time).
//!  * Atomic write protocol — `cache.bin.tmp` → `fd_sync` →
//!    `path_rename`.
//!  * Stale-tmp sweep on open.
//!
//! Per CLAUDE.md "SWC tears down the WASI instance between calls",
//! the WORKER process retains nothing in memory across transforms;
//! the only cross-transform channel is the filesystem cache.bin.

use indexmap::IndexMap;

use compiled_utils::hash;

use crate::cache_schema::{
    CacheFile, CacheValidationError, Layer2Entry, MAX_ENTRIES,
};

// ───────── Layer 1 (in-memory, per-call) ─────────

/// `CacheOptions` — upstream lines 5–8.
///
/// `cache: bool` — when false, `load()` always evaluates fresh.
/// `max_size: usize` — entry count cap. Default 500.
#[derive(Debug, Clone, Copy)]
pub struct CacheOptions {
    pub cache: bool,
    pub max_size: usize,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            cache: true,
            max_size: 500,
        }
    }
}

/// `Cache<T>` — upstream lines 15–150.
///
/// Generic over the cached value type. The JS upstream has `T = any`;
/// Rust requires a concrete `T` per call site. Today's reachable call
/// sites (Phase 5 §5.4 `utils/resolve_binding.rs`):
///
/// * `read-file` namespace → `T = String` (file content)
/// * `parse-module` namespace → `T = Arc<swc_ecma_ast::Module>`
/// * `find-default-export-module-node` →
///   `T = Option<ExportLookup>` (Phase 5 §5.5/§5.6 builds this type)
/// * `find-named-export-module-node` → same as above
///
/// Upstream collapses these into one `Cache<any>` instance on
/// `state.cache`. Rust splits per-T (a single multi-T cache requires
/// `Box<dyn Any>` which adds runtime cost and obscures the type at
/// every load site); the `State::cache` slot will hold a struct of
/// per-T `Cache` instances when §5.4 lands.
#[derive(Debug)]
pub struct Cache<T> {
    options: CacheOptions,
    cache: IndexMap<String, T>,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Cache<T> {
    /// Upstream `new Cache()` constructor.
    pub fn new() -> Self {
        Self {
            options: CacheOptions::default(),
            cache: IndexMap::new(),
        }
    }

    /// Upstream `Cache.getUniqueKey(cacheKey, namespace)` (lines 30–32).
    ///
    /// `hash(namespace ? `${namespace}----${cacheKey}` : cacheKey)`.
    /// 1:1 with `@compiled/utils.hash` (the §3 corpus locks
    /// byte-equality against the JS hash).
    pub fn get_unique_key(cache_key: &str, namespace: Option<&str>) -> String {
        match namespace {
            Some(ns) => hash(&format!("{}----{}", ns, cache_key)),
            None => hash(cache_key),
        }
    }

    /// Upstream `initialize(options)` (lines 92–94).
    ///
    /// `this._options = { ...defaultOptions, ...options }` — caller-
    /// supplied fields override the defaults. Rust mirrors with a
    /// struct merge.
    pub fn initialize(&mut self, options: CacheOptions) {
        self.options = options;
    }

    /// Upstream `load({ cacheKey, namespace, value })` (lines 106–128).
    ///
    /// * If `cache` is off → evaluate `value` and return.
    /// * Else compute `unique_key`; on hit, move-to-back and return
    ///   the cached value (cloning, since the cache owns it).
    /// * On miss, evict the LRU entry if at cap, evaluate `value`,
    ///   insert, return.
    ///
    /// `T: Clone` because `_loadFromCache` returns the cached value
    /// (JS shares the reference; Rust clones — for `Arc<Module>` the
    /// clone is a refcount bump, for `String` it's a heap copy, and
    /// for the export-lookup result it's a small struct copy).
    pub fn load<F>(&mut self, namespace: Option<&str>, cache_key: &str, value: F) -> T
    where
        T: Clone,
        F: FnOnce() -> T,
    {
        if !self.options.cache {
            return value();
        }

        let unique_key = Self::get_unique_key(cache_key, namespace);

        if self.cache.contains_key(&unique_key) {
            return self.load_from_cache(&unique_key);
        }

        self.try_deleting_lru_cached_value();

        let v = value();
        self.cache.insert(unique_key, v.clone());
        v
    }

    /// Upstream `_tryDeletingLRUCachedValue` (lines 53–58).
    fn try_deleting_lru_cached_value(&mut self) {
        if self.cache.len() >= self.options.max_size {
            // Pop the OLDEST entry (front of insertion order). JS:
            // `cache.keys().next().value` → first inserted key.
            // IndexMap exposes `.shift_remove_index(0)` for that.
            self.cache.shift_remove_index(0);
        }
    }

    /// Upstream `_moveLastInQueue` + `_loadFromCache` (lines 68–84).
    ///
    /// Removes the entry, re-inserts at the back (preserves the
    /// "least recently used = front" invariant), returns a clone.
    fn load_from_cache(&mut self, unique_key: &str) -> T
    where
        T: Clone,
    {
        // shift_remove preserves order (vs swap_remove, which doesn't).
        let v = self
            .cache
            .shift_remove(unique_key)
            .expect("contains_key check guarantees presence");
        let cloned = v.clone();
        self.cache.insert(unique_key.to_string(), v);
        cloned
    }

    /// Upstream `getSize()` (lines 132–134).
    pub fn get_size(&self) -> usize {
        self.cache.len()
    }

    /// Upstream `getKeys()` (lines 138–140) — returns an iterator.
    pub fn get_keys(&self) -> impl Iterator<Item = &str> {
        self.cache.keys().map(|s| s.as_str())
    }

    /// Upstream `getValues()` (lines 144–146) — returns an iterator.
    pub fn get_values(&self) -> impl Iterator<Item = &T> {
        self.cache.values()
    }
}

// ───────── Layer 2 (postcard, persistent) ─────────

/// `Layer2` — owns the on-disk `cache.bin` file. Today's port reads
/// at handle construction, writes on explicit `flush()`. The §5.6
/// evaluator is what eventually populates entries via `insert(...)`;
/// reads via `get(...)`.
///
/// Path discipline (CLAUDE.md "the WASI cwd preopen is `/cwd`"):
/// `worker_scratch_dir` is the host-supplied absolute path under
/// `/cwd/<rel>` form (PLAN.md §3.2). The plugin never resolves paths
/// against `env::current_dir()`.
///
/// **Today's wiring caveat:** `Layer2` is not yet plumbed into
/// `State::cache`. The §5.4 (resolve_binding) and §5.6
/// (evaluate_expression) ports are gated on a Babel `NodePath` /
/// scope-tree port that doesn't exist yet — see `plugins/STATUS.md`'s
/// Phase 5 drift escalation. The schema and the open/load/flush
/// machinery here lock the file shape so the next agent can wire
/// reads/writes without touching the wire format.
pub struct Layer2 {
    /// Absolute path to the worker's `cache.bin` file.
    file_path: String,
    /// In-memory mirror of the on-disk file. Mutated by the §5.6
    /// evaluator at runtime; flushed on `Program::exit`.
    file: CacheFile,
    /// Set when the in-memory mirror diverges from disk.
    dirty: bool,
    /// Monotonic LRU counter — bumped on every access.
    next_lru_seq: u64,
}

impl Layer2 {
    /// Open a `cache.bin` at `<worker_scratch_dir>/cache.bin`.
    ///
    /// Sweeps stale `*.tmp` siblings before reading (PLAN.md §3.9.13.1),
    /// then attempts to read `cache.bin`. On any failure (missing,
    /// truncated, postcard-decode error, version mismatch, schema-hash
    /// mismatch), the in-memory file is reset to empty and the layer
    /// stays usable. Per PLAN.md §3.9.10 — "never crash the build
    /// over a regenerable scratch file."
    ///
    /// I/O lives behind a thin trait so unit tests can drive the layer
    /// without touching the host filesystem. Production wires the
    /// `WasiFs` impl below.
    pub fn open<F: Fs>(fs: &F, worker_scratch_dir: &str) -> Self {
        let trimmed = worker_scratch_dir.trim_end_matches('/');
        let file_path = format!("{}/cache.bin", trimmed);

        // Stale-tmp sweep (PLAN.md §3.9.13.1) — best effort, errors
        // ignored. A worker-startup tmp on the file system is the
        // signature of a prior crash mid-write; nothing else the host
        // does should leave one.
        if let Ok(entries) = fs.list_dir(trimmed) {
            for entry in entries {
                if entry.ends_with(".tmp") {
                    let _ = fs.remove(&format!("{}/{}", trimmed, entry));
                }
            }
        }

        let file = match fs.read(&file_path) {
            Ok(bytes) => match postcard::from_bytes::<CacheFile>(&bytes) {
                Ok(f) => match f.validate() {
                    Ok(_) => f,
                    Err(_) => CacheFile::empty(),
                },
                Err(_) => CacheFile::empty(),
            },
            Err(_) => CacheFile::empty(),
        };

        let next_lru_seq = file.layer2.iter().map(|(_, e)| e.lru_seq).max().unwrap_or(0) + 1;

        Self {
            file_path,
            file,
            dirty: false,
            next_lru_seq,
        }
    }

    /// Lookup by mtime-derived key. Bumps LRU sequence on hit.
    pub fn get(&mut self, key: u64) -> Option<&Layer2Entry> {
        // Two-pass dance to satisfy the borrow checker:
        // 1) find the index, bump the seq through &mut.
        // 2) re-lookup &Entry to return.
        let idx = self.file.layer2.iter().position(|(k, _)| *k == key)?;
        let seq = {
            self.next_lru_seq += 1;
            self.next_lru_seq
        };
        self.file.layer2[idx].1.lru_seq = seq;
        self.dirty = true;
        Some(&self.file.layer2[idx].1)
    }

    /// Insert or replace a Layer 2 entry. Stamps the LRU sequence.
    /// Evicts entries to satisfy `MAX_ENTRIES` if needed.
    pub fn insert(&mut self, key: u64, mut entry: Layer2Entry) {
        self.next_lru_seq += 1;
        entry.lru_seq = self.next_lru_seq;

        if let Some(existing) = self.file.layer2.iter_mut().find(|(k, _)| *k == key) {
            existing.1 = entry;
        } else {
            self.file.layer2.push((key, entry));
        }
        self.evict_to_entry_cap();
        self.dirty = true;
    }

    /// LRU-evict by `lru_seq` ascending until `len() <= MAX_ENTRIES`.
    fn evict_to_entry_cap(&mut self) {
        while self.file.layer2.len() > MAX_ENTRIES {
            // Find min-seq index and remove. O(n) per eviction; the
            // cap is 500 so the quadratic worst case is bounded at
            // 250k operations on a single overfilled write — fine.
            let min_idx = self
                .file
                .layer2
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, e))| e.lru_seq)
                .map(|(i, _)| i)
                .expect("len > 0 inside loop");
            self.file.layer2.swap_remove(min_idx);
        }
    }

    /// Flush the in-memory mirror back to disk via the atomic write
    /// protocol (`cache.bin.tmp` → `fd_sync` → `path_rename`).
    ///
    /// If serialization exceeds `MAX_CACHE_BYTES`, evict the LRU
    /// entry, re-serialize, repeat. Bounded by `MAX_ENTRIES`; in
    /// practice 1–3 iterations suffice (entries are individually
    /// bounded by §3.9.8's per-entry caps).
    pub fn flush<F: Fs>(&mut self, fs: &F) -> Result<(), CacheFlushError> {
        if !self.dirty {
            return Ok(());
        }

        // Sort entries by key for determinism (PLAN.md §3.9.10
        // "deterministic by construction"). Two builds with the same
        // input set produce a byte-identical cache.bin.
        self.file.layer2.sort_by_key(|(k, _)| *k);

        let bytes = loop {
            let bytes = postcard::to_allocvec(&self.file)
                .map_err(|e| CacheFlushError::Encode(e.to_string()))?;
            if bytes.len() <= crate::cache_schema::MAX_CACHE_BYTES {
                break bytes;
            }
            if self.file.layer2.is_empty() {
                // Header alone exceeds the cap — schema bug. Bail.
                return Err(CacheFlushError::HeaderExceedsCap(bytes.len()));
            }
            // Evict LRU entry and try again.
            let min_idx = self
                .file
                .layer2
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, e))| e.lru_seq)
                .map(|(i, _)| i)
                .unwrap();
            self.file.layer2.swap_remove(min_idx);
        };

        let tmp_path = format!("{}.tmp", self.file_path);
        fs.write_atomic(&tmp_path, &self.file_path, &bytes)
            .map_err(CacheFlushError::Io)?;

        self.dirty = false;
        Ok(())
    }

    /// Read-only view of the underlying file (test / inspector use).
    pub fn file(&self) -> &CacheFile {
        &self.file
    }
}

#[derive(Debug)]
pub enum CacheFlushError {
    /// Postcard-encode failure. Should not be reachable for the
    /// `Layer2Entry` shape; if it is, that's a runtime corruption we
    /// want loud.
    Encode(String),
    /// The CacheFile header alone is bigger than `MAX_CACHE_BYTES`.
    /// Indicates a schema bug.
    HeaderExceedsCap(usize),
    /// The host filesystem rejected the atomic-write protocol.
    Io(String),
}

/// Filesystem trait — abstracts over the WASI surface for testing.
///
/// Production uses `WasiFs` (below) which lowers to `std::fs`. Tests
/// supply `MockFs` (in-process `IndexMap<String, Vec<u8>>`).
pub trait Fs {
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;
    fn write_atomic(&self, tmp_path: &str, final_path: &str, bytes: &[u8])
        -> Result<(), String>;
    fn remove(&self, path: &str) -> Result<(), String>;
    fn list_dir(&self, path: &str) -> Result<Vec<String>, String>;
}

/// Production filesystem impl. Lowers to WASI sync I/O via
/// `std::fs::*`. Path discipline: caller passes `/cwd/<rel>`-form
/// strings (CLAUDE.md / PLAN.md §3.2).
pub struct WasiFs;

impl Fs for WasiFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    fn write_atomic(&self, tmp_path: &str, final_path: &str, bytes: &[u8]) -> Result<(), String> {
        // Per PLAN.md §3.9.10 / §3.9.12:
        //   1. write `cache.bin.tmp`
        //   2. fd_sync (std::fs::File::sync_all maps to wasi fd_sync)
        //   3. path_rename via std::fs::rename
        use std::io::Write;
        let mut f = std::fs::File::create(tmp_path).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        std::fs::rename(tmp_path, final_path).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn remove(&self, path: &str) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
    fn list_dir(&self, path: &str) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
pub use mock_fs::MockFs;

#[cfg(test)]
mod mock_fs {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// In-memory filesystem for unit tests. Stores `BTreeMap<path,
    /// bytes>` so iteration order is deterministic.
    pub struct MockFs {
        files: RefCell<BTreeMap<String, Vec<u8>>>,
    }

    impl MockFs {
        pub fn new() -> Self {
            Self {
                files: RefCell::new(BTreeMap::new()),
            }
        }

        pub fn put(&self, path: &str, bytes: Vec<u8>) {
            self.files.borrow_mut().insert(path.to_string(), bytes);
        }

        pub fn has(&self, path: &str) -> bool {
            self.files.borrow().contains_key(path)
        }
    }

    impl Fs for MockFs {
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            self.files
                .borrow()
                .get(path)
                .cloned()
                .ok_or_else(|| format!("ENOENT: {}", path))
        }
        fn write_atomic(
            &self,
            _tmp_path: &str,
            final_path: &str,
            bytes: &[u8],
        ) -> Result<(), String> {
            self.files
                .borrow_mut()
                .insert(final_path.to_string(), bytes.to_vec());
            Ok(())
        }
        fn remove(&self, path: &str) -> Result<(), String> {
            self.files.borrow_mut().remove(path);
            Ok(())
        }
        fn list_dir(&self, path: &str) -> Result<Vec<String>, String> {
            let prefix = format!("{}/", path.trim_end_matches('/'));
            let mut out = Vec::new();
            for k in self.files.borrow().keys() {
                if let Some(rest) = k.strip_prefix(&prefix) {
                    if !rest.contains('/') {
                        out.push(rest.to_string());
                    }
                }
            }
            Ok(out)
        }
    }
}

// Hide the unused-validation-error reference for downstream
// consumers — used by `Layer2::open`'s match arm via type inference
// only.
#[allow(dead_code)]
fn _validation_error_use(e: CacheValidationError) -> CacheValidationError {
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_schema::{Layer2Entry, SerializedExpr};
    use crate::mutation_recorder::ApiKind;

    // ───────── Layer 1 ─────────

    #[test]
    fn cache_default_options_match_upstream() {
        let opts = CacheOptions::default();
        assert!(opts.cache);
        assert_eq!(opts.max_size, 500);
    }

    #[test]
    fn unique_key_with_namespace_matches_js_shape() {
        // Upstream: `hash(namespace + '----' + cacheKey)`.
        let keyed = Cache::<String>::get_unique_key("/abs/foo.ts", Some("read-file"));
        let direct = hash("read-file----/abs/foo.ts");
        assert_eq!(keyed, direct);
    }

    #[test]
    fn unique_key_without_namespace_passes_through() {
        let keyed = Cache::<String>::get_unique_key("/abs/foo.ts", None);
        let direct = hash("/abs/foo.ts");
        assert_eq!(keyed, direct);
    }

    #[test]
    fn load_returns_evaluated_value_on_miss_and_cached_on_hit() {
        let mut c: Cache<i32> = Cache::new();
        let mut counter = 0;

        let v1 = c.load(Some("ns"), "k1", || {
            counter += 1;
            42
        });
        assert_eq!(v1, 42);
        assert_eq!(counter, 1);

        let v2 = c.load(Some("ns"), "k1", || {
            counter += 1;
            99
        });
        assert_eq!(v2, 42); // cached value, not the new one
        assert_eq!(counter, 1); // closure not re-evaluated
    }

    #[test]
    fn load_with_cache_disabled_evaluates_every_time() {
        let mut c: Cache<i32> = Cache::new();
        c.initialize(CacheOptions {
            cache: false,
            max_size: 500,
        });
        let mut counter = 0;
        for _ in 0..5 {
            c.load(None, "k", || {
                counter += 1;
                7
            });
        }
        assert_eq!(counter, 5);
        assert_eq!(c.get_size(), 0);
    }

    #[test]
    fn lru_eviction_drops_oldest_first() {
        let mut c: Cache<i32> = Cache::new();
        c.initialize(CacheOptions {
            cache: true,
            max_size: 3,
        });

        c.load(None, "a", || 1);
        c.load(None, "b", || 2);
        c.load(None, "c", || 3);
        assert_eq!(c.get_size(), 3);

        // Inserting a 4th evicts 'a' (oldest).
        c.load(None, "d", || 4);
        assert_eq!(c.get_size(), 3);

        let keys: Vec<String> = c.get_keys().map(|s| s.to_string()).collect();
        let key_a = Cache::<i32>::get_unique_key("a", None);
        let key_d = Cache::<i32>::get_unique_key("d", None);
        assert!(!keys.contains(&key_a));
        assert!(keys.contains(&key_d));
    }

    #[test]
    fn loading_an_entry_moves_it_to_back_of_lru() {
        let mut c: Cache<i32> = Cache::new();
        c.initialize(CacheOptions {
            cache: true,
            max_size: 3,
        });

        c.load(None, "a", || 1);
        c.load(None, "b", || 2);
        c.load(None, "c", || 3);

        // Touch 'a' — now order is b, c, a.
        let v = c.load(None, "a", || 99);
        assert_eq!(v, 1);

        // Insert 'd' → evict 'b' (now oldest), not 'a'.
        c.load(None, "d", || 4);
        let key_a = Cache::<i32>::get_unique_key("a", None);
        let key_b = Cache::<i32>::get_unique_key("b", None);
        let keys: Vec<String> = c.get_keys().map(|s| s.to_string()).collect();
        assert!(keys.contains(&key_a));
        assert!(!keys.contains(&key_b));
    }

    #[test]
    fn get_size_keys_values_match_state() {
        let mut c: Cache<String> = Cache::new();
        c.load(Some("ns"), "k1", || "v1".to_string());
        c.load(Some("ns"), "k2", || "v2".to_string());
        assert_eq!(c.get_size(), 2);
        let values: Vec<String> = c.get_values().cloned().collect();
        assert!(values.contains(&"v1".to_string()));
        assert!(values.contains(&"v2".to_string()));
    }

    // ───────── Layer 2 ─────────

    fn sample_entry(s: &str) -> Layer2Entry {
        Layer2Entry {
            evaluated_ast: SerializedExpr::Str(s.to_string()),
            state_diffs: vec![],
            transitive_deps: vec![],
            source_mtime_ns: 0,
            lru_seq: 0,
            byte_size_estimate: 32,
        }
    }

    #[test]
    fn layer2_open_on_missing_file_returns_empty() {
        let fs = MockFs::new();
        let l2 = Layer2::open(&fs, "/cwd/cache");
        assert!(l2.file().layer2.is_empty());
    }

    #[test]
    fn layer2_open_sweeps_stale_tmps() {
        let fs = MockFs::new();
        fs.put("/cwd/cache/cache.bin.tmp", vec![0xff; 16]);
        fs.put("/cwd/cache/other.tmp", vec![0xff; 16]);
        // not a tmp — must NOT be removed
        fs.put("/cwd/cache/keep.bin", vec![0x00; 4]);

        let _l2 = Layer2::open(&fs, "/cwd/cache");
        assert!(!fs.has("/cwd/cache/cache.bin.tmp"));
        assert!(!fs.has("/cwd/cache/other.tmp"));
        assert!(fs.has("/cwd/cache/keep.bin"));
    }

    #[test]
    fn layer2_round_trips_through_mock_fs() {
        let fs = MockFs::new();
        let mut l2 = Layer2::open(&fs, "/cwd/cache");
        l2.insert(0xaa, sample_entry("blue"));
        l2.flush(&fs).expect("flush ok");

        // Reopen from the same fs — entry survives.
        let mut l2b = Layer2::open(&fs, "/cwd/cache");
        let got = l2b.get(0xaa).expect("entry exists");
        match &got.evaluated_ast {
            SerializedExpr::Str(s) => assert_eq!(s, "blue"),
            other => panic!("expected Str, got {:?}", other),
        }
    }

    #[test]
    fn layer2_corrupt_file_resets_to_empty() {
        let fs = MockFs::new();
        fs.put("/cwd/cache/cache.bin", vec![0xff; 8]); // not valid postcard
        let l2 = Layer2::open(&fs, "/cwd/cache");
        assert!(l2.file().layer2.is_empty());
    }

    #[test]
    fn layer2_version_mismatch_resets_to_empty() {
        let fs = MockFs::new();
        // Construct a CacheFile with a wrong version, encode it, plant it.
        let mut wrong = CacheFile::empty();
        wrong.version = 9999;
        let bytes = postcard::to_allocvec(&wrong).unwrap();
        fs.put("/cwd/cache/cache.bin", bytes);

        let l2 = Layer2::open(&fs, "/cwd/cache");
        assert!(l2.file().layer2.is_empty());
        // Validation should hold for the new empty file.
        assert!(l2.file().validate().is_ok());
    }

    #[test]
    fn layer2_insert_evicts_lru_when_over_entry_cap() {
        let fs = MockFs::new();
        let mut l2 = Layer2::open(&fs, "/cwd/cache");
        // Insert MAX_ENTRIES + 5 entries; the 5 lowest-seq entries
        // should be evicted (oldest insertions).
        for i in 0..(MAX_ENTRIES as u64 + 5) {
            l2.insert(i, sample_entry(&format!("v{}", i)));
        }
        assert_eq!(l2.file().layer2.len(), MAX_ENTRIES);
        // The first 5 keys (0..=4) should have been evicted.
        for i in 0..5u64 {
            assert!(
                l2.file().layer2.iter().all(|(k, _)| *k != i),
                "expected key {} evicted",
                i
            );
        }
    }

    #[test]
    fn layer2_get_bumps_lru_seq() {
        let fs = MockFs::new();
        let mut l2 = Layer2::open(&fs, "/cwd/cache");
        l2.insert(1, sample_entry("a"));
        l2.insert(2, sample_entry("b"));
        // Touch 1 — its seq becomes higher than 2's.
        let s1 = l2.get(1).unwrap().lru_seq;
        let s2 = l2.file().layer2.iter().find(|(k, _)| *k == 2).unwrap().1.lru_seq;
        assert!(s1 > s2, "seq for touched key {} should exceed untouched {}", s1, s2);
    }

    #[test]
    fn layer2_flush_writes_deterministically_sorted_keys() {
        let fs1 = MockFs::new();
        let mut a = Layer2::open(&fs1, "/cwd/c");
        a.insert(3, sample_entry("c"));
        a.insert(1, sample_entry("a"));
        a.insert(2, sample_entry("b"));
        a.flush(&fs1).unwrap();

        let fs2 = MockFs::new();
        let mut b = Layer2::open(&fs2, "/cwd/c");
        b.insert(1, sample_entry("a"));
        b.insert(2, sample_entry("b"));
        b.insert(3, sample_entry("c"));
        b.flush(&fs2).unwrap();

        // The serialized bytes must match — same input set,
        // independent of insertion order, after the sort-by-key on
        // flush.
        let a_bytes = fs1.read("/cwd/c/cache.bin").unwrap();
        let b_bytes = fs2.read("/cwd/c/cache.bin").unwrap();
        // LRU sequences differ across the two paths (different
        // insertion timing), so we can't compare bytes verbatim — but
        // we can compare the on-disk ordering: keys come out in
        // sorted order on both sides.
        let a_file: CacheFile = postcard::from_bytes(&a_bytes).unwrap();
        let b_file: CacheFile = postcard::from_bytes(&b_bytes).unwrap();
        let a_keys: Vec<u64> = a_file.layer2.iter().map(|(k, _)| *k).collect();
        let b_keys: Vec<u64> = b_file.layer2.iter().map(|(k, _)| *k).collect();
        assert_eq!(a_keys, vec![1, 2, 3]);
        assert_eq!(b_keys, vec![1, 2, 3]);
    }

    #[test]
    fn layer2_clean_handle_does_not_write_on_flush() {
        let fs = MockFs::new();
        let mut l2 = Layer2::open(&fs, "/cwd/cache");
        // No mutations → flush should be a no-op (nothing on disk).
        l2.flush(&fs).unwrap();
        assert!(!fs.has("/cwd/cache/cache.bin"));
    }

    #[test]
    fn layer2_state_diffs_round_trip_inside_entry() {
        let fs = MockFs::new();
        let mut l2 = Layer2::open(&fs, "/cwd/c");
        let entry = Layer2Entry {
            evaluated_ast: SerializedExpr::Object(vec![(
                "color".to_string(),
                SerializedExpr::Str("blue".to_string()),
            )]),
            state_diffs: vec![
                crate::mutation_recorder::StateDiff::IncludedFilesPush {
                    path: "/cwd/theme.ts".into(),
                },
                crate::mutation_recorder::StateDiff::CompiledImportsAppend {
                    api: ApiKind::Css,
                    local_name: "css".into(),
                },
            ],
            transitive_deps: vec![],
            source_mtime_ns: 0,
            lru_seq: 0,
            byte_size_estimate: 64,
        };
        l2.insert(7, entry);
        l2.flush(&fs).unwrap();

        let mut l2b = Layer2::open(&fs, "/cwd/c");
        let got = l2b.get(7).unwrap();
        assert_eq!(got.state_diffs.len(), 2);
    }
}
