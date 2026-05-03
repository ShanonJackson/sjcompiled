# COMMANDS — build, test, verify, and add fixtures

Operational runbook for `packages/css/src/transform.ts` → Rust port. Read
`CLAUDE.md`, `PLAN.md`, `crates/EXECUTION_PLAN.md`, `crates/PARITY_VERSIONS.md`,
and `crates/STATUS.md` first if you have not. **This file is just commands.**

---

## TL;DR — verify a clean build end-to-end

```bash
# 1. Cargo unit + integration tests across every crate.
cargo test --workspace --no-fail-fast

# 2. Differential parity gate for the full transformCss pipeline.
cargo run -p parity-runner -- \
  --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css

# 3. Determinism: JS oracle must produce identical bytes across two spawns.
cargo run -p parity-runner -- \
  --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css \
  --determinism

# 4. NAPI marshaling gate (rebuild step 5 first if you changed Rust).
bun run packages/css/scripts/verify-napi-transform-css.mjs
```

Expected as of Phase 8b ship: `1226/1226` cargo, `30/30` parity-runner,
`30/30` determinism, `30/30` NAPI verifier.

---

## Prerequisites

- **Rust toolchain**: stable. The workspace pins via `rust-toolchain.toml`
  (or rustup will pick a compatible version).
- **Node**: `>= 14` (per `packages/css-native/package.json`).
- **Bun**: used to invoke the JS verifier scripts (`bun run …`). Node also
  works but Bun matches what the corpus/verify scripts assume.

---

## Building the NAPI module (`@compiled/css-native`)

The Rust pipeline ships to JS via a single `cdylib` per platform.

### Dev build (the only build that works on this machine)

```bash
RUSTFLAGS="" cargo build -p compiled-css-napi
```

**Important:**

- `RUSTFLAGS=""` clears any inherited `RUSTFLAGS` from the shell (LTO
  flags break `cdylib` builds in dev mode).
- **Do NOT run `cargo build -p compiled-css-napi --release`** on this
  Windows dev box. LLVM release-mode codegen of the autoprefixer crate
  OOMs hard (≥32 GB RAM required; three confirmed full-system OOMs).
  See `crates/compiled-css-napi/Cargo.toml` warning block for context.
  Dev-mode bytes are byte-identical to release-mode bytes per Phase 8a's
  `verify-napi-autoprefixer` precedent — there is no functional reason
  to use release mode here.

### Copy the built artifact into `packages/css-native/`

After `cargo build -p compiled-css-napi`:

```bash
# Windows (current platform):
cp target/debug/compiled_css_napi.dll \
   packages/css-native/compiled-css.win32-x64-msvc.node
```

For other platforms (future Phase 8c), the loader in
`packages/css-native/index.js` already maps `process.platform`+`process.arch`
to the right filename:

| Platform / arch | Expected filename |
|---|---|
| `win32` `x64` | `compiled-css.win32-x64-msvc.node` |
| `linux` `x64` | `compiled-css.linux-x64-gnu.node` |
| `linux` `arm64` | `compiled-css.linux-arm64-gnu.node` |
| `darwin` `x64` | `compiled-css.darwin-x64.node` |
| `darwin` `arm64` | `compiled-css.darwin-arm64.node` |

### Verify the binary loads

```bash
node -e "console.log(Object.keys(require('./packages/css-native/index.js')))"
# Expected: [ 'sort', 'autoprefixer', 'transformCss' ]
```

---

## Cargo tests

```bash
# Whole workspace (1226 passed at Phase 8b ship):
cargo test --workspace --no-fail-fast

# A single crate:
cargo test -p compiled-css   --no-fail-fast    # local plugins (121)
cargo test -p css            --no-fail-fast    # transform.rs + sort.rs (27)
cargo test -p autoprefixer   --no-fail-fast
cargo test -p postcss-core   --no-fail-fast
# … one per crate under crates/

# A single test by name:
cargo test -p compiled-css comment_interleave_with_top_level_decls
```

---

## Parity-runner — the differential harness

The parity-runner runs each fixture through both the JS oracle and the Rust
port and byte-compares output. Stages live under
`crates/parity-runner/src/stages.rs`; a corpus is a directory of `.css`
fixtures (and optional `.opts.json` siblings for non-default opts).

### Run a stage

```bash
cargo run -p parity-runner -- \
  --stage <stage-name> \
  --corpus crates/parity-runner/corpus/<stage-name>
```

### Available stages

The corpus directories under `crates/parity-runner/corpus/` are 1:1 with the
stage names. As of Phase 8b ship:

