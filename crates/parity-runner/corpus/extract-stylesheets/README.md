# Corpus — `extract-stylesheets`

Each `*.css` file is one parity-runner input. The harness runs the
plugin (which is read-only — it just calls a callback per top-level
child), captures the emitted sheets in document order, joins them with
U+001E (record separator), and diffs the resulting string against the
JS pipeline's joined output.

Sheets are produced via `node.toString()` upstream — that means each
sheet has no leading `raws.before` and (for top-level decls) no
trailing semicolon. Both sides must agree byte-for-byte.

## Coverage

- `01..02` — single rule, multiple rules.
- `03` — at-rule with body.
- `04` — top-level declaration.
- `05` — mixed kinds at top level (decl, rule, atrule, comment).
- `06` — selectors with pseudos, descendant/sibling combinators.
- `07` — keyframes (nested rules with percentage selectors).
- `08` — `@charset` + rule.
- `09` — comment-only input.
- `10` — blank input.
- `11` — URL with query string.
- `12` — no trailing semicolons (raws.between/raws.semicolon edge cases).
