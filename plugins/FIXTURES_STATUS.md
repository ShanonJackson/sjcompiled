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

## Closed (continued)

- **[FIXED 2026-05-07] sheet ordering cluster — 6 fixtures**:
  `ct-xcss-css-map-atlaskit`, `ct-navigation-system-logo-xcss`,
  `ct-complex-runtime-combo`, `ct-css-null-literal-styles`,
  `ct-css-null-literal-variables`, `ct-editor-big`. Root cause:
  the Rust port's `visit_mut_jsx_element` ran `n.visit_mut_children_with(self)`
  BEFORE handler dispatch (children-first / exit-time), reversing
  the order of `hoist_sheet` calls relative to upstream's
  enter-time `JSXElement` / `JSXOpeningElement` visitors at
  `packages/babel-plugin/src/babel-plugin.ts:351-367`. Same atomic
  CSS rule set on both sides; only the `const _N = "..."` numeric
  labels were permuted.

  Two coordinated fixes (faithful to upstream):

  1. **`state.transform_cache` field** in
     `crates/babel-plugin/src/state.rs` — port of upstream's
     `state.transformCache: WeakMap<NodePath, true>` (`types.ts:210`,
     `babel-plugin.ts:118`). Keyed on JSXOpeningElement `Span`
     (the SWC analog of upstream's `NodePath<JSXOpeningElement>`
     granularity). Out-of-capture per STATE_MUTATIONS.md
     classification.

  2. **xcss-prop cache gate** at the head of
     `crates/babel-plugin/src/xcss_prop/mod.rs::try_handle_jsx_element`
     — `if state.transform_cache_has(opening_span) { return None; }`
     then `state.transform_cache_insert(opening_span)`, BEFORE
     any other early-bail (matches upstream `xcss-prop/index.ts:58-64`
     ordering: stamp before propPath/container lookup).

  3. **Enter-time dispatch flip** in
     `crates/babel-plugin/src/babel_plugin.rs::visit_mut_jsx_element`
     — handlers run BEFORE `n.visit_mut_children_with(self)`,
     mirroring Babel's enter visitors. After replacement, the
     explicit child walk is the SWC analog of `@babel/traverse`'s
     automatic descent into `replaceWith` results: xcss-prop's
     wrapper `<CC><CS/>{originalEl}</CC>` is recursed into, the
     inner re-enters the handler with its span already in the
     cache, short-circuits → terminates. css-prop doesn't need
     the cache because upstream `css-prop/index.ts:77` strips the
     `css` attribute pre-wrap; the inner re-entry sees no `css`
     attr → no-op.

  Audits performed before flipping (per DEV_LOOP.md drift-discipline):
  - `crates/babel-plugin/src/utils/hoist_sheet.rs::emit_hoisted_sheets`
    confirmed faithful: each sheet inserted at the SAME `insert_idx`
    pushes earlier inserts down, producing reverse-of-arrival body
    order — exactly Babel's `parentBody.filter(!isImport)[0]
    .insertBefore(...)` semantics with `path` re-evaluated each call.
  - `crates/babel-plugin/src/css_prop/mod.rs::try_handle_jsx_element:203`
    confirmed strips the css attribute pre-wrap (`el.opening.attrs
    .remove(attr_idx)`), matching upstream line 77.

  Verified: §6.5 JSON corpus holds at 476/477. /fixtures corpus
  282 → 288 parity (+6). Cargo lib+integration tests 511 passed,
  1 failed (the pre-existing
  `resolver::engine::tests::build_from_config_with_transforms_doesnt_break_default_resolution`).
  +6 parity.

## Deferred — architectural blockers

**Resolved 2026-05-07** — see "[FIXED 2026-05-07] sheet ordering
cluster — 6 fixtures" in the **Closed (continued)** section above.
The `transform_cache` port + enter-time dispatch flip landed in
this session. Cluster is closed.

## Closed (continued)

- **[FIXED 2026-05-07] scope-index snapshot staleness — Option 1
  (pre-pass)** — the `init_expr` field on `Binding`
  (`crates/babel-plugin/src/compat/scope.rs:216`) is a clone
  taken at `ScopeIndex::build` time, but upstream Babel reads
  `binding.path.node.init` LIVE every call
  (`packages/babel-plugin/src/utils/resolve-binding.ts:261`).
  The `ct-hover-display` fixture surfaced this: `tabStyles`'s
  init contains arrows whose params get rewritten by
  `normalize_props_usage` during `visit_mut_expr`, which fires
  AFTER `ScopeIndex::build`. The styled handler later resolved
  `tabStyles` and read the pre-rename snapshot — hash inputs
  diverged.

  **Fix (Option 1 of three considered):** hoist
  `normalize_props_usage` to a pre-pass that runs BEFORE
  `ScopeIndex::build`, so the clones captured into `init_expr`
  are post-rename. Two phases at `Program::enter`:
  1. Walk `module.body` for `ModuleDecl::Import` and call
     `record_compiled_import` (idempotent — strips API
     specifiers, so the children-walk re-call no-ops).
  2. With `state.compiled_imports` populated, walk every Expr
     and call `normalize_props_usage` on Compiled `css(...)` /
     `styled(...)` / `css\`...\`` / `styled\`...\`` sites.

  Implementation: new `PreNormalizeVisitor` struct in
  `crates/babel-plugin/src/babel_plugin.rs` + new method
  `BabelPluginVisitor::pre_pass_normalize_props_usage`, called
  from `visit_mut_program` between `scan_jsx_pragma_comments`
  and `ScopeIndex::build`. The `visit_mut_expr` retains its own
  `normalize_props_usage` call (idempotent on already-renamed
  arrows) for parity with upstream's per-Expr trigger and as a
  safety net for any Compiled call sites that escape the
  pre-pass (currently none — `PreNormalizeVisitor` recurses
  into all positions `visit_mut_expr` would).

  Doc-comment on `Binding::init_expr` and `destructured_init`
  in `compat/scope.rs` records the new invariant: any future
  in-place mutator that touches an `init_expr`-eligible Expr
  MUST be hoisted into the same pre-pass, OR the snapshot
  architecture itself revisited (Option 2 of the three
  considered).

  Cost: one extra full-module visit on `Program::enter` —
  negligible vs SWC's parser/traversal advantage; preserves the
  eager-Q1 lock (`compat/scope.rs:11-18`).

  Verified: §6.5 JSON corpus 476/477 holds. Cargo
  lib+integration 511 passed, 1 known-fail (the pre-existing
  `resolver::engine::tests::build_from_config_with_transforms_doesnt_break_default_resolution`).

  **`ct-hover-display` closed in same session — see next entry.**

