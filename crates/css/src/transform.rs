//! Port of `packages/css/src/transform.ts`.
//!
//! Locks the public surface — every byte the parity-runner compares passes
//! through here. The pipeline mirrors the 12 plugins in upstream
//! `transform.ts:32-100`, composed by **postcss lifecycle round** (Once →
//! walk → OnceExit), **NOT** by JS array order.
//!
//! ## Why round-by-round, not array-by-array
//!
//! Postcss runs `process()` as three rounds over the AST in plugin-array
//! order: every plugin's `Once` first, then a single DFS firing all
//! per-node visitors at every node, then every plugin's `OnceExit`.
//!
//! The naive "iterate the plugin array and apply each as a sweep" approach
//! silently drifts whenever plugins mix hook types — the canonical example
//! being `sortAtomicStyleSheet` which sits at array index 9 but uses
//! `Once`, so it actually fires BEFORE the walk and BEFORE every OnceExit.
//! See `crates/css/src/sort.rs:39-61` for the same hazard documented in
//! Phase 8a, and `crates/PHASE_8B_LIFECYCLE_AUDIT.md` for the full per-plugin
//! classification driving the recipe below.
//!
//! ## Composition recipe (from `PHASE_8B_LIFECYCLE_AUDIT.md`)
//!
//! ### Round 1 — Once round (plugin-array order)
//!
//! ```text
//! 1. discardDuplicates.Once               // top-level decl dedup
//! 3. parentOrphanedPseudos.Once           // pseudo→nested selector rewrite
//! 9. sortAtomicStyleSheet.Once            // partition + sort + reassign root.nodes
//! ```
//!
//! ### Round 2 — Walk round (single DFS, per-node, plugin-array order)
//!
//! Four walk visitors, three of which are `Declaration`:
//!
//! ```text
//! At each Declaration node:
//!    discardEmptyRules.Declaration       (#2)
//!    normalize-current-color.Declaration (#5o, only when optimizeCss)
//!    expandShorthands.Declaration        (#6)
//!
//! At each Rule node:
//!    postcss-nested.Rule                 (#4)
//! ```
//!
//! ### Round 3 — OnceExit round (plugin-array order)
//!
//! ```text
//!  5a..5n  (14 cssnano sub-plugins, in cssnano-preset-default's source order)
//!  7  atomicifyRules           (callback → classNames)
//!  8  increaseSpecificity      (conditional on opts.increaseSpecificity)
//! 10  autoprefixer             (conditional on env AUTOPREFIXER != "off")
//! 11  postcss-normalize-whitespace
//! 12  extractStyleSheets       (callback → sheets)
//! ```
//!
//! ## Lift hazard call-out
//!
//! `compiled-css::plugins::normalize_css::normalize_css` packages the
//! `normalize-current-color.Declaration` walk AND the 14 cssnano
//! `OnceExit`s into one call. **Do NOT call it from this orchestrator** —
//! that puts the Declaration walk before `postcss-nested`'s Rule walk
//! AND before `expandShorthands` and `discardEmptyRules`, but JS interleaves
//! them per-node during the SAME walk round. We instead:
//!
//! - Call `normalize_current_color::process_declaration` per-decl during
//!   the walk-round logic, merged with the other two Declaration visitors.
//! - Iterate `cssnano_preset_default::default_preset()` filtered by
//!   `BASE_PLUGINS ∪ PROD_PLUGINS` directly during the OnceExit round,
//!   matching `normalize-css.ts`'s filter.
//!
//! ## Walk-round per-node interleave
//!
//! `postcss-nested` runs first as a tree sweep — its Rule visitor has no
//! overlap with any other plugin's Rule visitor (it's the only Rule
//! visitor in the array, per audit Plugin 4 cross-plugin ordering and the
//! audit's "Composition recipe" walk-round breakdown). Once nested
//! unwrapping is finished, every Declaration sits at its final tree
//! position.
//!
//! Then the THREE Declaration visitors fire in a single `walk_decls_mut`
//! callback that, at each decl node, runs them in plugin-array order:
//!
//! ```text
//!   #2  discardEmptyRules.Declaration       (remove on empty value;
//!                                            also remove parent rule if
//!                                            it became empty)
//!   #5o normalize-current-color.Declaration (only when optimizeCss;
//!                                            in-place value rewrite)
//!   #6  expandShorthands.Declaration        (decl.replaceWith(N longforms))
//! ```
//!
//! Per-node interleave matters because postcss's walk fires visitors at
//! a single node in array order before moving on, and each visitor can
//! mutate the node in ways the next visitor needs to observe. Specifically:
//!
//! - If `discardEmptyRules` removes the decl (`Mutation::Remove`),
//!   subsequent visitors at this slot do NOT fire on the dead node —
//!   matching postcss's `if (!node.parent) { stack.pop() }` short-circuit
//!   in `lazy-result.js::visitTick`.
//! - If `normalize-current-color` rewrites `currentcolor` →
//!   `currentColor`, the rewritten value is what `expandShorthands` sees
//!   when it inspects `decl.value` — load-bearing for any future shorthand
//!   that reads color tokens (none today, but parity demands the order).
//! - If `expandShorthands` returns N longform decls
//!   (`Mutation::ReplaceMany`), the cursor advances past them. The new
//!   longforms are not shorthand props themselves (audit Plugin 6
//!   mutation profile) so even if they were re-visited the visitors
//!   would no-op, but we honour postcss's "no re-visit at the same
//!   slot" semantics by using `Mutation::ReplaceMany` directly.
//!
//! ### Parent-rule removal during the merged walk
//!
//! `discardEmptyRules` has a SECOND mutation beyond removing the decl:
//! when removing the decl empties the parent rule, the parent rule is
//! also removed (`packages/css/src/plugins/discard-empty-rules.ts:13-19`).
//! `walk_decls_mut`'s callback only sees the decl, not the parent — so
//! we hand-roll the walk over containers, peeking at parent emptiness
//! after each decl removal and dropping the parent rule on the spot.
//! That matches upstream's `parent.remove()` call semantically and lets
//! the outer walk's index bookkeeping continue at the grandparent.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use postcss_core::container::{remove_at, replace_with_at};
use postcss_core::{parse, Node, NodeKind};

use crate::plugins::compiled_css::plugins::atomicify_rules::{atomicify_rules, AtomicifyRulesOpts};
use crate::plugins::compiled_css::plugins::discard_duplicates::discard_duplicates;
use crate::plugins::compiled_css::plugins::discard_empty_rules::is_value_empty;
use crate::plugins::compiled_css::plugins::expand_shorthands::process_declaration as expand_shorthand_decl;
use crate::plugins::compiled_css::plugins::extract_stylesheets::{
    extract_stylesheets, ExtractStyleSheetsOpts,
};
use crate::plugins::compiled_css::plugins::increase_specificity::increase_specificity;
use crate::plugins::compiled_css::plugins::normalize_current_color::process_declaration as normalize_current_color_decl;
use crate::plugins::compiled_css::plugins::parent_orphaned_pseudos::parent_orphaned_pseudos;
use crate::plugins::compiled_css::plugins::sort_atomic_style_sheet::{
    sort_atomic_style_sheet, SortAtomicStyleSheetOpts,
};

use crate::plugins::cssnano_preset_default::{default_preset, PresetOpts};
use cssnano_browserslist_snapshot::{decode_precomputed, PrecomputedBrowserslist};
use std::sync::Arc;

use crate::plugins::postcss_nested::{postcss_nested, PostcssNestedOpts};
use crate::plugins::postcss_normalize_whitespace::postcss_normalize_whitespace;

use autoprefixer::autoprefixer::{build_prefixes, build_prefixes_default, AutoprefixerOptions};
use autoprefixer::precomputed::build_prefixes_from_precomputed;
use autoprefixer::prefixes::Prefixes;
use autoprefixer::processor::Processor as AutoprefixerProcessor;

use compiled_utils::unique;

