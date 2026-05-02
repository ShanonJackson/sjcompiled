# corpus/sort/

End-to-end fixtures for `Stage::Sort` — the full `packages/css/src/sort.ts`
pipeline (`postcss-discard-duplicates@6 → mergeDuplicateAtRules → sortAtomicStyleSheet`).

Each individual stage already has its own corpus
(`sort-atomic-style-sheet/`, `merge-duplicate-at-rules/`,
`npm-postcss-discard-duplicates/`). These fixtures specifically exercise
the **composition** of all three: inputs where stage N's output becomes
stage N+1's input.

Run via:

```
cargo run -p parity-runner -- --stage sort --corpus crates/parity-runner/corpus/sort
```