- **[FIXED 2026-05-07] ct-hover-display invalid-DOM-prop walk
  drift (§6.8x retraction of §6.8g/§6.8h/§6.8p)** — the
  scope-index snapshot fix above exposed a SECOND, pre-existing
  drift in `styled_template`'s invalid-DOM-prop derivation.

  **Root cause:** upstream
  `packages/babel-plugin/src/utils/build-styled-component.ts:123`
  calls `getInvalidDomProps(meta.parentPath)` — a
  `path.traverse` over the styled CallExpr / TaggedTpl AST
  subtree. `path.traverse` walks the static AST; it does NOT
  auto-resolve identifier arguments. For `styled.div(tabStyles)`
  it sees only the literal `styled.div(tabStyles)` text — no
  `__cmplp.<name>` MemberExprs reachable through the
  `tabStyles` Identifier. `invalidDomProps = []` → no
  destructure.

  The Rust port was walking `opts.class_names`,
  `opts.variables[].expression`, and a post-extraction CSS
  node (§6.8g/§6.8h/§6.8p). All three carry post-CSS-extraction
  artifacts that include resolved-init expansions — i.e. the
  inlined contents of `tabStyles`'s init. Those carry
  `__cmplp.isDraggable` / `__cmplp.isDragging` refs which
  triggered a spurious
  `const { isDragging, isDraggable, ...__cmpldp } = __cmplp;`
  destructure and `...__cmpldp` spread (where Babel emits
  `...__cmplp` directly).

  **Proof harness** at `parity-harness/_drift_proof.mjs`
  (deleted post-fix) ran three cases:
  1. `ct-hover-display` smoking-gun: pre-fix Babel emits no
     destructure, SWC emits one — divergent. Post-fix:
     byte-equal.
  2. Minimal isolation (`css({ color: ({ isPrimary }) => … })`
     resolved into `styled.div(myStyles)`): same shape as case
     1 with one prop / one arrow / one resolution level.
     Pre-fix divergent, post-fix byte-equal.
  3. **Control: inline styled call**
     (`styled.div({ color: ({ isPrimary }) => … })` — no
     `css(...)` indirection). Babel's parentPath traversal
     CAN see `__cmplp.isPrimary` here; both engines emitted
     the destructure pre-fix; both emit the destructure
     post-fix. Byte-equal pre AND post-fix. This control
     proves the divergence was specifically about the
     identifier-resolution boundary, not the detection logic
     itself.

  **Fix:**
  - Renamed `StyledTemplateOpts::original_css_node` →
    `original_styled_call`. The field now stores the
    ORIGINAL styled CallExpr / TaggedTpl AST node (the
    `expr` `try_visit_styled` enters with), NOT a
    post-extraction CSS payload.
  - `styled_template`'s invalid-DOM-prop loop now walks
    ONLY `opts.original_styled_call` — exactly mirroring
    upstream's `getInvalidDomProps(meta.parentPath)`. The
    `class_names` and `variables[].expression` walks were
    DROPPED in full.
  - Caller in `crates/babel-plugin/src/styled/mod.rs`
    threads the full styled-call `expr`, not the extracted
    `css_node_expr`.

  **Verified:**
  - `/fixtures` triage: 289 parity (+1, the closed
    `ct-hover-display`), 0 divergence, 2 swc-throws (the
    documented WASM-panic cases).
  - §6.5 JSON corpus 476/477 lock holds.
  - `cargo test -p babel-plugin --release`: 511 passed, 1
    pre-existing resolver-test fail (unchanged).
  - All 3 proof-harness cases byte-equal.

  **Drift retraction note:** the §6.8g/§6.8h/§6.8p comments
  in `build_styled_component.rs` claimed the broader walk was
  "byte-equivalent to upstream's parentPath walk" and
  "matches upstream's source-order traversal". That claim
  was wrong: it never matched, the over-reporting was
  masked by the scope-index snapshot bug for the entire
  history of those changes. With the snapshot fix landed,
  the broader walk produced visible drift on the first
  fixture that exercised resolved-init expansion through a
  styled call. Comments rewritten to record the corrected
  understanding.



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

