# Corpus — `sort-atomic-style-sheet`

Combined coverage of the orchestrator: bucketing, shorthand sort,
LVFHA pseudo sort, at-rule media-query sort, recursive at-rule sort.

## Coverage

- `01` — at-rules move below regular rules (catchAll → rules → atRules).
- `02` — full LVFHA + focus-within/focus-visible ordering at root.
- `03` — pseudo sort INSIDE an at-rule body, mixed with decls.
- `04` — recursive at-rule sort (nested @media + @supports).
- `05..06` — min-width ascends, max-width descends.
- `07` — at-rule name `localeCompare`: `@layer` < `@media` < `@supports`.
- `08` — shorthand-bucket sort: all (0) → border (1) → border-color (2)
  → border-top (4) → outline-width (∞).
- `09` — top-level decls go to catchAll, ahead of rules and at-rules.
- `10` — blank input.
- `11` — single rule (no-op).
- `12` — interleaved min/max-width queries; tie-break order matters.
