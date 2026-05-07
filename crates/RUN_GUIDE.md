# Running Parity Tests — Fast Dev Loop Guide

Read this before running anything in `crates/`. Following these steps
takes the parity-runner from "OOM after 5 minutes" or "hangs forever"
back down to **~1.5s end-to-end** for a 30-fixture corpus.

---

## TL;DR

```bash
cd /Users/sjackson3/Documents/sjcompiled/crates

# One-time / after editing crates: build the runner (5–15s clean,
# <2s incremental). Always use `dev` profile, never `--release`.
cargo build -p parity-runner

# Run any parity stage. ~1–2 seconds total per stage on a 30-input corpus.
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/transform-css
```

Available stages map 1:1 to corpus directories under
`parity-runner/corpus/`. The full list is enumerated in
`parity-runner/src/main.rs` (search `Stage::`).

---

## Why `--release` is poison here

Three landmines stack on top of each other if you `cargo run --release`:

1. **The user's global `~/.cargo/config.toml` (or shell `RUSTFLAGS`)
   may set `lto=thin` + `codegen-units=1` + `opt-level=3`.** That
   combination OOMs on this workspace because `autoprefixer` +
   `cssnano-preset-default` produce a huge LTO call graph. The
   warning at the bottom of `crates/Cargo.toml` documents this.
2. **`cargo run --release`** invokes the linker even on tiny edits,
   and release linking with LTO is multiple minutes per cycle.
3. **The previous `target/release/` is invalidated** any time a
   workspace crate compiles with different flags than last time
   (e.g. you switched between `cargo test` and `cargo run --release`),
   so you re-pay the full release rebuild constantly.

The dev profile in `crates/Cargo.toml` is tuned the opposite way:

```toml
[profile.dev]
opt-level = 0
lto = "off"
codegen-units = 256
debug = false
incremental = true
```

Result: ~10s clean, ~1s incremental. Runtime is fast enough — the
parity-runner spends most of its wall-clock in the JS oracle
subprocess (node + postcss + autoprefixer), not in Rust.

**Never use `cargo run --release` for parity work.** Build with
`cargo build -p parity-runner`, then invoke the binary directly.

---

## The two flags that matter

```bash
./target/debug/parity-runner \
    --stage <stage-name> \
    --corpus <directory of *.css fixtures>
    [--determinism]
```

- `--stage`: which pipeline shape to diff. One of the stage names
  enumerated in `parity-runner/src/main.rs`. Examples:
  `transform-css`, `sort`, `autoprefixer`, `cssnano-band`,
  `atomicify-rules`, `expand-shorthands`, `postcss-colormin`, etc.
- `--corpus`: directory containing `*.css` fixtures. Files are read
  in lexicographic order; the file stem is the divergence label.
- `--determinism`: run the JS oracle TWICE on the same inputs and
  diff the two outputs. Use this when JS-vs-JS is suspect (caniuse
  drift, browserslist resolution, env-var leak). If this fails, do
  not chase Rust diffs — fix the JS oracle first.

Exit codes: `0` = byte-clean across the whole corpus, `1` = at least
one divergence (details printed to stderr), `2` = setup error
(missing corpus, bun not on PATH, JS bridge crash).

---

## Common one-liners

### Run the headline transformCss gate

```bash
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/transform-css
# OK — 30 inputs, all byte-clean (JS vs Rust)
```

### Sweep every stage that has a corpus

```bash
for d in parity-runner/corpus/*/; do
    stage=$(basename "$d")
    echo "=== $stage ==="
    ./target/debug/parity-runner --stage "$stage" --corpus "$d" || true
done
```

### Add a fixture and re-run

```bash
cp /tmp/divergent.css \
    parity-runner/corpus/transform-css/31_my_repro.css
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/transform-css
```

### Confirm the JS oracle is stable

```bash
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/transform-css \
    --determinism
# OK — 30 inputs, JS oracle is deterministic across two spawns
```

### Run the cargo integration test wrappers (slower but the same gate)

```bash
cargo test -p parity-runner --no-fail-fast
# Each Stage::* has a `tests/<stage>.rs` integration test that goes
# through the same diff harness. Useful for CI; the binary above is
# what you want for local iteration.
```

---

## Why the bridge runs under node, not bun

The JS oracle is `packages/css/scripts/parity-bridge.mjs`, spawned by
`crates/parity-runner/src/js_bridge.rs` as:

```
node --no-warnings --no-deprecation \
     --require    packages/css/scripts/parity-bridge-cjs-hook.cjs \
     --experimental-loader file://…/parity-bridge-ts-loader.mjs \
     packages/css/scripts/parity-bridge.mjs
```

We run under **node**, not bun, because the AFM monorepo runs
`transformCss` under node V8 in production. Bun runs JavaScriptCore.
V8 (TimSort) and JSC (merge-sort) implement
`Array.prototype.sort` with different stable-sort algorithms and
disagree on the final order for non-transitive comparators —
specifically `sort-shorthand-declarations`'s comparator returns 0
for nodes without a first declaration, which makes it non-transitive
on inputs that mix decls with comments / nested rules. Running the
oracle under bun was masking real V8-correct Rust output as
"diverged".

Two loader files cover the TS plugin imports:

