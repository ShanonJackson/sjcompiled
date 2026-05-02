# Parity harness

The verification oracle for the `@sjcompiled/babel-plugin` and
`@sjcompiled/babel-plugin-strip-runtime` Rust SWC ports
(`plugins/PLAN.md` §2). Asserts:

```
prettier(babelOutput, { parser: 'babel' }) ===
prettier(swcOutput,   { parser: 'babel' })
```

byte-for-byte across the fixture corpus.

## Layout

```
parity-harness/
  README.md                                  # this file
  strip-runtime/
    fixtures/                                # (input, opts) snapshots
      <name>.json
    engines.ts                               # babel-runner + swc-runner
    harness.test.ts                          # bun test driver
```

`babel-plugin/` parity harness lands alongside in Phase 2.

## Run

```bash
# Build the SWC plugin first (passthrough at Phase 0 — Phase 1 will
# replace this with the real port).
RUSTFLAGS="" cargo build -p babel-plugin-strip-runtime \
  --target wasm32-wasip1 --release

# Run the harness
bun test parity-harness/strip-runtime/harness.test.ts
```

## What this asserts

- **Babel-vs-itself determinism** — running the Babel pipeline twice on
  the same input produces byte-equal output. If this fails, the oracle
  is broken — fix that before any port work.
- **Babel-vs-SWC parity** — Babel and SWC produce the same post-prettier
  bytes for every fixture. At Phase 0 with a passthrough SWC plugin,
  every fixture EXCEPT pass-through inputs will FAIL — that confirms
  the harness can detect drift.

## Fixture format

```jsonc
{
  "name": "removes-css-prop-runtime-automatic",
  "source": "<raw input source>",
  "opts": {
    "run": "extract",                          // 'bake' | 'extract' | 'both'
    "runtime": "automatic",                    // 'classic' | 'automatic'
    "styleSheetPath": null,
    "compiledRequireExclude": false,
    "extractStylesToDirectory": null
  },
  "preBaked": "<JS-baked source>",            // for run='extract' fixtures
                                              // pre-baked by the babel-plugin once
                                              // and frozen here so SWC strip-runtime
                                              // is tested on identical input.
                                              // omitted for run='bake' / 'both'
  "expected": "<prettier-normalized expected output>"
}
```

For `run: 'both'` and `run: 'bake'`, the harness needs the full
`babel-plugin` pipeline, which is not yet ported. Phase 1 fixtures
filter to `run: 'extract'` (strip-runtime only) plus pre-baked
inputs. Phase 2 onwards adds `run: 'both'` fixtures.

## Phase 0 status

Three seed fixtures:
- `extract-automatic-passthrough.json` — pre-baked input runs
  unchanged when strip-runtime is no-op (validates harness-without-
  drift case)
- `extract-automatic-stripped.json` — strip-runtime should remove
  CC/CS wrappers (FAILS at Phase 0; PASSES at Phase 1 exit)
- `extract-classic-stripped.json` — same, classic runtime

Full corpus extraction (38 strip-runtime tests, ~50+ babel-plugin
tests) is Phase 1 task 1 / Phase 2 task 1.
