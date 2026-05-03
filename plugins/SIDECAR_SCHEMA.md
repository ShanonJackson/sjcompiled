# `plugins/SIDECAR_SCHEMA.md` — sidecar manifest schema (v1)

> Locked in Phase 1 §1.6. This file is the single source of truth for
> every disposable file the SWC plugins write and every persistent file
> they own. Both the Rust writers and the JS host parsers cite this
> document by section. Drift between writers and readers MUST be caught
> here, not in production.

## How to read this file

- **Path:** location relative to the host-supplied scratch root.
- **Owner:** which plugin writes it. Reading is the host wrapper.
- **Lifetime:** how long the file lives.
- **Encoding:** wire format. JSON for human-readable per-call sidecars,
  postcard binary for the persistent cache.
- **Schema:** the on-disk shape, locked at `version: 1`. A mismatch
  on read MUST hard-fail (sidecars) or wipe (cache). The
  cache-wipe-on-mismatch behaviour is documented inline; sidecars
  fail because the host requires them to drive HMR / SSR metadata.

Cardinal rule: **versioned + mismatch = loud failure.** The version
field is non-negotiable. A new field is a minor bump; a removed or
re-typed field is a major bump and a coordinated plugin/host release.

## Where files live

Per `plugins/PLAN.md` §3.9.5:

```
<projectRoot>/node_modules/.cache/sjcompiled-swc/
  worker-<pid>/                          # mkdir at worker init
    cache.bin                            # §3 below; persistent
    call-<uuid>/                         # mkdir per transform()
      included-files.json                # §1 below; per-call
      style-rules.json                   # §2 below; per-call
```

Two host-config knobs (PLAN.md §3.9.6) thread these paths into the
plugin:

- `workerScratchDir`: absolute path of `worker-<pid>/`. Stable for the
  lifetime of the worker. Only `cache.bin` lives here. Plugin-owned —
  the host's only contact is `rmSync` on worker exit.
- `callScratch`: absolute path of the per-call `call-<uuid>/` dir.
  Fresh per `transformSync`. Cleaned up by the host's `finally`
  block. Holds the two per-call sidecars.

Both paths are host-absolute when set in plugin config; the WASI
`/cwd` mount means the plugin sees them as `/cwd/<rel>`-prefixed
inside cap-std (PLAN.md §3.2). The host is responsible for the
translation; the plugin never resolves paths against
`env::current_dir()`.

---

## §1 `<callScratch>/included-files.json` — JSON, per-call

