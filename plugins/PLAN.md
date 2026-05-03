# Plan: `@compiled/babel-plugin` + `@compiled/babel-plugin-strip-runtime` → Rust SWC plugins

> **Audience.** An engineer or agent picking this up cold. Read this in
> order. This file is the source of truth for hard constraints, the verification
> oracle, and the production call site we must satisfy. Read
> `plugins/PARCEL_USAGE_EXAMPLE.md` alongside §8 — that's the consumer shape
> we slot into. This file tells you **what to build**, **in what order**,
> **with what verification**, and **why** every decision below is the way it is.
>
> **The non-negotiable.** Every input file must produce output that is
> byte-identical between Babel and SWC **after** running both outputs through
> `prettier` (workspace-pinned version, `parser: 'babel-ts'`). One byte of
> drift in any string literal — including embedded class names like
> `_a1b2c3` — fails the gate. There is no "close enough." If you cannot make
> bytes match, stop and escalate.

---

## 0. The 30-second summary

We are porting two Babel plugins to Rust SWC plugins compiled to
`wasm32-wasip1`, loadable by `@swc/core@1.15.8`. Output must be a
drop-in replacement for the existing Babel plugins — bugs and all — verified
by post-prettier byte equality across a fixture corpus.

The two plugins:

| Babel | Rust target | Size | Role |
|---|---|---|---|
| `packages/babel-plugin/` | `crates/babel-plugin/` | ~50 source files, ~6kLOC | Transforms Compiled API call sites (`css({...})`, `styled.div\`...\``, `cssMap`, `<ClassNames>`, `xcss`) into runtime JSX + CSS strings. Statically evaluates imports across files. Calls `transformCss` from `@compiled/css`. |
| `packages/babel-plugin-strip-runtime/` | `crates/babel-plugin-strip-runtime/` | 6 source files, ~600LOC | Strips `<CC>` / `<CS>` runtime wrappers from already-baked code, extracts CSS rules into either `require()` calls, an external `.compiled.css` file, or sidecar metadata for SSR. |

Both run, in series, inside Parcel's transformer (see
`plugins/PARCEL_USAGE_EXAMPLE.md`). Our SWC port must slot into that exact
shape: same options, same outputs (`code`, `includedFiles` for HMR
invalidation, `styleRules` for the optimizer), same edge-case behavior.

---

## 1. Hard constraints

These are constraints, not preferences. They override every other decision in
this document.

1. **`opts.resolver` (custom JS resolver function) is out of scope.** The
   plugin runs inside a WASI sandbox and cannot synchronously call back into
   JS. Production callers today pass `opts.resolver` as a JS object with a
   `.resolveSync(filename, request)` method (see `createDefaultResolver` in
   `packages/parcel-transformer/src/utils.ts`, which wraps
   `enhanced-resolve@5.x`). Constraint #2 covers the replacement; the plugin
   no longer accepts `opts.resolver` and emits a config-time error if a
   function or wrapper is passed.
2. **Module resolution is in-plugin via `oxc_resolver`.**
   `oxc_resolver` is the Rust analogue of webpack's `enhanced-resolve` and
   covers the same surface (extensions, `package.json#main` / `#exports` /
   `#imports` / conditional exports, `tsconfig` paths, symlink realpath, the
   browser field). Two distinct JS code paths must be replaced in Rust:
   - **JS-injected `state.resolver.resolveSync(...)`** at
     `packages/babel-plugin/src/utils/resolve-binding.ts:191-193` — what
     production callers (Parcel, webpack) wire up. This is the
     `enhanced-resolve` path that `oxc_resolver` directly replaces.
   - **Fallback `resolve.sync(...)` from the npm `resolve` package** at
     `resolve-binding.ts:185-189` — used when no resolver is injected. This
     path also collapses into `oxc_resolver`; configure
     `oxc_resolver` with `opts.extensions` (default `['.js','.jsx','.ts',
     '.tsx']`) so its behaviour matches the npm-`resolve` fallback as well.
   Phase 0 produces a difference matrix between
   `enhanced-resolve@5.x` (as used by `createDefaultResolver`),
   the npm `resolve.sync()` fallback, and `oxc_resolver`. Any divergence
   on a real path becomes a class-name divergence — this matrix is a hard
   gate before Phase 5 starts.
3. **`packages/css/src/transform.ts` ships in Rust *before* this plugin
   starts Phase 4.** A parallel agent is porting `transform.ts` to Rust;
   their crate (`crates/compiled-utils` plus the CSS crates already
   scaffolded under `crates/`) exposes a synchronous Rust function we link
   in directly. **No NAPI bridge. No two-pass scan/apply protocol.**
   Earlier drafts of this plan described a two-pass workaround because
   the WASI sandbox forbids host callbacks; landing the Rust CSS port
   first eliminates that whole class of complexity (no scan-pass marker
   tokens, no `css-requests.json` sidecar, no second `transformSync`
   call per file). Phase 4 cannot start until the Rust CSS port is
   stable and its hash function is byte-identical to the JS version
   (see Phase 3).
4. **File/folder structure is identical.** Every file in
   `packages/babel-plugin/src/<path>.ts` maps to
   `crates/babel-plugin/src/<path>.rs` (or `<path>/mod.rs`). Same for
   strip-runtime. Filenames switch from kebab-case to snake_case (Rust
   requirement); folder structure matches exactly. **If you feel the urge to
   deviate, stop and ask.**
5. **Missing Babel equivalents go in `crates/<plugin>/src/compat/*.rs`.**
   Where SWC has no analogue (e.g. `@babel/generator` for one node subtree),
   build a complete impl in `compat/` matching the Babel one byte-for-byte.
   No half-bakes — there are 10K+ call sites in production code; missing
   features will be discovered at integration. For each `compat/*.rs`
   shim, an explicit *coverage manifest* lists every input shape that can
   reach it, derived from the consuming monorepo (see Phase 4 entry gate
   for the `compat/generator.rs` instance). Half-bakes are a port defect.
6. **Replicate Babel bugs.** This is a drop-in replacement. Existing bugs,
   quirks, even "wrong" CSS output stay. Fixing bugs changes hashes, which
   renames every class in production. Bugs are features.
7. **Build target: `wasm32-wasip1`, ABI-compatible with `@swc/core@1.15.8`.**
   The matching `swc_core` Rust crate version is locked in
   `crates/PARITY_VERSIONS.md`. Bumping `@swc/core` requires a coordinated
   `swc_core` bump and a full corpus rerun.
8. **Plugin in/out is 1:1; the Parcel transformer can be modified
   freely.** The plugin's input (source string + `PluginOptions`) and
   output (transformed code + sidecar JSON) are fixed. The host wrapper
   in `packages/parcel-transformer/` may be modified arbitrarily to
   adapt to SWC's calling conventions (e.g. it can drop its upstream
   Babel parse and feed source straight to `swc.transformSync`); only
   the plugin contract is the parity surface.

If you hit anything you cannot replicate 1:1, **stop and escalate.** Do not
guess. Do not improvise. The cost of a wrong substitution is months of
debugging at a 10M-LOC monorepo scale.

---

## 2. The verification oracle (read this before writing any code)

### 2.1 What the contract is

```
prettier(babelOutput, { parser: 'babel-ts' })
  ===
prettier(swcOutput, { parser: 'babel-ts' })
```

Bytewise equality. Run on the workspace-pinned prettier version (already
imported in `packages/babel-plugin/src/test-utils.ts` as `import { format }
from 'prettier'`). Pin the exact prettier version in
`crates/PARITY_VERSIONS.md` alongside the SWC pin.

### 2.2 What prettier normalizes (drift you get for free)

| Concern | Normalized? |
|---|---|
| Quote style (`'` vs `"`) | Yes |
| Semicolons | Yes |
| Indent / line breaks / line length | Yes |
| Trailing commas (per config) | Yes |
| Paren insertion around `await` / `yield` / arrow params | Yes |
| Blank lines collapsed | Yes |

### 2.3 What prettier preserves (drift you must eliminate)

| Concern | Preserved? | Implication |
|---|---|---|
| AST structural shape | Yes | Visitor mutations must produce the same tree |
| Statement ordering | Yes | Insertion order of imports/declarations must match |
| String literal **contents** | Yes — byte-for-byte | **CSS class names live here. Hash output must match.** |
| Numeric literal **representation** (`0x10` vs `16`) | Yes | Don't normalize numeric forms during transform |
| Object property insertion order | Yes | `IndexMap` everywhere |
| Comment text **and attachment node** | Yes (mostly) | Place comments on the same nodes Babel does |
| Template literal contents | Yes | Quasi values are part of the AST |
| `null` vs `undefined` literal vs omitted | Yes | Choose the same one Babel does |

### 2.4 Why the CSS hash contract is hardcoded into this oracle

Class names like `_1wyb1fwx` end up inside string literals in the emitted JS.
Prettier preserves string contents byte-for-byte. So whatever produces those
hashes — today's JS `transformCss`, eventually Rust `transformCss` via
NAPI — must produce identical bytes for identical inputs. **This is why
constraint 3 above exists.** While the Rust CSS port is being built, we call
the JS impl through NAPI to guarantee hash parity. Swapping NAPI-to-JS for
NAPI-to-Rust later is zero-behaviour-change because the parallel agent
guarantees byte equality on its side.

---

## 3. Architectural decisions (locked, with reasoning)

These were arrived at through a series of spikes documented earlier in the
project history. Restating them here so this document stands alone.

### 3.1 SWC plugin ABI is pinned

