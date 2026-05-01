# Corpus — `flatten-multiple-selectors`

The plugin clones each multi-selector rule into N single-selector
clones. Tests cover both no-op and split paths, including paren/quote
edge cases that selector-parser handles.

## Coverage

- `01` — no-op single selector.
- `02..03` — basic 2- and 3-selector splits.
- `04..05` — inside `@media` and nested `@supports`/`@media`.
- `06` — `:is(.a, .b), .c` — comma inside `:is(...)` must not split.
- `07` — `[data-x="a,b"]` — comma inside attribute string must not split.
- `08` — attribute-then-nesting selector grouped with another.
- `09` — broad mix of complex selectors (the upstream "complex" test).
- `10` — blank input.
- `11` — two separate multi-selector rules adjacent.