- **Owner:** `crates/babel-plugin` (Phase 5 §5.7 wires this; not yet
  emitted by today's port).
- **When written:** `Program::exit`, only if `included_files` is
  non-empty.
- **Lifetime:** per-call. Host drains after `transformSync`, then
  rmSyncs the parent `call-<uuid>/` dir.
- **Encoding:** UTF-8 JSON, no comments.

### Shape

```json
{
  "version": 1,
  "files": ["<absPath>", "..."]
}
```

| Field      | Type            | Constraint |
|------------|-----------------|------------|
| `version`  | integer (u32)   | MUST be `1`. Any other value: host throws `"sidecar version mismatch: included-files.json got vN, expected 1"`. |
| `files`    | array of string | Pre-realpath absolute paths, matching Babel's `onIncludedFiles` callback contract verbatim. Order preserved (insertion order — `IndexMap` semantics on the Rust side). |

### Host parser

Parcel transformer (`packages/parcel-transformer/`) reads this, calls
`asset.invalidateOnFileChange(file)` for each entry. Reference:
PLAN.md §8 "Integration shape" snippet. The reader treats a missing
file as `[]` (zero invalidations), but a *malformed* file (parse
error or version mismatch) as a hard error.

### Rust writer

Phase 5 §5.7 will emit this. Until then no writer references this
section; the schema is locked here so the future writer has a fixed
target.

---

## §2 `<callScratch>/style-rules.json` — JSON, per-call

- **Owner:** `crates/babel-plugin-strip-runtime`. Today's writer:
  `crates/babel-plugin-strip-runtime/src/lib.rs` Program::exit
  branch (`StyleRulesSidecar` Rust struct, search the file for the
  exact line).
- **When written:** `Program::exit`, only if
  `compiledRequireExclude=true` AND `style_rules` is non-empty. An
  empty rule set writes no file (matches Babel's "if no styleRules,
  no metadata" behaviour).
- **Lifetime:** per-call. Host drains after `transformSync`, exposes
  as `result.styleRules`, Parcel assigns to `asset.meta.styleRules`.
- **Encoding:** UTF-8 JSON, no comments.

### Shape

```json
{
  "version": 1,
  "rules": ["<css rule string>", "..."]
}
```

| Field     | Type            | Constraint |
|-----------|-----------------|------------|
| `version` | integer (u32)   | MUST be `1`. Mismatch: host throws `"sidecar version mismatch: style-rules.json got vN, expected 1"`. |
| `rules`   | array of string | Atomic CSS rule strings, in the order the visitor accumulated them. Each entry is a complete `.<className>{<decls>}` rule (pre-`sort()`). Order is significant only for the Babel-parity oracle — Babel's `this.styleRules.forEach(push)` preserves insertion order on the metadata array. |

Babel cross-mapping: this file is the SWC equivalent of Babel's
`file.metadata.styleRules` (PLAN.md §3.4 / §3.9.13 host-side
mapping table). The host re-exposes as `result.styleRules` in the
SWC pipeline so existing `asset.meta.styleRules` consumers see
identical shape.

### Host parser

Parcel transformer (PLAN.md §8 sample at the lines that read
`readJsonIfExists(join(callScratch, 'style-rules.json'))?.rules`).
The harness reader at
`parity-harness/strip-runtime/engines.ts` mkdirs the per-call
scratch, threads `callScratch` to the plugin in
`/cwd/<rel>` form via `toWasiPath`, and rmSyncs in `finally`.

### Rust writer

`crates/babel-plugin-strip-runtime/src/lib.rs` defines:

```rust
#[derive(Debug, serde::Serialize)]
struct StyleRulesSidecar<'a> {
    version: u32,           // hard-coded 1
    rules: &'a [String],
}
```

Serialised via `serde_json::to_string(&payload)` and written with
`std::fs::write(path, json)`. Path is
`format!("{}/style-rules.json", scratch.trim_end_matches('/'))`.

A serialisation failure panics (it shouldn't be reachable for this
shape; if it is, that's a runtime corruption we want loud). A write
failure panics with the path and underlying I/O error.

---

## §3 `<workerScratchDir>/cache.bin` — postcard binary, persistent

- **Owner:** `crates/babel-plugin`. Phase 5 §5.3 wires this; no
  writer exists today.
- **When written:** `Program::exit`, only if Layer 2 was mutated
  during the transform (`cache_dirty == true`). Atomic via
  `cache.bin.tmp` → `fd_sync` → `path_rename`.
- **When read:** `Program::enter`, every transform.
- **Lifetime:** worker process. Host rmSyncs `worker-<pid>/` on
  worker exit (`process.on('exit', ...)`).
- **Encoding:** **postcard** (NOT JSON). Compact, deterministic,
  schema-stable Rust serde binary format. Hard caps: 500 entries,
  5 MiB serialized. Full rationale: PLAN.md §3.9.10.
- **Plugin-owned:** the host MUST NEVER read or write this file. Its
  only contact is the rmSync on worker exit. Reading from JS would
  race with the plugin and the binary format is host-incompatible.

### Shape (Rust struct, postcard-encoded)

```rust
// in crates/babel-plugin/src/cache_schema.rs (Phase 5 §5.3)
pub struct CacheFile {
    pub version: u32,                // hard-coded 1
    pub schema_hash: [u8; 32],       // SHA-256 over the Layer2Entry
                                     // type signature + plugin
                                     // version. A code change that
                                     // alters Layer2Entry's shape
                                     // bumps this hash; mismatch =
                                     // wipe.
    pub layer2: Vec<(u64, Layer2Entry)>,  // key = mtime hash;
                                          // entry shape per
                                          // PLAN.md §3.9.10.
}
```

| Field         | Type            | Constraint |
|---------------|-----------------|------------|
| `version`     | u32             | MUST be `1`. Mismatch: plugin treats file as missing, wipes, regenerates (PLAN.md §3.9.10 "never crash the build over a regenerable scratch file"). |
| `schema_hash` | `[u8; 32]`      | SHA-256 over `Layer2Entry`'s serde signature + plugin version string. Mismatch: same wipe behaviour as version mismatch. |
| `layer2`      | `Vec<(u64, Layer2Entry)>` | Layer 2 cache entries. Key = mtime-derived hash. Insertion-order preserved for LRU eviction. Empty vec is valid. |

### Atomic write protocol

Per PLAN.md §3.9.10:

1. Write `cache.bin.tmp`.
2. `fd_sync` on the tmp file's fd.
3. `path_rename` from `cache.bin.tmp` to `cache.bin`.

A worker crash mid-write leaves `cache.bin.tmp` behind. Worker
startup sweeps stale `*.tmp` siblings before reading
`cache.bin` (PLAN.md §3.9.13.1).

### Cache-wipe vs sidecar-fail asymmetry

`cache.bin` is regenerable from source — a corrupt or
version-mismatched cache is a slow-build, not a wrong-build, so the
plugin silently wipes and rebuilds. Sidecars carry information the
host can't reconstruct (HMR file lists, SSR rule extraction), so a
malformed sidecar is a hard error.

---

## §4 In-memory plugin config (NOT a file, but versioned with sidecars)

Read by both `babel-plugin` and `babel-plugin-strip-runtime` from
`@swc/core` `experimental.plugins[i][1]`. Documented here because
the host's plugin-config wrapper IS the producer that populates
`workerScratchDir` and `callScratch` for the file paths above.

### Shape

```jsonc
{
  // Host-injected (every transform):
  "workerScratchDir": "<absPath>",   // §3.9.6 — stable per worker, holds cache.bin
  "callScratch":      "<absPath>",   // §3.9.6 — fresh per call, holds sidecars
  "cacheLevel":       "value" | "ast" | "off",  // default "value"

  // Plugin-specific (forwarded from user PluginOptions, minus
  // unsupported keys per PLAN.md constraint 1 — e.g. opts.resolver
  // is dropped, JS callbacks aren't reachable from WASI):
  // ... rest matches packages/babel-plugin types.PluginOptions
  // ... or packages/babel-plugin-strip-runtime types.PluginOptions
  //     (the latter additionally takes `sourceFileName` because
  //     SWC has no equivalent of `file.opts.generatorOpts.sourceFileName`)
}
```

### Strip-runtime additions (today's port)

Beyond the upstream JS shape, the SWC plugin reads these
host-threaded keys (`crates/babel-plugin-strip-runtime/src/lib.rs`
`PluginOptions`):

- `callScratch`: where `style-rules.json` gets written.
- `sourceFileName`: Babel reads this off
  `file.opts.generatorOpts.sourceFileName` natively;
  SWC's plugin metadata channel doesn't expose it, so the host
  threads it through plugin config.

Both are `Option<String>`; absent values mean "in-process tests
omitting the host wrapper" (the production wiring always sets them).

---

## Versioning policy

- **`version: 1`** on every per-call JSON sidecar today.
- A new optional field that readers can ignore: keep `version: 1`,
  document the field as additive.
- A renamed field, removed field, or re-typed field: bump to
  `version: 2`. Plugin and host MUST ship together; a mismatch is a
  hard error.
- The `cache.bin` `schema_hash` covers code-level changes that
  don't touch the on-disk version field. Bumping the postcard
  struct's serde signature (e.g. adding a variant to an enum used
  inside `Layer2Entry`) changes `schema_hash` and triggers the
  wipe path. The version field is reserved for breaking changes
  that even a re-derived schema_hash can't isolate (e.g. a
  format change from postcard to a different binary codec).

## Drift surfaces — known watch points

- The Rust `StyleRulesSidecar` struct and the JS host parser MUST
  agree on field names. Both reference this file. A change to
  either MUST update §2 in the same commit.
- `parity-harness/strip-runtime/engines.ts` is a JS-side reader
  today (drains sidecars in `finally`). When the Parcel
  transformer wrapper at `packages/parcel-transformer/src/index.ts`
  comes online (Phase 4 §4.7), it MUST share a parser with the
  harness — duplicating the read logic is a drift surface.
- `cache.bin`'s on-disk format is postcard. The `cargo postcard`
  schema export feature (if enabled) provides a machine-checkable
  schema hash; until then, `schema_hash` is computed in-tree
  per PLAN.md §3.9.10.