- `parity-bridge-ts-loader.mjs` — node 20.15 ESM loader hook
  (`module.register`-compatible). Resolves extension-less imports
  (`./foo` → `./foo.ts`) and transpiles `.ts` files via
  `@babel/preset-typescript` + `transform-modules-commonjs` so
  CJS-package named-export interop works the same way bun's loader
  handles it.
- `parity-bridge-cjs-hook.cjs` — preloaded with `--require`. Adds
  `.ts` / `.tsx` to `require.extensions` so nested CJS-graph
  requires (the post-transpile `require('./peer')` calls) find
  adjacent `.ts` files.

### Batching, not streaming

The bridge reads all requests from stdin, then emits responses on
stdout. The runner writes every request, closes stdin (EOF), then
drains stdout. We chunk batches at `BATCH_MAX = 256` so the per-batch
response stream stays well below the kernel-pipe + node libuv
userspace buffer ceiling for every stage, including `transform-css`
which emits ~5KB of `{sheets, classNames}` JSON per fixture. A
streaming request-per-line protocol would still risk deadlock under
node (libuv's stdout writes block on full pipe buffers); the
batched protocol sidesteps the question.

---

## Troubleshooting

### "no such file or directory: crates"

You're already inside `crates/`. Drop the `crates/` prefix:

```bash
# Wrong (already inside crates/)
cd crates && cargo build -p parity-runner

# Right
cargo build -p parity-runner
```

### Hang with no output, runner using 0% CPU

The runner writes all requests, closes stdin, then reads stdout.
If the JS bridge's response stream exceeds kernel-pipe + node
userspace buffers before EOF on stdin, libuv blocks and we
deadlock. `BATCH_MAX = 256` in `js_bridge.rs` keeps each batch well
under that ceiling; if you raise it, expect hangs on `transform-css`
first (largest per-fixture output).

To isolate, run the bridge directly:

```bash
echo '{"stage":"transform-css","css":"a{color:red}"}' \
    | node --no-warnings --no-deprecation \
        --require packages/css/scripts/parity-bridge-cjs-hook.cjs \
        --experimental-loader file://$PWD/packages/css/scripts/parity-bridge-ts-loader.mjs \
        packages/css/scripts/parity-bridge.mjs
# Expected: one JSON line on stdout, exit 0.
```

### "node is required for the parity harness"

Install node 20.15 or newer (the loader hook requires
`module.register`, which landed in 20.6 and stabilised in 20.15):

```bash
which node || brew install node@20
node --version  # must be >= v20.15
```

We deliberately do **not** support bun for the parity harness — see
the "Why the bridge runs under node, not bun" section above.

### `cargo build` is slow / OOMs

You probably have a global `RUSTFLAGS` setting LTO + opt-level=3.
Confirm with `echo $RUSTFLAGS`. The fast `[profile.dev]` in
`crates/Cargo.toml` already overrides codegen settings, but a global
`RUSTFLAGS=-Clto=fat` (or similar) bypasses the profile. Unset for
this workspace:

```bash
cd /Users/sjackson3/Documents/sjcompiled/crates
RUSTFLAGS="" cargo build -p parity-runner
```

If you need this often, add a workspace `.cargo/config.toml`
clearing `RUSTFLAGS` — but this hasn't been needed in practice yet
because the `[profile.dev]` settings are usually enough.

### "JS bridge returned N responses for M inputs"

The bridge crashed mid-batch. The stderr it captured should print in
the error message. Common causes:

- A new stage was added in `parity-runner/src/main.rs::Stage` but no
  matching `case` exists in `parity-bridge.mjs::STAGES` — the JS
  side returns `{ ok: false, error: "unknown stage: ..." }` for
  every fixture. That counts as a response, not a crash. If counts
  match but every entry is a JS error, this is what happened.
- A plugin import in `parity-bridge.mjs` failed at startup
  (e.g. dependency not installed). `bun install` from the workspace
  root.

### `--determinism` reports JS-vs-JS divergence

Stop. Do not chase the Rust port. The JS oracle is non-deterministic,
which means every diff result on the Rust side is suspect. Check:

- `BROWSERSLIST` / `BROWSERSLIST_CONFIG` / `BROWSERSLIST_DISABLE_CACHE`
  env vars across the two runs (the bridge sets `BROWSERSLIST=chrome 100`
  for `transform-css`, but other stages don't).
- `caniuse-lite` version (`bun -e
  "console.log(require('caniuse-lite/package.json').version)"`).
  Must match `1.0.30001766` per `PARITY_VERSIONS.md`.
- File-system traversal order (rare).
- The fixture corpus shouldn't change between runs.

---

## Where to go next

- **Adding a new fixture for a divergence you found:** drop a `.css`
  file under the appropriate `parity-runner/corpus/<stage>/`. Name
  it `NN_short_description.css`. Re-run the stage.
- **Porting a new plugin:** read `crates/PLUGIN_IMPLEMENTATION_GUIDE.md`,
  add a corpus directory + `Stage::` enum + JS bridge case + Rust
  splice in `stages.rs` + integration test in `tests/`.
- **Diff at scale (Phase 9):** see `crates/EXECUTION_PLAN.md` Phase 9.
  Currently corpus replay is the gate; coverage-guided fuzzing
  hasn't been wired yet.
- **What "byte-clean" means and why it's the contract:** read
  `crates/PARITY_VERSIONS.md`. The TL;DR is class names are hashes
  of these output bytes; one byte of drift renames every class in
  ~10M LOC of consumer code.