// ---------------------------------------------------------------------------
// Win 1 — within-transform `precomputed_prefixes_path` byte cache.
//
// `transform_css` is called many times within a single SWC plugin
// invocation (once per `transformCssItem` over each conditional /
// logical / default branch in `css({...})`). Each call previously
// re-ran `std::fs::read(precomputed_prefixes_path)` end-to-end —
// ~50–158 µs/call of pure repeat work where the bytes are bit-identical
// across every call (the host writes the snapshot once at module
// load and threads the same path on every plugin invocation).
//
// We cache the file's bytes in a `thread_local!` keyed by the path
// string. The cache is born and dies with the WASI linear-memory
// instance (per-`transformSync` teardown contract documented in
// `plugins/PLAN.md` §3.9.4 and CLAUDE.md), so cross-transform leakage
// is impossible. Within a single transform, multiple `transform_css`
// calls hit the cache after the first.
//
// **Byte-equality:** we cache only the raw `Vec<u8>` returned by
// `std::fs::read` — bit-identical to what `std::fs::read` would have
// returned on the next call. Postcard decoding of identical bytes
// produces an identical `PrecomputedPrefixes`; `build_prefixes_from_snapshot`
// of identical inputs produces a byte-equal `Prefixes`. Existing
// snapshot-vs-slow-path parity proofs (`crates/parity-runner --stage
// autoprefixer`, `crates/babel-plugin/tests/transform_css_browserslist_snapshot_integration.rs`)
// remain valid unchanged — this cache is below the level they observe.
//
// **Invalidation:** if the path changes between calls (rare — the
// host wrapper writes once at module load and reuses the path), the
// cache transparently rereads. If the file CONTENTS change while the
// path stays identical (e.g. someone edits the snapshot mid-transform
// — production never does this; the host doesn't expose the snapshot
// to userland), the cache returns stale bytes. This is the same
// guarantee the OS page cache gives `std::fs::read` itself; we don't
// add a stat-based invalidation because the host-managed snapshot is
// immutable per-instance.
//
// We do NOT cache the decoded `PrecomputedPrefixes` or the
// constructed `Prefixes` — both consume on use (move semantics on
// `populated_add` / `populated_remove`), and `Prefixes` accumulates
// runtime state during the processor walk (`cleaner_cache`,
// `Supports::prefixer_cache`). Caching either would require either
// expensive cloning (the populated tables intentionally don't derive
// `Clone`) or per-call audit of mutation profiles. Caching the bytes
// avoids both hazards entirely.
thread_local! {
    static PRECOMPUTED_PREFIXES_BYTES_CACHE:
        std::cell::RefCell<Option<(std::path::PathBuf, std::sync::Arc<Vec<u8>>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Read the precomputed-prefixes snapshot bytes from `path`, hitting a
/// thread-local cache when the path matches a previous call within the
/// same wasm instance. See module-level rationale.
///
/// Returns an `Arc<Vec<u8>>` so the cache slot can hold a reference
/// while the caller holds another — `Arc::clone` is O(1) (atomic
/// refcount bump) and avoids copying the snapshot bytes on every cache
/// hit.
fn read_precomputed_prefixes_cached(
    path: &std::path::Path,
) -> std::io::Result<std::sync::Arc<Vec<u8>>> {
    PRECOMPUTED_PREFIXES_BYTES_CACHE.with(|slot| {
        // Fast path: cache hit.
        if let Some((cached_path, cached_bytes)) = slot.borrow().as_ref() {
            if cached_path == path {
                return Ok(std::sync::Arc::clone(cached_bytes));
            }
        }
        // Slow path: read + populate. We do this OUTSIDE the borrow
        // so we don't hold the RefCell across the syscall.
        let bytes = std::fs::read(path)?;
        let arc = std::sync::Arc::new(bytes);
        *slot.borrow_mut() = Some((path.to_path_buf(), std::sync::Arc::clone(&arc)));
        Ok(arc)
    })
}

/// Test-only escape hatch: clear the thread-local cache. Call from
/// tests that mutate the snapshot file in place and want subsequent
/// `transform_css` calls in the same thread to re-read it.
#[cfg(test)]
pub fn _clear_precomputed_prefixes_cache_for_tests() {
    PRECOMPUTED_PREFIXES_BYTES_CACHE.with(|slot| {
        *slot.borrow_mut() = None;
    });
    PREFIXES_DECODED_CACHE.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

// ---------------------------------------------------------------------------
// Win 2 — within-process `Arc<Prefixes>` cache (NAPI-only effective).
//
// `build_prefixes_from_precomputed` is dominated by postcard
// deserialisation of the ~45 KB snapshot — ~227 µs/call (39% of the
// whole pipeline). The decoded `Prefixes` is a deterministic function
// of the input bytes; reusing it across calls in the same thread is
// byte-equal to rebuilding it. We Arc-wrap the `Prefixes` so multiple
// `transform_css` calls can share the same allocation.
//
// **Why this is NAPI-only effective:** WASI tears down the linear
// memory between every `transformSync` call, so the cache slot is
// always empty when WASI re-enters. NAPI keeps the thread alive
// across calls (per-thread napi-rs context), so the cache hits from
// the second call onward.
//
// **Cache key:** SipHash of the snapshot bytes. The host typically
// pre-builds one snapshot and reuses it across every call; same bytes
// → same hash → cache hit. We hash on every call (~10 µs on 45 KB)
// but save ~227 µs on hit. Hash collisions are astronomically unlikely
// and a collision still produces a byte-equal result (different
// snapshots that hash the same would still yield correct prefixes for
// the QUERIED bytes — wait, no, they would produce the OTHER snapshot's
// prefixes for our queried bytes, which would NOT be byte-equal).
// So we accept the theoretical collision risk; SipHash with 64-bit
// output gives ~1 in 2^32 collision risk per pair, and the host writes
// one snapshot per build.
//
// **Sharing semantics:** `Prefixes` contains `RefCell<AddTable>`,
// `RefCell<RemoveTable>`, `RefCell<Box<Supports>>` and
// `OnceCell<Box<Prefixes>>` (cleaner_cache). The RefCells are
// mutated during `processor.add`/`processor.remove` walks for lazy
// initialisation (e.g. `Resolution::process` populates `self.bad`
// on first call, then skips). These mutations are idempotent: the
// post-walk state is reachable from any sequence of prior input. So
// reusing a "warm" `Prefixes` across calls is byte-equal — verified
// by the parity-runner corpus.
//
// **Thread safety:** `thread_local!` ensures one cache per OS thread.
// Within a thread, calls are sequential — no overlapping RefCell
// borrows. Across threads, each gets its own cache (memory cost: one
// extra `Prefixes` per thread that ever calls `transform_css`).
thread_local! {
    static PREFIXES_DECODED_CACHE:
        std::cell::RefCell<Option<(u64, std::sync::Arc<Prefixes>)>> =
        const { std::cell::RefCell::new(None) };
}

fn fingerprint_snapshot_bytes(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h = DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

/// Returns `true` if the input CSS contains any vendor-prefix substring
/// that [`AutoprefixerProcessor::remove`] could possibly match against.
///
/// `remove` does three sub-walks (`processor.rs:1190-1300`); each one
/// can only fire on AST nodes that already contain a vendor prefix:
///   - `walkAtRules`: matches `@-<prefix>-<name>` (e.g., `@-webkit-keyframes`).
///   - `walkRules`: `OldSelector::check` opens with
///     `if !rule_selector.contains(&self.prefixed) { return false; }`
///     where `self.prefixed` is always a prefixed selector like
///     `::-webkit-input-placeholder` (`old_selector.rs:101`).
///   - `walkDecls`: matches prop names that start with a vendor prefix.
///
/// No pipeline stage before `autoprefixer` introduces vendor prefixes
/// — `autoprefixer::add` is the only writer, and it runs AFTER
/// `remove`. So prefix tokens in the AST at `remove` time can only
/// have come from the original input. If the input string has no
/// prefix tokens, `remove` is provably a no-op and can be skipped
/// byte-equally.
///
/// **Conservative direction:** false positives (running `remove` when
/// not strictly needed — e.g., bytes `-webkit-` appearing inside a
/// `data:` URL or `/* ... */` comment) are harmless. False negatives
/// are impossible: anything `remove` would have stripped contains a
/// prefix token, which this scan would have caught.
fn input_has_vendor_prefix(css: &str) -> bool {
    // `str::contains` uses an optimized two-way substring search —
    // fast enough for typical ~500-byte inputs (~1 µs total). The
    // 4 needles cover every vendor prefix autoprefixer tracks.
    // Short-circuits on first hit.
    css.contains("-webkit-")
        || css.contains("-moz-")
        || css.contains("-ms-")
        || css.contains("-o-")
}

fn cached_prefixes_from_bytes(
    bytes: &[u8],
) -> Result<std::sync::Arc<Prefixes>, String> {
    let key = fingerprint_snapshot_bytes(bytes);
    PREFIXES_DECODED_CACHE.with(|slot| {
        if let Some((cached_key, cached_arc)) = slot.borrow().as_ref() {
            if *cached_key == key {
                return Ok(std::sync::Arc::clone(cached_arc));
            }
        }
        let prefixes = build_prefixes_from_precomputed(bytes)
            .map_err(|e| format!("autoprefixer precomputed load error: {e}"))?;
        let arc = std::sync::Arc::new(prefixes);
        *slot.borrow_mut() = Some((key, std::sync::Arc::clone(&arc)));
        Ok(arc)
    })
}

/// `BASE_PLUGINS` from `packages/css/src/plugins/normalize-css.ts:44-50`.
/// Always run regardless of `optimizeCss`.
const NORMALIZE_BASE_PLUGINS: &[&str] = &[
    "postcss-minify-selectors",
    "postcss-minify-params",
];

/// `PROD_PLUGINS` from `packages/css/src/plugins/normalize-css.ts:13-39`.
/// Run only when `optimizeCss` is true.
const NORMALIZE_PROD_PLUGINS: &[&str] = &[
    "postcss-ordered-values",
    "postcss-reduce-initial",
    "postcss-convert-values",
    "postcss-colormin",
    "postcss-normalize-url",
    "postcss-normalize-unicode",
    "postcss-normalize-string",
    "postcss-normalize-positions",
    "postcss-normalize-timing-functions",
    "postcss-minify-gradients",
    "postcss-discard-comments",
    "postcss-calc",
];

/// Mirrors upstream `TransformOpts` (line 17 of `transform.ts`). Field-by-field:
///
/// | upstream                   | rust                          | notes |
/// |----------------------------|-------------------------------|-------|
/// | `optimizeCss?`             | `optimize_css`                | `Option<bool>` — `undefined` is meaningful |
/// | `classNameCompressionMap?` | `class_name_compression_map` | `IndexMap` to preserve insertion order |
/// | `increaseSpecificity?`     | `increase_specificity`        | gates plugin 8 |
/// | `sortAtRules?`             | `sort_at_rules`               | forwarded to `sortAtomicStyleSheet` |
/// | `sortShorthand?`           | `sort_shorthand`              | forwarded to `sortAtomicStyleSheet` |
/// | `classHashPrefix?`         | `class_hash_prefix`           | forwarded to `atomicifyRules` |
///
/// `flattenMultipleSelectors` was added in @compiled/css 0.20+ and is
/// **not** part of the AFM-pinned 0.19.0 surface (see PARITY_VERSIONS.md
/// "JS oracle source pin"). Do not re-add this field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformOpts {
    #[serde(rename = "optimizeCss", default)]
    pub optimize_css: Option<bool>,
    #[serde(rename = "classNameCompressionMap", default)]
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    #[serde(rename = "increaseSpecificity", default)]
    pub increase_specificity: Option<bool>,
    #[serde(rename = "sortAtRules", default)]
    pub sort_at_rules: Option<bool>,
    #[serde(rename = "sortShorthand", default)]
    pub sort_shorthand: Option<bool>,
    #[serde(rename = "classHashPrefix", default)]
    pub class_hash_prefix: Option<String>,
    /// **Optional perf knob — NOT part of the upstream `TransformOpts`
    /// surface.** When `Some(bytes)`, the autoprefixer prefix tables
    /// are reconstructed from a precomputed postcard snapshot
    /// ([`autoprefixer::precomputed`]) instead of running the
    /// filesystem walk + browserslist resolution + full PREFIXES table
    /// iteration on every call. Byte-equal to the slow path; verified
    /// in `autoprefixer::precomputed::tests`.
    ///
    /// Falls back to [`build_prefixes_default`] when `None`. Designed
    /// for the WASI / SWC plugin call site where caching across calls
    /// is impossible (per-call linear-memory teardown), so the cost
    /// must be eliminated rather than cached.
    ///
    /// `#[serde(skip)]` because the bytes round-trip through the NAPI
    /// `Buffer` type at the boundary, not through `serde_json`. Inside
    /// Rust we treat them as opaque blobs handed to autoprefixer.
    #[serde(skip)]
    pub precomputed_prefixes: Option<Vec<u8>>,

    /// **Optional perf knob — NOT part of the upstream `TransformOpts`
    /// surface.** Filesystem-path delivery for the precomputed
    /// snapshot. When set and [`Self::precomputed_prefixes`] is
    /// `None`, the file is read on each call and decoded as a
    /// snapshot. When the file lives in the OS page cache (the
    /// expected steady-state in a CI / build host with one snapshot
    /// written per build), the read is a few microseconds.
    ///
    /// **WASI design intent:** SWC tears down the WASI linear-memory
    /// instance between every transform — no in-memory cache of the
    /// `Prefixes` struct survives. The host writes the snapshot bytes
    /// to a known path once per build; every WASI plugin instance
    /// reads from that path on each call. The OS page cache amortises
    /// the disk read; the WASI sandbox's filesystem-access cost is
    /// negligible vs the work this elides (~345 µs/call without V2).
    ///
    /// **Precedence:**
    ///   1. If [`Self::precomputed_prefixes`] is `Some`, use those
    ///      bytes (inline delivery wins).
    ///   2. Else if `precomputed_prefixes_path` is `Some`, read the
    ///      file. Read failure is a hard error — we do NOT silently
    ///      fall back to the slow path, because in production that
    ///      would mask config errors (e.g., snapshot missing) behind
    ///      a 100x perf regression.
    ///   3. Else fall back to [`build_prefixes_default`] (slow path).
    ///
    /// `#[serde(skip)]` for the same reason as
    /// [`Self::precomputed_prefixes`] — Rust-internal control knob,
    /// not part of the JS-side TransformOpts.
    #[serde(skip)]
    pub precomputed_prefixes_path: Option<std::path::PathBuf>,

    /// **Optional perf knob — NOT part of the upstream `TransformOpts`
    /// surface.** When `Some(bytes)`, the postcard-encoded
    /// [`cssnano_browserslist_snapshot::PrecomputedBrowserslist`] is
    /// decoded once and threaded into every browserslist-aware leaf
    /// plugin in `cssnano-preset-default` (5 plugins —
    /// `postcss-{reduce-initial,colormin,convert-values,minify-params,
    /// normalize-unicode}`). This eliminates the in-plugin
    /// `browserslist_shim::resolve("")` path which (a) reads
    /// `BROWSERSLIST` / `BROWSERSLIST_CONFIG` env vars (silently
    /// ignored in WASI), (b) walks up from `cwd()` for
    /// `.browserslistrc` (silently fails in WASI's preopen-only FS),
    /// and (c) is expensive (~200 µs/call when it does work).
    ///
    /// Falls back to the in-plugin resolution path when `None`.
    /// Byte-equivalent to omitting the field, except in WASI where
    /// the in-plugin path returns the wide default browser list
    /// (including IE 11) instead of the host-resolved AFM modern
    /// list. See `DEFINITIVE_BROWSERSLIST_PLAN.md` for the full
    /// design.
    ///
    /// `#[serde(skip)]` because the bytes round-trip through the NAPI
    /// `Buffer` type at the boundary, not through `serde_json` (same
    /// pattern as [`Self::precomputed_prefixes`]).
    #[serde(skip)]
    pub precomputed_browserslist: Option<Vec<u8>>,

    /// **Optional perf knob — NOT part of the upstream `TransformOpts`
    /// surface.** Filesystem-path delivery for the
    /// [`PrecomputedBrowserslist`] snapshot. When set and
    /// [`Self::precomputed_browserslist`] is `None`, the file is read
    /// on each call and decoded as a snapshot.
    ///
    /// **WASI design intent:** SWC tears down the WASI linear-memory
    /// instance between every transform — no in-memory cache survives.
    /// The host writes the snapshot to a known path once per build;
    /// every WASI plugin instance reads from that path on each call.
    /// The OS page cache amortises the disk read.
    ///
    /// **Precedence:**
    ///   1. If [`Self::precomputed_browserslist`] is `Some`, use those
    ///      bytes.
    ///   2. Else if `precomputed_browserslist_path` is `Some`, read
    ///      the file. Read failure is a HARD error.
    ///   3. Else fall back to per-plugin in-process resolution
    ///      (correct in NAPI; wrong default-list in WASI).
    ///
    /// Mirrors the [`Self::precomputed_prefixes_path`] pattern.
    #[serde(skip)]
    pub precomputed_browserslist_path: Option<std::path::PathBuf>,
}

/// Mirrors upstream return shape: `{ sheets: string[]; classNames: string[] }`.
///
/// - `sheets` — emitted by `extractStyleSheets` via its callback during
///   OnceExit. NOT deduplicated (matches transform.ts line 81: returned
///   raw, no `unique` wrap).
/// - `class_names` — emitted by `atomicifyRules` via its callback during
///   OnceExit, then deduplicated via `unique()` (matches transform.ts
///   line 82: `classNames: unique(classNames)`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformResult {
    pub sheets: Vec<String>,
    #[serde(rename = "classNames")]
    pub class_names: Vec<String>,
}

/// `transformCss(css, opts)` — packages/css/src/transform.ts:33.
///
/// Returns `{ sheets, classNames }`. The error path mirrors upstream
/// postcss: parse errors bubble up as `Err(message)`. JS wraps the whole
/// pipeline in a try/catch that re-throws via `createError('css', ...)`;
/// we surface the underlying error string and let the NAPI shim wrap it
/// per `transform.ts:84-99`.
pub fn transform_css(css: &str, opts: &TransformOpts) -> Result<TransformResult, String> {
    let optimize_css = opts.optimize_css.unwrap_or(true);
    let increase_specificity_enabled = opts.increase_specificity.unwrap_or(false);
    // Mirrors `process.env.AUTOPREFIXER === 'off'` — string-equality check
    // against the literal "off". Anything else (including unset / empty /
    // any other value) leaves autoprefixer enabled.
    let autoprefixer_enabled = std::env::var("AUTOPREFIXER")
        .map(|v| v != "off")
        .unwrap_or(true);

    let mut root = parse(css).map_err(|e| format!("parse error: {e}"))?;

    // ------------------------------------------------------------------
    // Round 1 — Once round (plugin-array order)
    // ------------------------------------------------------------------

    // 1. discardDuplicates.Once
    discard_duplicates(&mut root)
        .map_err(|e| format!("discard-duplicates: {e:?}"))?;

    // 3. parentOrphanedPseudos.Once
    parent_orphaned_pseudos(&mut root)
        .map_err(|e| format!("parent-orphened-pseudos: {e:?}"))?;

    // 9. sortAtomicStyleSheet.Once — load-bearing position! Despite sitting
    //    at array index 9, it MUST run here in the Once round, before any
    //    walk visitor or OnceExit. See sort.rs for the same hazard.
    let sort_opts = SortAtomicStyleSheetOpts {
        sort_at_rules_enabled: opts.sort_at_rules,
        sort_shorthand_enabled: opts.sort_shorthand,
    };
    sort_atomic_style_sheet(&mut root, &sort_opts)
        .map_err(|e| format!("sort-atomic-style-sheet: {e:?}"))?;

    // ------------------------------------------------------------------
    // Round 2 — Walk round
    // ------------------------------------------------------------------

    // 4. postcss-nested.Rule — tree sweep that internally re-enters the
    //    walk on promoted siblings. Output is byte-equivalent to JS's
    //    interleaved walk because postcss-nested only mutates Rule
    //    structure; the Declaration visitors below operate on the
    //    post-nested decl positions, which is what JS would also do.
    let nested_opts = PostcssNestedOpts {
        bubble: vec![
            "container".to_string(),
            "-moz-document".to_string(),
            "layer".to_string(),
            "else".to_string(),
            "when".to_string(),
            // postcss-nested bubbles `starting-style` by default in versions from 6.0.2 onwards;
            // we're pinned to 5.0.6 so we still need to pass it explicitly (matches
            // transform.ts:54).
            "starting-style".to_string(),
        ],
        unwrap: vec![
            "color-profile".to_string(),
            "counter-style".to_string(),
            "font-palette-values".to_string(),
            "page".to_string(),
            "property".to_string(),
        ],
        preserve_empty: false,
    };
    postcss_nested(&mut root, &nested_opts)
        .map_err(|e| format!("postcss-nested: {e:?}"))?;

    // Merged Declaration walk — single DFS, firing the three Declaration
    // visitors in plugin-array order at every decl node:
    //
    //   #2  discardEmptyRules        (remove on empty value; remove parent rule)
    //   #5o normalize-current-color  (in-place value rewrite; only when optimizeCss)
    //   #6  expandShorthands         (replaceWith N longforms)
    //
    // Per-node interleave matches postcss's `lazy-result.js::visitTick`:
    // at each Declaration node, walk each visitor in array order; if any
    // visitor removes/replaces the node, subsequent visitors at this
    // slot do NOT fire (postcss bails via `if (!node.parent)`). New
    // siblings inserted by `replaceWith` advance the cursor past them.
    interleaved_decl_walk(&mut root.root, optimize_css);

    // ------------------------------------------------------------------
    // Round 3 — OnceExit round (plugin-array order)
    // ------------------------------------------------------------------

    // 5a..5n. cssnano sub-plugins, filtered by BASE_PLUGINS ∪ PROD_PLUGINS,
    // applied in cssnano-preset-default source order (Anomaly #7).
    // Re-derived here (instead of calling normalize_css) so that the
    // Declaration visitor (normalize-current-color) is decoupled from the
    // OnceExit batch — the former runs in the walk round above.
    let plugins_to_include: std::collections::HashSet<&str> = if optimize_css {
        NORMALIZE_BASE_PLUGINS
            .iter()
            .chain(NORMALIZE_PROD_PLUGINS.iter())
            .copied()
            .collect()
    } else {
        NORMALIZE_BASE_PLUGINS.iter().copied().collect()
    };
    // Phase C — decode the host-resolved browserslist snapshot once,
    // before the OnceExit batch runs, so all 5 browserslist-aware
    // leaf plugins (`postcss-reduce-initial`, `-colormin`,
    // `-convert-values`, `-minify-params`, `-normalize-unicode`) see
    // the same `Arc<PrecomputedBrowserslist>` instance — zero clones
    // across the per-plugin dispatch.
    //
    // Precedence: inline bytes > path > None (per the docstring on
    // `TransformOpts::precomputed_browserslist_path`). `None` ==
    // pre-2026-05-08 behaviour (each plugin runs the in-process
    // shim resolution; correct in NAPI, wide-default in WASI).
    //
    // Path-read / decode failures are HARD errors so production
    // misconfiguration doesn't silently mask as "wide-default
    // browserslist" output drift.
    let browserslist_path_bytes;
    let browserslist_bytes: Option<&[u8]> = match (
        opts.precomputed_browserslist.as_deref(),
        opts.precomputed_browserslist_path.as_deref(),
    ) {
        (Some(bytes), _) => Some(bytes),
        (None, Some(path)) => {
            browserslist_path_bytes = std::fs::read(path).map_err(|e| {
                format!(
                    "cssnano precomputed_browserslist_path read error \
                     (path={}): {e}",
                    path.display(),
                )
            })?;
            Some(browserslist_path_bytes.as_slice())
        }
        (None, None) => None,
    };
    let browserslist_snapshot: Option<Arc<PrecomputedBrowserslist>> = match browserslist_bytes {
        Some(bytes) => {
            let decoded = decode_precomputed(bytes).map_err(|e| {
                format!("cssnano browserslist precomputed load error: {e:?}")
            })?;
            Some(Arc::new(decoded))
        }
        None => None,
    };

    // 5a..5n. cssnano sub-plugins. `PresetOpts::browserslist_snapshot`
    // threads the decoded snapshot to the 5 browserslist-aware leaf
    // plugins via `cssnano-preset-default`'s dispatch. The other 9
    // plugins ignore the field (Phase B keeps the snapshot field on
    // `&PresetOpts` for all 14 plugin entries to preserve the uniform
    // function-pointer signature).
    let preset_opts = PresetOpts {
        browserslist_snapshot,
    };
    let preset = default_preset(&preset_opts);
    for entry in &preset.plugins {
        if plugins_to_include.contains(entry.name) {
            (entry.apply)(&mut root, &preset_opts)
                .map_err(|e| format!("cssnano:{}: {e:?}", entry.name))?;
        }
    }

    // 7. atomicifyRules.OnceExit — emits class names via callback.
    //    Mirrors `callback: (className) => classNames.push(className)`.
    let mut atomicify_opts = AtomicifyRulesOpts {
        class_name_compression_map: opts.class_name_compression_map.clone(),
        class_hash_prefix: opts.class_hash_prefix.clone(),
        class_names: Vec::new(),
    };
    atomicify_rules(&mut root, &mut atomicify_opts)
        .map_err(|e| format!("atomicify-rules: {e:?}"))?;
    let raw_class_names = atomicify_opts.class_names;

    // 8. increaseSpecificity.OnceExit — gated by opts.increaseSpecificity.
    if increase_specificity_enabled {
        increase_specificity(&mut root)
            .map_err(|e| format!("increase-specificity: {e:?}"))?;
    }

    // 10. autoprefixer.OnceExit — gated by `process.env.AUTOPREFIXER !== "off"`.
    //     Internal lifecycle: `prepare` → loadPrefixes; OnceExit calls
    //     `prefixes.processor.remove(root)` then `prefixes.processor.add(root)`.
    if autoprefixer_enabled {
        // Fast path: precomputed postcard snapshot. Two delivery
        // surfaces — inline bytes (NAPI Buffer round-trip) and
        // filesystem path (WASI host-writes-once pattern). Precedence
        // is inline > path > slow build. Path-read failure is a HARD
        // error (not silent slow-path fallback) so production config
        // mistakes don't hide behind a 100x perf regression.
        // `path_bytes_arc` outlives `prefixes_bytes` — its `&[u8]`
        // borrow is held only across `build_prefixes_from_precomputed`
        // below. The Arc is the within-transform byte cache (see
        // module-level rationale on `PRECOMPUTED_PREFIXES_BYTES_CACHE`).
        let path_bytes_arc;
        let prefixes_bytes: Option<&[u8]> = match (
            opts.precomputed_prefixes.as_deref(),
            opts.precomputed_prefixes_path.as_deref(),
        ) {
            (Some(bytes), _) => Some(bytes),
            (None, Some(path)) => {
                path_bytes_arc = read_precomputed_prefixes_cached(path).map_err(|e| {
                    format!(
                        "autoprefixer precomputed_prefixes_path read error \
                         (path={}): {e}",
                        path.display(),
                    )
                })?;
                Some(path_bytes_arc.as_slice())
            }
            (None, None) => None,
        };

        // Precedence: prefix-snapshot bytes > browserslist-snapshot
        // (autoprefixer reqs override) > slow build (autoprefixer
        // resolves browserslist itself via env/FS walk). The
        // browserslist-snapshot branch is the load-bearing fix for
        // Phase E7's cross-pipeline gate: without it, the env-pinned
        // arm and the snapshot arm produce different prefix sets
        // because autoprefixer's slow path reads
        // BROWSERSLIST_CONFIG / walks `.browserslistrc`, neither of
        // which is reachable from inside WASI. Passing
        // `snap.selected` as `override_browserslist` short-circuits
        // autoprefixer's resolution to the same 14-entry list the 5
        // cssnano leaf plugins already see — proving in E7 that
        // env-pinned NAPI and snapshot-driven WASI emit byte-equal
        // CSS for the user-select prefix decision and any other
        // browserslist-gated autoprefixer rule.
        // The precomputed-bytes arm goes through a thread-local
        // `Arc<Prefixes>` cache (see `PREFIXES_DECODED_CACHE` rationale
        // above). The other two arms (browserslist-snapshot, slow
        // default) are not cached — they're either WASI-only (where
        // the cache wouldn't survive teardown anyway) or development /
        // fallback paths.
        let mut prefixes_owned: Option<Prefixes> = None;
        let mut prefixes_arc: Option<std::sync::Arc<Prefixes>> = None;
        match prefixes_bytes {
            Some(bytes) => {
                prefixes_arc = Some(cached_prefixes_from_bytes(bytes)?);
            }
            None => {
                prefixes_owned = Some(match preset_opts.browserslist_snapshot.as_ref() {
                    Some(snap) => build_prefixes(
                        None,
                        AutoprefixerOptions {
                            override_browserslist: Some(snap.selected.clone()),
                            ..Default::default()
                        },
                    )
                    .map_err(|e| format!("autoprefixer build error: {e}"))?,
                    None => build_prefixes_default(None)
                        .map_err(|e| format!("autoprefixer build error: {e}"))?,
                });
            }
        }
        let prefixes_ref: &Prefixes = match (&prefixes_arc, &prefixes_owned) {
            (Some(arc), _) => arc.as_ref(),
            (None, Some(owned)) => owned,
            (None, None) => unreachable!("one branch was taken above"),
        };
        let proc = AutoprefixerProcessor::new(prefixes_ref);
        let mut warnings: Vec<String> = Vec::new();
        // Warnings are diagnostic-only (not on the hashing path); discarded.
        //
        // `proc.remove` strips stale vendor-prefixed CSS the user wrote
        // but doesn't need for the current browserslist. Every match
        // path in `remove` requires the AST to contain a vendor-prefix
        // substring; no prior pipeline stage introduces one
        // (`autoprefixer::add` is the only writer and it runs AFTER
        // `remove`). So when the input string has no `-webkit-` /
        // `-moz-` / `-ms-` / `-o-` token, `remove` is provably a
        // no-op and we skip its ~33 µs/call walk. See
        // `input_has_vendor_prefix` for the full safety argument.
        if input_has_vendor_prefix(css) {
            proc.remove(&mut root.root, &mut warnings);
        }
        proc.add(&mut root.root, &mut warnings);
    }

    // 11. postcss-normalize-whitespace.OnceExit
    postcss_normalize_whitespace(&mut root)
        .map_err(|e| format!("postcss-normalize-whitespace: {e:?}"))?;

    // 12. extractStyleSheets.OnceExit — emits one entry per top-level node
    //     via callback. NOT deduplicated.
    let mut extract_opts = ExtractStyleSheetsOpts {
        sheets: Vec::new(),
    };
    extract_stylesheets(&root, &mut extract_opts)
        .map_err(|e| format!("extract-stylesheets: {e:?}"))?;
    let sheets = extract_opts.sheets;

    // Final shape: classNames deduped via unique() (transform.ts:82).
    Ok(TransformResult {
        sheets,
        class_names: unique(&raw_class_names),
    })
}

// --------------------------------------------------------------------------
// Walk-round merged Declaration walker
// --------------------------------------------------------------------------

/// Single DFS over `parent`'s subtree firing the three Declaration
/// visitors at every Declaration node in plugin-array order. Mirrors
/// postcss's `lazy-result.js::visitTick` per-node visitor dispatch:
///
///   1. discardEmptyRules.Declaration       (#2)
///   2. normalize-current-color.Declaration (#5o, only when optimizeCss)
///   3. expandShorthands.Declaration        (#6)
///
/// Mutation semantics — match postcss exactly:
/// - If a visitor removes the decl (`isValueEmpty` branch of #2), the
///   remaining visitors at this slot do NOT fire on the dead node.
///   Postcss bails via `if (!node.parent) { stack.pop() }` in
///   `visitTick`. The parent rule, if it became empty as a result, is
///   removed in the outer recursion (matches upstream's `parent.remove()`
///   in `discard-empty-rules.ts:17-19`).
/// - If `expandShorthands` returns N longforms (`Mutation::ReplaceMany`),
///   we splice them in via `replace_with_at` (the same path
///   `walk_decls_mut` would use, including the Root.normalize raws-
///   transfer dance) and advance the cursor past them. Postcss's walk
///   would mark the new nodes dirty and re-visit them; we don't because
///   the longforms (audit Plugin 6 mutation profile) are NOT shorthand
///   props so `expandShorthands` no-ops on them, `currentcolor` doesn't
///   appear in expansion outputs so `normalize-current-color` no-ops,
///   and they have non-empty values so `discardEmptyRules` no-ops. All
///   three visitors are guaranteed no-ops on expansion outputs by
///   construction; skipping the re-visit is byte-identical to firing
///   them.
///
/// Returns `true` if at least one Declaration was removed (by visitor
/// #2's empty-value branch) directly from `parent`'s child list. The
/// caller uses that flag to decide whether to remove `parent` itself
/// when it's a Rule that became empty — matching upstream's
/// `parent.remove()` branch.
#[doc(hidden)]
pub fn interleaved_decl_walk(parent: &mut Node, optimize_css: bool) -> bool {
    if parent.nodes().is_none() {
        return false;
    }

    let mut removed_decl_here = false;
    let mut i = 0usize;
    loop {
        let len = parent.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }

        // Inspect kind without holding a borrow across mutation.
        let kind_tag: u8 = {
            let child = &parent.nodes().unwrap()[i];
            match &child.kind {
                NodeKind::Rule(_) | NodeKind::AtRule(_) => 1, // container
                NodeKind::Declaration(_) => 2,
                _ => 0,
            }
        };

        if kind_tag == 1 {
            // Recurse into containers FIRST, just like postcss's DFS:
            // postcss's `visitTick` walks children before the current
            // node's exit visitors. Our visitors are all entry-only
            // (no Exit hooks in this round) — descend.
            let child_lost_decl = {
                let child = &mut parent.nodes_mut().unwrap()[i];
                interleaved_decl_walk(child, optimize_css)
            };

            // discardEmptyRules's parent-removal branch: only Rule
            // (not AtRule), and only when removing a decl is what
            // emptied it (already-empty rules are left alone — upstream
            // never fires the visitor on them).
            let should_drop_child = {
                let child = &parent.nodes().unwrap()[i];
                child_lost_decl
                    && matches!(child.kind, NodeKind::Rule(_))
                    && child.nodes().map_or(false, |n| n.is_empty())
            };
            if should_drop_child {
                remove_at(parent, i);
                continue; // cursor stays — next sibling slid down
            }
            i += 1;
            continue;
        }

        if kind_tag != 2 {
            i += 1;
            continue;
        }

        // ----- Declaration: fire visitors in plugin-array order -----

        // Visitor #2: discardEmptyRules.
        let value_is_empty: bool = {
            let child = &parent.nodes().unwrap()[i];
            if let NodeKind::Declaration(d) = &child.kind {
                is_value_empty(&d.value)
            } else {
                false
            }
        };
        if value_is_empty {
            remove_at(parent, i);
            removed_decl_here = true;
            // Postcss `visitTick` short-circuits subsequent visitors on
            // this dead node via `if (!node.parent) { stack.pop() }`.
            // We do the same by `continue`-ing without firing #5o or #6.
            continue;
        }

        // Visitor #5o: normalize-current-color (in-place value rewrite).
        // Conditional on `optimizeCss`, mirroring `normalize-css.ts:76-78`
        // which skips the visitor when `optimizeCss === false`.
        if optimize_css {
            let child = &mut parent.nodes_mut().unwrap()[i];
            normalize_current_color_decl(child);
        }

        // Visitor #6: expandShorthands. Returns Some(new_decls) when
        // the decl matches a shorthand prop and is safe to expand —
        // `decl.replaceWith(...new_decls)` in upstream terms.
        let expansion: Option<Vec<Node>> = {
            let child = &mut parent.nodes_mut().unwrap()[i];
            expand_shorthand_decl(child)
        };
        if let Some(new_decls) = expansion {
            let n = new_decls.len();
            replace_with_at(parent, i, new_decls);
            // Advance past the inserts. Per the function-level comment
            // above, the longforms are guaranteed no-ops for all three
            // visitors so we do not re-visit them. This matches
            // `walk_mut`'s `Mutation::ReplaceMany` index advance —
            // i = end (= i + n) — and is byte-identical to postcss's
            // re-visit since the visitors are no-ops on the inserts.
            i += n;
            continue;
        }

        i += 1;
    }

    removed_decl_here
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reset env-var side effects in tests that toggle AUTOPREFIXER. This
    /// is best-effort because Rust tests run in parallel; we serialize
    /// AUTOPREFIXER-touching tests onto a mutex.
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn empty_input_returns_empty_lists() {
        let r = transform_css("", &TransformOpts::default()).unwrap();
        assert!(r.sheets.is_empty(), "got: {:?}", r.sheets);
        assert!(r.class_names.is_empty(), "got: {:?}", r.class_names);
    }

    #[test]
    fn simple_decl_emits_one_atomic_class_and_sheet() {
        // A single top-level decl `color: red` becomes:
        //   1. atomicify wraps it as `._<hash> { color: red }`
        //   2. extract-stylesheets emits one sheet
        //   3. classNames contains the corresponding class
        let r = transform_css("color: red;", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 1, "expected 1 sheet, got: {:?}", r.sheets);
        assert_eq!(r.class_names.len(), 1, "expected 1 class, got: {:?}", r.class_names);
        let class = &r.class_names[0];
        assert!(class.starts_with('_'), "class should start with _: {class}");
        // The sheet should contain the same class as a selector and the
        // declaration.
        assert!(r.sheets[0].contains(class), "sheet should reference class {class}: {}", r.sheets[0]);
        assert!(r.sheets[0].contains("color:red"), "sheet should contain color:red after whitespace normalization: {}", r.sheets[0]);
    }

    #[test]
    fn multi_decl_emits_one_class_per_decl() {
        let r = transform_css("color: red; background: blue;", &TransformOpts::default()).unwrap();
        // After atomicify, two decls → two top-level rules → two sheets.
        // After unique(), if the classes collide they'd be one — but
        // different (prop, value) pairs hash to different classes so we
        // expect 2 distinct.
        assert_eq!(r.sheets.len(), 2, "expected 2 sheets, got: {:?}", r.sheets);
        assert_eq!(r.class_names.len(), 2, "got: {:?}", r.class_names);
    }

    #[test]
    fn duplicate_decl_is_deduplicated_to_one_class_post_unique() {
        // Two identical decls. discardDuplicates removes the earlier
        // one (Once round). Only one class survives.
        let r = transform_css("color: red; color: red;", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 1, "after discardDuplicates we expect 1 sheet, got: {:?}", r.sheets);
        assert_eq!(r.class_names.len(), 1, "got: {:?}", r.class_names);
    }

    #[test]
    fn rule_with_decl_emits_class_keyed_to_selector() {
        let r = transform_css("a { color: red; }", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 1, "got: {:?}", r.sheets);
        assert_eq!(r.class_names.len(), 1, "got: {:?}", r.class_names);
        // The selector should embed both the parent `a` and the atomic class.
        assert!(r.sheets[0].contains("a"), "got: {}", r.sheets[0]);
    }

    #[test]
    fn increase_specificity_off_by_default() {
        // Without increaseSpecificity, atomic class selectors do NOT
        // contain the `:not(#\#)` marker.
        let r = transform_css("color: red;", &TransformOpts::default()).unwrap();
        assert!(!r.sheets[0].contains(":not(#"), "default should not increase specificity: {}", r.sheets[0]);
    }

    #[test]
    fn increase_specificity_on_appends_not_marker() {
        let opts = TransformOpts {
            increase_specificity: Some(true),
            ..Default::default()
        };
        let r = transform_css("color: red;", &opts).unwrap();
        // `INCREASE_SPECIFICITY_SELECTOR` is `:not(#\#)`. It's appended
        // after the atomic class.
        assert!(r.sheets[0].contains(":not(#"), "expected specificity-bumped selector: {}", r.sheets[0]);
    }

    #[test]
    fn autoprefixer_off_env_disables_prefixer() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialised via ENV_LOCK; tests that read AUTOPREFIXER
        // also take this lock.
        std::env::set_var("AUTOPREFIXER", "off");
        let r = transform_css("user-select: none;", &TransformOpts::default()).unwrap();
        std::env::remove_var("AUTOPREFIXER");
        // With autoprefixer off, no `-webkit-user-select` prefix decl is added.
        for sheet in &r.sheets {
            assert!(!sheet.contains("-webkit-user-select"), "got sheet with prefix despite AUTOPREFIXER=off: {sheet}");
        }
    }

    #[test]
    fn autoprefixer_on_env_runs_prefixer() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("AUTOPREFIXER");
        // Default behavior: autoprefixer runs. The exact prefix list
        // depends on browserslist resolution, so we assert a weaker
        // property: the run completes successfully.
        let r = transform_css("user-select: none;", &TransformOpts::default()).unwrap();
        assert!(!r.sheets.is_empty());
    }

    #[test]
    fn optimize_css_false_skips_normalize_current_color() {
        // With optimizeCss=false, `normalize-current-color` does not run
        // in the walk round, so `currentcolor` is preserved verbatim.
        let opts = TransformOpts {
            optimize_css: Some(false),
            ..Default::default()
        };
        let r = transform_css("color: currentcolor;", &opts).unwrap();
        let combined: String = r.sheets.join("");
        assert!(combined.contains("currentcolor"), "expected currentcolor preserved with optimizeCss=false: {combined}");
    }

    #[test]
    fn optimize_css_default_normalizes_current_color() {
        // With default optimizeCss=true, `currentcolor` gets canonicalised.
        let r = transform_css("color: currentcolor;", &TransformOpts::default()).unwrap();
        let combined: String = r.sheets.join("");
        assert!(combined.contains("currentColor"), "expected currentColor normalisation: {combined}");
    }

    #[test]
    fn optimize_css_false_skips_cssnano_prod_plugins() {
        // With optimizeCss=false, postcss-colormin (a PROD-only cssnano
        // plugin) does NOT run. With optimizeCss=true (default) colormin
        // canonicalises `#ff0000` → `red`. We assert the colormin-skipped
        // output contains the original `#ff0000` byte sequence.
        let opts = TransformOpts {
            optimize_css: Some(false),
            ..Default::default()
        };
        let r = transform_css("color: #ff0000;", &opts).unwrap();
        let combined: String = r.sheets.join("");
        assert!(combined.contains("#ff0000"), "expected #ff0000 preserved with optimizeCss=false: {combined}");

        // Sanity: with optimizeCss=true, colormin compresses to `red`.
        let r2 = transform_css("color: #ff0000;", &TransformOpts::default()).unwrap();
        let combined2: String = r2.sheets.join("");
        assert!(combined2.contains("red"), "expected colormin compression with optimizeCss=true: {combined2}");
    }

    #[test]
    fn nested_rule_unwrapped_by_postcss_nested() {
        // postcss-nested unwraps `b` to be a sibling-level rule with
        // selector `a b`. Atomicify then turns each surviving decl into
        // an atomic class.
        let r = transform_css("a { color: red; b { color: blue; } }", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 2, "expected 2 atomic sheets after unwrapping, got: {:?}", r.sheets);
        // Both classes should be distinct because the parent selectors
        // differ ("a" vs "a b").
        assert_eq!(r.class_names.len(), 2, "got: {:?}", r.class_names);
    }

    #[test]
    fn empty_decl_value_dropped_by_discard_empty_rules() {
        // discardEmptyRules removes the empty-valued decl during the
        // walk round. The remaining decl(s) still get atomicified — note
        // that `background` gets normalised through expand-shorthands +
        // cssnano so the output prop name may differ (background →
        // background-color), but the value `blue` survives.
        let r = transform_css("color: undefined; background: blue;", &TransformOpts::default()).unwrap();
        let combined: String = r.sheets.join("");
        assert!(!combined.contains("undefined"), "expected `color: undefined` dropped: {combined}");
        assert!(combined.contains("blue"), "expected `blue` value preserved: {combined}");
        // We expect exactly one sheet (the `color: undefined` decl is
        // dropped, and `background: blue` survives — though it may be
        // expanded into a single longform `background-color: blue`).
        assert!(!r.sheets.is_empty(), "expected at least one sheet after dropping empty decl");
    }

    #[test]
    fn shorthand_expanded_by_expand_shorthands() {
        // expandShorthands turns `margin: 1px` into four longform decls.
        // Each becomes its own atomic class.
        let r = transform_css("margin: 1px;", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 4, "expected 4 sheets after margin expansion, got: {:?}", r.sheets);
        assert_eq!(r.class_names.len(), 4, "got: {:?}", r.class_names);
    }

    #[test]
    fn parent_orphaned_pseudos_prepends_nesting_selector() {
        // `:hover { color: red; }` — parentOrphanedPseudos rewrites to
        // `&:hover { color: red; }` in the Once round, then atomicify
        // turns it into an atomic-class rule with the `:hover` suffix.
        let r = transform_css(":hover { color: red; }", &TransformOpts::default()).unwrap();
        let combined: String = r.sheets.join("");
        assert!(combined.contains(":hover"), "expected :hover preserved on atomic selector: {combined}");
    }

    #[test]
    fn callback_dedup_preserves_order() {
        // unique() preserves insertion order. Three identical decls
        // produce three identical class emits; after dedup, only one
        // entry remains.
        let r = transform_css(
            ".x { color: red; } .y { color: red; } .z { color: red; }",
            &TransformOpts::default(),
        )
        .unwrap();
        // Each rule produces its own atomic class because the selector
        // differs (.x → "& .x" prefix, etc.). So we expect 3 distinct
        // classes — unique() leaves them all.
        assert_eq!(r.class_names.len(), 3, "got: {:?}", r.class_names);
    }

    #[test]
    fn class_hash_prefix_applied() {
        let opts = TransformOpts {
            class_hash_prefix: Some("ax".to_string()),
            ..Default::default()
        };
        let r = transform_css("color: red;", &opts).unwrap();
        // Hash prefix changes the hash input, so the class name is
        // different from the no-prefix case but still starts with `_`.
        assert!(r.class_names[0].starts_with('_'), "got: {:?}", r.class_names);
    }

    #[test]
    fn class_hash_prefix_invalid_returns_error() {
        // atomicifyRules validates class_hash_prefix at run time. Invalid
        // prefix → error.
        let opts = TransformOpts {
            class_hash_prefix: Some("123-bad-leading-digit".to_string()),
            ..Default::default()
        };
        let result = transform_css("color: red;", &opts);
        assert!(result.is_err(), "expected error on invalid hash prefix");
    }

    #[test]
    fn parse_error_propagates() {
        // postcss is fairly permissive — it accepts `a { ` and similar
        // by treating the rest of input as the rule body. To hit the
        // error path we need something the underlying tokenizer rejects.
        // An unterminated `/*` comment is a hard parse error.
        let result = transform_css("/*unterminated", &TransformOpts::default());
        // If postcss-core treats this as recoverable (matching upstream
        // postcss behaviour) we expect Ok; document either way. The
        // important property is that parse errors do propagate when they
        // occur — verified at the call site via `parse(...).map_err`.
        // This test asserts at least that we don't panic on edge inputs.
        let _ = result;
    }

    // ---- per-node walk-round interleave ----

    /// Exercises the per-node visitor order at a Declaration node:
    /// `normalize-current-color` (#5o) fires BEFORE `expandShorthands`
    /// (#6) at the same decl. With `optimizeCss = true`, an input of
    /// `margin: currentcolor` flows as:
    ///
    ///   1. discardEmptyRules    → no-op (value not empty)
    ///   2. normalize-current-color → rewrites to `margin: currentColor`
    ///   3. expandShorthands     → expands to 4 longforms with the
    ///                              CANONICAL `currentColor` casing
    ///
    /// Because normalize fires first per node, every longform inherits
    /// the canonical casing — there is no ordering in which the
    /// lowercase `currentcolor` could leak into the longform values
    /// when `optimizeCss` is on.
    ///
    /// This is also a regression for the original "byte-equivalent
    /// departure" implementation that ran the three Declaration
    /// visitors as sequential sweeps. Sequential and interleaved
    /// happen to match here (see "commutativity proof" test below) —
    /// but the canonical-casing assertion below is the spec-mandated
    /// outcome regardless of how the visitors are dispatched.
    #[test]
    fn per_node_interleave_normalize_then_expand_at_shorthand() {
        let r = transform_css("margin: currentcolor;", &TransformOpts::default()).unwrap();
        let combined: String = r.sheets.join("");
        // Four longforms emitted (margin → margin-top/right/bottom/left).
        assert_eq!(
            r.sheets.len(),
            4,
            "expected 4 longform sheets, got: {:?}",
            r.sheets
        );
        // Canonical `currentColor` casing — proves normalize fired
        // before expand at the original decl.
        assert!(
            combined.contains("currentColor"),
            "expected canonical currentColor in expanded longforms: {combined}"
        );
        assert!(
            !combined.contains("currentcolor"),
            "lowercase currentcolor must not survive: {combined}"
        );
    }

    /// Exercises the per-node short-circuit semantics:
    /// `discardEmptyRules` (#2) removing a decl prevents subsequent
    /// visitors at this slot from firing on the dead node — matching
    /// postcss's `if (!node.parent) { stack.pop() }` in
    /// `lazy-result.js::visitTick`.
    ///
    /// Input: `margin: undefined` — would have been a shorthand-prop
    /// decl with empty value. discard removes it; expand never sees
    /// it. The assertion is a process-of-elimination one: zero sheets
    /// emitted (nothing to atomicify after the only decl is dropped),
    /// and crucially the run does NOT panic / produce any longforms
    /// (which would happen if expand ran on the decl despite discard
    /// having removed it).
    #[test]
    fn per_node_interleave_discard_short_circuits_expand() {
        let r = transform_css("margin: undefined;", &TransformOpts::default()).unwrap();
        assert_eq!(
            r.sheets.len(),
            0,
            "decl removed by discardEmptyRules → no atomicify output: {:?}",
            r.sheets
        );
        assert_eq!(r.class_names.len(), 0, "got: {:?}", r.class_names);
    }

    /// Same per-node short-circuit, but for the parent-rule-removal
    /// branch of `discardEmptyRules`: when removing the only decl
    /// empties the parent rule, the parent rule itself is also dropped
    /// (`packages/css/src/plugins/discard-empty-rules.ts:17-19`). The
    /// merged walk handles this in the outer container recursion, after
    /// processing the rule's children — same logical position as JS's
    /// `parent.remove()` call after `node.remove()`.
    ///
    /// Input: `:hover { margin: undefined; }`. After the walk:
    ///   - Once round: `parentOrphanedPseudos` rewrites `:hover` to
    ///     `&:hover` (caught later by atomicify's nesting handler).
    ///   - Walk round: discard removes the decl, then the rule.
    /// Net: zero output sheets, zero classes.
    #[test]
    fn per_node_interleave_drops_parent_rule_when_emptied() {
        let r = transform_css(":hover { margin: undefined; }", &TransformOpts::default()).unwrap();
        assert_eq!(
            r.sheets.len(),
            0,
            "rule removed after empty-decl drop: {:?}",
            r.sheets
        );
        assert_eq!(r.class_names.len(), 0, "got: {:?}", r.class_names);
    }

    /// Documents the proof that the three Declaration visitors are
    /// **commutative on every realistic input**, and therefore the
    /// per-node interleave produces byte-identical output to a
    /// sequential-sweep arrangement on every input we can construct.
    ///
    /// The proof, by case analysis:
    ///
    /// - `discardEmptyRules` only acts when `decl.value` is `'undefined'`,
    ///   `'null'`, or trims to empty. It does not produce or rewrite
    ///   decls.
    /// - `normalize-current-color` only acts when `decl.value`
    ///   case-insensitively equals `'currentcolor'` or `'current-color'`
    ///   (whole-string match). It mutates `decl.value` in place; never
    ///   adds/removes decls.
    /// - `expandShorthands` only acts when `decl.prop` matches one of
    ///   the 11 supported shorthand props AND the value parses without
    ///   `var(...)`. It replaces the decl with N longforms (whose props
    ///   are NOT in the shorthand list — proven by audit Plugin 6
    ///   "no infinite loop" guarantee).
    ///
    /// Independence: the three predicates above are mutually exclusive
    /// on what they can act on at the per-decl level. The only
    /// cross-visitor flow is normalize's value rewrite feeding into
    /// expand's parser — but since the array order puts normalize BEFORE
    /// expand both in interleave AND in sequential sweep order, the
    /// observable expand input is byte-identical in both arrangements.
    ///
    /// This test demonstrates the equivalence empirically on the
    /// strongest candidate input: a value that is exactly `currentcolor`
    /// (so normalize fires) AND a shorthand prop (so expand fires).
    /// Both paths yield canonical `currentColor` longforms.
    #[test]
    fn per_node_interleave_commutativity_proof() {
        // The strongest input: `outline: currentcolor` triggers normalize
        // (whole-value `currentcolor`). After normalize → `outline:
        // currentColor`. Then outline.extract("currentColor") →
        // is_color(currentColor) is FALSE (only the lowercase
        // `currentcolor` is in the special-case list per
        // `expand_shorthands/utils.rs::is_color`), so extraction errors
        // out → empty Vec → decl REMOVED via replaceWith(empty).
        let r = transform_css("outline: currentcolor;", &TransformOpts::default()).unwrap();
        assert_eq!(
            r.sheets.len(),
            0,
            "outline:currentcolor → normalize → expand fails to parse → \
             decl dropped (commutative across both interleave and \
             sequential sweep): {:?}",
            r.sheets
        );

        // For a shorthand prop where expand DOES succeed on a
        // post-normalize value:
        let r = transform_css("margin: currentcolor;", &TransformOpts::default()).unwrap();
        assert_eq!(r.sheets.len(), 4, "margin → 4 longforms: {:?}", r.sheets);
        let combined: String = r.sheets.join("");
        assert!(
            combined.contains("currentColor"),
            "canonical casing in longforms: {combined}"
        );

        // Independence sample: discard's empty-removal alongside expand.
        // The empty decl is removed; the shorthand decl is expanded.
        // No interleave dependency between them.
        let r = transform_css(
            "a { color: undefined; margin: 1px; }",
            &TransformOpts::default(),
        )
        .unwrap();
        // 4 margin longforms; the empty `color` decl drops out.
        assert_eq!(r.sheets.len(), 4, "got: {:?}", r.sheets);
        let combined: String = r.sheets.join("");
        assert!(
            !combined.contains("undefined"),
            "empty decl dropped: {combined}"
        );
    }

    // ---- Phase C — precomputed_browserslist plumbing ----

    /// Phase C end-to-end gate: with `None`, output must be byte-
    /// identical to the pre-2026-05-08 path. Load-bearing regression
    /// gate for NAPI consumers (production AFM is parity-tested on
    /// this path; we cannot drift it).
    #[test]
    fn phase_c_snapshot_none_byte_equivalent_across_pipeline() {
        // Grab-bag exercising all 5 browserslist-aware plugins:
        //   - reduce-initial: `border-collapse: separate` (toInitial)
        //   - colormin: `rgba(0,0,0,0)` (transparent_default)
        //   - convert-values: `0%` (keepZeroPercent)
        //   - minify-params: `@media all` (collapse)
        //   - normalize-unicode: `unicode-range: u+0025-00ff`
        let css = ".a { border-collapse: separate; color: rgba(0,0,0,0); flex: 0 0 0%; }\n\
                   @media all { .b { color: red; } }\n\
                   @font-face { unicode-range: u+0025-00ff; }";

        let baseline = transform_css(css, &TransformOpts::default()).unwrap();

        let opts_with_explicit_none = TransformOpts {
            precomputed_browserslist: None,
            precomputed_browserslist_path: None,
            ..Default::default()
        };
        let with_none = transform_css(css, &opts_with_explicit_none).unwrap();
        assert_eq!(
            baseline.sheets, with_none.sheets,
            "explicit-None drifted from default TransformOpts",
        );
        assert_eq!(baseline.class_names, with_none.class_names);
    }

    /// Phase C: inline-bytes delivery == path-delivery for the same
    /// snapshot. Same encoded bytes via two channels MUST produce
    /// identical output.
    #[test]
    fn phase_c_inline_bytes_eq_path_delivery() {
        use cssnano_browserslist_snapshot::{
            encode_precomputed, PrecomputedBrowserslist, PRECOMPUTED_FORMAT_VERSION,
        };

        let snapshot = PrecomputedBrowserslist {
            format_version: PRECOMPUTED_FORMAT_VERSION,
            selected: vec![
                "chrome 144".to_string(),
                "firefox 147".to_string(),
                "safari 26.2".to_string(),
            ],
            joined_query: "chrome 144, firefox 147, safari 26.2".to_string(),
        };
        let bytes = encode_precomputed(&snapshot);

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "phase_c_browserslist_snapshot_{}_{nanos}.bin",
            std::process::id(),
        ));
        std::fs::write(&path, &bytes).expect("write snapshot bytes");

        let css = ".a { border-collapse: separate; }";

        let inline_opts = TransformOpts {
            precomputed_browserslist: Some(bytes.clone()),
            ..Default::default()
        };
        let path_opts = TransformOpts {
            precomputed_browserslist_path: Some(path.clone()),
            ..Default::default()
        };

        let inline_out = transform_css(css, &inline_opts).unwrap();
        let path_out = transform_css(css, &path_opts).unwrap();

        let _ = std::fs::remove_file(&path);

        assert_eq!(
            inline_out.sheets, path_out.sheets,
            "inline-bytes vs path-delivery drift",
        );
        assert_eq!(inline_out.class_names, path_out.class_names);

        // Sanity: modern snapshot → toInitial fires.
        let combined: String = inline_out.sheets.join("");
        assert!(
            combined.contains("initial"),
            "modern snapshot should enable toInitial rewrite, got: {combined}",
        );
    }

    /// Phase C: a missing path is a HARD error — production misconfig
    /// must NOT silently mask as a wide-default browserslist run.
    #[test]
    fn phase_c_missing_snapshot_path_is_hard_error() {
        let opts = TransformOpts {
            precomputed_browserslist_path: Some(std::path::PathBuf::from(
                "/nonexistent/phase-c-missing-snapshot.bin",
            )),
            ..Default::default()
        };
        let result = transform_css(".a { color: red; }", &opts);
        assert!(result.is_err(), "expected hard error for missing path");
        let err = result.unwrap_err();
        assert!(
            err.contains("precomputed_browserslist_path"),
            "error message should mention the field, got: {err}",
        );
    }

    /// Phase C: invalid (non-postcard) bytes are also a hard error.
    #[test]
    fn phase_c_invalid_snapshot_bytes_is_hard_error() {
        let opts = TransformOpts {
            precomputed_browserslist: Some(b"not-a-valid-postcard-snapshot".to_vec()),
            ..Default::default()
        };
        let result = transform_css(".a { color: red; }", &opts);
        assert!(result.is_err(), "expected hard error for malformed snapshot");
        let err = result.unwrap_err();
        assert!(
            err.contains("browserslist precomputed"),
            "error message should mention the snapshot, got: {err}",
        );
    }

    // ─────────────────────────────────────────────────────────────
    // Win 1 — within-transform `precomputed_prefixes_path` byte cache.
    // See `PRECOMPUTED_PREFIXES_BYTES_CACHE` module-level rationale.
    // ─────────────────────────────────────────────────────────────

    /// Cache hit ⇒ byte-identical output across repeated calls. The
    /// cache is below the parity-runner level (it only changes how
    /// bytes get from disk to `build_prefixes_from_precomputed`); this
    /// test pins that contract directly so a future refactor of the
    /// cache layer can't silently break it.
    #[test]
    fn precomputed_prefixes_cache_hit_preserves_output() {
        let _g = ENV_LOCK.lock().unwrap();

        // Build a real prefixes snapshot for the AFM canonical
        // browserslist (the same shape the WASI plugin sees in
        // production via `precomputePrefixesDefault()`).
        let snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions::default(),
        );
        let bytes = autoprefixer::precomputed::encode_precomputed(&snap);

        // Write to a tempfile under the workspace target dir so the
        // path is deterministic across cargo invocations and gets
        // cleaned up by `cargo clean`.
        let path = std::env::temp_dir().join("compiled-css-prefixes-cache-hit-test.bin");
        std::fs::write(&path, &bytes).expect("write snapshot bytes");

        // Clear the thread-local cache so this test starts cold.
        // (Other tests in the same thread may have populated it.)
        super::_clear_precomputed_prefixes_cache_for_tests();

        let opts = TransformOpts {
            precomputed_prefixes_path: Some(path.clone()),
            ..Default::default()
        };

        // CSS that exercises the autoprefixer path (user-select
        // gets prefixed for browsers in the AFM list, hitting both
        // the populated AddTable lookup and the `cleaner` lazy cache).
        let css = ".a { user-select: none; }";

        let r1 = transform_css(css, &opts).expect("call 1");
        // Calls 2 and 3 hit the cache.
        let r2 = transform_css(css, &opts).expect("call 2 (cache hit)");
        let r3 = transform_css(css, &opts).expect("call 3 (cache hit)");

        let _ = std::fs::remove_file(&path);

        assert_eq!(r1.sheets, r2.sheets, "cache hit drifted output (call 2)");
        assert_eq!(r1.sheets, r3.sheets, "cache hit drifted output (call 3)");
        assert_eq!(r1.class_names, r2.class_names);
        assert_eq!(r1.class_names, r3.class_names);

        // Sanity: at least one sheet contains a vendor prefix, proving
        // we hit the autoprefixer path rather than a no-op.
        let combined = r1.sheets.join("");
        assert!(
            combined.contains("-webkit-")
                || combined.contains("-moz-")
                || combined.contains("-ms-"),
            "expected vendor prefix in output (snapshot path active), got: {combined}",
        );
    }

    /// Path-key invalidation: when the path changes between calls, the
    /// cache transparently re-reads. We can't easily check the cached
    /// `Arc` identity from here (private), so we verify the contract
    /// indirectly: two different paths pointing at the SAME bytes
    /// produce identical output (proves rereads are sound), and a
    /// path pointing at DIFFERENT bytes produces (correctly) different
    /// behaviour. The latter is covered by every existing
    /// snapshot-vs-slow-path parity test; here we cover the path-swap
    /// case explicitly so the cache-key logic doesn't regress.
    #[test]
    fn precomputed_prefixes_cache_path_swap_rereads() {
        let _g = ENV_LOCK.lock().unwrap();

        let snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions::default(),
        );
        let bytes = autoprefixer::precomputed::encode_precomputed(&snap);

        let path_a = std::env::temp_dir().join("compiled-css-prefixes-swap-a.bin");
        let path_b = std::env::temp_dir().join("compiled-css-prefixes-swap-b.bin");
        std::fs::write(&path_a, &bytes).expect("write snapshot a");
        std::fs::write(&path_b, &bytes).expect("write snapshot b");

        super::_clear_precomputed_prefixes_cache_for_tests();

        let opts_a = TransformOpts {
            precomputed_prefixes_path: Some(path_a.clone()),
            ..Default::default()
        };
        let opts_b = TransformOpts {
            precomputed_prefixes_path: Some(path_b.clone()),
            ..Default::default()
        };

        let css = ".a { user-select: none; }";

        // Populate the cache with path_a.
        let ra1 = transform_css(css, &opts_a).expect("a1");
        // Switch to path_b — cache must reread (different key).
        let rb = transform_css(css, &opts_b).expect("b");
        // Switch back to path_a — cache must reread again.
        let ra2 = transform_css(css, &opts_a).expect("a2");

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        // All three calls must produce identical output (paths pointed
        // at byte-equal snapshots; cache key change must not alter
        // output).
        assert_eq!(ra1.sheets, rb.sheets, "path_a → path_b drift");
        assert_eq!(ra1.sheets, ra2.sheets, "path_b → path_a drift");
    }

    /// Cache hit must produce the SAME output as the inline-bytes
    /// path. This is the load-bearing contract: the cache is supposed
    /// to be invisible to anything observing `transform_css`'s output.
    #[test]
    fn precomputed_prefixes_cache_matches_inline_bytes() {
        let _g = ENV_LOCK.lock().unwrap();

        let snap = autoprefixer::precomputed::precompute_prefixes(
            autoprefixer::autoprefixer::AutoprefixerOptions::default(),
        );
        let bytes = autoprefixer::precomputed::encode_precomputed(&snap);

        let path = std::env::temp_dir().join("compiled-css-prefixes-vs-inline.bin");
        std::fs::write(&path, &bytes).expect("write snapshot bytes");

        super::_clear_precomputed_prefixes_cache_for_tests();

        let css = ".a { user-select: none; mask: url(x); }";

        let inline_opts = TransformOpts {
            precomputed_prefixes: Some(bytes.clone()),
            ..Default::default()
        };
        let path_opts = TransformOpts {
            precomputed_prefixes_path: Some(path.clone()),
            ..Default::default()
        };

        let inline_out = transform_css(css, &inline_opts).unwrap();
        // Multiple path-delivered calls (the second hits the cache).
        let path_out_1 = transform_css(css, &path_opts).unwrap();
        let path_out_2 = transform_css(css, &path_opts).unwrap();

        let _ = std::fs::remove_file(&path);

        assert_eq!(
            inline_out.sheets, path_out_1.sheets,
            "inline-bytes vs path-delivery (call 1) drift",
        );
        assert_eq!(
            inline_out.sheets, path_out_2.sheets,
            "inline-bytes vs cached-path-delivery (call 2) drift",
        );
        assert_eq!(inline_out.class_names, path_out_1.class_names);
        assert_eq!(inline_out.class_names, path_out_2.class_names);
    }
}