```
atomicify-rules                    extract-stylesheets
autoprefixer                       increase-specificity
cssnano-band                       merge-duplicate-at-rules
discard-duplicates                 normalize-current-color
discard-empty-rules                npm-postcss-discard-duplicates
expand-shorthands                  parent-orphaned-pseudos
postcss-calc                       postcss-normalize-string
postcss-colormin                   postcss-normalize-timing-functions
postcss-convert-values             postcss-normalize-unicode
postcss-core-roundtrip             postcss-normalize-url
postcss-discard-comments           postcss-normalize-whitespace
postcss-minify-gradients           postcss-ordered-values
postcss-minify-params              postcss-reduce-initial
postcss-minify-selectors           sort
postcss-nested                     sort-atomic-style-sheet
postcss-normalize-positions        transform-css
```

### Useful flags

```bash
# Run JS oracle twice and assert byte-stability (catches non-determinism
# in browserslist resolution, env-dependent output, etc.).
cargo run -p parity-runner -- --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css \
  --determinism

# Stop on first divergence (default reports all then exits non-zero):
cargo run -p parity-runner -- --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css \
  --bail

# Single fixture (debugging):
cargo run -p parity-runner -- --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css \
  --only 22_comments_at_positions.css
```

(Check `crates/parity-runner/src/main.rs` for the canonical flag list — the
above are the most-used.)

---

## NAPI verifier scripts

These prove the NAPI marshaling layer (UTF-16/UTF-8 string round-trip,
`IndexMap` insertion-order plumbing for `classNameCompressionMap`,
result-vec → JS array marshalling, error-string marshalling) doesn't add
or strip any bytes versus the JS oracle.

```bash
# Full transformCss pipeline through NAPI:
bun run packages/css/scripts/verify-napi-transform-css.mjs

# Phase 8a sort:
bun run packages/css/scripts/verify-napi-sort.mjs

# Autoprefixer in isolation:
bun run packages/css/scripts/verify-napi-autoprefixer.mjs

# Engine flag (sort.ts COMPILED_CSS_ENGINE round-trip):
bun run packages/css/scripts/verify-engine-flag.mjs
```

All four pin `BROWSERSLIST=chrome 100` and clear `AUTOPREFIXER` so both
engines target the same set.

---

## Adding fixtures to the `transform-css` corpus

You said you have ~100 fixtures ready. This is exactly the path to integrate
them.

### Layout

```
crates/parity-runner/corpus/transform-css/
├── 01_blank.css                   # input CSS — REQUIRED
├── 01_blank.opts.json             # optional sibling; default opts if absent
├── 02_single_decl.css
├── 02_single_decl.opts.json
…
```

- The basename is the fixture id. Convention so far is `NN_short_description.css`
  zero-padded to two digits, kebab-case description. With 100 fixtures, jump
  to three digits (`100_…`, `101_…`) — the parity-runner sorts lexically.
- The `.opts.json` is the second argument to `transformCss(css, opts)`.
  When absent, the runner uses `{}` (default opts: `optimizeCss=true`,
  `increaseSpecificity` unset, no `classNameCompressionMap`).
- Both engines pin `BROWSERSLIST=chrome 100` and clear `AUTOPREFIXER`. If
  your fixtures depend on a different browserslist target, file an issue —
  the harness pin is intentional and changing it requires a coordinated
  update on both the Rust stage handler and the JS bridge.

### Example `.opts.json`

```json
{
  "optimizeCss": false,
  "increaseSpecificity": true,
  "classNameCompressionMap": {
    "color": "_a",
    "padding": "_b"
  }
}
```

(Only set the fields you need — the rest fall back to JS defaults.)

### After dropping the files in

```bash
# 1. Cargo first — catches compilation regressions.
cargo test --workspace --no-fail-fast

# 2. Parity gate — this is the byte-clean assertion.
cargo run -p parity-runner -- \
  --stage transform-css \
  --corpus crates/parity-runner/corpus/transform-css

# 3. NAPI verifier — proves the JS-side wrapper too.
bun run packages/css/scripts/verify-napi-transform-css.mjs
```

If a fixture diverges, the parity-runner reports the smallest divergent
byte range with surrounding context. **Per CLAUDE.md drift-detection rules,
do NOT patch the new fixture or special-case it in the bridge** — every
divergence points at a real bug somewhere in the Rust port. Surface it as
a drift escalation in `crates/STATUS.md` like the existing entries
("Drift detected in `<crate>` — `<byte-level explanation>`").

### Tips for naming

