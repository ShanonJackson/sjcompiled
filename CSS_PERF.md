# transformCss perf — current state + realistic path to 4-5x JS

Captured 2026-05-04 after V2 precomputed-prefixes snapshot landed as the
standard path (`fast-match` feature gate dropped, V1 snapshot removed).
Bench inputs: `scripts/perf-test.ts` (NAPI) and
`crates/css/examples/perf_precomputed.rs` (in-process). Sample: 504-byte
SAMPLE_CSS, AFM browserslist resolution.

## Current state

| Path | ops/s | µs/call | vs JS |
|---|---|---|---|
| JS (`@compiled/css`) | 1290 | 775 | 1.00x |
| **NAPI Rust (V2 standard)** | **1455** | **687** | **1.13x** |
| **In-process Rust (V2, inline bytes)** | **1939** | **516** | **1.50x** |
| In-process Rust (V2, path delivery) | 1748 | 572 | 1.36x |
| In-process Rust (slow path, no snapshot) | 93 | 10700 | 0.07x |

Byte-equality verified on every path:
- `bun ./scripts/perf-test.ts` reports `Outputs match byte-for-byte.`
- `parity-runner --stage autoprefixer`: 65/65 byte-clean
- `parity-runner --stage transform-css`: 30/30 byte-clean
- `crates/css/examples/repro_user_select.rs`: JS = NAPI = source build under both AFM and `chrome 100` browserslist

## Cost decomposition (in-process, ~516 µs/call)

| Component | Cost | % of total |
|---|---|---|
| Snapshot decode (postcard, 49 KB payload) | ~226 µs | ~44% |
| Parse (postcss-core) | ~50 µs (estimate) | ~10% |
| 11 other plugins + walks | ~150 µs (estimate) | ~29% |
| Autoprefixer add/remove walks | ~50 µs (estimate) | ~10% |
| Stringify + extract | ~40 µs (estimate) | ~7% |

The **snapshot decode dominates at 44%**. Postcard does per-field varint
+ length-prefix decoding; at 49 KB of nested IndexMap/Vec/struct data the
overhead is structural to that serializer choice.

NAPI adds **~210 µs/call** structural marshalling overhead (Buffer copy,
JsObject opt walk, JS↔native call boundary, error wrap). This is why
NAPI lands at 1.13x while in-process lands at 1.50x.

---

## Path A — mechanical, 2-3 weeks → realistically lands 3.4-4.6x JS

### A1. rkyv migration of `PrecomputedPrefixes` (~1 week, biggest single win)

Replace postcard with rkyv. rkyv decodes near-zero by treating bytes as
an in-place validated archive — read `&Archived<PrecomputedPrefixes>`
directly off the byte slice. Estimated decode: <10 µs (vs 226 µs today).
**Saves ~220 µs/call.**

- *Cost:* every type in the populated tables needs `#[derive(Archive,
  Serialize, Deserialize)]` instead of serde. ~30 derive sites. Archived
  types are read-only views, which fits the WASI consumer pattern (the
  snapshot is immutable per build). Adds ~150 KB to binary.
- *Drift risk:* low. Same data, different decoder. The existing property
  test (`precomputed::tests::populated_*_table_equals_live`) catches any
  divergence.
- *Confidence:* high. Pure-data types, no `dyn`, no `Rc`. rkyv is
  well-established for this exact use case.

**Effect alone:** 516 → 296 µs/call → **3.4x JS**. Just shy of 4x.

### A2. Plugin-skip detection (3-5 days)

Most plugins walk the entire AST regardless of whether the input has
anything they care about. `expandShorthands` is a no-op without a
shorthand decl. `atomicifyRules` is a no-op without rules.
`autoprefixer.add` is a no-op without prefix-table-matching decls. Each
plugin gets a cheap "do I have work?" prelude — usually a single-pass
tag check on the AST.

- *Saving estimate:* ~80 µs/call on the perf-test input (skips ~3-4
  plugins). On real corpus, varies — sometimes more, sometimes less.
- *Drift risk:* medium. Each prelude gate has to be exact — a
  false-negative skips work that should have run. Tractable but needs
  careful spec for each plugin.

**Combined A1+A2:** 516 → ~216 µs/call → **4.6x JS**. ✓ in the 4-5x band.

### Confidence of hitting ≥4x via Path A: ~70%

Worst case (rkyv lands clean, plugin-skip half-delivers): ~3.9x.
Best case: ~4.8x.

---

## Path B — architectural, 1-3 months → 6-10x band but high risk

### B1. Build-time static embedding for AFM

Run `precompute_prefixes_default()` at `build.rs` time, emit the
populated tables as `static` Rust source (mirroring the existing
`data/prefixes.rs` codegen pattern). Zero decode cost at runtime — the
data is already laid out in `.rodata`. **Saves the full ~226 µs.**

