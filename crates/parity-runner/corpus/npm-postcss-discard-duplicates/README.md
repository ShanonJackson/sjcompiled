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
