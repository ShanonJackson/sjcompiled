# `parity-harness/resolver-matrix/`

Phase 5 §5.4a entry-gate oracle. The byte-parity contract for
`crates/babel-plugin/src/resolver/` — the in-plugin replacement for
the JS host's `createDefaultResolver` (per `plugins/PLAN.md` §1
constraint 2 and `plugins/RESOLVER_SPEC_PART_TWO.md`).

Same role `parity-harness/compat-evaluation/` plays for §5.0c, and
`parity-harness/compat-generator/` plays for §4.3.

## What this asserts

```
oxc_resolver(fixture)  ===  enhanced-resolve@5.18.3(fixture)
```

byte-for-byte, where "fixture" is a `(fromFile, request, extensions)`
triple from `fixtures.json`. The 9 corpus axes are enumerated in
`crates/babel-plugin/RESOLVER_MATRIX.md`.

The npm `resolve@1.x` resolver is also captured for diagnostic-diff
visibility (it's the `resolve-binding.ts:185-189` fallback path) but
is not the parity contract — production callers go through the host
wrapper, which uses enhanced-resolve.

## Layout

```
parity-harness/resolver-matrix/
  README.md              # this file
  oracle.mjs             # pin-guarded oracle: runs both JS resolvers, writes corpus
  fixtures.json          # CHECKED-IN: declarative fixtures + expected output (oracle-self-consistency-checked)
  fixtures-source/       # CHECKED-IN: real npm-package skeletons backing each fixture
    axis-1-pkg-main/
      <fixture-name>/
        package.json
        consumer.js
        node_modules/<dep-name>/...
    axis-2-exports-conditions/
      ...
    ...
```

The oracle output (cargo-readable corpus) lands at
`crates/babel-plugin/tests/resolver_matrix_corpus.json` and is
**gitignored** — same convention as `compat_scope_corpus.json` and
`compat_evaluation_corpus.json`. Always regenerable from
`fixtures.json` + `fixtures-source/`.

## Run

```bash
# 1. Install deps (idempotent)
bun install

# 2. Run the JS oracle — writes the gitignored corpus
bun parity-harness/resolver-matrix/oracle.mjs

# 3. Run the Rust gate (will be ignored until §5.4b lands)
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration
```

## Pin guards

The oracle's first ~30 lines assert exact pinned versions of
`enhanced-resolve@5.18.3` and `resolve@1.22.12` (see
`crates/PARITY_VERSIONS.md`). Pin drift fails fast — the corpus is
only meaningful against a known oracle version.

If a Rust pin bump happens (e.g. `oxc_resolver` major version),
the divergence-action protocol in `crates/babel-plugin/RESOLVER_MATRIX.md`
applies: match (adjust `resolver/default.rs` config), shim
(`resolver/default.rs` wrapper), or escalate (add row to the
"Confirmed unreachable" table in `RESOLVER_MATRIX.md`).

## Adding a fixture

Per the §5.0c pattern:

1. Add a fixture skeleton under
   `fixtures-source/<axis-N-name>/<fixture-name>/` — real
   `package.json` + real source files. Symlinks-on-Windows is the
   only platform-conditional case; oracle skips that axis on
   non-Linux/Darwin runners.
2. Add the fixture entry to `fixtures.json`:
   ```jsonc
   {
     "label": "axis-N-<descriptive-name>",
     "axis": "<axis-key>",
     "fromFile": "fixtures-source/axis-N-name/fixture-name/consumer.js",
     "request": "<the-import-specifier>",
     "extensions": [".js", ".jsx", ".ts", ".tsx"],  // null = use enhanced-resolve default
     "expected": {
       "enhancedResolve": { "kind": "ok", "path": "<rel-path-from-corpus-root>" },
       "npmResolve":      { "kind": "err", "errorClass": "MODULE_NOT_FOUND" }
     }
   }
   ```
3. Run `bun parity-harness/resolver-matrix/oracle.mjs`. The oracle
   self-consistency-check fires if `expected` doesn't match what
   the JS resolver actually produced — fix the fixture's `expected`
   then.
4. Run the Rust gate. If `oxc_resolver` diverges, apply the
   divergence-action protocol.

## Why fixtures-source/ is checked in (not gitignored)

Unlike compat-evaluation's source-string fixtures, the resolver
contract depends on **filesystem reality**: package.json contents,
file-on-disk presence, directory layouts, symlink shapes. The
`fixtures-source/` skeletons ARE the parity surface — they cannot
be regenerated from a script the way an AST corpus can. They're
small (each fixture is a tiny package.json + 1-2 source files).