- *Caveat:* AFM-only. Other browserslist queries fall back to runtime
  precompute. Acceptable since AFM is the production path.
- *Drift risk:* low — same precompute function, just runs at compile
  time.
- *Effort:* harder than rkyv because the populated types need
  const-friendly representations OR a `LazyLock<PrecomputedPrefixes>`
  constructed by linking the bytes in. Latter is simpler but adds ~10 µs
  first-call cost.

### B2. Fused single-pass walker

Combine the 12 plugins into one AST traversal. JS postcss does this
poorly (callback-driven; can't fuse across plugins without breaking
semantics). A Rust port can — declare each plugin's per-node-kind
handlers as compile-time fragments and let the optimizer fuse them.

- *Saving:* ~30-50% on walk-bound work. Combined with A1+A2, possibly
  **6-10x JS**.
- *Drift risk:* HIGH. Plugin lifecycle ordering interacts. Once-vs-
  OnceExit ordering matters. Visitor mutation during walk vs after walk
  matters. Every fused fragment needs a byte-equality gate.
- *Effort:* 4-8 weeks. Probably the biggest engineering project in the
  migration.

---

## Path C — orthogonal, build-tool layer, ~1 week → effective 5-10x on real builds

### C1. LRU cache keyed on input CSS bytes

`transformCss(css, opts)` is a pure function. In a 90 GB monorepo build,
similar template strings appear thousands of times. Cache the output by
`(hash(css), hash(opts))`.

- *Saving:* depends on hit rate. At 80% hit rate (typical for repeated
  component templates), effective speedup is **5x average over a build**.
  Per-call cold cost unchanged.
- *Drift risk:* near zero — cache invalidation aside, the function is
  deterministic.
- *Effort:* small, 3-5 days. Lives in the babel-plugin / SWC-plugin
  layer, not in `transform_css` itself.
- *Catch:* doesn't help with *single-call* perf. If running
  `transformCss` once per build, no help. If running 100k times, huge.

This is independent of A and B. Stack them together and a real-world
monorepo build sees **10-20x effective speedup** at ≥80% hit rate.

---

## What's NOT realistic

### NAPI 4-5x

Marshalling overhead is ~210 µs/call structural. Even if Rust dropped to
0 µs, the JS↔native call boundary is bounded ~80-100 µs minimum (Buffer
marshalling, JsObject hashmap walk, error wrap). Best NAPI ceiling:
~1.5-1.8x JS. NAPI is not the production path; it's a measurement
vehicle. Move to WASI for the production story.

### swc_css / lightning-css rewrite

Faster CSS engines, but they emit different bytes (different stringifier
whitespace conventions, quote style, operator spacing). Byte-equality
contract dies. Months of re-validation against JS oracle for every
fixture in the 90 GB monorepo. Not realistic.

### Fast-path heuristics in the parser

High drift risk for low gain. The "trivial input" detector has to be
exact across the input space. Don't.

### Plugin parallelism (rayon)

Useless on small inputs (the perf-test sample has 1 rule). Could help
large inputs (many independent rules) but ordering invariants make it
tricky. Not the bottleneck on perf-test.

---

## Bottom line — sequenced plan

| Effort | In-process JS multiplier | Confidence |
|---|---|---|
| Today (V2 mandatory) | 1.50x | done |
| + rkyv (1 week) | 3.4x | high |
| + plugin-skip (1 more week) | 4.6x | medium |
| + build-time static AFM (2 weeks) | 5-6x | high |
| + fused walker (4-8 weeks) | 8-10x | medium |
| + LRU cache at babel-plugin (orthogonal) | effective 10-20x on real builds | high |

The **pragmatic stop-line is rkyv + plugin-skip = ~4.6x JS**. Two-three
weeks of mechanical work, all gated by the existing byte-equality
property tests + parity-runner corpus. No architectural rewrite, no
new abstractions, no drift trap.

Beyond that, you're trading engineering months for diminishing returns
on per-call cost — but the LRU cache at the babel-plugin layer is a
high-leverage standalone win that turns any per-call number into 5-10x
effective on real builds without touching `transform_css` at all.

### Suggested sequencing

1. **Ship current 1.50x** as standard.
2. **Add LRU cache at babel-plugin layer** (week, biggest real-world impact, orthogonal to per-call work).
3. **rkyv migration** (week, biggest per-call impact, ~3.4x).
4. **Plugin-skip detection** (week, finishes the 4-5x story, ~4.6x).
5. **Stop or push to fused walker** depending on whether 4.6x is enough.

After step 4, only fused-walker remains as a high-effort high-reward
swing. Steps 1-4 are roughly 3 weeks of focused work; step 5 is 4-8
weeks.
