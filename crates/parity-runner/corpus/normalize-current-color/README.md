# Corpus — `normalize-current-color`

Decl-visitor plugin that normalizes case-insensitive `currentcolor` and
`current-color` to canonical `currentColor`. Pure value-rewrite — no
structural changes.

## Coverage

- `01..02` — base lowercase/kebab forms.
- `03` — already canonical (round-trip identity).
- `04..05` — uppercase / mixed-case branches via `to_ascii_lowercase`.
- `06` — `currentColors` (substring match must NOT trigger).
- `07` — rewrite inside at-rule body (recursive walk).
- `08` — multi-decl rule with mix of forms.
- `09` — blank input.
- `10` — empty decl value next to a target value (smoke for
  walk_decls_mut path).
