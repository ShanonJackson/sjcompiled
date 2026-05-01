# Corpus — `expand-shorthands`

Coverage spans all 11 conversion functions plus the `var(--…)` bailout
and the unrelated-prop pass-through.

- `01..05` — margin/padding (1, 2, 3, 4 args).
- `06..07` — overflow.
- `08..10` — place-content (single, invalid `left`, double).
- `11..12` — place-items, place-self.
- `13..22` — flex (auto/none/initial/inherit/number/two/two-with-basis/triple/calc/invalid).
- `23..24` — flex-flow.
- `25..27` — outline (color-only, full, thin/dashed).
- `28..30` — text-decoration variants.
- `31..32` — background (color-only fully expanded; complex pass-through).
- `33` — `var(--…)` bailout: padding stays as-is.
- `34..35` — unrelated prop / blank input.
- `36` — outline single-style default.
- `37..38` — at-rule containment + multi-decl mix.