Target: `@swc/core@1.15.8`. Plugins compile against the matching `swc_core`
crate version. Cross-reference the SWC plugin compatibility matrix
([github.com/swc-project/swc](https://github.com/swc-project/swc)) to find
the exact `swc_core` version. Pin it in `crates/PARITY_VERSIONS.md`. CI must
rebuild on every `@swc/core` bump.

### 3.2 The WASI sandbox is the FS boundary — cwd is mounted at `/cwd`

A SWC plugin is a sandboxed Wasm module. The host (`@swc/core`) preopens
**only** `std::env::current_dir()` of the calling Node/Bun process,
**virtually mounted at `/cwd`**. Source of truth:
`swc_plugin_backend_wasmer/src/lib.rs` (full read+write access) and
empirically verified by Phase 0 probes at `@swc/core@1.15.8` /
`swc_core@54.0.0` — see `crates/babel-plugin/PHASE0_FINDINGS.md`.

**Plugin-side path discipline (load-bearing):**

```rust
// ✅ works — uses the /cwd mount
fs::read("/cwd/package.json")?;
fs::write("/cwd/node_modules/.cache/compiled-swc/cache.bin", bytes)?;

// ❌ fails — physical absolute path is outside the preopen
fs::write("C:/Users/.../package.json", bytes)?;

// ❌ fails — bare-relative resolves against `/`, not the preopen
fs::write("package.json", bytes)?;

// ❌ fails — env::current_dir() returns Ok("/") cosmetically; joining
// against it lands at `/`, not `/cwd`
fs::write(env::current_dir()?.join("foo"), bytes)?;
```

The same rule applies to reads. See
`plugins/READ_WRITE.md` for the canonical setup; SWC issue
swc-project/swc#4997 documents the same pattern from a maintainer.

**Host-side path translation contract:** the host wrapper (Parcel
transformer in `packages/parcel-transformer/`) computes scratch paths
in host-absolute form (e.g.
`<projectRoot>/node_modules/.cache/compiled-swc/worker-12345/`) for
its own `mkdirSync` / `rmSync` use, then translates to
`/cwd/<rel-from-project-root>` form before threading into plugin
config. The plugin only ever sees `/cwd`-prefixed paths and never
attempts `env::current_dir()`-based path construction.

Implications:

- `Options.cwd` in `@swc/core` does **not** affect the sandbox. It controls
  `.swcrc` lookup and source-map paths only. Setting it to the monorepo root
  does not enlarge what the plugin can read.
- The mount equivalence is `host(<projectRoot>/foo) ≡ plugin(/cwd/foo)`.
  Both refer to the same physical file; both reads and writes succeed.
- `..` and absolute paths above `/cwd` are denied at the cap-std layer,
  not by string validation. No string-trick escapes.
- Symlinks are followed only if the **target** is also inside the preopen.
  pnpm content-addressed stores, Yarn PnP caches, root-hoisted `node_modules`
  pointing to sibling workspaces — all denied unless their physical
  target sits under `<projectRoot>`.
- There is no JS or `.swcrc` knob to grant additional preopens at this SWC
  version. The upstream `swc_plugin_backend_wasmer` source has a TODO to
  make additional preopens configurable; until that lands, `/cwd` is the
  whole world.
- Worker spawn cwd MUST be the monorepo root, not the package root, so the
  preopen covers every source file the plugin's resolver may walk
  (§3.9.13.7).

### 3.3 The plugin owns its own resolver — no host pre-walker

Empirical audit of the consuming monorepo (run via
`scripts/audit-included-files.ts`, see Phase 0 in §5) found ~100 outlier
files where the Babel plugin's static evaluator opens a file whose realpath
escapes the package cwd. The user has confirmed those will be refactored
manually before the SWC plugin ships.

After the refactor, every file the plugin opens via static evaluation
resolves (post-realpath) to a path inside the invocation cwd. Therefore the
WASI cap-std preopen at the package cwd is sufficient for in-plugin import
resolution.

This collapses the architecture:

- **No host pre-walker** (no `swc-plugin-host` package).
- **No two-pass scan/apply protocol** of any kind.
- The plugin walks the import graph itself, reading files via the `/cwd`
  preopen, parsing them with `swc_ecma_parser`, caching results in a
  `Mutex<LruCache>`.
- `transform_css` is a direct synchronous Rust call (§3.5), not a
  host bridge. The plugin runs in a single pass per file.

### 3.4 Out-of-band data uses sidecar JSON manifests

Anything that cannot live in the AST — `includedFiles` (HMR invalidation
input for Parcel), SSR-mode `styleRules` (read by
`@compiled/parcel-optimizer`), diagnostics — travels through versioned JSON
files written into a per-compile scratch directory.

The host (Parcel transformer wrapper) opens a `mkdtemp` scratch dir, passes
the path in plugin config, drains files after `transform()` returns, deletes
the dir. Schema lives in `plugins/SIDECAR_SCHEMA.md` (locked in Phase 1).
All sidecars are `{ version: 1, ... }` so a plugin/host version mismatch
fails loudly.

Why sidecar files vs comment sentinels in the output JS:
- `includedFiles` can be hundreds of absolute paths; embedding them in a
  comment risks prettier reflow and bloats output.
- Comments survive into the parity harness and would need to be stripped on
  both sides — an extra layer of fragility.
- File I/O in the WASI preopen is reliable and well-bounded.

### 3.5 `transformCss` is a direct synchronous Rust call

Per constraint 3 in §1, the Rust port of `packages/css/src/transform.ts`
ships **before** Phase 4 starts. That eliminates the WASI host-callback
problem entirely: the plugin links the CSS port as a normal Rust crate
(`crates/compiled-utils` plus the CSS crates already scaffolded in
`crates/`) and calls `transform_css(css, &opts)` synchronously inside
the visitor. No NAPI, no two-pass scan/apply, no marker tokens, no
`css-requests.json` sidecar, no second `transformSync` per file.

Phase 4 cannot start until two preconditions hold:

1. The Rust `transform_css` is callable as a sync Rust function and its
   public signature matches what `utils/css-builders.ts` expects (the
   same call shape as the JS `transformCss({ css, opts })`).
2. The hash function used for keyframe names / CSS variable names /
   cache keys is byte-identical between JS and Rust (Phase 3 gates this
   independently and is consumed from the same shared crate).

If either precondition is not yet met when Phase 3 exits, Phase 4
blocks until it is. Do not stand up a temporary scaffold — the
two-pass design was exactly that scaffold and it has been removed
deliberately.

**Why this matters for perf.** A two-pass design would have doubled
SWC parse + visit cost for every transformed file until the CSS port
landed. Single-pass + direct Rust call keeps the per-file cost at one
parse + one visit, which is the floor below which the plugin cannot
go and is what the §3.9.16 perf targets assume.

### 3.6 No shared `swc-plugin-host` JS package

Each `@swc/core` integration point inlines a ~30-line wrapper that creates a
scratch dir, invokes SWC once via `transformSync`, drains sidecars, returns
results. Today there's exactly one integration: the Parcel transformer in
`packages/parcel-transformer/`. Future integrations (webpack, jest, bun-test)
duplicate the snippet, not abstract it. Re-evaluate if the wrapper grows past
~50 lines per integration.

### 3.7 1:1 file layout with the upstream Babel plugins

This is constraint 4 from §1, restated as architecture. Every Rust file maps
to a TypeScript file in upstream. Filenames switch kebab-case to snake_case;
folder structure is identical. The 1:1 mapping enables side-by-side review.

### 3.8 `compat/` directory for missing Babel equivalents

Constraint 5 from §1, restated as architecture. When SWC has no analogue for
a Babel API the plugin uses, the port lives in
`crates/<plugin>/src/compat/<name>.rs`. Known cases (non-exhaustive):

- `@babel/generator` → needed for the keyframe-name hash input
  (`hash(generate(expression).code)`). SWC's printer differs from Babel's;
  for that one hash-input use case, port a minimal subtree printer that
  matches `@babel/generator` output byte-for-byte.
- `@babel/template` → SWC has `swc_ecma_quote`, but its output for
  template-string parsing differs in subtle ways. Where Babel's plugin uses
  `template.ast(...)`, port to a `compat/template.rs` helper that produces
  the Babel-equivalent AST.
- `@babel/helper-plugin-utils.declare` → an SWC plugin entry is just
  `#[plugin_transform] fn`, but the `declare()` semantics around
  `api.assertVersion(7)` and `inherits: jsxSyntax` need a manual translation
  in `compat/declare.rs`.
- Babel `path.scope.getBinding(name)` → SWC's scope analysis is different.
  Port the binding-lookup helpers in `compat/scope.rs`.

Add to this list as you find them. **No half-bakes.**

### 3.9 Resolution cache strategy (host responsibilities, plugin contract, perf design)

> **Read this section in full before touching `utils/cache.rs` or the
> Parcel transformer wrapper.** It describes how a per-worker resolution
> cache is plumbed end-to-end. Every word below has a reason behind it;
> shortcuts here turn into correctness bugs at HMR time or perf
> regressions at monorepo scale.

#### 3.9.0 The two-sentence summary

The Wasm plugin instance is torn down per `transform()` call (verified
empirically against `swc_plugin_runner@23.0.0` — see Phase 0 ABI gate),
so plugin-static memory cannot persist a cache across files. Instead,
the **host (the Parcel transformer wrapper) owns a worker-scoped scratch
directory under `<projectRoot>/node_modules/.cache/compiled-swc/`**
(reachable from inside the WASI cwd preopen — see §3.9.5), the plugin
reads/writes a **single compact binary cache file** (postcard-encoded,
hard-capped at 5 MB / 500 entries) inside it via WASI sync I/O, and
mtime-based invalidation keeps the cache safe under HMR.

The cache is **Layer 2 only on disk** (evaluated values + state diffs;
small entries, big payoff). Layer 1 (parsed ASTs) is in-memory only,
scoped to a single `transform()` call. Re-parsing across files is
cheap (SWC parses at ~50 MB/s; OS page cache holds source bytes);
re-evaluating an imported `theme.ts` 1000× across consumers is what
actually hurt Babel, and that's exactly what Layer 2 eliminates.

This redesign is deliberate: an earlier draft persisted Layer 1 (full
serde-JSON of `swc_ecma_ast::Module`) to disk per-transform. On a
60-90 GB monorepo with deep transitive deps, the per-file
read/parse/serialize/write of a multi-megabyte JSON file would have
dwarfed the parse cost it tried to save. Binary format + Layer-2-only
+ size cap keeps the cache *bounded* and *cheap to touch*.

#### 3.9.1 What Babel does today (and why the SWC port can do better)

Babel's `globalCache` is a misnomer. `babel-plugin.ts:82-87` reassigns
`globalCache = new Cache()` on every `pre()` hook (i.e., every
`transform()` call), and the variable is never read elsewhere in the
codebase (`grep -n globalCache packages/babel-plugin/src/` returns
exactly three references: declaration + two writes). The cache is
**effectively per-file regardless of `opts.cache: true | false`**.

The only behavioural difference between `opts.cache: true` and
`opts.cache: false` is that `Cache._options.cache = false` makes
`load()` short-circuit at `cache.ts:115` and never memoize. With
`true`, lookups within a single file's transform are memoized; across
files, nothing persists.

This means **at monorepo scale Babel re-parses every imported file
once per consuming file**. A `theme.ts` exporting 50 tokens, imported
by 1000 components, is parsed 1000 times by Babel. We can do
dramatically better without changing output bytes — that is the entire
point of this section.

#### 3.9.2 Correctness invariant (non-negotiable)

> **The cache must never change output bytes.**
>
> Every cache hit must return a value that is byte-equivalent (after
> prettier) to a fresh evaluation of the same expression in the same
> source file. If invalidation is wrong, output bytes diverge — that's
> a parity-harness failure, not a perf regression. Bytes are the
> contract; cache is the speedup.

Concretely this means:
- Cache keys must capture every input that determines the value.
- Source mtime invalidation must be eager (drop entries whose source
  changed before any consumer reads them).
- Side-effects on plugin `state` (Layer 2 only) must be replayed on
  hit so HMR `includedFiles`, Compiled-import tracking, and
  `sheets`/`cssMap` accumulation behave identically to live
  evaluation.

If you cannot prove a cache shape is byte-safe, do not add it to the
cache. Fall through to live evaluation; perf is recoverable, parity
is not.

#### 3.9.3 Lifetime model (why "per-worker", not "per-build" or "global")

The cache lifetime equals the **Parcel worker process lifetime**.

- **Process, not thread:** Webpack/Parcel parallelism is across
  separate Node worker *processes*, each with its own V8 isolate, its
  own module-level state, its own `swc.transform()` invocations. JS
  has no shared-memory concurrency primitive that would let Babel
  share `globalCache` across workers — and indeed Babel doesn't.
  Matching this means our cache is also per-worker, with no
  cross-worker coordination.

- **Worker, not file:** within one worker, the cache persists across
  every `transform()` call that worker handles. This is the whole
  optimization. A worker that processes 1000 files reads `theme.ts`
  once, parses it once, evaluates each token once.

- **Not cross-build / cross-process:** the cache is wiped when the
  worker exits. We do not write to `node_modules/.cache/...`.
  Cross-build persistence is out of scope; it would require versioning
  the cache against plugin version, parser plugins, swc_core version,
  and consumer config — a much larger correctness surface.

  *If* a future iteration wants cross-build cache, it lives in a
  separate file with a `schema_hash` derived from
  `(plugin_version, swc_core_version, sorted(parser_babel_plugins),
  sorted(extensions), classHashPrefix)`. Mismatch on any input wipes.
  Defer.

#### 3.9.4 Concurrency model (why `swc.transformSync` is mandatory)

`@swc/core` exposes both `transform` (Promise-returning, runs on
libuv's threadpool) and `transformSync` (blocks the Node main thread).
napi can in principle run multiple `transform()` calls in parallel on
different libuv threads within one Node process, each with its own
fresh wasmer `Instance` (verified at `transform_executor.rs:~178,
~225, ~257`).

Parallel `transform` calls within one worker process would race on the
shared cache file. To eliminate the race entirely:

> **The host wrapper MUST use `swc.transformSync`. This is a hard
> contract, not a recommendation.**

With `transformSync`, all `transform` calls in one worker run serially
on the main thread. The cache file is touched by exactly one writer at
a time. No locking, no per-call additions files, no merger.

Phase 0 ABI gate includes a contract test that imports
`@swc/core/transformSync` at the pinned version and asserts the export
exists. If a future SWC release renames or removes it, the cache
strategy breaks loudly at version-bump time, not silently at runtime.

If a future host integration (jest worker, bun-test) cannot use
`transformSync`, that integration must implement per-call
`<workerScratch>/cache-out-<callId>.json` files plus a host-side
merger — outside the scope of this plan, and gated on a real
requirement.

#### 3.9.5 Scratch directory layout (host responsibility)

The host owns scratch directory creation, lifetime, and cleanup. The
plugin only reads/writes files inside paths the host hands it via
plugin config.

**Critical: scratch must be inside the WASI cwd preopen.** §3.2 says
the SWC plugin host preopens *only* `std::env::current_dir()`. An
earlier draft of this plan put scratch at
`mkdtempSync(join(tmpdir(), 'compiled-swc-worker-'))`, but `/tmp` (or
`%TEMP%` on Windows) is *not* under the project root, so the plugin
literally could not open any file there. The corrected location:

```
<projectRoot>/node_modules/.cache/compiled-swc/    # gitignored by convention
  worker-<pid>/                                      # mkdir at worker init
    cache.bin                                        # Layer 2 only, postcard, capped 5 MB
    call-<uuid>/                                     # mkdir per transform()
      included-files.json                            # written by plugin on Program::exit
      style-rules.json                               # written by strip-runtime plugin
```

If `node_modules/.cache/` does not exist or is not writable (rare —
some sandboxed CI configs), the host falls back in this order:

1. `<projectRoot>/.compiled-swc-cache/worker-<pid>/`
2. `<projectRoot>/.cache/compiled-swc/worker-<pid>/`
3. Hard error: emit a clear diagnostic naming both attempted paths
   and the resolved cwd. Do **not** silently fall back to `/tmp` —
   the plugin will appear to work but every cache lookup will fail
   with `EACCES`/`ENOENT` from inside the WASI sandbox.

The chosen `<projectRoot>` is the directory the worker process was
spawned with (`process.cwd()` at module load). For Parcel, that must
be the monorepo root or a subtree containing every source file the
plugin will need to read; see §3.9.13.7. The Phase 0 location probe
(§3.9.14 #6) verifies the chosen path is reachable from inside WASI
*before any plugin code runs*.

**`workerScratchDir`:** created exactly once per worker process by the
Parcel transformer wrapper at module load. Path is
`<projectRoot>/node_modules/.cache/compiled-swc/worker-<pid>/` (or a
fallback per above). Created via `mkdirSync(..., { recursive: true })`
— not `mkdtempSync`, because we want a stable per-pid path so a
crashed worker leaves a deterministically-named directory the next
worker can clean up. Cleaned up via
`process.on('exit', () => rmSync(workerScratchDir, { recursive: true,
force: true }))`. Same path passed into every `transform()` call this
worker makes.

**`call-<uuid>/`:** created per-transform by the wrapper, passed in
via plugin config as `callScratch`. Cleaned up in the wrapper's
`finally` block. Holds disposable per-call sidecars (`included-files`,
`style-rules`); never holds cache state.

**`cache.bin`:** lives in `workerScratchDir`, persists for the
worker's lifetime. Postcard-encoded, single compact binary file (see
§3.9.10). Plugin reads on `Program::enter`, writes on `Program::exit`
**only if mutated** during this transform. The plugin is the only
writer — host never touches the file directly except to delete the
worker scratch dir on worker shutdown.

#### 3.9.6 Plugin config contract (what the host passes per call)

```ts
// host -> plugin, every transform()
{
  ...userPluginOptions,
  workerScratchDir: "<absPath>",   // stable across calls in this worker
  callScratch:      "<absPath>",   // unique per call
  cacheLevel:       "ast" | "value" | "off",   // default "value"
}
```

- `workerScratchDir`: where `cache.bin` lives (postcard binary;
  Layer 2 only). Stable across calls in this worker.
- `callScratch`: where per-call sidecars go.
- `cacheLevel`:
  - `"value"`: Layer 1 (in-memory) + Layer 2 (persisted) (default; max perf).
  - `"ast"`: Layer 1 (in-memory) only (rip-cord; matches Babel's
    effective hit rate within a transform but loses the cross-file
    evaluated-value win).
  - `"off"`: no caching, every lookup live (debug aid; equivalent to
    Babel `cache: false`).

The host does NOT pass cache *contents* through plugin config. The
plugin reads them from disk. Plugin config stays small and bounded;
the cache itself can grow to MB-scale without boundary serialization
cost.

#### 3.9.7 Plugin lifecycle per `transform()` call

```
Program::enter:
  1. Read <workerScratchDir>/cache.bin. Empty map if missing or read
     fails (corruption is regenerable; never crash the build).
  2. Postcard-deserialize. Validate `version` and `schema_hash`
     fields. Mismatch -> wipe entire cache (treat as missing).
     Mismatch happens on plugin version bump, swc_core bump, or
     cacheable-shape change.
  3. Build in-memory HashMap<u64, Layer2Entry> from the file.
     Initialize an empty in-memory HashMap<u64, Layer1Entry>; Layer 1
     is *not* persisted to disk (see §3.9.8). Set
     `cache_dirty = false` and `mtime_observed: HashMap<PathBuf, u128>
     = HashMap::new()` (per-transform mtime memo to dedupe stats —
     see step 6).
  4. **Lazy invalidation, not eager.** Do NOT stat every cached file
     up front (O(cache_size) syscalls per transform). Instead,
     stat-on-hit (see traversal step).

During traversal:
  5. cache.load() hits the in-memory maps; logic identical to Babel's
     cache.ts (LRU eviction at maxSize, move-to-end on hit).
  6. On Layer 1 hit (in-memory only, never persisted):
       - mtime_observed memoizes per-transform stats: first lookup of
         a path issues `std::fs::metadata(path).mtime`; subsequent
         lookups in the same transform reuse the memoized value.
       - matches entry.source_mtime_ns -> hit valid
       - mismatch -> evict this entry AND any Layer 2 entries whose
         transitiveDeps include this path (set `cache_dirty = true`);
         fall through to live read+parse
  7. On Layer 2 hit:
       - validate entry.source_mtime_ns AND every transitiveDeps[i]
         mtime via the memoized stat (step 6 mechanism)
       - any mismatch -> evict (set `cache_dirty = true`); fall
         through to Layer 1 lookup + live evaluation
       - all match -> replay stateDiffs against the consumer's
         meta.state, return entry.evaluatedAst (no dirty bit; reads
         don't mutate disk state)
  8. New Layer 2 entries written into the in-memory map with current
     mtime captured at write time. Set `cache_dirty = true` on every
     Layer 2 insert/evict/touch-LRU. Layer 1 inserts do NOT set the
     dirty bit (Layer 1 is in-memory only).

Program::exit:
  9. If `cache_dirty == false`, skip the write entirely. (Most
     transforms are pure cache hits or pure misses against unchanged
     entries; skipping no-op writes is the dominant case for warm
     workers.)
  10. Otherwise, postcard-serialize Layer 2 only. Enforce size cap:
      if `serialized.len() > MAX_CACHE_BYTES (5 MiB)`, evict by LRU
      until it fits. Atomic write: `cache.bin.tmp` ->
      `fd_sync` -> `path_rename` to `cache.bin`. Single writer
      guaranteed by §3.9.4.
  11. Sidecars (included-files, style-rules) go to
      callScratch as before.
```

The lazy-invalidation choice (step 4) plus the per-transform mtime
memo (step 6) keeps per-transform overhead proportional to *unique
paths touched*, not *cache size or hit count*. Cold paths in the
cache cost nothing.

The `cache_dirty` short-circuit (step 9) is important: a worker that
processes 10000 files where 9990 are pure cache hits writes the cache
file ~10 times instead of 10000.

#### 3.9.8 Two-layer cache structure

**Layer 1 — source AST cache (in-memory only, per `transform()`):**

| Key namespace | Key payload | Cached value |
|---|---|---|
| `read-file` | `modulePath` | file content `String` |
| `parse-module` | `modulePath` | `Arc<swc_ecma_ast::Module>` |
| `find-default-export-module-node` | `modulePath` | `{ found_node_ref, found_parent_byte_pos }` |
| `find-named-export-module-node` | `modulePath=...&exportName=...` | same shape |

Layer 1 is **never persisted to disk**. It exists only inside a single
`transform()` call's `Program` lifecycle and is dropped at
`Program::exit`. Mirrors Babel's existing `utils/cache.ts` call sites
exactly (so the `cache.load()` API surface is unchanged), but gives up
cross-file persistence.

Skips (within one transform): redundant `fs.read` + parse + AST
traversal when the same module is referenced multiple times during one
file's evaluation. Pays: re-parses each module on the first transform
that touches it, then keeps the parse for the rest of *that* transform.

**Why not persist Layer 1?**
- `swc_ecma_ast::Module` does not have a stable, supported
  serde-Serialize/Deserialize impl that survives `swc_core` version
  bumps. Roll-our-own serialization is a maintenance liability.
- Serialized AST is typically 5–20× the source byte size. Persisting
  500 entries of large modules across a 60-90 GB monorepo blows the
  cache size cap immediately.
- SWC parsing is fast (~50 MB/s on commodity hardware). Reading from
  warm OS page cache + re-parsing is competitive with deserializing a
  binary AST blob, and avoids the version-stability problem.
- The real win is Layer 2 — re-evaluating an imported `theme.ts` 1000×
  is what hurt Babel. Re-parsing it 1000× is a smaller cost we can
  swallow in exchange for design simplicity.

**Layer 2 — evaluated-value cache (persisted to disk):**

```rust
struct Layer2Entry {
    evaluated_ast: SerializedExpr,           // bounded subset; see §3.9.10
    state_diffs: Vec<StateDiff>,
    transitive_deps: Vec<TransitiveDep>,     // capped at MAX_TDEPS_PER_ENTRY
    source_mtime_ns: u128,
    lru_seq: u64,
    byte_size_estimate: u32,                 // for size-cap eviction
}

enum StateDiff {
    IncludedFilesPush { path: String },
    CompiledImportsAppend { api: ApiKind, local_name: String },
    SheetsInsert { sheet_text: String, hoisted_name: String },
    CssMapInsert { binding: String, sheets: Vec<String> },
    IgnoreMemberExprMark { name: String },
}

enum ApiKind { ClassNames, Css, Keyframes, Styled, CssMap }
// Reconciled against actual mutation-site enumeration in
// crates/babel-plugin/STATE_MUTATIONS.md (Phase 0 artefact).
// 5 variants, not 4 — `IgnoreMemberExprMark` was missing from
// earlier sketches and would have caused silent perf regressions
// + transitive-dep-miss risk on Layer 2 hits.

struct TransitiveDep {
    path: String,
    mtime_ns: u128,
}
```

`SerializedExpr` and `SerializedValue` are *bounded* shapes — only the
AST node kinds in §3.9.9's "cacheable Layer 2" table can appear. We do
NOT cache arbitrary `swc_ecma_ast::Expr`. Bounded shapes give us a
stable wire format under postcard.

Layer 2 entry size is bounded by construction:
- `MAX_TDEPS_PER_ENTRY = 32` — if an evaluation pulls in more than 32
  distinct files, mark `cacheable_at_layer2 = false` and don't cache.
  Such evaluations are rare (theme/token files transitively reach a
  small set of constants) and the long tail isn't worth caching.
- `evaluated_ast` is bounded by §3.9.9; large object literals
  (>4 KiB serialized) are not cached.
- `state_diffs` length is bounded by `MAX_STATE_DIFFS = 64`; longer
  diff logs decline caching.

These bounds are sentries, not normal cases. If a real input regularly
trips them, retune in `crates/PARITY_VERSIONS.md` (perf-only knobs).

Skips: everything Layer 1 skips PLUS `evaluateExpression`'s walk
through bindings, recursive resolution, and the live mutation of
`state`.
Pays: replaying state diffs (cheap; bounded vector ops).

**State-diff capture:** during a Layer 1 + live-eval path, the plugin
runs evaluation through a `MutationRecorder` that wraps every state
write. The recorder both performs the write AND appends to a per-
evaluation diff log. On evaluation completion (and only if the result
is a candidate for Layer 2 caching — see next subsection), the diff
log is stored as the entry's `state_diffs`. On a future Layer 2 hit,
the plugin replays diffs against the consumer's state without running
evaluation.

**Compiler-enforced state encapsulation (added to remediate
"missed-mutation" risk).** Earlier drafts described `MutationRecorder`
as a wrapper that *should* be used at every state-write site. That is
a vibes check — easy to miss one write during the port, easy to
regress one in a future change. The remediation:

1. **Make `State` fields private.** The struct's mutable fields
   (`included_files`, `compiled_imports`, `sheets`, `css_map`,
   `ignore_member_expressions`) are `pub(self)` only. `pub fn` getters
   are read-only.
2. **Mutation is impossible without a `MutationRecorder`.** `State`
   exposes no public mutators. The only path that writes into the
   private fields is `MutationRecorder::apply(diff: StateDiff,
   state: &mut State)`, which lives in the same module as `State` and
   has the only `pub(super)` mutator. Outside `state.rs`, there is
   no syntactic way to mutate state without going through a diff.
3. **Pre-commit lint gate:** before any port code lands, run a
   pre-commit lint that fails on any `state\.[a-z_]+\.(push|set|
   add|insert|remove|extend)` regex match outside `state.rs` and
   `mutation_recorder.rs`. Document the gate in
   `crates/PARITY_VERSIONS.md`.
4. **Enumerate every Babel-side mutation site as a Phase 0 task.**
   Run `grep -nE 'state\.(includedFiles|compiledImports|sheets|cssMap|
   ignoreMemberExpressions)\b'` over
   `packages/babel-plugin/src/utils/evaluate-expression.ts` (and the
   `traverse_expression/` subtree) and write the count + path list into
   `crates/babel-plugin/STATE_MUTATIONS.md`. For each, record the
   corresponding `StateDiff` variant. Phase 5 task 0 is the
   reconciliation step that re-runs the enumeration and confirms it's
   still current (see §5). Any unaccounted mutation = port defect, fix
   before merging.

Together (1)+(2) make *missed mutation captures* a Rust compile error,
not a silent runtime divergence. (3)+(4) make *new* mutations
(introduced post-port) impossible to merge without explicit
StateDiff-variant work.

**Transitive-dep capture:** the resolver hooks every `fs::read` (or
parse-module/find-export lookup) during evaluation and appends to the
current evaluation's `transitive_deps` set. Centralized capture; cannot
be missed because the only way to read a file from inside the plugin
is through this resolver. Same encapsulation pattern as state above:
the resolver type's `read()` and `parse_module()` are the *only*
public ways to ingest source bytes, and both unconditionally append to
the active dep set.

#### 3.9.9 What is and is NOT a Layer 2 candidate

Cache to Layer 2 only when the evaluation is consumer-context-free.
Decision rule, applied at evaluation completion:

| Result shape | Cache Layer 2? | Reason |
|---|---|---|
| `Lit::Num`, `Lit::Str`, `Lit::Bool`, `Lit::Null` | YES | Pure data, no scope dependence |
| `ObjectLit` of cacheable-shape properties (recursive) | YES | Pure data |
| `ArrayLit` of cacheable-shape elements (recursive) | YES | Pure data |
| `Tpl` (template literal, no expressions) | YES | Pure data |
| `Tpl` with expressions that themselves cache | YES | Recurse into expressions |
| Compiled `keyframes(...)` call result | YES | Result is itself a tagged literal call |
| `FnExpr` / `ArrowExpr` | NO | Inlined per-consumer with consumer args; cache the AST at Layer 1 only |
| Fallback to original input expression (line 104 in evaluate-expression.ts) | NO | Fallback semantics; live eval may pick a different fallback in different consumer scopes |
| Anything that threw during eval | NO | Don't cache failure paths |

Implementation: `EvaluateResult` carries a `cacheable_at_layer2: bool`
flag set by traversers as they build up the result. Conservative
default: `false`. Only flip to `true` when every step of the build is
itself Layer 2-safe.

#### 3.9.10 Cache file schema (`cache.bin`)

Format: **postcard** (compact, deterministic, schema-stable Rust serde
binary format — `postcard` crate). NOT JSON. Rationale:

- 5–10× faster encode/decode than serde-JSON for the same data.
- 3–6× smaller on disk (no field names, no whitespace).
- u128 round-trips natively (no string-encoding workaround).
- Stable byte layout under a fixed schema — combined with
  `schema_hash` validation, version drift is detected at load time.

JSON was tempting for human-readable debugging; the cache file is
plugin-internal scratch state, debug-readability is a non-goal,
and `cache_inspect.rs` (CLI in `crates/babel-plugin/src/bin/`)
provides a `--dump-as-json` flag for the rare case we need to look.

```rust
// crates/babel-plugin/src/cache_schema.rs
// Frozen wire format. Bumping any field => bump CACHE_VERSION
// AND recompute SCHEMA_HASH (constant inputs in a build.rs).

const CACHE_VERSION: u32 = 1;
const MAX_CACHE_BYTES: usize = 5 * 1024 * 1024;       // 5 MiB hard cap
const MAX_ENTRIES: usize = 500;                       // matches Babel maxSize
const MAX_TDEPS_PER_ENTRY: usize = 32;                // §3.9.8
const MAX_STATE_DIFFS: usize = 64;                    // §3.9.8

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,                 // == CACHE_VERSION
    schema_hash: [u8; 32],        // BLAKE3 of (plugin_version,
                                  //          swc_core_version,
                                  //          sorted parser_babel_plugins,
                                  //          sorted extensions,
                                  //          classHashPrefix)
    layer2: Vec<(u64, Layer2Entry)>,   // sorted by key for determinism
    // NB: Layer 1 is in-memory only (§3.9.8). Not in this struct.
}
```

Versioned + schema-hashed. Mismatch on either is a hard wipe, not an
error — cache is regenerable.

**On size cap:** the 5 MiB cap is enforced at write time
(§3.9.7 step 10). If serialized output exceeds the cap, evict by LRU
until it fits, then re-serialize. The combination of `MAX_ENTRIES`,
per-entry size bounds (§3.9.8), and the 5 MiB cap ensures the cache
file is always small enough to read+decode in <5 ms even on slow
filesystems — critical when running on a 60-90 GB monorepo where
hundreds of worker processes may touch their cache files
concurrently.

**On corruption recovery:** if `postcard::from_bytes()` returns an
error (truncated write, partial filesystem corruption, version drift
that schema_hash didn't catch), treat the file as missing. Wipe and
proceed. Never crash the build over a regenerable scratch file.

**On atomic writes:** `cache.bin.tmp` is written, fsynced, then
`path_rename`'d over `cache.bin`. WASI supports both `fd_sync` and
`path_rename`. A worker crash mid-write leaves the tmp file behind;
the next worker startup deletes any stale `*.tmp` siblings before
reading `cache.bin`.

#### 3.9.11 Eviction

**Two simultaneous caps; eviction by LRU until both are satisfied:**

1. **Entry-count cap:** `MAX_ENTRIES = 500` per layer (matches Babel's
   `cache.ts:11` `maxSize` exactly for parity of *which entries
   survive* under the same access pattern).
2. **Byte cap (Layer 2 only):** `MAX_CACHE_BYTES = 5 MiB` on the
   serialized postcard file. This is the load-bearing cap on a 60-90 GB
   monorepo — a single misbehaving evaluation that produces a
   2 MiB cached object literal cannot poison the cache; it just gets
   evicted on the next write.

Eviction order:
- On in-memory mutation: drop by `lru_seq` ascending until count <=
  `MAX_ENTRIES`.
- On `Program::exit` write: serialize, check byte size, drop by
  `lru_seq` ascending until serialized size <= `MAX_CACHE_BYTES`,
  re-serialize. (At most 2-3 iterations in practice; entries are
  bounded.)

Eviction order does not affect output bytes (cache hit value === cache
miss value when source unchanged), so this is the one place a
perf-only divergence is acceptable: profiling may show
`MAX_ENTRIES: 5000` and `MAX_CACHE_BYTES: 50 MiB` are right for very
large monorepos. **Document any deviation in
`crates/PARITY_VERSIONS.md` as perf-only.**

**Disk pressure on large monorepos.** Per worker, the cache file is
capped at 5 MiB. Per-project total disk use scales with worker count:
e.g., 16 workers = 80 MiB peak, well within
`node_modules/.cache/` budgets. The cleanup-on-worker-exit pattern
keeps stale dirs from accumulating across builds.

#### 3.9.12 Filesystem and platform constraints

- **WASI sync I/O is the only FS path.** `std::fs::read`,
  `std::fs::write`, `std::fs::metadata` work inside `wasm32-wasip1`
  and lower to `fd_read` / `fd_write` / `fd_filestat_get` syscalls,
  serviced synchronously by wasmer's WASI implementation in
  `swc_plugin_runner@23`. Verified by Phase 0 probe (write→read
  round-trip and metadata mtime read).
- **Cap-std preopen scope.** The plugin can only access files under
  `std::env::current_dir()` of the host process. **Therefore
  `workerScratchDir` MUST be a path under that cwd
  (§3.9.5).** `/tmp` is unreachable from inside the plugin at
  `@swc/core@1.15.8` — there is no preopen knob exposed on the JS
  side at this version. The host MUST set cwd such that the chosen
  scratch dir AND the consuming workspace's source files all fall
  under one preopen. For Parcel this means spawning workers with cwd
  at the monorepo root, not the package directory. The Phase 0
  scratch-dir reachability probe (§3.9.14 #6) is a *hard* gate before
  any plugin code lands.
- **mtime resolution.** Linux/macOS = nanosecond on most filesystems;
  Windows NTFS = 100ns; FAT32/exFAT = 2s (rare on dev machines).
  Network filesystems and Docker bind mounts can have unreliable
  mtime. For these cases, expose `opts.cacheInvalidation: 'mtime' |
  'content-hash'` (default `'mtime'`). `'content-hash'` recomputes a
  fast non-cryptographic hash (xxhash3) of the source on each lookup
  — slower but mtime-resolution-independent. The per-transform mtime
  memo (§3.9.7 step 6) applies to both modes.
- **u128 mtime representation.** `std::fs::Metadata::modified()`
  returns `SystemTime`; convert to `u128` nanoseconds since UNIX
  epoch. Postcard encodes u128 natively as 16 bytes — no string
  workaround needed (this is one of the reasons we dropped JSON).
- **Atomic writes.** Cache file writes use the standard
  write-temp-then-rename pattern: write `cache.bin.tmp`, fsync,
  rename over `cache.bin`. This survives a worker crash mid-write
  without corrupting the cache. WASI `fd_sync` is available; rename
  via `path_rename`. Both work in `wasm32-wasip1`. Worker startup
  deletes any stale `*.tmp` siblings before reading.

#### 3.9.13 What lives outside `@swc/core` (the host's job)

The host (Parcel transformer wrapper) is responsible for:

1. **Worker-scoped scratch dir creation and teardown.** Path MUST be
   under the WASI cwd preopen (§3.9.5, §3.9.12). `mkdtempSync` in
   `os.tmpdir()` is **forbidden** — that path is unreachable from
   inside the plugin sandbox. Use a stable per-pid path under
   `node_modules/.cache/`:
   ```ts
   // module-load (worker init), once per worker process
   import { mkdirSync, rmSync, existsSync } from 'fs';
   import { join } from 'path';
   const projectRoot = process.cwd();   // see §3.9.13.7
   const cacheRoot   = pickCacheRoot(projectRoot); // node_modules/.cache/...
                                                   // with documented fallbacks
   const workerScratchDir = join(cacheRoot, `worker-${process.pid}`);
   mkdirSync(workerScratchDir, { recursive: true });
   // sweep stale tmp files from a prior crashed worker with this pid
   for (const f of readdirSync(workerScratchDir)) {
     if (f.endsWith('.tmp')) rmSync(join(workerScratchDir, f), { force: true });
   }
   process.on('exit', () =>
     rmSync(workerScratchDir, { recursive: true, force: true })
   );
   ```
2. **Per-call sub-scratch dir creation and teardown.**
   ```ts
   // inside transform({ asset, ... }) — synchronous block
   import { randomUUID } from 'crypto';
   const callScratch = join(workerScratchDir, `call-${randomUUID()}`);
   mkdirSync(callScratch);
   try {
     /* invoke swc.transformSync ... */
   } finally {
     rmSync(callScratch, { recursive: true, force: true });
   }
   ```
3. **Calling `swc.transformSync` (NOT `swc.transform`).** Required by
   §3.9.4. Wrapping in `await` for API compatibility is fine; the
   inner call must be sync.
4. **Threading `workerScratchDir`, `callScratch`, `cacheLevel` into
   plugin config.**
5. **Draining per-call sidecars after `transformSync` returns.**
   Cache file is left untouched — plugin already wrote it.
6. **Honouring `opts.cacheInvalidation`.** Pass through to plugin
   config.
7. **Setting cwd correctly when spawning workers.** Project root, not
   package root, so the WASI preopen covers all source files the
   resolver may walk.
8. **Never reading or writing `cache.bin` from JS.** It's
   plugin-owned. The host's only contact with the file is `rmSync`
   on worker exit. Reading/writing from JS would race with the
   plugin and is unnecessary. Postcard binary format is also not
   trivially parseable from JS, which is by design.

What the host is NOT responsible for:

- Cache invalidation logic. The plugin handles mtime checks.
- Cache content shape. Plugin owns `version`, `schema_hash`,
  serialization.
- Cross-worker synchronization. There is none — each worker is
  independent.
- Cross-build persistence. Out of scope (§3.9.3).

#### 3.9.14 Phase 0 contract tests (gate the design before any plugin code)

Before Phase 1 can begin, Phase 0 must include:

1. **WASI sync I/O probe.** Test plugin opens a file in its preopen,
   writes "hello", reads it back, asserts equality. Confirms the FS
   transport.
2. **WASI mtime probe.** Test plugin reads `metadata().modified()`
   on a known file and asserts it returns a non-zero `SystemTime`.
   Confirms invalidation mechanism is implementable.
3. **`transformSync` ABI probe.** JS test imports
   `transformSync` from the pinned `@swc/core@1.15.8` and asserts
   it's a function. If a future SWC version removes this export, the
   cache strategy breaks; this test catches it at version-bump time.
4. **Instance-teardown probe.** Test plugin holds a `static
   AtomicU64` counter; runs two `transformSync` calls back to back;
   asserts the counter starts at 0 in the second call. Confirms
   §3.9.0's premise that plugin-static memory does not persist.
5. **Cache-file race probe (negative test).** Spawn two
   `transform()` (async, NOT sync) calls in parallel and assert the
   cache file ends up internally consistent OR fails loudly. This
   isn't a feature; it's a guardrail proving why §3.9.4's
   `transformSync` mandate exists. Document the failure mode for
   future readers.
6. **Scratch-dir reachability probe (HARD GATE before any plugin
   code lands).** Host creates the scratch dir at the path chosen
   per §3.9.5 / §3.9.13.1. JS test invokes `transformSync` with a
   probe plugin that:
   - opens `<workerScratchDir>/probe.bin` for write,
   - writes 8 known bytes,
   - reads them back,
   - opens `<callScratch>/probe.bin` for write+read,
   - asserts both round-trip equal.
   If any open returns `EACCES`/`ENOENT`, the chosen path is outside
   the WASI cwd preopen — the test reports the resolved cwd, the
   attempted path, and exits non-zero. **No plugin development
   begins until this passes on Linux, macOS, and Windows.**
7. **Postcard round-trip probe.** Probe plugin postcard-encodes a
   fixture `CacheFile { version: 1, schema_hash: [42; 32], layer2:
   vec![] }`, writes to `<workerScratchDir>/probe.bin`, reads it
   back, deserializes, asserts `version == 1`. Confirms postcard +
   WASI binary I/O works as designed.
8. **Byte-cap eviction probe.** Construct an in-memory
   `CacheFile` with 1000 fake Layer 2 entries, call the eviction
   routine with `MAX_CACHE_BYTES = 64 KiB`, assert the result
   serializes to <= 64 KiB and contains the most-recently-touched
   entries. Pure unit-test — no WASI needed — but lives in the same
   harness so failures here gate plugin work the same way the
   sandbox probes do.
9. **Resolver difference matrix.** Run a corpus of representative
   resolution requests through (a) `enhanced-resolve@5.x` configured
   the same way `createDefaultResolver(config)` configures it, (b)
   the npm `resolve.sync()` fallback path with `opts.extensions =
   ['.js','.jsx','.ts','.tsx']`, and (c) `oxc_resolver` with
   matching options. Inputs cover: `package.json#main`,
   `package.json#exports` with conditions (`import`, `require`,
   `node`, `default`), `tsconfig` `paths`, symlink realpath via
   pnpm-style stores, browser-field, file extensions, directory
   index resolution. Failures = port defects to fix in `oxc_resolver`
   configuration before Phase 5. **Hard gate before Phase 5 starts.**

#### 3.9.15 Phase 5 parity tests (gate Layer 2 before enabling by default)

When `cacheLevel: "value"` ships, run these in the parity harness
for at least one calendar month before considering the rip-cord
removed:

1. **Shadow-eval test.** For every Layer 2 hit during a transform,
   run a shadow live-eval in parallel; assert
   `state-after-replay === state-after-live-eval` field by field
   (`includedFiles`, `compiledImports`, `sheets`, `cssMap`). Any
   mismatch is a Layer 2 design defect. Disable Layer 2 in
   production until fixed.
2. **HMR invalidation test.** Programmatically transform file A
   (caches `theme.ts`), edit `theme.ts` on disk, transform file B,
   assert the Layer 1 entry was evicted, the cascade evicted Layer 2
   entries with `theme.ts` in `transitive_deps`, and live evaluation
   ran for the new value.
3. **Transitive-dep miss test.** Construct a dep chain
   `A -> B -> C`. Cache `A`'s evaluation. Edit `C`. Transform a
   consumer of `A`. Assert Layer 2 evicted because
   `transitive_deps` covered `C`.
4. **Worker-restart test.** Run `transformSync` 10×, kill the
   worker, restart, run 10×. Assert the second worker sees no stale
   cache (the scratch dir was cleaned up on the first worker's
   exit).

Failures in any of (1)-(3) are correctness bugs. Failures in (4) are
host-cleanup bugs. Don't ship `cacheLevel: "value"` as default until
all four are stable.

#### 3.9.16 Performance targets (anchor the design's value)

Reference workload: a single Parcel build over a workspace with 1000
components, each importing 5-10 tokens from a shared `theme.ts`
(50 exported constants).

| Mode | `theme.ts` parses | Per-token evals | Target wall-time vs Babel |
|---|---|---|---|
| Babel today | 1000× | 50,000× | 1.0× (baseline) |
| SWC, `cacheLevel: "off"` | 1000× | 50,000× | 0.7× (raw SWC speed only) |
| SWC, `cacheLevel: "ast"` | 1× | 50,000× | 0.3× |
| SWC, `cacheLevel: "value"` | 1× | 50× | 0.05× |

Numbers are illustrative, not measured. Phase 0 perf baseline
(measure Babel `cache: true` vs `cache: false` over a representative
workspace) gives the actual baseline; Phase 8 corpus diff captures
the achieved numbers. If `cacheLevel: "value"` doesn't beat
`cacheLevel: "ast"` by ≥3× on token-heavy workloads, the
diff-replay machinery is overhead without payoff and should be
disabled by default.

#### 3.9.17 Failure modes and rip-cords

| Symptom | Likely cause | Rip-cord |
|---|---|---|
| Output bytes diverge under HMR | Layer 2 missed a transitive dep, or state-diff replay is incomplete | Set `cacheLevel: "ast"` in Parcel config; corpus diff should clear immediately. Open a defect ticket against §3.9.8 / §3.9.15-shadow-eval. |
| Output bytes diverge in cold build | Layer 1 invalidation missed a source change; or `schema_hash` didn't bump on a cache-shape change | Set `cacheLevel: "off"`; bytes must clear. Investigate which schema input changed. |
| Wall-time worse than Babel | `transformSync` not used; or cache file thrashing; or LRU eviction churn | Confirm `transformSync` per §3.9.4; raise `maxSize`; profile with `cacheLevel: "off"` to isolate. |
| Worker crashes loop | Atomic-rename pattern not used and a partial write is being read | Wipe `<workerScratchDir>/cache.bin` (and any `*.tmp` siblings) manually; confirm §3.9.12 atomic-write pattern + worker-startup tmp-sweep are in place. |
| Cross-worker test inconsistency | Two workers somehow see the same scratch dir | Each worker calls `mkdtempSync` independently — verify the wrapper is module-scoped, not import-cached at a higher level. |

In all cases, `cacheLevel: "off"` is the universal kill switch:
behaviour reduces to live evaluation per call, byte-equivalent to a
worst-case Babel run minus per-file Babel caching. Slow but
guaranteed correct.

---

## 4. Repo layout

```
crates/
  babel-plugin/                              # ports packages/babel-plugin/
    Cargo.toml
    src/
      lib.rs                                 # SWC plugin entry, dispatcher
      babel_plugin.rs                        # ports babel-plugin.ts
      types.rs                               # ports types.ts
      constants.rs                           # ports constants.ts
      class_names/mod.rs                     # ports class-names/index.ts
      css_prop/mod.rs                        # ports css-prop/index.ts
      css_map/
        mod.rs                               # ports css-map/index.ts
        process_selectors.rs                 # ports css-map/process-selectors.ts
      styled/mod.rs                          # ports styled/index.ts
      xcss_prop/mod.rs                       # ports xcss-prop/index.ts
      keyframes/mod.rs                       # cleanup-only handler
      utils/
        cache.rs                             # ports utils/cache.ts (LRU)
        ast.rs
        append_runtime_imports.rs
        build_compiled_component.rs
        build_styled_component.rs
        build_css_variables.rs
        build_display_name.rs
        compress_class_names_for_runtime.rs
        comments.rs
        css_builders.rs                      # ports utils/css-builders.ts (largest)
        css_map.rs
        evaluate_expression.rs
        get_jsx_attribute.rs
        get_runtime_class_name_library.rs
        has_numeric_value.rs
        hoist_sheet.rs
        is_compiled.rs
        is_empty.rs
        is_jsx_function.rs
        manipulate_template_literal.rs
        normalize_props_usage.rs
        object_property_to_string.rs
        resolve_binding.rs                   # uses oxc_resolver, reads via /cwd
        transform_css_items.rs
        types.rs
        traverse_expression/
          mod.rs
          traverse_binary_expression.rs
          traverse_call_expression.rs
          traverse_function.rs
          traverse_identifier.rs
          traverse_unary_expression.rs
          traverse_member_expression/
            mod.rs
            traverse_access_path/
              mod.rs
              evaluate_path/
                mod.rs
                namespace_import.rs
                object.rs
              resolve_expression/
                mod.rs
                function_args.rs
                identifier.rs
        traversers/
          mod.rs
          get_export.rs
          object.rs
          set_imported_compiled_imports.rs
          types.rs
      compat/                                # Babel APIs without SWC analogues
        mod.rs
        generator.rs                         # @babel/generator subset
        template.rs                          # @babel/template subset
        declare.rs                           # @babel/helper-plugin-utils
        scope.rs                             # path.scope.getBinding
        # add more as discovered
    tests/
      parity.rs                              # runs Babel + SWC + prettier diff
      fixtures/                              # mirrors packages/babel-plugin/src/__tests__
        ...

  babel-plugin-strip-runtime/                # ports packages/babel-plugin-strip-runtime/
    Cargo.toml
    src/
      lib.rs                                 # ports index.ts
      types.rs                               # ports types.ts
      utils/
        is_automatic_runtime.rs
        is_cc_component.rs
        is_create_element.rs
        remove_style_declarations.rs
        to_uri_component.rs
      compat/
        mod.rs
        # as needed
    tests/parity.rs

plugins/
  PARCEL_USAGE_EXAMPLE.md                    # production call site we satisfy
  PLAN.md                                    # this file — source of truth for constraints
  SIDECAR_SCHEMA.md                          # cross-language interface (created Phase 1)
```

---

## 5. Phased execution

Each phase has an **exit gate**. **Do not skip exit gates.** A divergence
missed at phase N becomes an unfindable bug at phase N+5.

### Phase 0 — Prerequisites and parity harness

**Goal.** Stand up the verification oracle and prove it's stable before any
plugin code ships.

Tasks:

1. Pin `@swc/core@1.15.8` and the matching `swc_core` Rust crate version in
   `crates/PARITY_VERSIONS.md`. Cross-reference the SWC plugin compatibility
   matrix.
2. Pin the workspace prettier version in `crates/PARITY_VERSIONS.md`.
3. Build `crates/babel-plugin/tests/parity.rs` (and its strip-runtime twin):
   - Loads a fixture `(input, opts) → expected Babel output`.
   - Runs Babel pipeline → output A.
   - Runs SWC pipeline (initially: pass-through plugin) → output B.
   - Runs prettier on both with `parser: 'babel-ts'`.
   - Asserts byte-equality. Reports the smallest divergent byte range with
     surrounding context on failure.
4. **Babel-against-itself baseline.** Run Babel + prettier round-trip across
   the whole corpus. Any non-determinism here is a blocker (fix
   `process.env.TEST_PKG_VERSION` to a fixed string in the harness, etc.).
5. Snapshot every existing test from both packages (38 tests in
   strip-runtime, ~50+ in babel-plugin including subdirs) as fixtures:
   `(input, opts) → output`.
6. Add **`scripts/audit-included-files.ts`** — runs the existing Babel
   plugin across every workspace that consumes Compiled, with
   `onIncludedFiles` instrumented. Realpath-canonicalizes every included
   path. Fails if any path escapes the invocation cwd. This becomes a CI
   guardrail post-Phase 5.
7. **State-mutation enumeration (architecture-discovery gate).**
   Produce `crates/babel-plugin/STATE_MUTATIONS.md` listing every
   site in `packages/babel-plugin/src/utils/evaluate-expression.ts`
   and the `traverse_expression/` subtree where
   `state.includedFiles`, `state.compiledImports`, `state.sheets`,
   `state.cssMap`, or `state.ignoreMemberExpressions` is mutated.
   For each, record the proposed `StateDiff` variant that captures it
   (§3.9.8). Why this is in Phase 0, not Phase 5: the StateDiff enum
   shape is a *load-bearing architectural decision* — if real Babel
   has 30 distinct mutation kinds rather than the 5 sketched in
   §3.9.8, the cache replay design changes. Discover the count
   before architecture commits, not after. The enumeration also
   validates that compiler-enforced encapsulation is tractable (no
   mutation site is hidden inside an external library that we can't
   route through `MutationRecorder::apply`).
8. **Run all nine §3.9.14 probes** (WASI sync I/O, mtime,
   `transformSync` ABI, instance-teardown, race, scratch-dir
   reachability, postcard round-trip, byte-cap eviction,
   resolver difference matrix). Each is a hard gate; failures block
   Phase 1 (probes 1-8) or Phase 5 (probe 9).

**Exit gate.** Babel-against-itself round-trip is byte-stable across all
fixtures and across at least three machines (CI + two dev machines). The
audit script reports its raw count for every workspace using Compiled
(target: ≤100 outliers, refactor list captured). Pass-through SWC plugin
produces byte-equal output through the prettier oracle. **All nine
§3.9.14 probes pass on Linux, macOS, and Windows** — the
scratch-reachability probe (§3.9.14 #6) is the cheapest of the nine
and the most expensive to discover failing later, so it must be
landing-blocked. **`STATE_MUTATIONS.md` is complete and reviewed**;
the StateDiff enum shape in §3.9.8 has been reconciled with the actual
Babel mutation-site enumeration (any new variants needed are added to
this PLAN before Phase 2 starts). **Resolver difference matrix
(§3.9.14 #9)** captures every divergence between
`enhanced-resolve@5.x` / npm-`resolve` / `oxc_resolver` and
documents how the plugin's `oxc_resolver` configuration closes each
gap (or escalates if it can't).

**Effort.** 1–2 weeks.

---

### Phase 1 — `babel-plugin-strip-runtime` (smaller, validates the toolchain)

**Goal.** Port the simpler of the two plugins end-to-end. Validates the WASI
build, prettier oracle, sidecar manifests, SWC ABI pin, file writes via
preopen, both JSX runtimes (classic + automatic), Parcel wrapper plumbing.

Why this plugin first: 6 source files, ~600 LOC, no cross-file resolution,
no `transformCss` calls. It's the lightest path to an end-to-end working
toolchain.

Tasks (in order):

1. Crate scaffold: `crates/babel-plugin-strip-runtime/`, `Cargo.toml` with
   `swc_core` and `serde_json` deps, `wasm32-wasip1` target.
2. Port `to_uri_component.rs` — pure function, trivial. URL-encode + escape `!`
   to `%21`.
3. Port `is_automatic_runtime.rs`, `is_cc_component.rs`,
   `is_create_element.rs` — predicate helpers.
4. Port `remove_style_declarations.rs` — extracts CSS strings from
   `<CS>{...}</CS>` declarations via scope binding lookup. SWC scope
   resolution differs from Babel; this is the first non-trivial port. Use
   `compat/scope.rs` (created here) for binding lookup.
5. Port `lib.rs` (entry + dispatcher visitor): `Program::exit`,
   `ImportSpecifier`, `JSXElement`, `CallExpression`. Lock the
   `Program::exit` ordering exactly: (a) emit version-banner comment, (b)
   `preserveLeadingComments` equivalent, (c) inject `require()` statements
   OR write `.compiled.css` OR emit metadata sentinel — never two of the
   three.
6. Sidecar handlers:
   - `compiledRequireExclude=true` SSR mode → write
     `<scratch>/style-rules.json`. Host strips and exposes as
     `result.styleRules` (which Parcel transformer assigns to
     `asset.meta.styleRules`).
   - `extractStylesToDirectory.dest` → `mkdir_all` + `write` via
     `/cwd`-preopen. Validate `dest` is inside the preopen at plugin entry;
     if not, emit a clear error.
7. Lock `plugins/SIDECAR_SCHEMA.md` v1 (see §7).
8. Inline the wrapper in `packages/parcel-transformer/` matching the shape
   in `plugins/PARCEL_USAGE_EXAMPLE.md`. Specifically, the `transform()`
   function's middle block (lines 146–197) becomes an SWC invocation; the
   sidecar drain replaces `metadata.styleRules` and the `includedFiles`
   array.

**Exit gate.** All 38 existing strip-runtime tests pass through the parity
harness. Run the harness across an additional 1000-file sample of
synthesized fixtures (already-baked Compiled output passed through `bake`
first) — zero divergence. Parcel transformer round-trip on a real consuming
project produces byte-equal `.compiled.css` files and byte-equal asset code.

**Effort.** 2 weeks.

---

### Phase 2 — `babel-plugin` scaffold + dispatcher

**Goal.** Stand up the visitor skeleton and confirm pass-through is
byte-equal before porting any handler logic.

Tasks:

1. Crate scaffold mirroring `packages/babel-plugin/src/`'s file tree. Every
   file is created with a header comment
   (`// Ports packages/babel-plugin/src/<path>.ts`) and a stub.
2. Port `types.rs`, `constants.rs` first (data only, no logic).
3. `lib.rs` entry + top-level `babel_plugin.rs` visitor:
   - `Program::enter` (pragma detection — `@jsx`, `@jsxImportSource` regex
     scanning of leading comments; state init).
   - `Program::exit` (runtime imports, cleanup queue, version banner).
   - `ImportDeclaration` (Compiled API detection + specifier removal).
   - `TaggedTemplateExpression | CallExpression` (dispatch — stubbed).
   - `JSXElement` (ClassNames dispatch — stubbed).
   - `JSXOpeningElement` (xcss/css prop dispatch — stubbed).
4. Each per-API handler is initially a no-op that records "would have
   visited" in a debug log and leaves the AST unchanged.
5. State struct (`State` in Rust) with `IndexMap` everywhere —
   `compiled_imports`, `css_map`, `sheets`, `ignore_member_expressions`.
   **All these fields are `pub(self)`; the only public mutator is
   `MutationRecorder::apply` (§3.9.8).** Layer 5 of this scaffold is
   compiler-enforced state encapsulation; subsequent phases never
   touch these fields directly.
   **Cache lifetime is governed by §3.9, not by Babel's `opts.cache`
   flag.** Plugin-static state is impossible at this SWC version
   (instance teardown per call, see §3.9.0). The per-call `State`
   carries an in-memory Layer 2 `HashMap` populated from
   `<workerScratchDir>/cache.bin` (postcard) on `Program::enter`
   and serialized back on `Program::exit` *only if mutated*. Layer 1
   is in-memory only (never persisted). `cacheLevel: "ast" |
   "value" | "off"` controls which layers are active; "value" is the
   default and is the mode that beats Babel's effective behaviour.

**Exit gate.** Plugin runs as a no-op across every fixture and produces
byte-equal output through the prettier oracle. Confirms visitor wiring,
state setup, scope traversal, comment preservation in pass-through.

**Effort.** 1 week.

---

### Phase 3 — Hash compatibility (`@compiled/utils.hash`)

**Goal.** Adopt the Rust `hash` function the parallel CSS-port agent
ships in `crates/compiled-utils`, and prove it is byte-identical to
the JS `@compiled/utils.hash` from this plugin's perspective. **This
gates everything downstream.**

Per the user's direction, the parallel CSS port lands its `hash`
implementation **before** this plugin starts Phase 3. That collapses
the scope of this phase from "port the hash function" to "consume the
shared crate and prove parity from the consuming side." Both ports
must depend on exactly the same crate symbol — there is no per-port
copy of the hash.

Tasks:

- Confirm `crates/compiled-utils` exports `pub fn hash(input:
  &str) -> String` (or the matching signature) and that the
  CSS-port crate already depends on it. **Single source of truth;
  no per-plugin reimplementation.**
- Confirm the JS `@compiled/utils.hash` source has not drifted vs
  what the Rust port replicates; if it has, the parallel agent owns
  closing the gap before Phase 3 exits.
- Build a corpus of `(input string, expected hash)` test vectors
  covering at minimum: ASCII, UTF-8 multibyte, empty string, string
  with embedded NUL, very long strings (>4KB), strings with
  leading/trailing whitespace, and the exact inputs we see in
  practice for keyframe expressions and CSS variable names. Test
  vectors come from running the JS `hash` against the inputs in
  CI; freeze the results.
- Add the corpus as a parity test in `crates/babel-plugin/tests/`
  (NOT in `crates/compiled-utils/` — we want this plugin's own
  consumer-side guarantee, independent of the parallel agent's
  internal tests).

**Exit gate.** The shared Rust `hash` produces byte-identical output
for 100% of the test-vector corpus, plus 10K random inputs generated
by `cargo-fuzz` and diffed against a Node subprocess running JS
`hash`. **Zero divergence.** If divergence is found, Phase 3 cannot
exit until the parallel agent has fixed the shared crate.

**Effort.** 2–3 days (was 1 week — the actual port work is already
done by the parallel agent; this phase is now a parity-check on a
shared dependency).

---

### Phase 4 — `buildCss` with direct synchronous `transformCss` Rust call

**Goal.** Port the CSS extraction subsystem (`utils/css-builders.ts`,
~1145 LOC) and wire it to the parallel-agent's Rust `transform_css`
implementation as a direct synchronous Rust call (§3.5). One pass, one
visit, no scan/apply scaffolding.

**Hard preconditions (do not start Phase 4 without all four):**

1. Phase 3 hash parity is green and the shared `crates/compiled-utils`
   crate exposes the hash function as a `pub fn` consumable by this
   plugin.
2. The parallel CSS port has shipped a `pub fn transform_css(css: &str,
   opts: &TransformCssOpts) -> TransformCssResult` callable as a normal
   Rust function. Output must be byte-identical to the JS `transformCss`
   for the entire fixture corpus the parallel agent uses; the
   integration parity test (Phase 4 task 0 below) confirms this from
   the babel-plugin's vantage point.
3. `compat/generator.rs` coverage manifest is complete (Phase 4 task 1
   below).
4. Cardinal rule: **no temporary scaffold.** If a precondition is not
   ready, Phase 4 blocks. Do not add a NAPI bridge "just for now" —
   that path was deliberately removed from this plan.

Tasks (load-bearing, in order):

0. **`transform_css` integration parity.** Wire the Rust
   `transform_css` into a small driver in `crates/babel-plugin/tests/`
   that calls it for every input the JS `transformCss` is run against
   in the existing test corpus. Diff outputs byte-for-byte (sheets,
   class names, rules, embedded hash strings). Zero divergence. This
   gate is Phase 4's first commit; it confirms precondition #2 holds
   from this plugin's perspective and *not* just from the parallel
   agent's.

1. **`compat/generator.rs` coverage manifest (entry gate).** Build
   `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` enumerating
   every AST node type that can legitimately appear inside an
   expression we feed into `generate(...)`. The two consumers are:
   - **Keyframe name input:** `hash(generate(expression).code)` —
     `expression` is whatever the user puts inside `keyframes(...)`.
     Grep the consuming monorepo's `keyframes(...)` call sites and
     classify the AST shapes: identifiers, member chains, calls,
     tagged templates, conditionals, arrow functions, JSX (yes, it
     happens), template literals, object/array literals, binary
     ops, sequence ops, `as` / `satisfies` casts, parenthesized
     expressions.
   - **Any other call sites in `css-builders.ts` that route through
     `generate(...)`** — enumerate them all and add to the manifest.
   For each node kind: record the Babel printer output for a
   representative sample, the corresponding port in
   `compat/generator.rs`, and a parity fixture under
   `tests/compat-generator/`. The manifest is an exhaustive checklist;
   any node kind reached at corpus time that is not on the list is a
   port defect, not a missing-feature ticket.

2. Port `utils/css_builders.rs` line-for-line. Hot spots:
   - **Keyframe name generation:** `k${hash(generate(expression).code)}`.
     Implementation lives in `compat/generator.rs` per task 1.
   - **CSS variable names:** `--_${hash(name)}`.
   - **`invalidDynamicIndirectSelectorRegex`** — replace with `regex` crate
     equivalent and corpus-test.
   - **`contentValuePattern`** for CSS `content` property normalization.
   - **`transformCss` call sites:** call the Rust `transform_css`
     directly. Any reshaping of inputs/outputs to match Babel's exact
     call shape stays in this plugin (do not modify the parallel
     agent's API to suit this plugin).

3. Port `utils/transform_css_items.rs` and `utils/build_css_variables.rs`.

4. Update `lib.rs` so the visitor performs both extraction and
   substitution in a single traversal. No `mode: 'scan' | 'apply'`,
   no css-requests.json, no marker tokens. The visitor's call sequence
   on hitting a `css({...})` site is: build CSS string → call
   `transform_css` → splice the resulting sheets/classNames into the
   output AST.

5. Update the Parcel wrapper (§8) to a single `transformSync` call.
   The wrapper modifications can be invasive — the host is not part of
   the parity surface (constraint 8).

**Exit gate.** All fixtures exercising `keyframes`, `css`, and `cssMap`
APIs (the call sites that route through `transform_css`) pass the parity
harness end-to-end with a single `transformSync` call per file. The
`compat/generator.rs` coverage manifest is reviewed and signed off; the
parity-harness keyframe-name corpus is byte-clean.

**Effort.** 3–4 weeks (was already 3–4 weeks; saving the two-pass work
is offset by the explicit `compat/generator.rs` enumeration).

---

### Phase 5 — In-plugin resolver and expression evaluator

**Goal.** Port `utils/resolve_binding.rs` and the entire
`traverse_expression/` subtree. This is what reads other files via the WASI
preopen and statically evaluates imported values.

Tasks:

0. **Confirm `STATE_MUTATIONS.md` is current.** The artefact was
   produced in Phase 0 (architecture-discovery gate). Re-run its
   underlying enumeration (`grep -nE 'state\.(includedFiles|
   compiledImports|sheets|cssMap|ignoreMemberExpressions)\b'` over
   `packages/babel-plugin/src/utils/evaluate-expression.ts` and the
   `traverse_expression/` subtree) and confirm every match is still
   accounted for. If upstream Babel has mutated since Phase 0,
   amend `STATE_MUTATIONS.md` and any new `StateDiff` variants
   *before* porting touches the affected files. Pair with the §3.9.8
   compiler-enforced encapsulation (`State` fields private; mutation
   only via `MutationRecorder::apply`); together they make
   missed-mutation-capture a Rust compile error rather than a silent
   runtime divergence. Any port commit that introduces a state
   mutation not enumerated here is rejected at review.
1. **Land the ~100-file refactor in the consuming monorepo.** Until it's
   merged, the in-plugin resolver cannot replace Babel safely. Block this
   phase on it.
2. `utils/cache.rs` — port the LRU cache. JS uses a `Map`-based LRU keyed by
   `hash(namespace + cacheKey)`. Rust impl: `lru::LruCache` wrapped in
   `Mutex`, plus an `IndexMap` for `'file-pass'` mode. Match eviction order
   and key derivation exactly. Layer 1 is in-memory only (§3.9.8); Layer 2
   persists to disk via §3.9.10's postcard format. Enforce both
   `MAX_ENTRIES` and `MAX_CACHE_BYTES` caps (§3.9.11).
3. `utils/resolve_binding.rs`:
   - Use `oxc_resolver` (constraint 2, §1) for module resolution against the
     WASI preopen. Configure with `ResolveOptions { extensions: ... }`
     matching `opts.extensions` (default `['.js', '.jsx', '.ts', '.tsx']`).
   - Resolution that escapes the preopen returns `Err`; the plugin treats
     this as "binding not statically resolvable" — same fallback as today's
     JS plugin. The 100-file refactor + CI guardrail mean this should never
     fire in practice; if it does, it's a regression to fix.
   - Honour `opts.parserBabelPlugins` for source files that need TS/Flow/JSX
     parsing.
   - **Custom JS resolver (`opts.resolver` as function) is not supported**
     in this phase (constraint 1, §1). Add a config-time error if it's a
     function.
4. Port the `traverse_expression/` subtree file-for-file. Order: leaves
   first (`traverse_unary_expression`, `traverse_identifier`), then
   `traverse_binary_expression`, then `traverse_call_expression` and
   `traverse_member_expression/`.
5. Port `traversers/` — the import/export traversal helpers used by the
   evaluator.
6. Port `evaluate_expression.rs` — the entry point that ties together
   resolver + traversers.
7. Wire `includedFiles` accumulation. Every file the resolver opens gets
   pushed to a per-pass set. On `Program::exit`, write
   `<scratch>/included-files.json`. Host (Parcel transformer wrapper) reads
   it and passes to `asset.invalidateOnFileChange(file)` for each entry.
8. Promote `scripts/audit-included-files.ts` to a CI guardrail that fails
   any PR re-introducing outside-cwd resolution.

**Exit gate.** All `module-traversal.test.ts` (21k) and
`expression-evaluation.test.ts` (14k) cases pass. `resolver.test.ts` passes
**except** any case using `opts.resolver` as a function (those are
explicitly skipped with a recorded TODO referencing constraint 1).
`included-files.json` content matches what Babel's `onIncludedFiles` would
have reported, after realpath canonicalization. CI guardrail is green.
**STATE_MUTATIONS.md is complete and the
`grep -nE 'state\.[a-z_]+\.(push|set|add|insert|remove|extend)' \
   crates/babel-plugin/src --include '*.rs' \
   | grep -v 'src/state.rs\|src/mutation_recorder.rs'`
pre-commit lint returns zero matches** — i.e., every state mutation in
the Rust port goes through `MutationRecorder::apply`. The
`MutationRecorder` test suite shadows live evaluation against
diff-replay for the full `expression-evaluation` corpus and reports
zero replay/live divergences before the phase exits.

**Effort.** 4 weeks.

---

### Phase 6 — Per-API handlers (ascending complexity)

**Goal.** Port each Compiled API's handler. Order is least-risk first; each
gates on its own subset of the parity corpus.

| Order | API | Sources | Notes |
|---|---|---|---|
| 6a | `keyframes` | `babel-plugin.ts` cleanup branch | Replace-with-null only, no real generation. |
| 6b | `css` (utility) | `babel-plugin.ts` cleanup branch | Replace-with-null only. |
| 6c | `cssMap` | `css_map/mod.rs`, `process_selectors.rs` | First handler that actually emits CSS. |
| 6d | `xcss-prop` | `xcss_prop/mod.rs` | Reads `state.cssMap`, transforms in place. |
| 6e | `css-prop` | `css_prop/mod.rs` | Wraps parent JSXElement; comment placement is sensitive. |
| 6f | `ClassNames` | `class_names/mod.rs` | Children traversal, `style` prop rewriting. |
| 6g | `styled` | `styled/mod.rs`, `utils/build_styled_component.rs` | Largest handler. `forwardRef` injection, prop validation via `@emotion/is-prop-valid` (port that table verbatim). |

For each: port handler, run its dedicated test suite under
`<feature>/__tests__/` through the parity harness. Move on only after that
band is byte-clean.

**Exit gate.** All ~50 babel-plugin tests pass through the parity harness.
Cross-handler tests (`__tests__/index.test.ts`, 14k) pass. JSX
automatic-runtime fixtures pass. Custom-import-source fixtures pass.

**Effort.** 4–5 weeks (wide variance — `styled` alone is ~1.5 weeks).

---

### Phase 7 — Comment placement and `Program::exit` ordering

**Goal.** Hunt the long tail of comment-attachment divergences.

Comments are the most likely source of post-prettier divergence because Babel
and SWC attach them to subtly different nodes. Specific concerns:

- **Version banner.** `path.addComment('leading', ' ${filename} generated by ${packageJson.name} v${version} ')`
  followed by `path.unshiftContainer('body', t.noop())`. SWC has no `Noop`.
  Insert an empty statement (`;`) and verify prettier preserves the leading
  comment on it; if prettier strips the empty statement, reattach the
  comment to the first real body statement.
- **`preserveLeadingComments` (from `@compiled/utils`).** Port exact
  semantics — it shifts leading comments off the first body statement to
  the Program node before mutations, then restores. The shift point matters.
- **`appendRuntimeImports` order.** `unshiftContainer('body', ...)` calls
  happen in a specific sequence. Mirror exactly: React import (if needed)
  → `forwardRef` → runtime imports → version banner. Out-of-order
  insertion = different prettier output for import blocks.
- **`@compiled-disable-line` / `@compiled-disable-next-line` directives.**
  These live in source comments. Verify the plugin's directive-detection
  attaches to the same node Babel does.

Tasks:

- Build a "comment-shape diff" tool: parse both prettier outputs back with
  `@babel/parser`, walk the comment array, compare attachment by node type
  + position. Use it to triage failures.
- Fix comment-related divergences by adjusting visitor mutation order or
  by using SWC's `Comments` collection directly when needed.

**Exit gate.** Full corpus through parity harness — zero divergence on
comment placement or attachment.

**Effort.** 1–2 weeks (mostly debugging).

---

### Phase 8 — Corpus diff at scale and rollout gate

**Goal.** Prove byte-equality on real-world inputs at volume before flipping
any consumer.

Tasks:

- Run the parity harness across the 100k+ Compiled call sites in the
  consuming monorepo (every workspace that imports `@compiled/react`).
  Capture every divergence; treat each as a blocking bug.
- Stand up `cargo-fuzz` targets that synthesize plausible Compiled inputs
  (random JSX with random `css({...})` and `styled.X` patterns) and assert
  parity-harness equality.
- Shadow mode in CI: real builds use Babel; SWC runs in parallel; hash both
  outputs; alarm on divergence; no production impact yet.

**Exit gate.** Two consecutive weeks of zero divergence on full corpus and
shadow-mode CI runs. Fuzzing finds no new divergence patterns in 72 hours
of continuous runtime.

**Effort.** 2–3 weeks (calendar).

---

### Phase 9 — Rollout

1. Engine flag default: Babel.
2. Ship Rust artefacts: `napi build` for the platform set (linux-x64-gnu,
   linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc). Each
   platform binary is a separate parity surface; verify on each.
3. Internal opt-in via env var (`COMPILED_TRANSFORMER=swc`).
4. Hash-shadow in production: compute SWC output, hash it, compare to
   Babel hash, log divergence. Don't use SWC output yet.
5. After N weeks of zero divergence in production traffic, flip default to
   SWC.
6. Babel pipeline stays in tree as the parity oracle for ≥1 year.

**Effort.** 6+ weeks calendar (mostly waiting).

---

## 6. Hazard register

| Hazard | Why it bites | Mitigation |
|---|---|---|
| **swc_core ABI ↔ @swc/core@1.15.8 mismatch** | Wrong pin → plugin rejected at load time. | Cross-check the SWC compatibility matrix; pin in `PARITY_VERSIONS.md`; CI rebuilds on every `@swc/core` bump. |
| **`@compiled/utils.hash` not bit-identical** | Every class name renames; corpus diff explodes. | Phase 3 standalone gate before any css-builders work. |
| **Comment attachment divergence** | Prettier preserves comments verbatim including attachment node. Even matched text can differ if attached to a different child. | Phase 7 dedicated; comment-shape diff tool; reproduce with minimal fixtures before fixing. |
| **`@babel/generator` vs SWC printer for hash inputs** | Keyframe name uses `hash(generate(expression).code)`. SWC's printer differs from Babel's for the same AST. | `compat/generator.rs` matching Babel byte-for-byte for the relevant AST subtrees. |
| **`t.noop()` has no SWC equivalent** | Anchor for the leading version-banner comment. | Empty statement + verify under prettier; fall back to attaching the comment to the first real body statement. |
| **WASI preopen regression** | A new file added to a workspace imports something outside cwd. Plugin breaks at runtime. | CI guardrail (`audit-included-files.ts`) blocks the PR. Realpath canonicalization is mandatory. |
| **Symlink target outside preopen** | cap-std denies; resolver falls back to "not statically evaluable"; class names silently change. | Same audit script — its realpath check catches symlink-out-of-cwd before merge. |
| **`opts.resolver` (JS function)** | Cannot run inside WASI; not portable. | Constraint 1 (§1) explicitly drops support; documented; user has agreed to revisit later. |
| **`onIncludedFiles` semantics** | Production callers (Parcel) drive HMR invalidation off it. Wrong list → stale builds. | Sidecar JSON; host calls `asset.invalidateOnFileChange` with the same array shape Babel produced; canonicalize paths the same way Babel did (no realpath at this stage — it matches Babel's behavior). |
| **`compiledRequireExclude` SSR mode → `file.metadata.styleRules`** | SWC has no `file.metadata`. Parcel transformer reads `metadata.styleRules` (see `PARCEL_USAGE_EXAMPLE.md` line 191). | Sidecar JSON; host re-exposes as `result.styleRules`; transformer assigns to `asset.meta.styleRules` identically. |
| **`process.env.TEST_PKG_VERSION` and version banner** | Banner contains plugin name + version. SWC plugin has a different name. | Plugin emits the **same** banner string Babel does (`@compiled/babel-plugin v…`), reading the version from a shared workspace constant. Bit-parity over self-identification. |
| **Parallel test workers and scratch dirs** | Multiple SWC compiles in flight per process. | `mkdtemp` per-compile; never share. |
| **Cross-platform path canonicalization** | cap-std on Windows handles symlinks/junctions differently than POSIX. | All sandbox-bound logic tests run on both Linux and Windows in CI. |
| **`opts.cache` semantics vs Wasm instance lifetime** | Babel's `globalCache` is misnamed; reassigned per-pre-hook, never read elsewhere — effectively per-file. Wasm instance is torn down per `transform()` call (verified `swc_plugin_runner@23.0.0`). Plugin-static cross-file state is impossible. | §3.9 fully specifies the resolution: per-worker scratch dir + `cache.bin` + `swc.transformSync` mandate + mtime invalidation + two-layer (AST / evaluated-value) design. Output bytes never change with cache hit/miss; cache is perf-only. |
| **`enhanced-resolve` ↔ `oxc_resolver` divergence** | Production callers wrap `enhanced-resolve@5.x` in `createDefaultResolver`; this plugin replaces it with in-plugin `oxc_resolver`. Any divergence on `package.json#exports`, `tsconfig` paths, symlink realpath, or extension order = class names diverge in production. | §3.9.14 #9 resolver difference matrix is a hard Phase 0 gate. The plugin's `oxc_resolver` configuration must close every gap the matrix surfaces, or the gap is escalated. Phase 5 cannot start until the matrix is green. |
| **Layer 2 state-diff replay incompleteness** | `evaluateExpression` mutates `state.includedFiles`, `state.compiledImports`, `state.sheets`, `state.cssMap`. Caching the evaluated value but missing a mutation = silent HMR breakage. | §3.9.8 `MutationRecorder` pattern centralizes capture; §3.9.15 shadow-eval parity test runs for ≥1 month post-ship; `cacheLevel: "ast"` rip-cord falls back to Layer 1 only. |
| **Layer 2 transitive-dep miss** | Cached entry's value depends on a deeper file that mtime-check didn't cover. Stale hit → wrong output. | §3.9.8 resolver-hooked transitive-dep capture: every `fs::read` during eval auto-appends to the entry's `transitive_deps`. §3.9.15 transitive-dep test gates the design. |
| **mtime unreliable on some filesystems** | Docker bind mounts, networked drives, FAT32. | `opts.cacheInvalidation: 'mtime' \| 'content-hash'` (§3.9.12) — content-hash uses xxhash3 of source bytes, slower but resolution-independent. |
| **`swc.transformSync` removed in a future SWC release** | §3.9.4 is a hard contract; if `transformSync` disappears the cache strategy breaks. | Phase 0 ABI gate (§3.9.14) imports `transformSync` and asserts it's a function — fails loudly at version-bump time. |
| **Babel bug compatibility** | "Fixing" a bug while porting renames classes in production. | Constraint 6 (§1): **bugs are features.** Every behavioural difference under the parity harness is a port defect, not a bug fix opportunity. |
| **Scratch dir outside WASI cwd preopen** | An earlier draft put `workerScratchDir` under `os.tmpdir()`; the WASI sandbox at `@swc/core@1.15.8` only preopens `current_dir()`, so that path is unreachable from inside the plugin. Cache + sidecar I/O would silently fail. | §3.9.5 puts scratch at `<projectRoot>/node_modules/.cache/compiled-swc/worker-<pid>/`. §3.9.14 #6 is a hard Phase 0 probe. Documented fallback chain. JS host MUST NOT use `mkdtempSync(tmpdir(), ...)`. |
| **Cache file size unbounded on 60-90 GB monorepo** | An earlier draft persisted Layer 1 (full `swc_ecma_ast::Module` serde-JSON) per-file. Worker reading/writing tens of MB of JSON per `transform()` would dwarf any saved parse cost. | §3.9.0/§3.9.8/§3.9.10 redesign: Layer 2 only on disk, postcard binary format, 5 MiB hard cap, 500-entry cap, 32-tdep-per-entry cap, dirty-bit short-circuits no-op writes. Layer 1 stays in-memory per transform. |
| **`compat/generator.rs` coverage gap** | Keyframe-name input is `hash(generate(expression).code)`. SWC's printer differs from Babel's; we port a Babel-equivalent printer. If a single AST node kind reachable via `keyframes(...)` is missing or wrong, that one keyframe renames in production — the kind of bug that's individually low-blast-radius but collectively erodes confidence. | Phase 4 entry gate produces `COMPAT_GENERATOR_COVERAGE.md` enumerating every node kind reachable from `keyframes(...)` call sites in the consuming monorepo, plus parity fixtures per kind. Any node kind reached in the corpus that is not on the list = port defect. No half-bakes (constraint 5). |
| **State mutation site missed by `MutationRecorder`** | A single `state.compiledImports.set(...)` not routed through the recorder = silent HMR breakage; bytes match (state isn't in the AST), corpus diff is clean, production is wrong. | §3.9.8: make `State` fields `pub(self)`; only public mutator is `MutationRecorder::apply`. Phase 0 produces `STATE_MUTATIONS.md`; Phase 5 task 0 reconciles it; pre-commit grep lint catches new sites. §3.9.15 shadow-eval gates the rip-cord removal. Together: missed mutation = compile error or pre-commit reject. |

---

## 7. Sidecar manifest schema

Lock these in `plugins/SIDECAR_SCHEMA.md` during Phase 1. Sketched here so
the plan stands alone.

```jsonc
// <callScratch>/included-files.json
// Written by: babel-plugin
// When: Program::exit, if includedFiles non-empty
{
  "version": 1,
  "files": ["<absPath>", ...]   // pre-realpath; matches Babel's onIncludedFiles
}

// <callScratch>/style-rules.json
// Written by: babel-plugin-strip-runtime
// When: Program::exit, if compiledRequireExclude=true and styleRules non-empty
{
  "version": 1,
  "rules": ["<css rule string>", ...]
}

// <workerScratchDir>/cache.bin
// Written by: babel-plugin
// When: Program::exit, ONLY IF cache_dirty == true (atomic via temp+rename)
// Lifetime: worker process; see §3.9 for full spec
// Format: postcard (NOT JSON). Layer 2 only; Layer 1 is in-memory
// per-transform. Hard caps: 500 entries, 5 MiB serialized.
// (full schema in §3.9.10)
//
// Pseudo-shape (Rust struct, postcard-encoded):
// CacheFile {
//   version: u32,
//   schema_hash: [u8; 32],
//   layer2: Vec<(u64, Layer2Entry)>,
// }

// Plugin config (in-memory, passed via @swc/core experimental.plugins[i][1])
// Read by: babel-plugin
{
  "workerScratchDir": "<absPath>",   // §3.9.6 — stable per worker, holds cache.bin
  "callScratch":      "<absPath>",   // §3.9.6 — fresh per call, holds sidecars
  "cacheLevel":       "value" | "ast" | "off",  // default "value"
  // ... rest matches packages/babel-plugin types.PluginOptions
  // (minus opts.resolver, which is unsupported per constraint 1) ...
}
```

Versioned. Mismatch = hard error.

---

## 8. Integration shape (the production call site)

The Parcel transformer is the production caller. Today's shape (full source
in `plugins/PARCEL_USAGE_EXAMPLE.md`) calls `transformFromAstAsync` from
`@babel/core` with both Compiled plugins in series. Lines 146–197 are the
load-bearing block:

```ts
// (excerpted from PARCEL_USAGE_EXAMPLE.md)
const result = await transformFromAstAsync(ast.program, code, {
  // ... babel options ...
  plugins: [
    asset.isSource && ['@compiled/babel-plugin', { ..., onIncludedFiles, resolver, cache: false }],
    extract && ['@compiled/babel-plugin-strip-runtime', { compiledRequireExclude: true, ... }],
  ].filter(toBoolean),
});

includedFiles.forEach((file) => asset.invalidateOnFileChange(file));

if (extract) {
  asset.meta.styleRules = [
    ...((asset.meta?.styleRules as string[]) ?? []),
    ...(result?.metadata as BabelFileMetadata).styleRules ?? [],
  ];
}
```

Our SWC-equivalent shape (inlined in `packages/parcel-transformer/`).
**See §3.9 for the cache strategy this snippet implements** —
worker-scoped persistent cache, per-call disposable sidecars,
`transformSync` mandate.

```ts
import { transformSync } from '@swc/core';   // §3.9.4 — sync, NOT async
import { mkdirSync, readdirSync, rmSync, existsSync } from 'fs';
import { join } from 'path';
import { randomUUID } from 'crypto';

// Module-load (worker init), once per worker process.
// Lives for the entire worker lifetime; NEVER re-created per transform.
// §3.9.5 + §3.9.13.1.
//
// CRITICAL: workerScratchDir must live UNDER projectRoot so it falls
// inside the WASI cwd preopen. tmpdir() is unreachable from the plugin.
const projectRoot = process.cwd();   // §3.9.13.7 — must be monorepo root
const cacheRoot = pickCacheRoot(projectRoot);
  // → <projectRoot>/node_modules/.cache/compiled-swc/  (preferred)
  // fallback: <projectRoot>/.compiled-swc-cache/
  // fallback: <projectRoot>/.cache/compiled-swc/
  // hard error otherwise (do NOT silently fall back to tmpdir).
const workerScratchDir = join(cacheRoot, `worker-${process.pid}`);
mkdirSync(workerScratchDir, { recursive: true });
// Sweep stale tmp files from any prior crashed worker that reused this pid.
for (const f of readdirSync(workerScratchDir).filter(n => n.endsWith('.tmp'))) {
  rmSync(join(workerScratchDir, f), { force: true });
}
process.on('exit', () => {
  try { rmSync(workerScratchDir, { recursive: true, force: true }); } catch {}
});
// (also wire SIGINT/SIGTERM handlers if Parcel's worker harness requires it)

// Inside transform({ asset, config, ... }):
const callScratch = join(workerScratchDir, `call-${randomUUID()}`);   // §3.9.13.2
mkdirSync(callScratch);
try {
  // Single-pass: the Rust transformCss is linked into the plugin
  // directly, so one transformSync call covers extraction +
  // substitution + (optionally) strip-runtime. SYNC required by §3.9.4.
  const result = transformSync(code, {
    filename: asset.filePath,
    jsc: { experimental: { plugins: [
      ['@compiled/swc-plugin', {
        ...config,
        workerScratchDir,                  // §3.9.6 — stable per worker, holds cache.bin
        callScratch,                       // §3.9.6 — fresh per call
        cacheLevel: config.cacheLevel ?? 'value',
      }],
      extract && ['@compiled/swc-plugin-strip-runtime', {
        compiledRequireExclude: true,
        extractStylesToDirectory: config.extractStylesToDirectory,
        callScratch,                       // strip-runtime only needs callScratch
      }],
    ].filter(Boolean)}},
  });

  // Drain disposable sidecars. cache.bin is plugin-owned — DO NOT TOUCH IT. §3.9.13.5/8
  const includedFiles =
    readJsonIfExists(join(callScratch, 'included-files.json'))?.files ?? [];
  const styleRules =
    readJsonIfExists(join(callScratch, 'style-rules.json'))?.rules ?? [];

  includedFiles.forEach((file) => asset.invalidateOnFileChange(file));
  if (extract) {
    asset.meta.styleRules = [
      ...((asset.meta?.styleRules as string[]) ?? []),
      ...styleRules,
    ];
  }

  return { code: result.code, map: result.map };
} finally {
  // Per-call scratch only. workerScratchDir survives until worker exit.
  rmSync(callScratch, { recursive: true, force: true });
}
```

Note: per constraint 8 in §1, this wrapper can be modified freely —
the existing Parcel transformer already calls `parseAsync` upstream
and feeds a Babel AST to `transformFromAstAsync`, which SWC does not
consume. The migration drops the upstream `parseAsync` and feeds
source bytes (`asset.getCode()`) directly to `swc.transformSync`.
The plugin is the parity surface; the wrapper is not.

Things the host wrapper MUST get right (recap of §3.9.13, surfaced
here because the wrapper is where bugs of this kind hide):

- **`workerScratchDir` is module-scoped** — created once per worker
  process, NOT once per transform. Re-creating it per call wipes the
  cache between every file and silently degrades perf to
  `cacheLevel: "off"` while looking healthy.
- **`workerScratchDir` lives under `<projectRoot>/node_modules/.cache/`**
  — never under `os.tmpdir()`. `/tmp` is outside the WASI cwd preopen
  at `@swc/core@1.15.8` and the plugin cannot open files there. The
  Phase 0 reachability probe (§3.9.14 #6) is the gate that catches
  this; if it fails, fix the path, do not try to widen the preopen
  (no JS knob exists to do so at this version).
- **`transformSync`, not `transform`.** Using async releases libuv
  threads to race on `cache.bin` (§3.9.4).
- The host never reads or writes `cache.bin` itself
  (§3.9.13.8). Plugin-owned, postcard binary format. Host's only
  contact is the `process.on('exit')` cleanup.
- Worker spawn cwd must cover both `workerScratchDir` and the
  consuming workspace's source tree under one preopen
  (§3.9.13.7) — typically monorepo root, not package root.

---

## 9. Effort summary

| Phase | Description | Calendar weeks (1 strong eng) |
|---|---|---|
| 0 | Parity harness + audit + version pins + STATE_MUTATIONS.md + resolver matrix | 2 |
| 1 | strip-runtime end-to-end | 2 |
| 2 | babel-plugin scaffold + dispatcher | 1 |
| 3 | consume shared Rust hash + parity tests | 0.5 |
| 4 | css-builders + compat/generator coverage manifest + direct Rust transformCss call | 3–4 |
| 5 | resolver + traverse-expression subtree | 4 |
| 6 | per-API handlers (keyframes → styled) | 4–5 |
| 7 | comment placement debug | 1–2 |
| 8 | corpus diff at scale + fuzz | 2–3 |
| 9 | rollout (mostly calendar) | 6+ |
| **Total** | | **~25–30 weeks**, with 6+ being calendar wait |

Compresses with parallelization: Phases 4, 5 are partly independent and
can run alongside one another with two engineers. Phase 3 is now small
enough to fold into Phase 2's tail rather than running as a standalone
phase if calendar pressure is high.

**Dependency on the parallel CSS port:** Phase 4 cannot start until the
parallel agent ships (a) the Rust `hash` function and (b) the Rust
`transform_css` function as consumable crate exports (§3.5
preconditions). Phases 0–3 are independent of the CSS port and can
run in parallel with it.

---

## 10. Empirical anchors

These are the load-bearing measurements the architecture depends on. If they
change, re-evaluate the plan before continuing.

- **Outside-cwd `includedFiles` count, post-realpath:** ~100 across the
  consuming monorepo. To be refactored to zero before Phase 5 ships.
- **CI guardrail:** `scripts/audit-included-files.ts` enforces zero
  outside-cwd includes per workspace, on every PR. Runs in <2 minutes per
  workspace.
- **`@swc/core` ABI surface:** `Options.cwd` does not affect WASI preopens
  (verified against `swc_plugin_backend_wasmtime/src/lib.rs` and
  `swc_plugin_backend_wasmer/src/lib.rs`). The plugin's only FS access is
  the cap-std-gated preopen of `std::env::current_dir()`.
- **Workspace import patterns:** monorepo workspaces use isolated
  `node_modules` per package; cross-workspace imports symlink to sibling
  source. The audit handles this case via realpath canonicalization.

---

## 11. Cardinal rules (specific to this port)

Cardinal rules from `crates/PARITY_VERSIONS.md` apply. Additionally:

1. **Bytes after prettier are the contract.** Not "looks right." Not
   "passes tests." Bytes.
2. **Compiled class names live inside string literals.** Prettier preserves
   string contents. Therefore CSS hashing is part of the byte contract,
   even though it's "just data."
3. **No FS reads inside the plugin outside `/cwd`.** Ever. The cap-std
   layer enforces this; do not write code that tries to escape.
4. **No JS callbacks from the plugin.** All host-bound side effects go
   through sidecar JSON. Including `onIncludedFiles`, including
   `file.metadata.styleRules`, including future hooks.
5. **Don't bump `@swc/core` casually.** ABI breaks; the plugin won't load;
   the corpus diff must be re-run end-to-end.
6. **Don't bypass the parity harness.** Adding a test via `expect.toEqual`
   alone is not sufficient; the test must run under the harness with
   prettier-on-both.
7. **Keep Babel as the oracle for ≥1 year.** Don't delete the Babel
   pipeline until divergence rate is durably zero.
8. **Bugs are features.** Constraint 6 (§1). Behavioural differences under
   the parity harness are port defects, not bug-fix opportunities. If you
   spot a "real" bug, log it and continue replicating it.
9. **1:1 file mapping is enforced.** Constraint 4 (§1). If you feel the
   urge to deviate, stop and ask.
10. **No half-baked compat shims.** Constraint 5 (§1). If
    `crates/<plugin>/src/compat/<name>.rs` is incomplete, it will break in
    production. Finish it or escalate.

---

## 12. What to do when stuck

**Hard rule.** *If you cannot replicate something 1:1, stop your work
immediately and raise the issue. Do not invent a workaround silently.*
The cost of a wrong substitution at 10M-LOC monorepo scale dwarfs the
cost of a few hours of clarification.

Concretely:

- **Parity-harness divergence you can't explain in 2 hours:** stop. Capture
  the smallest reproducing fixture. Surface it.
- **Babel API with no SWC analogue:** check if a `compat/<name>.rs` already
  exists. If not, **don't invent one silently** — escalate so the user can
  decide on the shim's contract.
- **Constraint conflict** (e.g. honouring constraint 4 on file layout would
  require a Rust-illegal file name): escalate. Don't silently rename.
- **Performance regression vs Babel:** acceptable up to a point (we are
  paying for byte-parity, not speed, in this phase). Document, don't
  optimize prematurely.

The cost of stopping for a clarification is a few hours. The cost of a
silent wrong choice is months of debugging at 10M-LOC scale.