- **[FIXED 2026-05-07] ct-expression-export** — `compat::evaluation`'s
  CallExpression branch was a deopt-stub. Upstream's
  `@babel/traverse/lib/path/evaluation.js:312-342` folds calls whose
  callee is one of `VALID_OBJECT_CALLEES = ["Number","String","Math"]`
  (in member-callee form, e.g. `Math.max(...)`) or
  `VALID_IDENTIFIER_CALLEES = ["isFinite","isNaN","parseFloat",
  "parseInt","decodeURI",…]` (in identifier-callee form). The Rust
  port previously deopt'd unconditionally with a TODO comment. With
  `Math.max(base - 5, 0)` in `border-radius` no longer folding to `5`
  on the SWC side, the CSS-builder fell through to the dynamic
  CSS-variable path and emitted `border-radius:var(--_y28lkp)` +
  a `--_y28lkp: ix(Math.max(base - 5, 0), "px")` runtime, where
  Babel emits the static `border-radius:5px`. Ported the full
  CallExpression branch in `crates/babel-plugin/src/compat/evaluation.rs`:
  added a `Builtin` enum + `resolve_builtin_callee` mirroring upstream's
  callee-shape gate (including the binding-not-shadowed check on the
  identifier arm and the `INVALID_METHODS = ["random"]` exclusion on
  Math.*) + `apply_builtin` dispatching to f64 ops with JS-spec
  semantics (NaN propagation in Math.max/min, `Math.round` half-toward-
  +Infinity, `parseFloat`/`parseInt` with radix-prefix detection). +1
  parity.

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
parity               289  (+24 from baseline; 0 remaining ct-* divergences)
divergence           0
swc-throws           2
babel-throws         0
both-throw           2     ← negative-test fixtures (OK)
skipped-multifile    43    ← gated behind --include-multi
skipped-no-input     0
```

### ~~Drift detected — `build_styled_component` invalid-DOM-prop walk over-reports (1) — `ct-hover-display`~~ — CLOSED 2026-05-07

Closed in same session as the scope-index snapshot fix above.
See `[FIXED 2026-05-07] ct-hover-display invalid-DOM-prop walk
drift (§6.8x retraction of §6.8g/§6.8h/§6.8p)` in the **Closed
(continued)** section. Section retained below for the
historical triage record:

**The scope-index snapshot fix landed correctly** (closed
above). Hash inputs now match Babel byte-for-byte. But it
exposed a SECOND, pre-existing drift that had been masked by
the snapshot bug.

**Symptom:** for the input

```jsx
const tabStyles = css({
  '&:hover': ({ isDraggable }) => ({ ... }),
  ...({ isDragging }) => (isDragging ? { opacity: 0.1 } : {}),
});
export const Component = styled.div(tabStyles);
```

Babel emits `<C ...__cmplp ... />` (no destructure). The Rust
port emits `const { isDragging, isDraggable, ...__cmpldp } =
__cmplp;` and `<C ...__cmpldp ... />`. Both sides have the
same `__cmplp.isDragging` / `__cmplp.isDraggable` references in
the runtime body — only the destructure / spread target differs.

**Root cause:** upstream
`packages/babel-plugin/src/utils/build-styled-component.ts:123`
calls `getInvalidDomProps(meta.parentPath)`, where
`meta.parentPath` is the styled CALL EXPRESSION
(`styled.div(tabStyles)`). Babel's `path.traverse` walks the
AST subtree of that path; it does NOT auto-resolve identifier
arguments. So Babel sees only `styled.div(tabStyles)` —
literal text, no `__cmplp.<name>` MemberExprs reachable —
and `invalidDomProps` is `[]` → no destructure.

The Rust port at
`crates/babel-plugin/src/utils/build_styled_component.rs:330-367`
walks `opts.class_names`, `opts.variables[i].expression`, and
`opts.original_css_node` (the styled-call argument). All three
carry post-CSS-extraction artifacts that include resolved-init
expansions — i.e. the inlined contents of `tabStyles`'s init.
Those carry `__cmplp.isDraggable` / `__cmplp.isDragging` refs
which trigger the destructure.

The §6.8g / §6.8h / §6.8p comments at lines 332-366 explicitly
documented this as an INTENTIONAL extension to surface refs in
"dead branches" (e.g. `props.x ? undefined : null`) — but
that's drift: Babel wouldn't surface those either, since
`meta.parentPath` traversal also can't reach them.

**Status: drift flagged, not yet fixed.** Per CLAUDE.md
("Drift detected in X — `<Explanation>`"), surfaced here for
routing. Fix shape would be: walk ONLY the original styled
call expression (the AST node `try_visit_styled` receives as
`expr`), to match upstream's `meta.parentPath` granularity
exactly. Drop the class_names/variables/original_css_node
walks. Risk: any existing fixture that relied on the
"dead-branch surfacing" shape (the §6.8p case) would
regress; needs verification against the full corpus before
landing.

**Investigation needed before fixing:**
1. What fixture(s) does the §6.8p `original_css_node` walk
   currently power? `git log -p crates/babel-plugin/src/utils/build_styled_component.rs`
   should show the introducing commit.
2. Run that fixture against a "walk only `expr`" variant —
   does Babel actually surface the dead-branch refs? If not,
   §6.8p was over-engineering and dropping it is pure parity
   improvement.
3. Same audit for §6.8g (class_names) and §6.8h
   (class_names+variables ordering): do those serve real
   fixtures or are they hedges that introduced drift?

Closing `ct-hover-display` requires this audit + the surgical
walk-narrowing. Roughly 50 LOC + corpus re-verify.

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
