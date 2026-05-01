# `parity-runner` — JS↔Rust differential harness

Streams CSS inputs through the JS pipeline (subprocess) and the Rust
pipeline (in-process), byte-compares the outputs, and reports the
smallest divergent byte range on mismatch. Plugin authors run this to
prove their port is byte-clean against upstream JS.

## How it works

```
                 ┌────────────────────────────┐
 corpus/<stage>  │   parity-runner (Rust)     │
 ────►  CSS ────►│   for each input:          │
                 │     1. send to JS bridge   │
                 │     2. run Rust stage      │
                 │     3. byte-compare        │
                 │     4. emit diff on miss   │
                 └────────────┬───────────────┘
                              │ NDJSON over stdio
                              ▼
              ┌──────────────────────────────────┐
              │  packages/css/scripts/           │
              │  parity-bridge.mjs (Bun)         │
              │                                  │
              │  imports postcss + each plugin   │
              │  in isolation; one stage runs    │
              │  ONE plugin, not the full        │
              │  transformCss pipeline.          │
              └──────────────────────────────────┘
```

One bun subprocess covers a whole corpus run — Node startup happens
once, requests stream as NDJSON.

## Requirements

- **Bun** on PATH (`https://bun.sh`). Bun handles the `.ts` plugin
  imports natively. On Windows the harness picks up `bun.cmd` as well as
  `bun.exe`.
- The workspace's installed deps (`bun install` from the workspace root)
  — the bridge resolves `postcss` from `packages/css/node_modules/`.

## Running

### From `cargo test`

```bash
cargo test -p parity-runner --test discard_empty_rules
```

The integration test loads every `*.css` under
`corpus/discard-empty-rules/`, runs both pipelines, fails on the first
divergence with the byte-range diff in the panic message.

### Standalone CLI

```bash
cargo run -p parity-runner -- \
  --stage discard-empty-rules \
  --corpus crates/parity-runner/corpus/discard-empty-rules
```

Exit code 0 = all bytes equal; 1 = at least one divergence; 2 = setup
error. Useful for local iteration without going through `cargo test`'s
output buffering.

### Oracle-stability mode (`--determinism`)

```bash
cargo run -p parity-runner -- \
  --stage postcss-core-roundtrip \
  --corpus crates/parity-runner/corpus/postcss-core-roundtrip \
  --determinism
```

Spawns the JS bridge **twice** and diffs the two JS outputs against
each other. The Rust side isn't invoked at all. This is the Phase 0
oracle-stability check — if JS-against-JS produces different bytes on
the same machine, the oracle has hidden state (env vars, fs cache,
non-deterministic iteration) bleeding into the answer, and **all
downstream parity work is suspect**. Fix that before continuing.

The integration test `tests/js_determinism.rs` runs the same check at
`cargo test` time across both seed corpora.

## Adding a new plugin stage

1. **Add the variant** in `crates/parity-runner/src/stages.rs`:
   ```rust
   pub enum Stage {
       /* ... */
       MyPlugin,
   }
   ```
2. **Wire the Rust side** in `rust_run_stage()`:
   ```rust
   Stage::MyPlugin => {
       let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
       compiled_css::plugins::my_plugin::my_plugin(&mut root);
       Ok(stringify(&root))
   }
   ```
3. **Wire the JS side** in `packages/css/scripts/parity-bridge.mjs`:
   ```js
   import { myPlugin } from '../src/plugins/my-plugin.ts';
   STAGES['my-plugin'] = (css) =>
       postcss([myPlugin()]).process(css, { from: undefined }).css;
   ```
4. **Seed a corpus** at `crates/parity-runner/corpus/my-plugin/`. Start
   by copying every test input from
   `packages/css/src/plugins/__tests__/my-plugin.test.ts` verbatim, then
   add adversarial inputs covering each branch of the plugin's logic.
5. **Add an integration test** at
   `crates/parity-runner/tests/my_plugin.rs`. Easiest: copy
   `tests/discard_empty_rules.rs` and rename the stage / corpus dir.
6. Run `cargo test -p parity-runner --test my_plugin`. The test fails
   while the Rust plugin is `unimplemented!()`; it goes green when the
   port is byte-clean.

## What's "in isolation"?

Each stage runs **one plugin** wrapped in a bare postcss `parse → plugin
→ stringify` pipeline. We do NOT run the full `transformCss` pipeline at
this level — that would contaminate diffs with every plugin in the
chain. End-to-end `transformCss` parity is a separate stage, added
once every plugin is independently byte-clean.

## Reading a divergence report

```
[09_multiple_empties_in_one_rule] DIVERGE at byte 14
  JS:   "a {\n  border: 1px solid red;\n}\n"
  RUST: "a {\n\n  border: 1px solid red;\n}\n"
  (JS len=33, RS len=34)
```

- `[09_multiple_empties_in_one_rule]` — corpus filename stem.
- `DIVERGE at byte 14` — the index of the first non-matching byte.
- The two strings show ±40 bytes of context around the divergence.
- The length difference at the end pins down whether bytes were added
  or removed by your port.

When you fix the issue, the test re-runs and either advances the
divergence (different byte index = new bug to chase) or goes green.

## When the harness produces no output

- **`spawn JS bridge`** — bun isn't on PATH, or the bridge script can't
  import its deps. Sanity-check with:
  ```bash
  echo '{"stage":"discard-empty-rules","css":"a {}"}' | \
      bun run packages/css/scripts/parity-bridge.mjs
  ```
- **`unknown stage`** — the JS bridge doesn't know about your stage
  name. Check that the string in `Stage::name()` matches the key in
  `STAGES` exactly.
- **`bridge closed unexpectedly`** — the bridge process crashed during
  startup. Run the bridge command above by hand to see the actual error
  on stderr.
