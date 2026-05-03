# Parity harness

The verification oracle for the `@compiled/babel-plugin` and
`@compiled/babel-plugin-strip-runtime` Rust SWC ports
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
    fixtures/                                # 41 hand-curated (A/B/C/D)
      <name>.json
      synthesized/                           # §1.8 — 1000 generated
        synth-<NNNNN>-*.json                 # gitignored, regenerable
    engines.ts
    harness.test.ts
    generate-fixtures.mjs                    # extracts A/B/C/D from upstream tests
    synthesize-fixtures.mjs                  # §1.8 deterministic synth corpus
  babel-plugin/
    fixtures/                                # 477 extracted (gitignored, regenerable)
      *.json
    engines.ts
    harness.test.ts
    extract-fixtures.mjs                     # §2.0 runtime extractor (Bun.plugin hook)
```

Synthesised + extracted fixture corpora are gitignored — both
harnesses self-bootstrap by invoking the generator/extractor on a
fresh checkout.

## Run

```bash
# Build the SWC plugins
RUSTFLAGS="" cargo build \
  -p babel-plugin -p babel-plugin-strip-runtime \
  --target wasm32-wasip1 --release

# Strip-runtime (Phase 1 closed — 1132/1132 zero-divergence)
bun test parity-harness/strip-runtime/harness.test.ts

# Babel-plugin (Phase 2 in progress — pass-through baseline)
bun test parity-harness/babel-plugin/harness.test.ts

# Full-corpus determinism / parity for babel-plugin (used at §2.5
# exit gate; default sample is fast)
BABEL_PLUGIN_FULL_DETERMINISM=1 bun test parity-harness/babel-plugin/harness.test.ts
BABEL_PLUGIN_FULL_PARITY=1      bun test parity-harness/babel-plugin/harness.test.ts
```

## What this asserts

- **Babel-vs-itself determinism** — running the Babel pipeline twice
  on the same input produces byte-equal output. If this fails the
  oracle is broken; fix before any port work.
- **Babel-vs-SWC parity** — Babel and SWC produce the same
  post-prettier bytes for every fixture. Pre-handler-port phases run
  with a per-fixture `expectedToFail` discipline: fixtures Babel
  transforms assert NOT-equal vs the pass-through SWC plugin (so a
  regression to false-positive parity is caught); fixtures that
  pass-through identically through both engines assert equal.

## Fixture formats

### `strip-runtime/`

```jsonc
{
  "name": "C01-source-same-automatic-removes-runtime",
  "source": "<raw input source>",
  "opts": {
    "run": "extract",                  // 'bake' | 'extract' | 'both'
    "runtime": "automatic",            // 'classic' | 'automatic'
    "styleSheetPath": null,
    "compiledRequireExclude": false,
    "extractStylesToDirectory": null
  },
  "expectedToFail": false,             // optional — gates pre-port phases
  "failureReason": "<phase ref>"       // required when expectedToFail=true
}
```

### `babel-plugin/`

```jsonc
{
  "name": "<file-slug>/<test-path>",
  "sourceFile": "styled/__tests__/behaviour.test.ts",
  "testPath": ["styled component behaviour", "should ..."],
  "source": "<raw input source>",
  "opts": {                            // PluginOptions + harness flags
    "snippet": true,
    "pretty": true,
    "comments": true
    // ...PluginOptions keys (importReact, optimizeCss, classHashPrefix, ...)
  }
}
```

The harness re-runs Babel as oracle and compares against SWC; the
expected output is NOT frozen on disk (would drift on every Babel
update).

## Status

Source of truth: [`plugins/STATUS.md`](../plugins/STATUS.md). Quick
read at time of writing:

- Phase 1 closed: 1132/1132 strip-runtime tests across 1041 fixtures
  (41 hand-curated + 1000 synthesised).
- Phase 2 §2.0–§2.2 closed; §2.3 dispatcher skeleton landed
  (recognises imports, no AST mutation — pass-through preserved).
  152 sampled / 477 full-determinism babel-plugin tests green.
