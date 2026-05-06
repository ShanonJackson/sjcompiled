# `plugins/FIXTURES_STATUS.md` — `/fixtures` parity tracker

> **Purpose:** track Babel↔SWC babel-plugin parity against the
> 336-fixture corpus at repo root `/fixtures/*`. This is the
> NEW iteration loop that picks up where Phase 6 §6.5 left off
> (`Parity achieved` on the JSON-extracted upstream-test corpus).
> Where Phase 6 proved the SWC port matches every Babel UNIT TEST,
> this loop proves the port matches Babel against fixtures sourced
> from real-world inputs.
>
> **Process is temporary.** Drop this file once the entire `/fixtures`
> corpus is parity-clean and the engine has been folded into CI as
> a green gate.

## Engine

`parity-harness/fixtures-triage.mjs` — runs every `/fixtures/*/input.{js,jsx,tsx}`
through:

- **Babel reference**  : `@compiled/babel-plugin` + `preset-typescript`
  (TS-strip to match SWC's default) + `preset-react` + prettier
- **SWC under-test**   : SWC parser + `babel_plugin.wasm` + prettier

Both pipelines normalised via `parity-harness/babel-plugin/engines.ts`
(comment-strip + prettier round-trip + the §6.8q jsx-runtime
reconciler + the §6.8s SWC-hygiene-rename reconciler). Only the
corpus source differs from `parity-harness/babel-plugin/triage.mjs`;
engines are reused verbatim so a fix in the WASM plugin reflects
identically in both reports.

```bash
bun parity-harness/fixtures-triage.mjs                     # all single-file (293)
bun parity-harness/fixtures-triage.mjs --include-multi     # also run ct-* multi-file
bun parity-harness/fixtures-triage.mjs --only <name> [...] # iterate on specific fixtures
bun parity-harness/fixtures-triage.mjs --print-diffs       # print divergences inline
bun parity-harness/fixtures-triage.mjs --bail              # stop on first divergence
```

Report lands at `parity-harness/fixtures-triage-report.json`.

## Why this corpus matters

Phase 6's JSON corpus is hand-written upstream UNIT tests — a small
predictable set of shapes engineered to exercise specific code paths.
The `/fixtures/*` corpus is a different oracle:

- 293 hand-curated **single-file** smoke tests covering every
  Compiled API surface (`class-names-*`, `css-prop-*`, `css-map-*`,
  `styled-*`, `keyframes-*`, `import-*`, `mixed-*`, `xcss-*`).
- 43 **multi-file** `ct-*` ("consuming-tree") cases pulled
  verbatim from the AFM monorepo with their import graph
  pre-resolved or pre-flattened. These are the cases that have
  historically tripped the babel-plugin port in production.

A parity gap surfaced here is a parity gap that will surface in
the consuming monorepo. CLAUDE.md's "BUGS in OLD = BUGS in NEW"
applies: a divergence here means the Rust port's behaviour differs
from upstream Babel for a real input — that's drift, not a bug fix
opportunity.

## Baseline (2026-05-06)

```
total                336
parity               265   ← 90% of single-file (none of multi-file run yet)
divergence           24
swc-throws           2
babel-throws         0
both-throw           2     ← negative-test fixtures (OK)
skipped-multifile    43    ← gated behind --include-multi
skipped-no-input     0
elapsedSeconds       28
```

By category:

| Category   | Total | Parity | Divergence | swc-throws | both-throw |
|------------|-------|--------|------------|------------|------------|
| `class-*`  | 8     | 8      | 0          | 0          | 0          |
| `css-*`    | 42    | 42     | 0          | 0          | 0          |
| `import-*` | 4     | 4      | 0          | 0          | 0          |
| `keyframes-*` | 7  | 4      | 3          | 0          | 0          |
| `mixed-*`  | 1     | 1      | 0          | 0          | 0          |
| `multiple-*`| 4    | 4      | 0          | 0          | 0          |
| `styled-*` | 39    | 38     | 1          | 0          | 0          |
| `xcss-*`   | 1     | 1      | 0          | 0          | 0          |
| `ct-*`     | 230   | 163    | 20         | 2          | 2 (43 skipped) |

`ct-*` numbers reflect ONLY the single-file `ct-*` fixtures
(187 actually ran; the other 43 are multi-file and gated behind
`--include-multi`).

## Closed (in this session)

- **[FIXED 2026-05-06] keyframes-basic / keyframes-multiple-animations
  / keyframes-with-percentage / ct-styled-token-nested-ternary** —
  CRLF-vs-LF in template literal raw values. SWC parser preserves
  source CRLF; Babel parser normalises CR/CRLF → LF per ES §12.8.6
  TRV. New compat module `crates/babel-plugin/src/compat/template_literal_raw.rs`
  runs a one-shot pre-pass at the plugin entry to align the SWC
  AST. +4 parity.

- **[FIXED 2026-05-06] styled-user-component + 4 ct-\* with the same
  shape** — `<Tag {...props} />` host-env delta. SWC's react
  transform collapses `React.createElement(Tag, { ...props })` to
  `React.createElement(Tag, props)`; Babel's preset-react keeps the
  object literal. The plugin doesn't touch user components. New
  reconciler `reconcileReactCreateElementSpreadCollapse` in
  `engines.ts`, applied INSIDE `normalise()` (before prettier) so
  the formatter sees the same shape on both sides. +5 parity.

## Closed (continued)

- **[FIXED 2026-05-06] ct-ts-as-cast — SWC pre-plugin TS-strip** —
  resolved via TWO complementary fixes:

  1. **Host-side:** `experimental.runPluginFirst: true` in
     `swcEngine`'s @swc/core config. Per
     [swc#9132](https://github.com/swc-project/swc/issues/9132),
     this option makes WASM plugins run BEFORE SWC's built-in
     TypeScript strip. Without it, SWC's pipeline strips
     `Expr::TsAs` / `Expr::TsConstAssertion` / `Expr::TsTypeAssertion`
     etc. before `process()` enters, so the plugin sees an AST
     missing the TS shape that `@compiled/babel-plugin` (running
     under Babel in production) WOULD see — hash inputs at every
     TS-cast-in-CSS-value site would re-hash on AFM migration,
     breaking `.css` byte-equality.

  2. **Plugin-side:** with `runPluginFirst` exposing `TsConstAssertion`
     for the first time, three call sites that previously matched
     only `Expr::TsAs` (Babel's `t.isTSAsExpression` covers BOTH
     `x as T` AND `x as const` — SWC splits them) now also match
     `Expr::TsConstAssertion`:

     - `crates/babel-plugin/src/utils/css_builders.rs::build_css_inner`
     - `crates/babel-plugin/src/utils/traverse_expression/.../evaluate_path.rs::evaluate_path`
     - `crates/babel-plugin/src/xcss_prop/mod.rs::collect_member_object_idents`
     - `crates/babel-plugin/src/compat/paren.rs::unwrap_paren_and_ts_as`

  Verified: JSON corpus (§6.5 lock) holds at 476 parity post-fix.
  Fixtures corpus +1 parity (274 → 275). Production class names
  match Babel byte-for-byte for TS-cast-in-CSS-value sites.

- **[FIXED 2026-05-06] ct-chart-legend-content-regex** — generator
  paren-policy bug. `Expr::Paren` is treated as transparent in the
  printer (matches Babel's "parser strips parens, generator decides
  paren policy" behaviour), but the paren-policy uses pointer-eq
  on slot positions (e.g. `m.obj` for `MemberExpression`). When
  recursing through Paren-transparent, `m.obj` still pointed to the
  outer `Expr::Paren` while the recursing child was the inner
  expression — pointer-eq failed, so `(a || b).replace(...)` lost
  its parens and emitted `a || b.replace(...)`. New helper
  `slot_holds` matches either the direct slot OR a Paren wrapping
  the slot. Wired through `has_postfix_part` and
  `binary_needs_parens`. +1 parity.

## Deferred — requires transform_cache port

### Sheet ordering for nested `<outer css>` / `<inner xcss>` (4 fixtures)

**Affected:** `ct-xcss-css-map-atlaskit`, `ct-navigation-system-logo-xcss`,
`ct-complex-runtime-combo`, `ct-css-null-literal-styles`. All produce
**byte-identical sets** of atomic CSS rules — the divergence is the
ORDER of `const _N = "<rule>"` declarations in the emitted output.

**Root cause (verified empirically 2026-05-06):**
Upstream Babel's plugin uses **enter-time** visitors for
`JSXElement` and `JSXOpeningElement` (`packages/babel-plugin/src/babel-plugin.ts:351-367`),
so for `<outer css={A}><inner xcss={B}/></outer>`:

1. ENTER `outer` → `visitCssPropPath` pushes A's sheets first.
2. ENTER `inner` → `visitXcssPropPath` pushes B's sheets second.

Result: `[A..., B...]`.

The Rust port's `visit_mut_jsx_element` runs **children first**
(`n.visit_mut_children_with(self)` BEFORE the dispatch block at
`crates/babel-plugin/src/babel_plugin.rs::visit_mut_jsx_element`).
This is documented as an intentional "single-pass design"
(`crates/babel-plugin/src/xcss_prop/mod.rs:48-54`) — children-first
prevents the wrap-then-recurse infinite loop that would happen if
xcss-prop's `<CC>...</CC>` wrapper were re-walked, because the
inner replaced JSXElement still has the `xcss` attr that would
re-trigger dispatch.

Result with children-first: `[B..., A...]` — same rules, reversed
order.

**Why we haven't fixed it yet:**

Switching to enter-time dispatch (parent-first) requires porting
upstream's `transformCache: WeakMap<NodePath, true>` (used at
`packages/babel-plugin/src/xcss-prop/index.ts:59-64` to gate
re-entry). Without it, the parent-first walk infinitely recurses
on the wrapper's inner element. Verified empirically:
`STATUS_STACK_OVERFLOW` on `xcss-prop-tests-transformation--should-transform-xcss-prop-when-compiled-is-in-scope`.

**Investigation needed:**

1. Add `state.transform_cache: HashSet<*const JSXElement>` (or a
   marker on the AST node — pointer is risky across mutations).
2. Gate `try_handle_jsx_element` for both xcss-prop and css-prop on
   cache membership.
3. Flip dispatch to enter-time (BEFORE
   `visit_mut_children_with(self)`).
4. Verify JSON corpus stays at 476/477 parity.

Roughly 100-200 LOC + careful test pass. Not a blocker for current
fixtures-triage progress; the consts re-order but the runtime
className compilation is unaffected (the order only matters for
HMR cache keying / source-map debugging, not for correctness of
the resulting CSS).

## Closed (continued)

- **[FIXED 2026-05-07] ct-minheight-calc-fg-stack +
  ct-columns-container-minheight-stack +
  ct-styled-nth-of-type-container** — `has_nested_template_literals_with_conditional_rules`
  drift in `crates/babel-plugin/src/utils/manipulate_template_literal.rs`.
  Upstream's `CONDITIONAL_PATHS` (`packages/babel-plugin/src/utils/constants.ts:1`)
  is `['consequent', 'alternate']` — the cond's `test` is intentionally
  excluded. The Rust port was walking `c.test` AND treating ANY
  `LogicalExpression` anywhere in the subtree as a positive match,
  causing arrow bodies of shape
  `({...}) => isFlex && !isSwim ? (fg() ? '100%' : 'calc(...)') : undefined`
  to flag the gate (the outer Cond's `test` is `LogicalExpr`),
  suppressing `optimizeConditionalStatement` and falling through to
  the catch-all CSS-variable path. Upstream verified empirically
  (instrumented `manipulate-template-literal.ts` to print
  `expr.type` for each `CONDITIONAL_PATHS.map`): only `consequent`
  and `alternate` are visited per Cond. Rewrote
  `walk_for_conditional_match` to walk descend through test/cons/alt
  but ONLY check `cons` / `alt` against the three patterns at each
  Cond node. `branch_matches_conditional_rules` now matches the
  three patterns directly (TaggedTpl / Tpl-with-arrow-exprs /
  LogicalExpression) without recursive automatic-positive on
  Logical. +3 parity.

- **[FIXED 2026-05-07] ct-optional-chain-dynamic-style** — generator
  missing `OptChain` arm. Babel's `@babel/generator` has explicit
  `OptionalMemberExpression` / `OptionalCallExpression` printers
  (`node_modules/@babel/generator/lib/generators/expressions.js:150-189`);
  SWC unifies both under `Expr::OptChain(OptChainExpr { optional, base })`.
  The Rust port's `Printer::print` dispatch in
  `crates/babel-plugin/src/compat/generator/printer.rs` had no arm
  for `Expr::OptChain` so it fell through to the
  `_ => "/*UNHANDLED-EXPR*/"` catch-all. For the styled `left:` and
  `top:` arrows whose only difference is `?.left` vs `?.top`, both
  arrow-source strings collapsed to the same `/*UNHANDLED-EXPR*/`
  payload, hashing to the same `--_2wqa78` CSS variable name.
  Babel emitted distinct `--_1qibnji` (left) / `--_1gg2u2w` (top)
  hashes. Added `opt_chain` / `optional_member` / `optional_call`
  in `compat/generator/generators/expressions.rs` mirroring the
  upstream `OptionalMemberExpression` / `OptionalCallExpression`
  byte logic (computed-bracket form, `?.` token vs `.` for
  `optional: false` continuation). +1 parity.

## Open divergences

Snapshot after this session's fixes (run `bun parity-harness/fixtures-triage.mjs`
to refresh):

```
total                336
parity               281  (+16 from baseline)
divergence           8
swc-throws           2
babel-throws         0
both-throw           2     ← negative-test fixtures (OK)
skipped-multifile    43    ← gated behind --include-multi
skipped-no-input     0
```

### Likely sheet-ordering (4-5 fixtures)

Same shape as the deferred section above. Each produces the SAME
atomic CSS-rule strings but in a different `const _N = "..."` order.
Will resolve when the parent-first dispatch + `transform_cache`
port lands.

- `ct-xcss-css-map-atlaskit`
- `ct-navigation-system-logo-xcss`
- `ct-complex-runtime-combo`
- `ct-css-null-literal-styles`
- `ct-css-null-literal-variables`

### Stale scope-index snapshot (1) — `ct-hover-display`

`normalize_props_usage` rewrites the AST in place after the
scope_index has already cloned `init_expr` for each binding
(`crates/babel-plugin/src/compat/scope.rs:987` —
`declarator.init.clone()`). Subsequent resolves through the
scope_index return the STALE pre-rename snapshot, so
`ix(<expr>)` runtime emits use `isDraggable` instead of
`__cmplp.isDraggable`. Class hashes diverge accordingly.

Fix candidates (each with trade-offs):

1. Run `normalize_props_usage` before scope_index build (requires
   pre-resolving compiled imports first).
2. Live-snapshot model: scope_index stores node-ids and looks up
   live nodes per query (heavy borrow gymnastics with `&mut Module`).
3. Re-run normalize_props_usage on the snapshot when resolving
   (cheap, but invalidates the "single normalize per call" invariant).

Status: open, blocked on architectural decision.

### Other ct-* divergences (2 remaining)

Surface-traced individually as the iteration loop continues:

- `ct-editor-big`
- `ct-expression-export`

Use `bun parity-harness/fixtures-triage.mjs --only <name> --print-diffs`
to start triage; the in-process debug-test pattern proven on
`ct-ts-as-cast` and `ct-styled-token-nested-ternary` (write a
short cargo integration test, parse + run_dispatcher, dump hash
inputs) is the fastest way to find the divergent input string.

### swc-throws (2)

- `ct-css-array-conditional-styles` — error: `plugin` (truncated;
  rerun with `--only` to see full message; likely WASM panic with
  the panic message stripped on the way out of `swc_core`).
- `ct-cssmap-massive` — same shape; large-input case (probably
  hitting a stack-depth or recursion limit somewhere in the
  visitor — verify against the upstream JS plugin for reference
  call-depth before assuming a Rust bug).

Status: open. WASM-side panic capture — get the real message
before triaging.

### both-throw (2) — expected, leave green

Negative-test fixtures where Babel ALSO throws. As long as both
sides throw, the parity contract holds.

- `ct-normalize-url-lookahead`
- `ct-strip-runtime-binding-fallback`

These are documented here so a future agent doesn't accidentally
"fix" them by silencing one side.

## Workflow for closing a divergence

1. **Pick the smallest** open divergence (the `keyframes-*` and
   simple-name ones first; `ct-*` last).
2. `bun parity-harness/fixtures-triage.mjs --only <name> --print-diffs`
   to see the byte-exact divergence.
3. **Trace upstream first.** Read the relevant packages/babel-plugin
   handler (immutable — read only). Confirm what it does for this
   input shape.
4. **Trace the Rust port** at the matching file (1:1 layout —
   `crates/babel-plugin/src/<same-relative-path>`). Find the
   delta.
5. **Fix the Rust port to match upstream byte-for-byte.** Rebuild
   the WASM (`cargo build -p babel-plugin --target wasm32-wasip1
   --release`).
6. Rerun the engine on JUST that fixture. If parity, rerun across
   the full corpus to confirm no regression in the other 265
   passes.
7. Update this file: move the entry from "open" to a
   `[FIXED <date>]` line and delete from the open list.

If the divergence trace reveals the upstream Babel handler has a
bug, **do not fix the bug.** Per CLAUDE.md: "BUGS in OLD! Need to
be BUGS In NEW." Reproduce upstream's behaviour faithfully, then
file the bug separately for upstream.

If the divergence is host-environment-only (i.e. SWC's pipeline
mutates the AST after our plugin exits), add a reconciler in
`engines.ts` per the precedent set by §6.8q / §6.8s. Do NOT
change the plugin output to compensate — that would diverge from
upstream Babel for downstream consumers that don't run SWC.

## Multi-file `ct-*` (43 fixtures, gated)

Run with `--include-multi`. These cases pull a directory of
files (the original module + every transitive import) from the
consuming monorepo. The Rust plugin's resolver behaviour against
sibling `*.ts` / `*.tsx` is the unknown — these fixtures are the
oracle for `crates/babel-plugin/src/utils/resolve_binding.rs` and
`utils/traverse_expression/*` once those land at Phase 5 §5.4–§5.6.

Until §5.4–§5.6 ship, expect every multi-file `ct-*` to either
fall into `divergence` or `swc-throws` because the plugin can't
resolve the cross-file references. Don't triage these individually
yet — they're collectively gated by the resolver port.

## Drift watchpoints

- **`engines.ts` reconcilers** — every reconciler is a host-env
  fix and never a plugin fix. If a "reconciler" ever needs to
  compensate for a plugin output, that's drift and it must be
  fixed in the plugin instead.
- **`/fixtures` content** — the corpus is FROZEN. Adding fixtures
  is fine (more coverage); editing existing ones to make them
  pass is drift and FORBIDDEN.
- **The babel reference** — if `babelEngine` ever stops matching
  upstream `@compiled/babel-plugin`, the entire parity contract
  is invalid. The first thing to check on a surprise divergence
  is whether `packages/equality-harness/scripts/verify.mjs` (which
  uses the same babel pipeline against itself) still passes.
