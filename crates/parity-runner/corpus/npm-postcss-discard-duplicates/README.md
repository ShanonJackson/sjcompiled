# Corpus — `npm-postcss-discard-duplicates` (v6)

Phase 5c. Distinct from the LOCAL `discard-duplicates` (Phase 4a) —
this is the npm package version pinned at 6.0.0 used by `sort.ts`.

## Coverage

- `01..02` — basic decl + atrule dedupe.
- `03` — distinct nodes (no-op).
- `04` — rule merge: earlier-rule decl matched by later-rule decl is
  removed, surviving decls stay.
- `05` — rule fully emptied by dedupe → earlier rule removed.
- `06` — blank input.
- `07` — three consecutive duplicates.
- `08` — nested dedupe (recurses into at-rule body).
- `09` — two `@media` whose only diff is inner comment **text**: JS
  `equals()` has no `comment` case, so the earlier atrule is removed.
  Regression entry for the comment-text drift fix.
- `10` — rule whose body holds a leading comment + a duplicated decl:
  `dedupeNode` strips the matching decl from the earlier rule, then
  `empty(node)` ignores comment-only bodies and removes the rule.
- `11` — `!important` flag distinguishes two otherwise-equal decls
  (exercises `important` short-circuit in `equals`).
- `12` — same decl repeated with different `raws.before` whitespace:
  `trimValue` collapses the difference, dedupe still fires.
- `13` — same `@media` repeated with different `raws.before` /
  `raws.afterName` whitespace: `trimValue` makes them equal.
- `14` — nested rule inside duplicate `@media` with differing inner
  **comment text**: comment text equality is by-type-only, so the
  whole earlier atrule subtree is removed.
- `15` — duplicate `@media` separated by a U+FEFF (BOM/ZWNBSP) in
  the second atrule's `raws.before`. JS `String.prototype.trim()`
  strips BOM; Rust's `str::trim()` does NOT. Locks in the
  `is_ecma_whitespace` predicate inside `trim_str`.
- `16` — statement atrule dedupe (`@import url(...);` x2). Both
  sides have `nodes === undefined` (Rust: `Node::nodes()` returns
  `None` because `AtRule.has_block` is false). Confirms `equals`
  treats two `nodes`-less atrules as equal when name + params + raws
  match, and confirms `dedupe` dispatches to `dedupeNode` for atrule.
- `17` — four consecutive identical `a { color: red; }` rules.
  Exercises the index-shifting behavior of `dedupe()`'s outer loop:
  the rightmost rule's `dedupeRule` removes ALL three earlier rules
  in one pass, then the outer loop iterates back over the now-shifted
  array and re-processes the survivor (idempotently). Single rule
  must survive.
- `18` — `@media`, unrelated `b { ... }`, `@media` (same params).
  The right-to-left scan in `dedupeNode(last, nodes)` must walk past
  the middle rule (`equals` returns false on type mismatch) and
  remove the first `@media`. The middle `b { ... }` survives.
- `19` — same `prop` + `value` but different `raws.value.raw` (one
  has an inline comment between `:` and value, the other doesn't).
  JS `equals` only compares the high-level `value` field, NOT the
  byte-preserving `raws.value.raw`, so the two decls are equal and
  the EARLIER one is removed. The LATER node's bytes survive —
  locks in the order-sensitivity of dedupe on raws-preserving inputs.
