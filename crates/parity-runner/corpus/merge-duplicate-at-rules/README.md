# Corpus — `merge-duplicate-at-rules`

Top-level merge cases only. Per the upstream docstring "Currently does
not handle nested at-rules" — that path is intentionally out of scope.

## Coverage

- `01..03` — derived from upstream tests: identical at-rules merge,
  identical children dedupe, distinct at-rules stay separate.
- `04` — no at-rules → no-op.
- `05` — blank input.
- `06` — decl + at-rule mix; the decl stays at the top, at-rules merge
  and append at the end.
- `07` — interleaved at-rules with same/different params; each unique
  query string becomes one merged entry, in first-seen order.
