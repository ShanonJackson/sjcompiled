# Phase 0 findings — `crates/babel-plugin`

> **STATUS: BLOCKER for Phase 1.** A core PLAN.md architectural assumption
> (WASI cwd preopen with read+write access) is empirically false at the
> pinned `@swc/core@1.15.8` / `swc_core@54.0.0`. Plan revision is required
> before any Phase 1 work can land.

Generated 2026-05-02. Probes live in
`crates/babel-plugin-phase0-probes/` and `phase0-probes/probes.test.ts`.

## What we know empirically

The probe plugin compiled to `wasm32-wasip1` and loaded by
`@swc/core@1.15.8` via `transformSync` exhibits the following behaviour:

| Operation | Path | Result |
|---|---|---|
| `std::env::current_dir()` | — | `Ok("/")` |
| `fs::read("package.json")` | cwd-relative | `Err(Capabilities insufficient, os error 76)` |
| `fs::read("/package.json")` | absolute via virtual root | `Err(Capabilities insufficient, os error 76)` |
| `fs::read("crates/PARITY_VERSIONS.md")` | cwd-relative | `Err(Capabilities insufficient, os error 76)` |
| `fs::write("probe-cwd.bin")` | cwd-relative | `Err(Operation not permitted, os error 63)` |
| `fs::write("./probe-dot.bin")` | cwd-relative dot | `Err(Operation not permitted, os error 63)` |
| `fs::write("/probe-root.bin")` | absolute virtual root | `Err(Capabilities insufficient, os error 76)` |
| `fs::write("<absolute-host-path>")` | host Windows abs path | `Err(Capabilities insufficient, os error 76)` |

Two errno codes appear: `76` (`ENOTCAPABLE` — cap-std denies, no preopen
covers this path) and `63` (`EPERM` — operation not permitted). Both
indicate the sandbox does not grant the operation.

`env::current_dir()` succeeding with `/` is **cosmetic**, not evidence of
a usable preopen. WASI's default behaviour returns `/` even when no
preopens are configured; the actual capability check happens at
`open_at()` and fails uniformly.

## What this contradicts in PLAN.md

PLAN.md §3.2 currently asserts:

> The host (`@swc/core`) preopens **only** `std::env::current_dir()` of
> the calling Node/Bun process. Source of truth:
> `swc_plugin_backend_wasmtime/src/lib.rs:134-142` and
> `swc_plugin_backend_wasmer/src/lib.rs:194-209` in the upstream SWC
> repo.

This is empirically false at the pinned version. Either:
- the cited `swc_plugin_backend_wasmtime/wasmer` source is from a
  different SWC version and behaviour was different there, OR
- the preopen exists but with no granted capabilities (read or write),
  making it functionally absent.

Either way, the plugin we ship cannot perform filesystem I/O.

## What this invalidates downstream

