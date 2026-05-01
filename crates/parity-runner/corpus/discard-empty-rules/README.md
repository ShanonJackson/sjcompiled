# Corpus — `discard-empty-rules`

Each `*.css` file is one parity-runner input. Files are picked up in
filename order; prefix with `NN_` to control ordering.

## Sources

- `01..05` — verbatim from `packages/css/src/plugins/__tests__/discard-empty-rules.test.ts`.
  Lock these down so test inputs that already work in JS continue to work
  in Rust.
- `06..16` — adversarial inputs covering the surface area the upstream
  test suite doesn't:
  - whitespace-only values
  - nested at-rules
  - multiple empties in one rule
  - empty decl with `!important`
  - comments adjacent to empty decls
  - the word `undefined` *inside* a value (must NOT be removed)
  - already-empty rule
  - `url(undefined)` (looks empty but isn't)
  - mixed empty/non-empty rules
  - blank input

## Adding entries

Found a divergence in production? Drop the offending CSS at
`NN_<short_label>.css` and re-run the harness. The test suite picks it
up automatically.
