# Corpus — `parent-orphaned-pseudos`

Inputs are sourced verbatim from
`packages/css/src/plugins/__tests__/parent-orphaned-pseudos.test.ts`
where applicable, plus adversarial selectors covering pseudo-functions,
double-colon pseudo-elements, at-rule body recursion, and bare
selectors with no pseudos.

## Coverage

- `01..05` — verbatim from the upstream test suite (top-level orphan,
  already-nested, orphan-in-nested, combinator before, dangling pseudo
  with following nesting).
- `06..08` — verbatim from upstream (attribute then nesting, pseudo
  comma pseudo, tag comma pseudo).
- `09` — pseudo with parenthetical args (`:nth-child(2n+1)`) — exercises
  the Pseudo storage refactor (prefix-only `value` + parens rebuilt from
  child Selectors).
- `10` — pseudo-element double-colon `::before`.
- `11` — comment-only input (no rules).
- `12` — multi-rule at-rule body (recursion path).
- `13` — blank input.