| PLAN.md section | What's broken | Why |
|---|---|---|
| §3.3 "Plugin owns its own resolver" | Entire architecture | Resolver reads imported source files. No reads → no resolver. |
| §3.4 "Sidecar JSON manifests" | All sidecars | `included-files.json`, `style-rules.json` written by plugin. No writes → no sidecars. |
| §3.9 "Resolution cache strategy" | Entire `cache.bin` design | Plugin reads/writes `cache.bin` on `Program::enter`/`Program::exit`. No reads, no writes. |
| §3.9.5 "Scratch directory layout" | Entire scratch-dir-as-FS-channel | Both worker and call scratch are FS dirs the plugin must touch. |
| §3.9.6 "Plugin config contract" | `workerScratchDir` / `callScratch` fields | Pointless if plugin can't reach them. |
| §3.9.7 "Plugin lifecycle per `transform()` call" | Steps 1, 9, 10, 11 | All FS-bound. |
| §3.9.8 "Two-layer cache structure" | Layer 2 persistence | Layer 2's whole point is disk persistence. Layer 1 (in-memory only) survives. |
| §3.9.10–14 (§3.9.14 #1, #2, #6, #7) | Filesystem probes | Already failing. |
| §5 "Phase 5 — In-plugin resolver" | Entire phase | `oxc_resolver` is irrelevant if the plugin can't `fs::read` the resolved path. |
| §7 "Sidecar manifest schema" | All three schemas | `included-files.json`, `style-rules.json`, `cache.bin`. |
| §3.5 "transformCss is a direct synchronous Rust call" | Survives | This was already moved to a direct in-plugin Rust call after the user finished the CSS port; it does NOT depend on FS. ✅ |

## What still works

The plugin's *primary* job — AST mutation in a single `transform()`
call — is unaffected. Specifically:

- `swc.transformSync` loads the wasm plugin and dispatches a visitor.
- The plugin sees the input AST, the plugin config (a string), and
  emits an output AST.
- All `compat/*.rs` ports of Babel APIs are unaffected.
- `transform_css` linked as a Rust crate dep is unaffected (Phase 4).
- The `compat/generator.rs` keyframe-name printer is unaffected.
- `MutationRecorder` + `StateDiff` (§3.9.8) are unaffected — they are
  purely in-memory per `transform()` call.

The break is exclusively in the **plugin → host communication channel**
and the **plugin-side resolver**.

## Architectural alternatives (for plan revision)

The plugin has exactly two channels back to the host:
1. The **output AST** (returned from `transform()`).
2. SWC's **plugin metadata channel** —
   `TransformPluginProgramMetadata::set_transform_metadata_context` and
   the `swc_common::comments::Comments` proxy, depending on swc_core
   version.

Channel 1 (AST) always works. Channel 2 needs verification at swc_core
54.0.0. **Phase 0 task to be added**: probe the metadata channel — does
the plugin have a way to emit out-of-band data the host can read after
`transformSync` returns?

### Option A — Embed everything in output AST (no FS)

Every piece of out-of-band data the plugin needs to communicate to the
host travels as a comment or sentinel string in the emitted JS. Host
parses these out before / after prettier.

- `includedFiles`: emit a leading comment block
  `/* @sjcompiled-included <abs-path>, <abs-path>, ... */`. Host strips
  before prettier.
- `styleRules` (strip-runtime SSR mode): emit a sentinel call
  `__sjc_style_rules__(["rule1", "rule2"])` at module top. Host
  parses + strips.
- Cache state: there is no Layer 2 cache. We rely on `cacheLevel: "ast"`
  (Layer 1 in-memory per-transform only) plus repeated work across
  files. Perf target §3.9.16's `cacheLevel: "value"` row drops out of
  scope.

The "comment sentinel" approach was rejected in §3.4 due to "prettier
reflow risk" and "extra strip layer." With FS off the table, this
trade-off shifts: the strip layer is mandatory.

### Option B — Pre-walk import graph in host, pass evaluated values via plugin config

The host (Parcel transformer wrapper) walks the import graph before
calling the plugin, evaluates everything statically using the existing
JS infrastructure, and passes the evaluated `state.compiledImports`,
`cssMap`, `sheets`, etc. as plugin config. The plugin runs *only*
visitor-side AST transforms with no resolution.

- Splits the work: plugin = AST transformer; host = static evaluator.
- Loses the "single Rust port" property — static evaluation stays in
  JS. Performance regression vs the planned all-Rust path.
- Eliminates the in-plugin resolver entirely (§5 collapses).
- Plugin config becomes large (potentially MB-scale for big files).
  PLAN.md already calls out a 1MB cap; we'd lift that to whatever SWC
  accepts (probably 100MB+).

### Option C — Reshape the plugin as a Rust library called via NAPI

Drop the SWC plugin model entirely. Build the plugin as a NAPI-bound
Rust library (mirror of `compiled-css-napi`) that takes (source code,
config) and returns (transformed code, sideband). Use `swc_core` as a
library dep, not as a plugin host.

- No WASI sandbox. Full FS access. PLAN.md §3.9 works as designed.
- Loses the `@swc/core` plugin loader compatibility (plugin
  configurability via `.swcrc`). Consumers wire the bridge directly.
- Host wrapper is a NAPI call, not a `swc.transformSync` call.
- This is the architecture `compiled-css-napi` already uses for `sort`.

### Option D — Hybrid: AST sentinels for sideband, Option C for resolver

Keep the SWC plugin shape. Use Option A's AST sentinels for
side-channel data. For the resolver — the only thing that genuinely
needs FS read access — punt to Option B's "host pre-evaluates" model.

This is the most conservative shift from PLAN.md's current shape:
- §3.5 (Rust transformCss) survives unchanged.
- §3.4 swaps sidecars for AST sentinels.
- §3.3 / §5 (in-plugin resolver) gets pushed to host-side.
- §3.9 cache becomes Layer-1-only (no on-disk persistence).

## Recommended next step

**Stop Phase 0 here.** Surface this finding to the project owner. Pick
one of A / B / C / D (or a hybrid the owner prefers). Amend
PLAN.md sections 3.2-3.9, 5, 7. Re-run Phase 0 sandbox probes against
the new architecture before any Phase 1 work begins.

## Probes status (as currently run on win32-x64-msvc, @swc/core@1.15.8)

| Probe (PLAN.md §3.9.14) | Status |
|---|---|
| 1. WASI sync I/O round-trip | **FAIL** — no FS access at all |
| 2. WASI mtime | **N/A until #1 passes** |
| 3. `transformSync` ABI exists | PASS |
| 4. Instance teardown (counter resets) | **N/A until #1 passes** (writes result file) |
| 5. Race / serialisation | PASS (transformSync serialises trivially) |
| 6. Scratch-dir reachability | **FAIL** — same root cause as #1 |
| 7. Postcard round-trip via WASI I/O | **N/A until #1 passes** |
| 8. Byte-cap eviction (pure Rust unit test) | not yet run |
| 9. Resolver difference matrix (Phase 5 gate) | not run yet (Phase 5) |

Probe (3) and (5) are the only passing FS-independent probes. They
confirm `transformSync` exists and serialises correctly.

## To verify the finding on Linux / macOS

The above was run on Windows. Before drawing a final conclusion, the
same probes should run on Linux and macOS — there's a possibility the
WASI preopen behaviour is platform-specific in `@swc/core@1.15.8`'s
shipped binaries. If reads / writes succeed there, the architecture
is salvageable on those platforms (with Windows as the exception that
forces a hybrid). If they fail there too, the finding is universal
and plan revision is unconditional.

CI step to add: run `bun test phase0-probes/probes.test.ts` on
ubuntu-latest, macos-latest, windows-latest. Capture results in this
file under a "Cross-platform results" section.
