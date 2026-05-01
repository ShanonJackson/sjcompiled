# Corpus — `discard-duplicates` (LOCAL)

Each `*.css` file is one parity-runner input. Tests the LOCAL
`packages/css/src/plugins/discard-duplicates.ts` — distinct from the
npm `postcss-discard-duplicates@6` used by `sort.ts` (that lives at
`crates/postcss-discard-duplicates`, see Anomaly #9 in
`PARITY_VERSIONS.md`).

## Coverage

- `01..03` — basic happy paths (two/three duplicates of one prop, no-op).
- `04` — interleaved props; both groups should keep their last.
- `05` — top-level decls + rules side-by-side; rules must pass through.
- `06` — nested decls inside a rule must NOT be touched (`root.each` is
  non-recursive).
- `07` — `!important` on the removed decl.
- `08` — exercises the Root.removeChild raws cascade across multiple
  consecutive first-child removals.
- `09` — blank input.
- `10` — root has no top-level decls at all.
- `11` — comment interleaved with duplicates (comment is a top-level
  child but not a decl, so the plugin ignores it).

## Adding entries

Found a divergence in production? Drop the offending CSS at
`NN_<short_label>.css` and re-run. The integration test at
`tests/discard_duplicates.rs` picks it up automatically.
