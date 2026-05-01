# Corpus — `atomicify-rules` (CRITICAL)

This is the plugin whose hash output becomes the class names. Every
input here exercises a distinct path through the algorithm; if any
diff appears, **stop the world** — class names in production builds
are about to rotate.

## Coverage

- `01..02` — top-level decls (single + multi).
- `03..05` — selector-shape variants: attribute-then-nesting, double
  `&`, comma list of pseudos.
- `06..08` — tag/class compounds, multi-selector lists, descendant
  chains.
- `09..11` — `&` placement edge cases (mid, end, doubled with
  combinator).
- `12..15` — at-rule containment: top-level `@media`, nested
  `@media`/`@media`, `@media` containing rule, doubly-nested with rule.
- `16` — full sweep of the atomicifiable at-rule names (`@container`,
  `@-moz-document`, `@layer`, `@supports`, `@starting-style`).
- `17` — ignored at-rules pass through verbatim
  (`@font-face`, `@keyframes`, `@page`).
- `18..19` — `!important` flag handling and the boolean-coerced value
  hash (matches upstream's `value + true` quirk).
- `20` — comment skipping at root, in rules, in at-rules.
- `21` — blank input.
- `22` — `@when`/`@else` clauses (atomicifiable; conditional rules).
- `23..24` — element + pseudo selectors, double-tag descendant.

## What's intentionally NOT here

Inputs that require **autoprefixer** to produce the upstream snapshot
(e.g. `user-select: none` on Edge 16). The bridge runs atomicify
*standalone* — autoprefixer is a separate Phase 7 stage and would
contaminate the diff.