- Group by what the fixture stresses (`_at_media`, `_var_bailout`,
  `_atomicify_pseudo`) so a future drift report localises fast.
- Don't reuse numbers across the existing 30 fixtures; pick a fresh
  contiguous block.
- The README in `crates/parity-runner/corpus/transform-css/` (if present)
  documents conventions for the existing 30; mirror it.

---

## End-to-end byte-equality harness (`packages/equality-harness`)

Runs every fixture under `/fixtures` through Babel twice (engine off / engine
on) with the plugin chain `[@atlaskit/tokens/babel-plugin, @compiled/babel-plugin]`
and byte-compares `result.code`. This is the integration-level proof that
Rust `transformCss` is observationally indistinguishable from the JS oracle
when driven through the real Babel pipeline AFM uses in production.

```bash
# Full sweep (336 fixtures, ~3 minutes):
bun run --cwd packages/equality-harness verify

# Stop on first divergence:
bun run --cwd packages/equality-harness verify:bail

# Run only specific fixtures by directory name:
bun run --cwd packages/equality-harness verify -- --only ct-css-null-literal-styles ct-lozenge-mixed-cssmap-patterns
```

Expected output:

```
Total fixtures:     336
Skipped (no input): 0
Pass:               336
Fail:               0
```

Any byte divergence is reported with the smallest divergent byte range and
surrounding context. Per CLAUDE.md drift-detection: do NOT special-case
fixtures — every divergence points at a real bug somewhere in the Rust port.

---

## Engine flag — manual smoke test

`packages/css/src/transform.ts` and `packages/css/src/sort.ts` both honor
`COMPILED_CSS_ENGINE=rust` to delegate to the Rust NAPI binary.

```bash
# Default (JS engine):
node -e "
  const { transformCss } = require('./packages/css/src/transform.ts');
  console.log(transformCss('.a { color: red; }', {}));
"

# Rust engine:
COMPILED_CSS_ENGINE=rust BROWSERSLIST='chrome 100' \
  node -e "
    const { transformCss } = require('./packages/css/src/transform.ts');
    console.log(transformCss('.a { color: red; }', {}));
  "
```

Both should print byte-identical output.

---

## WASI probe (for the future SWC-plugin end state)

Phase 8b prep verified the Rust pipeline compiles cleanly to `wasm32-wasip1`.
Re-run before any major Phase 9+ change to catch new dep blockers early:

```bash
RUSTFLAGS="" cargo build --target wasm32-wasip1 -p css
RUSTFLAGS="" cargo build --target wasm32-wasip1 -p autoprefixer
```

Both should compile clean (one cosmetic clippy warning is preexisting).

---

## Common gotchas

| Symptom | Cause | Fix |
|---|---|---|
| `cargo build -p compiled-css-napi --release` hangs / OOMs | Dev box has < 32 GB free; LLVM release codegen of autoprefixer is the offender | Use dev mode. Bytes-out are byte-identical. |
| `Error: no prebuilt binary for <platform>-<arch>` from `index.js` | Built but didn't copy to `packages/css-native/` with the platform-suffixed name | `cp target/debug/compiled_css_napi.<ext> packages/css-native/compiled-css.<triple>.node` |
| Parity-runner reports diff at byte 0 with `\"sheets\":[…` JSON | Likely a real divergence; do NOT special-case | Investigate root cause; flag as drift in STATUS.md |
| Determinism gate fails (JS-vs-JS not stable) | Browserslist resolution depends on env that isn't pinned | Both bridges pin `BROWSERSLIST=chrome 100` and clear `AUTOPREFIXER`/`COMPILED_CSS_ENGINE` — verify nothing else in your shell is leaking through |
| `cargo test` is fast but `cargo run -p parity-runner` recompiles | Different profile than `test`; first run takes a minute | Subsequent runs are incremental |
| LTO / RUSTFLAGS-related linker errors | Inherited `RUSTFLAGS` from shell or parent process | Always prefix with `RUSTFLAGS=""` for napi/cdylib builds |

---

## Phase status — where to start next

Phase 8b shipped `transformCss` end-to-end. Both `transform.ts` and
`sort.ts` honor `COMPILED_CSS_ENGINE=rust`. Next phase: **Phase 9 — diff
at scale**, per `crates/EXECUTION_PLAN.md` Phase 9. That's:

- corpus replay as a required PR-time CI check
- `cargo-fuzz` targets (`crates/parity-fuzz/`)
- shadow runs in a real consumer codebase (hash-compare, no production impact)

Read `crates/EXECUTION_PLAN.md` Phase 9 for the full spec.
