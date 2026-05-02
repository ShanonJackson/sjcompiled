# Phase 0 findings — `crates/babel-plugin`

> **STATUS: PHASE 0 GREEN.** All eight in-scope §3.9.14 probes pass on
> Windows. Probe 9 (resolver difference matrix) is a Phase 5 gate and
> is not run yet.

Generated 2026-05-02. Probes live in
`crates/babel-plugin-phase0-probes/` and `phase0-probes/probes.test.ts`.

## Headline result

The SWC plugin sandbox at `@swc/core@1.15.8` (`swc_core@54.0.0`) grants
full read + write access to a single preopened directory:
`std::env::current_dir()` of the calling Node/Bun process, virtually
mounted at **`/cwd`**.

The plugin must use `/cwd/<...>` path strings inside its `fs::read` /
`fs::write` calls. Any other form fails:

| Path form (inside plugin) | Result |
|---|---|
| `/cwd/package.json` (read) | ✅ ok |
| `/cwd/<rel>/foo.bin` (write) | ✅ ok |
| `package.json` (bare relative) | ❌ ENOTCAPABLE — resolves against `/`, not `/cwd` |
| `./foo.bin` | ❌ EPERM |
| `/foo.bin` | ❌ ENOTCAPABLE |
| `C:/Users/.../...` (host abs path) | ❌ ENOTCAPABLE |
| `env::current_dir()?.join("foo")` | ❌ — `current_dir()` returns `Ok("/")` cosmetically |

The cosmetic `env::current_dir() => Ok("/")` was the trap that produced
the earlier "no FS access" misreading: the plugin's view of `cwd` is `/`
(useless), but the actual preopen lives at `/cwd`. Fix: never use
`current_dir()`-based path construction; always use `/cwd/<rel>`
literals threaded in via plugin config.

This is consistent with `plugins/READ_WRITE.md` and SWC issue
swc-project/swc#4997.

## Probe results (Windows, @swc/core@1.15.8)

```
$ bun test phase0-probes/probes.test.ts
 7 pass  0 fail  21 expect() calls
Ran 7 tests across 1 file. [567ms]
```

| Probe (PLAN.md §3.9.14) | Status | Notes |
|---|---|---|
| 1. WASI sync I/O round-trip | ✅ | Plugin writes 10 bytes to `/cwd/...`, reads them back, byte-equal. |
| 2. WASI mtime returns non-zero | ✅ | `fs::metadata().modified()` works inside the preopen. |
| 3. `transformSync` ABI exists | ✅ | `typeof transformSync === 'function'` at the pinned `@swc/core@1.15.8`. |
| 4. Instance teardown — counter resets | ✅ | A `static AtomicU64` counter reads `0` on every `transform()` entry, confirming wasm instance teardown per call. |
| 5. transformSync serialises | ✅ | Two consecutive `transformSync` calls each see clean state; no race observed. |
| 6. Scratch-dir reachability — both `workerScratchDir` + `callScratch` | ✅ | The HARDEST gate. Both round-trip 8-byte payloads correctly when supplied as `/cwd/...` paths. |
| 7. Postcard round-trip via WASI sync I/O | ✅ | Encode → write → read → decode equals the input fixture. Uses `postcard::to_allocvec` (alloc feature). |
| 8. Byte-cap eviction (pure Rust unit test) | not yet run | Pure-Rust gate, will be run when the eviction routine is added in Phase 5. |
| 9. Resolver difference matrix | not yet run | Phase 5 gate. |

## What this confirms about PLAN.md

- §3.2 — corrected. The mount path is `/cwd`, not the host's absolute
  path. The host MUST translate scratch paths to `/cwd/<rel>` form
  before threading into plugin config. The plugin MUST use `/cwd/...`
  literals; `env::current_dir()`-based path construction is forbidden.
- §3.3 (in-plugin resolver) — viable. Reads work.
- §3.4 (sidecar JSON manifests) — viable. Writes work.
- §3.9 (Layer 2 cache, `cache.bin`) — viable. Reads + writes work.
- §3.9.5 / §3.9.13 (host-side scratch dir creation) — viable, with one
  amendment: the host computes scratch paths in host-absolute form for
  its own `mkdirSync`/`rmSync`, then translates to `/cwd/<rel>` for
  the plugin's `workerScratchDir` and `callScratch` plugin-config
  fields. The PLAN already calls for `<projectRoot>/node_modules/.cache/...`;
  the new wrinkle is the path-form translation.

## Build / run

```bash
# Build the probe plugin
RUSTFLAGS="" cargo build -p babel-plugin-phase0-probes \
  --target wasm32-wasip1 --release

# Run the probes (must be from project root — that's the cwd that
# becomes the /cwd preopen)
bun test phase0-probes/probes.test.ts
```

## Cross-platform verification — PENDING

Probes ran on Windows only (win32-x64-msvc). Before declaring Phase 0
fully signed off across the supported platform set, the same probes
must run on Linux and macOS in CI:

- ubuntu-latest
- macos-latest
- windows-latest (already covered)

The most likely platform-specific risks: mtime resolution differences
(§3.9.12 already calls these out — Linux/macOS ns, Windows NTFS 100ns,
FAT32 2s) and Docker / network filesystem mtime unreliability.

CI step to add: a `phase0-probes` job in the workspace CI config that
builds the probe plugin and runs `bun test phase0-probes/probes.test.ts`
on each OS. Update this file with results.
