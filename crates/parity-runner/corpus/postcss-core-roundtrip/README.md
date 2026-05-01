# Corpus — `postcss-core-roundtrip`

Each `*.css` file is one parity-runner input. The harness asserts that
`postcss-core::stringify(postcss-core::parse(css))` produces byte-identical
output to `postcss.parse(css).toString()` for every entry.

This is the Phase 1a exit gate — the parser+stringifier must be byte-clean
on every input the JS pipeline accepts BEFORE any plugin layers it.

## Coverage

- `01..04` — simple rules, missing trailing semicolons, multiple decls,
  `@media` query whitespace.
- `05..08` — nested at-rules, `!important`, comments at every position
  (top-level, mid-value).
- `09..10` — `url()` with query strings, escaped quotes in strings.
- `11..14` — empty rules, IE-era property hacks (`_color`, `*background`),
  `@charset`, Unicode escapes in values.
- `15..16` — `calc()`, custom properties.
- `17..18` — pseudo-classes, pseudo-elements, descendant/sibling
  combinators, attribute selectors with all matchers.
- `19` — keyframes (multiple shapes: `from`/`to`, percentage stops).
- `20..22` — adversarial: blank input, whitespace-only, mixed line
  endings (file 22 may be saved as CRLF by the editor — that's the test).

## Adding entries

Found a divergence in production? Drop the offending CSS at
`NN_<short_label>.css` and re-run the harness. The integration test at
`tests/postcss_core_roundtrip.rs` picks it up automatically.
