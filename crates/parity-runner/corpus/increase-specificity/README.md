# Corpus — `increase-specificity`

Inputs cover every branch of upstream `increase-specificity.test.ts`
plus adversarial selectors that exercise the `selector.includes('._')`
filter (chained classes, classes inside `:is()`, classes adjacent to
attribute selectors, classes after IDs).

## Coverage

- `01` — no-op (`.foo` doesn't match `._`).
- `02` — single underscore class → append `:not(#\#)`.
- `03` — underscore class inside `@media`.
- `04` — `html`, `:root` unchanged (no `._` substring).
- `05` — class followed by `:hover` and `::before`; the inserted Pseudo
  must land BEFORE the trailing pseudo.
- `06` — `._foo._bar` (two adjacent classes) — both get the suffix.
- `07` — descendant selector with class then tag.
- `08` — class inside `:is(...)` — recursive walkClasses inserts inside
  the inner Selector. **Smoke test for the Pseudo storage refactor.**
- `09` — comma list with mixed classes; only those that contain `._`
  (after trim) get rewritten. `.bar` does NOT contain `._` so passes
  through.
- `10` — blank input.
- `11` — class adjacent to attribute selector.
- `12` — id followed by underscore class.
