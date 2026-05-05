# `plugins/` STATUS — checkpoint ledger

> **Purpose.** Single source of truth for where the SWC port stands.
> Read with `plugins/PLAN.md` (the design) and
> `plugins/READ_WRITE.md` (the WASI sandbox contract).
>
> Historical closure prose for Phase 0–5 was condensed 2026-05-05;
> see `STATUS.md.bak` (gitignored) or git history if you need the
> long-form rationale for a closed checkpoint.

## How to read this file

- **Status:** `☐` not started · `▶` in progress · `☑` done · `⚠` blocked
- Each row is a **checkpoint** (0%-or-100%, single-owner). If context
  is lost mid-checkpoint, the next agent re-reads this file and the
  artefacts and continues.

## Resume here

**Phase status:** 0 ☑ (modulo §0.10–§0.12 — Phase 5 gates, not Phase 4
blockers) · 1 ☑ · 2 ☑ · 3 ☑ · 4 ☑ (modulo §4.7 OUT OF SCOPE and §4.8
gated on Phase 6) · 5 ☑ · **Phase 6 §6.1–§6.7 ☑ (all 7 per-API
handlers shipped) · §6.8 ▶ (active triage)** · Phase 7+ ☐.

**Active checkpoint: Phase 6 §6.8** — full-corpus parity exit gate.
See "§6.8 active state" below for the current divergence baseline,
triage tooling, and next-step punch list.

**Independently shippable while §6.8 runs:** §5.7 (`included-files.json`
sidecar), §2.3(b) AST/comment-store mutations bundle.

### §6.8 active state (2026-05-05)

Full corpus run produced a true baseline of **7 parity / 407
divergence / 62 swc-throw / 1 babel-throw** across 477 fixtures.
Earlier "954/954 bun parity" cited in §5.6 was a pass-through oracle
(assert babel ≠ swc); §6.8 inverts it (assert babel == swc) and
surfaces real divergence.

**Triage tooling at `parity-harness/babel-plugin/`:**
- `triage.mjs` runs every fixture, emits categorised JSON
  (`triage-report.json`).
- `categorize.mjs` groups divergences by upstream test-file source.
- `dump-divergences.mjs` flat divergence + diff dump for grep.

These are investigation aids — they bypass the expect-divergence
assertion in `harness.test.ts`. Not test runners.

**Critical fix landed: stale fixtures regenerated.** The 477
extracted fixtures contained `@sjcompiled/react` import strings
(frozen pre-2026-05-04 fork-prefix revert). Our handlers match
`@compiled/react`; neither engine transformed any of those fixtures,
so the harness was producing trivial passthrough-vs-passthrough
"parity". `bun parity-harness/babel-plugin/extract-fixtures.mjs`
regenerated with the correct prefix; baseline above is post-fix.

**Harness alignment landed (`parity-harness/babel-plugin/engines.ts`):**
preset-typescript + preset-react (classic by default; automatic on
`@jsxImportSource` or `importReact === false`); `useSpread: true`;
strip ALL comments before prettier; always re-run prettier;
`preserveAllComments: false`. Phase 6 §6.8's gate is transform
correctness, not comment placement (Phase 7).

**Drift fix landed (§6.7 styled innerRef double-gate):**
`build_inner_ref_guard` was wrapping the inner `if (__cmplp.innerRef)
throw` in an outer `if (process.env.NODE_ENV !== 'production')`
runtime check; upstream emits ONLY the bare inner `if` (build-time
gated via `isDevelopmentEnv` — already mirrored in Rust at use site).
Snippet-slicing in `engines.ts::babelEngine` cuts on the FIRST
`if (process.env.NODE_ENV` substring, so the misplaced wrapper broke
81 fixtures. Removing the outer wrapper dropped throws 143 → 62.

### §6.8 cluster table (post-§6.8c re-triage; baseline parity=184/477, divergence=292, swc-throws=0)

| Cluster | Count | Likely root cause |
|---|---|---|
| `styled/behaviour` divergences | TBD | forwardRef body / sheet decls / runtime imports |
| `css-prop/object-literal` | TBD | wrapper emit-shape |
| `css-prop/behaviour` | TBD | same family as above |
| `keyframes/call-expression` | TBD | keyframes ref + dynamic-value handling |
| `styled/call-expression` + `tagged-template-expression` | TBD | sibling clusters of styled/behaviour |
| Color-name minification (`black` → `#000`, `white` → `#fff`) | many | lightningcss vs babel-postcss-css-syntax minifier divergence; cosmetic only |

NOTE: post-§6.8c the swc-throws column is empty (0 throws across 477).
Run `bun parity-harness/babel-plugin/triage.mjs` and re-categorise the
remaining 292 divergences before picking the next cluster.

### §6.8 punch list (next agent)

1. **Run `bun parity-harness/babel-plugin/triage.mjs`** to confirm
   baseline (7/407/62/1 at HEAD).
2. **§6.8a ☑ — `appendRuntimeImports` + React + forwardRef wired in
   `Program::exit`.** See `crates/babel-plugin/src/utils/append_runtime_imports.rs`
   (~125 LOC + 6 unit tests). Lib tests 421 → 427. **Cluster delta = 0**
   because: ~163 fixtures use `snippet: true` slicing that strips
   `import` statements; remaining ~314 had React renamed to `React1`
   by SWC's resolver. `§6.8a-i` fixed React rename via `program_scope_ctxt`
   threading the program-scope `SyntaxContext` into the inserted
   `Ident("React")` (92% of affected fixtures clean). Edge case: 32
   fixtures with no top-level Idents fall back to `SyntaxContext::empty()`
   and still rename — rare, of questionable production shape.
3. **Investigate the class-names tagged-template panic.**
   `class_names::CssCallReplacer` panics on
   `<div className={css\`${X}\`}>`-shaped templates ("failed to invoke
   plugin on 'None'"). Add a unit test, find the panic site, fix 1:1
   against upstream.
4. **Cluster-by-cluster fixes** — pick the top remaining group, look
   at 3-5 sample diffs, identify the shared root cause, fix in Rust,
   re-triage. Each fix is a §6.8a/b/... sub-checkpoint.
5. **`importSources` plugin option** — parse it from `PluginOptions`,
   thread to the `is_compiled_*_call_expression` matchers so they
   recognise renamed imports.
6. **§6.8a-iii ☑ — Specifier removal (this session, 2026-05-05).**
   See `crates/babel-plugin/src/babel_plugin.rs::record_compiled_import`
   (now `&mut`; uses `Vec::retain` to drop matched API specifiers
   in-place per `babel-plugin.ts:280-294`) +
   `remove_empty_compiled_imports` (post-children-walk filter that
   drops any Compiled-source `ImportDecl` with empty specifiers,
   covering both the "drained-by-strip" and "side-effect import"
   shapes). Two new unit tests:
   `record_compiled_import_keeps_unrecognised_specifiers_intact`
   (jsx + non-API specifiers preserved) and
   `remove_empty_compiled_imports_drops_emptied_imports`. Eight
   existing phase6a/6b/6c visitor tests updated for the new body
   shape. Lib tests 427 → 433.
7. **Once parity is at full corpus minus documented carve-outs**
   (comment-disable directive — §6.5; styled-in-arrow displayName —
   §6.7), flip `harness.test.ts:143-155` from expect-divergence to
   expect-parity, allowlist the carve-outs, and close §6.8.

**§6.8a-ii ☑ — sheet hoist emitter at `Program::exit` (this session,
2026-05-05).** Added
`crates/babel-plugin/src/utils/hoist_sheet.rs::emit_hoisted_sheets`:
reads `state.sheets()` (populated during the children walk by
`hoist_sheet`) and inserts a
`const <hoisted_name> = "<sheet>";` `ModuleItem::Stmt(VarDecl)` for
each, immediately before the first non-`ImportDeclaration` body
item — same insertion point as upstream's
`parentBody.filter(p => !p.isImportDeclaration())[0].insertBefore(...)`.
IndexMap insertion order preserved. Wired from
`babel_plugin.rs::visit_mut_program` exit AFTER the runtime / React /
forwardRef injections (so the "first non-import" target shifts
predictably). Edge cases: empty sheets → no-op; all-import module →
append at end (Babel skips the AST insert here; we emit defensively;
no fixture surfaces this shape today).

Four new unit tests in `hoist_sheet::tests`
(`emit_no_op_when_sheets_empty`,
`emit_inserts_const_before_first_non_import`,
`emit_appends_when_module_is_all_imports`,
`emit_preserves_indexmap_insertion_order`). Four existing phase6c
styled visitor tests updated to expect the new sheet-const item in
their post-exit body shape. Lib tests 421 → 427 then to 431 with
new tests.

**§6.8a-iii ☑ — Compiled import-specifier removal (this session,
2026-05-05).** Per item 6 above. Combined cluster delta after both
landings: re-running `bun parity-harness/babel-plugin/triage.mjs`
moved the report from `parity=7 / divergence=407 / swc-throws=62 /
babel-throws=1 / both=0` to `parity=10 / divergence=404 /
swc-throws=62 / babel-throws=0 / both=1`. The raw count change
understates the fix: pre-fix, the SWC output emitted a dangling
`[_0]` reference with no `const _0 = "..."` declaration — broken JS
that prettier still formatted, scored as "divergence". Post-fix the
declaration emits and the structural shape matches Babel; the
remaining divergences are now CONTAINED to:
- **§6.8a-iv ☑ — UID-name singleton format** (this session,
  2026-05-05). `state.rs::next_uid_name` rewritten to be 1-based
  with the suffix suppressed when `i == 1`, matching Babel's
  `_generateUid(name, i) { let id = name; if (i > 1) id += i;
  return '_' + id; }` shape. First call returns `_`; subsequent
  calls return `_2`, `_3`, ... — exact match for upstream when
  there are no `_<n>` user-source collisions (the common case).
  Six existing `hoist_sheet::tests` updated for the new
  expectations. Lib tests stay 433/433. **Triage delta: parity
  10 → 58, divergence 404 → 356** (+48 fixtures move from
  divergence to byte-equal parity in one swing).
- **§6.8a-v ☑ — hoist insertion-order parity** (this session,
  2026-05-05). Reframing of the original "multi-sheet UID counter"
  hypothesis: probing two representative fixtures
  (`css-prop/object-literal/should-inline-the-variable-when-it-is-a-constant-in-string-css`
  and `keyframes/call-expression/...longhand-syntax`) confirmed the
  UID NUMBERS are sequential in both engines (`_, _2, _3, _4, _5`
  on both sides — no collision-walk divergence). The actual
  divergence is body ORDER: Babel emits `_5, _4, _3, _2, _`
  (reverse of arrival) while SWC was emitting `_, _2, _3, _4, _5`.
  Root cause: upstream `hoistSheet` re-evaluates
  `parent.get('body').filter(p => !p.isImportDeclaration())[0]`
  on EVERY call — after the first hoist lands, the just-inserted
  VarDecl IS the new "first non-import", and the next
  `path.insertBefore(...)` targets it, pushing the new sheet in
  front of the old. Net effect: reverse-of-arrival.

  Fix: `emit_hoisted_sheets` now inserts at the SAME `insert_idx`
  for every sheet (no per-iteration offset), so each new insert
  pushes the previous ones backward — same body order Babel
  produces. Two `emit_*` unit tests updated for the new shape.
  Lib tests stay 433/433. **Triage delta: parity 58 → 87,
  divergence 356 → 327** (+29 byte-equal fixtures).

  The genuine collision-walk concern (Babel's `Scope.generateUid`
  loop checking `hasBinding(uid)` / `hasReference(uid)`) is still
  open as a §6.8a-vi follow-up if any fixture surfaces user-source
  `_<n>` collisions in practice. None of the 477 corpus fixtures
  appear to need it.
- **Residual React→React1 rename** in fixtures with no top-level
  Idents (the `program_scope_ctxt` fallback to
  `SyntaxContext::empty()` from §6.8a-i). Same edge case as flagged
  there.
- **§6.8a-vi ☑ — evaluator wired into extract_object_expression
  + extract_template_literal** (this session, 2026-05-05). Root
  cause for the keyframes / css-prop / styled CSS-extraction
  defects: `crates/babel-plugin/src/utils/css_builders.rs` had
  TWO stubs that bypassed the upstream `evaluateExpression(prop.value, meta)` /
  `evaluateExpression(nodeExpression, meta)` calls — the
  property-value path used `&*kv.value` directly (line 727
  comment: "Stubbed at the boundary"), and the template-literal
  interpolation path used `&node.exprs[index]` directly (line
  1118 `let _ = node_expression`). With both stubs, references
  like `animationName: fadeOut` (where
  `fadeOut = keyframes({...})`) bypassed the keyframes matcher
  and fell through to the `--_<hash>` CSS-variable catch-all,
  producing `var(--_xxx)` plus a dangling `style={...}` instead
  of hoisting the `@keyframes` sheet and inlining the generated
  keyframes name (`k1mv9s16` etc.).

  Fix: 1:1 ports of upstream's two `evaluateExpression(...)` call
  sites. The Rust port now calls
  `evaluate_expression(&kv.value / &node_expression, meta,
  scope_index, parent_scope, own_scope)`, owns the resulting
  `Box<Expr>`, and references it as `prop_value` / `evaluated_interp`
  for the rest of the prop iteration. Output flows through the
  recursive resolver → the keyframes call is detected via
  `is_compiled_keyframes_call_expression` → `extract_keyframes`
  emits the `@keyframes` sheet and returns the keyframes name as
  the value.

  One existing test (`hash_site_extract_object_expression_variable_name`)
  used a stub-era input shape (`UnaryExpression(-1)`) that relied
  on the resolver bypass; updated to use an unresolved Ident
  which correctly flows through the babel-evaluator fallback to
  reach the catch-all.

  **Knock-on fix: `compat::evaluation::evaluate` TaggedTemplate
  branch.** With the evaluator wired in, the
  `extract_object_expression` call site started invoking
  `babel_evaluate_expression(target)` on TaggedTpl values
  (e.g. `keyframes\`...\`` shorthand fixtures) when value-resolution
  returned None. The `compat/evaluation.rs` TaggedTpl branch
  panicked with `unimplemented!()` based on the (correct-at-the-time)
  premise that `evaluate-expression.ts:184` short-circuits Compiled
  tagged templates before reaching this evaluator — but the §6.8a-vi
  wiring legitimately reaches it for the babel-evaluator FALLBACK
  call. Replaced the panic with `deopt(state); None` per upstream
  Babel's behaviour for non-`String.raw` tagged templates (Babel's
  `path.evaluate()` returns `{confident: false}`, the JS try/catch
  wrapper returns `fallbackNode`).

  **Triage delta: parity 87 → 118 (+31), divergence 327 → 296
  (-31), swc-throws 62 → 62 (4 new keyframes-tagged-template
  panics resolved by the TaggedTpl deopt fix; net no change).**
  Cumulative across this session: parity 7 → 118 (+111, 23.3% of
  477); divergence 407 → 296 (-111).
- **§6.8b ☑ — three port-completions on stubbed code paths
  unblocking the swc-throws cluster** (this session, 2026-05-05).
  Three 1:1 ports of upstream branches that were stubbed at
  module boundaries:

  1. **`utils/object_property_to_string.rs::expression_to_string`
     Identifier/MemberExpression branch.** Was an
     `unimplemented!()` SHELL panic citing "Phase 4 Phase 6 rewires
     this call site". Replaced with the upstream
     `evaluateExpression(expression, meta)` call (object-property-to-string.ts:57-67),
     followed by recursive `expressionToString` on the folded value
     or a `Cannot statically evaluate` throw on deopt
     (`ResultPair::value == None`). Threading required adding
     `scope_index / parent_scope / own_scope` to
     `expression_to_string`, `template_literal_to_string`,
     `binary_expression_to_string`, and `object_property_to_string`
     (matching the §5.5 explicit-param trio convention). Single
     call site updated in `css_builders.rs::extract_object_expression`.
  2. **`utils/css_builders.rs::build_css_inner` Identifier branch.**
     Was a §4.6 stub that called `resolve_binding(...)` and threw
     the result away, then fell through to the catch-all "unable
     to extract" error. This was the root cause of the styled (33)
     + css-prop (18) swc-throw clusters: every fixture using
     `styled.div([identifier, ...])`, `styled.div(identifier)`, or
     `<div css={identifier} />` panicked. Replaced with the full
     upstream branch (build-css.ts:992-1024): resolve_binding;
     throw if `None`; throw if `binding.node` is `None`; cssMap
     collision check; recurse `build_css_inner` on the resolved
     init expression with the appropriate scope (same-file: keep
     the current `scope_index`; cross-file: build a fresh
     `ScopeIndex` from `imported_module` and route through its
     program scope, mirroring §5.6's cross-file dispatch);
     `assertNoImportedCssVariables` post-check (throw if an
     imported binding produced CSS variables).
  3. **`utils/css_builders.rs::extract_template_literal`
     `canBuildExpressionAsCss` arm.** Was missing entirely (the
     Rust port jumped straight from "evaluator wired" to "keyframes
     branch" to "catch-all CSS variable"). This was the root cause
     of the class-names tagged-template-expression cluster: every
     fixture using `` css`${color}` `` (where `color` evaluates to
     an ObjectExpression / Compiled CSS call / Compiled CSS tagged
     template) reached the `--_<hash>` CSS-variable path with a
     non-scalar value. Ported upstream lines 803-838 verbatim:
     compute `does_expression_contain_css_block` /
     `does_expression_have_conditional_css` /
     `can_build_expression_as_css`; if true, recurse
     `build_css_inner` on the evaluated interpolation (with
     `MetadataContext::Fragment` swapped in for the template-literal
     sub-recursion case); on success, push the
     accumulated-prefix-quasi as `Unconditional` then extend with
     the recursive result's css + variables; reset `acc` and
     `continue`.

  **Triage delta: parity 118 → 132 (+14), divergence 296 → 308
  (+12), swc-throws 62 → 36 (-26).** 26 fixtures moved out of
  swc-throw — 14 produce byte-equal output, 12 produce
  not-yet-byte-equal output that lands in divergence (cluster-by-
  cluster follow-ups). Lib tests stay 433/433. Cumulative across
  this session: parity 7 → 132 (+125, 27.7% of 477); divergence
  407 → 308 (-99); swc-throws unchanged-then-down 62 → 36 (-26).
  Remaining swc-throws by cluster: styled 20, css-prop 10,
  class-names 6.
- **§6.8c ☑ — Babel↔SWC paren-shim + 6 port-completions
  zeroing out the swc-throws cluster** (this session, 2026-05-05).
  Six fixes that drove `swc-throws 36 → 0` and `parity 132 → 184`:

  1. **`compat/paren.rs` — Babel parser strips
     `ParenthesizedExpression` by default; SWC keeps it.** New
     `unwrap_paren` / `unwrap_paren_and_ts_as` helpers wired into
     `evaluate_expression`'s `target_expression` normalisation,
     `is_compiled_*` predicates, `extract_template_literal`'s
     `Expr::Object`/conditional/logical pattern matches, and
     `build_css_inner`'s arrow-body match. Without this every
     `() => ({...})` (arrow returning object) deopted via the
     catch-all CSS-variable path because `arrow.body` was
     `Paren(Object)` instead of `Object`. Single biggest cluster
     fix — 17 throws cleared in one swing.
  2. **`compat/scope.rs::register_fn_decl` synthesised
     `init_expr`.** Function declarations were registered with
     `init_expr: None`, so `traverse_identifier` couldn't fold
     them. Babel's `binding.path.node` IS the FunctionDeclaration
     and `t.isFunction(FunctionDeclaration)` returns true, so
     upstream `evaluateExpression(binding.path.node)` flows into
     `traverseFunction`. Fix: synthesize an `Expr::Fn(FnExpr)`
     wrapping the same `Function` body — gives evaluator's
     `Expr::Fn|Arrow` arm something to fold.
  3. **`utils/css_builders.rs::extract_member_expression_optional`
     fallback branch — port-completion of the `evaluateExpression
     + buildCss` re-dispatch.** Was a §4.6 stub that discarded the
     ResultPair. Replaced with the full upstream behaviour
     (build-css.ts:746-749): evaluate the member expression,
     recurse `build_css_inner` on the folded value. Cleared all 3
     `css-prop/object-literal` `extract-collocated-mixin-from-*`
     panics.
  4. **`utils/css_builders.rs::extract_template_literal`
     mutable-quasi-raw walk + `suffix: after.variable_suffix`.**
     Upstream mutates `nextQuasis.value.raw = after.css`
     (build-css.ts:874) so the next iteration sees the
     suffix-stripped quasi. The Rust port walked `node.quasis` by
     `&` borrow; the mutation was deferred. Replaced with a
     parallel `Vec<String>` of working raws (mutated in place), and
     plumbed `after.variable_suffix` through to the emitted
     `Variable.suffix` (was always `None`). Without this, e.g.
     `content: "${dynamic}";` kept the closing `"` in the CSS,
     producing `content:"var(--_x)"<unclosed string>` and a
     parse-error panic. Single change cleared 8 styled
     suffix/prefix throws AND moved 13 fixtures into byte-equal
     parity.
  5. **`utils/css_builders.rs::extract_branch` Identifier arm —
     port-completion of conditional-CSS resolved-binding handling.**
     Was a §4.6 stub that discarded the evaluator result.
     Replaced with the full upstream behaviour (build-css.ts:374-385):
     `resolve_binding(ident)`; if resolved init is a Compiled
     `css(...)` call or `css\`...\`` tagged template, recurse
     `build_css_inner` on it. Cleared the `${(p) => p.x ? dark :
     light}` cluster.
  6. **`utils/css_builders.rs::extract_template_literal`
     StringLiteral/NumericLiteral fast-inline branch.** Was missing
     entirely. Ported upstream lines 798-801 verbatim: when
     evaluator returns a `Lit::Str` / `Lit::Num`, push directly
     into `acc` and `continue`. Without this, `${big}` where
     `big = \`...css...\`` (tpl-no-exprs folded to StringLiteral
     by `babel_evaluate`) reached the catch-all CSS-variable path.
  7. **`styled/mod.rs` invalid-expression check ordering.** The
     `has_invalid_expression(tpl)` panic ran BEFORE
     `extract_styled_data_from_node` returned `None` for non-
     Compiled tags — so a `styled-components` tagged template
     panicked even when the user's Compiled binding was a
     different name. Reordered: extract data first; the panic is
     now only reachable for tags recognised as Compiled-styled.

  **Triage delta: parity 132 → 184 (+52, 38.6% of 477),
  divergence 308 → 292 (-16), swc-throws 36 → 0 (-36 — cluster
  fully cleared).** Lib tests 433 → 439 (+6 from `compat::paren`
  unit tests). All other integration suites unchanged
  (hash_parity 4/4, transform_css 3/3, compat_evaluation 3/3,
  compat_scope 3/3, resolver_matrix 8/8). Cumulative across
  this session: parity 7 → 184 (+177); divergence 407 → 292
  (-115); swc-throws 62 → 0 (-62, cluster cleared);
  babel-throws 1 → 0.

### Verifying the current state from a cold pickup

```bash
# Plugin unit + integration tests.
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 427/427 (post-§6.8a)
RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity              # 4/4 over 10037 entries
RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration  # 3/3 over 120 entries
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration  # 3/3 (55/55 byte-exact)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_scope_integration       # 3/3 (23/23 corpus)
RUSTFLAGS="" cargo test -p babel-plugin --test compat_evaluation_integration  # 3/3 (45/45 corpus)
RUSTFLAGS="" cargo test -p babel-plugin --test resolver_matrix_integration    # 8/8
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56
RUSTFLAGS="" cargo test -p compiled-utils --lib                         # 31/31
RUSTFLAGS="" cargo test -p compiled-css --lib                           # 163/163

# Bun parity harnesses.
bun test parity-harness/strip-runtime/harness.test.ts                   # 1132/1132
BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 \
  bun test parity-harness/babel-plugin/harness.test.ts                  # 954/954 (pass-through oracle)

# §6.8 inverted oracle (where the real work is):
bun parity-harness/babel-plugin/triage.mjs                              # 7/407/62/1 baseline

# CSS-port producer-side gate.
bun run packages/equality-harness/scripts/verify.mjs                    # 336/336 (run under bun, NOT node)

# Optional: regenerate gitignored corpora.
bun parity-harness/hash/oracle.mjs
bun parity-harness/transform-css/oracle.mjs
bun parity-harness/compat-generator/oracle.mjs                          # 55 entries
bun parity-harness/compat-scope/oracle.mjs                              # 23 entries
bun parity-harness/compat-evaluation/oracle.mjs                         # 45 entries
bun parity-harness/resolver-matrix/oracle.mjs                           # 4+ entries
```

### WASI/SWC tear-down constraint

SWC tears down the WASI instance between `transformSync` calls.
`BabelPluginVisitor` is allocated fresh per `process()`. NO module-
level state, NO static caches, NO `lazy_static`. The Phase 5
`cache.bin` design (PLAN.md §3.9.10) reads at `Program::enter` and
writes at `Program::exit` — filesystem is the only viable cross-
transform channel.

---

## Standing architectural locks (cite when in doubt)

These were debated, recorded, and are NOT to be re-derived without
escalation. If a divergence pushes you toward changing one, escalate.

### Compat layer (§5.0a/b/c — `plugins/COMPAT_SCOPE_AUDIT.md`)

- **Q1 — eager pre-index.** `Program::enter` builds binding map +
  parent-pointer map + reference-paths map. Read-only navigation
  during the visit pass. SWC `&mut self` visitors don't compose with
  live-scope mutation.
- **Q2 — scoped `&mut Expr` for the IIFE site only.** The single
  `replaceWith` site (IIFE wrap in `traverseCallExpression`) gets
  `&mut Expr` passed down. Rest of `evaluate_expression` returns
  `Resolved` and stays read-only.
- **Q3 — full line-by-line port of `path.evaluate()`.** Not partial-
  port-by-corpus. Evidenced-unreachable branches MAY emit
  `unimplemented!()` with citation to
  `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`.
- **Eager pre-index is an INTENTIONAL semantic delta vs Babel's lazy
  `init()/crawl()`** — documented at `compat/scope.rs`. Don't "fix"
  it. If a fixture surfaces `getBinding → mutate → getBinding`
  observability, see Finding 7 in COMPAT_SCOPE_AUDIT.md.
- **`get_binding` / `get_own_binding` breadcrumb requirement.** Every
  call site in `utils/{traverse_expression,evaluate_expression,resolve_binding}/*.rs`
  MUST carry the comment:
  ```rust
  // If a fixture surfaces lazy-crawl observability here, see
  // plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
  ```
  Verified by grep at PR time.

### Resolver (§5.4 — `plugins/RESOLVER_SPEC_PART_TWO.md`)

- The library is **Jira-agnostic**: one generic resolver whose every
  behaviour is parameterised in JSON. New consumer quirks become new
  `packageJsonTransforms` / `preferFirst` entries in the consumer's
  `.compiledcssrc`, NOT new flags in the engine.
- 5 transform ops only: `ensureObject`, `renameKey`, `renameMapEntry`,
  `setDefault`, `deleteKey`.
- Strings/functions for `resolver` REJECTED at config-parse with a
  hard error citing the spec. PLAN.md §1 constraint 1 (no JS
  callbacks from WASI plugin).
- WASI-safe: no caching layer beyond `oxc_resolver`'s per-instance
  package.json cache during a single transform. The host's
  `CachedInputFileSystem(fs, 4000)` is intentionally NOT replicated
  (PLAN.md §3.9.4 — instance teardown invalidates).

### Visitor / state

- `MutationRecorder::apply` is the only mutator on `State` for the
  5 captured mutation sites. Encapsulation lint enforces this:
  `state\.[a-z_]+\.(push|set|add|insert|remove|extend)` MUST be zero
  outside `state.rs` / `mutation_recorder.rs`.
- `MutationRecorder` threading: every `extract_*` / `build_css*` fn
  in `utils/css_builders.rs` takes `recorder: &mut MutationRecorder`
  (landed §6.5). Same shape for any future fn that records.

### Sidecars (`plugins/SIDECAR_SCHEMA.md`)

- **`version: 1`** on every per-call JSON sidecar.
- **Cardinal rule:** versioned + mismatch = loud failure (sidecars).
  `cache.bin` schema-hash mismatch = silent wipe (regenerable).
- Plugin-owned `cache.bin`; host's only contact is `rmSync` on worker
  exit. Host MUST NEVER read or write it.

---

## Standing bug-parity flags & known divergences

Per CLAUDE.md "BUGS in OLD = BUGS in NEW". Don't "fix" these without
explicit authorisation.

- **`traverse_call_expression` IIFE wrap NOT persisted into AST.**
  JS Babel uses `replaceWith`; Rust uses transient `ScopeId` +
  `meta.own_scope_override`. May affect runtime-CSS-fallback
  emission on the deopt path. If a fixture surfaces byte-divergence,
  fix at §5.6 evaluator boundary, NOT in `traverse_call_expression`.
- **`is_css_prop_disabled` returns `false` (§6.5 incomplete).**
  Comment-disable directives (`@compiled-disable-line transform-css-prop`)
  WILL produce divergent output until the SourceMap-thread followup.
  Documented in `crates/babel-plugin/src/utils/comments.rs`.
- **§6.7 styled call inside arrow body inside VarDecl init has NO
  displayName emitted.** Upstream's `findParent` walk would fire
  there; Rust uses `visit_mut_var_decl` pre-walk. Rare in practice;
  scope-of-the-pre-detect divergence documented in `babel_plugin.rs`.
- **§6.3 cssMap object-property keys.** SWC `Ident` can't hold spaces
  /parens, so upstream's `t.identifier('@media screen and ...')`
  becomes a string-literal key (`PropName::Str`). Bytes equal because
  consumers read via `get_key_value`.
- **`manipulate_template_literal.rs` `[;|{|}]` regex.** Literal `|`
  inside char class — looks like a typo, ships in production.
  Documented at the regex declaration.
- **`extract_object_expression` arrow.body mutation, `extract_template_literal`
  nextQuasis.value.raw mutation, `extract_member_expression` map-cache
  miss.** Phase 5/6 mutable-walker work; not load-bearing today.

---

## Phase 0 — Prerequisites and parity harness

> **Goal:** stand up the verification oracle, validate every load-
> bearing architectural assumption, before any port code lands.
> **Phase exit gate:** ☑ — Phase 1 prereqs met. §0.10/§0.11 are Phase
> 5 gates; §0.12 cross-platform CI is hardening.

| ID | Status | Checkpoint | Verification |
|---|---|---|---|
| §0.1 | ☑ | Pin `swc_core@54.0.0` and `prettier@2.8.8` | `bun pm ls \| grep swc/core` shows `1.15.8` |
| §0.2 | ☑ | Scaffold `crates/babel-plugin` + `crates/babel-plugin-strip-runtime` for `wasm32-wasip1` | WASI cdylib builds clean |
| §0.3 | ☑ | Build `crates/babel-plugin/STATE_MUTATIONS.md` + reconcile §3.9.8's 5-variant `StateDiff` | grep over `packages/babel-plugin/src/` matches table |
| §0.4 | ☑ | Build §3.9.14 probe plugin; run probes 1–7 on Windows | `bun test phase0-probes/probes.test.ts` → 7 pass |
| §0.5 | ☑ | Document WASI `/cwd` mount semantics in PLAN.md §3.2 | Doc updated |
| §0.6 | ☑ | Stand up `parity-harness/strip-runtime/` skeleton | harness driver runs |
| §0.7 | ☑ | Babel-vs-itself determinism baseline | 3 baseline tests pass |
| §0.8 | ☑ | Seed 3 representative strip-runtime fixtures | fixtures present |
| §0.9 | ☑ | Confirm harness can detect drift | 2 expectedToFail tests pass-because-divergent |
| §0.10 | ☐ | Build `scripts/audit-included-files.ts` (≤100 outliers per workspace) | Phase 5 gate |
| §0.11 | ☑ (folded into §5.4a) | Resolver difference matrix | See §5.4a row |
| §0.12 | ☐ | Run probes 1–7 on Linux + macOS (CI matrix) | hardening |

### Phase 0 lessons (write-once)

- WASI mount path is `/cwd`, not `/`. Plugin must use `/cwd/<rel>`.
- User-global `RUSTFLAGS=-C lto=thin` breaks proc-macro deps. Build
  SWC plugin crates with `RUSTFLAGS=""`. Documented in Cargo.toml.
- `StateDiff` enum is 5 variants, not 4 (added `IgnoreMemberExprMark`).
- Bun's caret resolution drifts past `package.json` pins. Use root
  `overrides` + top-level `devDependencies` for byte-affecting deps
  (the §4.2 lesson — bun's isolated layout silently bypasses
  overrides for transitive deps).

---

## Phase 1 — `babel-plugin-strip-runtime` 1:1 port ☑

> **Goal:** ship a `wasm32-wasip1` SWC plugin byte-equivalent to
> `packages/babel-plugin-strip-runtime` after prettier normalisation.
> **Phase exit gate:** ☑ — 38 strip-runtime tests + 1000 synth pass.

| ID | Status | Checkpoint |
|---|---|---|
| §1.0 | ☑ | Extract all 38 fixtures from existing test files |
| §1.1 | ☑ | Port `utils/to_uri_component.rs` |
| §1.2 | ☑ | Port `utils/is_*` predicate utils |
| §1.3 | ☑ | Port `utils/remove_style_declarations.rs` + `compat/scope.rs` |
| §1.4 | ☑ | Port `lib.rs` entry + dispatcher; lock `Program::exit` ordering |
| §1.5 | ☑ | Sidecar handlers: `compiledRequireExclude` + `extractStylesToDirectory` |
| §1.6 | ☑ | Lock `plugins/SIDECAR_SCHEMA.md` v1 |
| §1.7 | — | Parcel-transformer is an EXAMPLE consumer (`plugins/PARCEL_USAGE_EXAMPLE.md`), not a deliverable |
| §1.8 | ☑ | Generate ≥1000 synth fixtures (deterministic mulberry32) |
| §1.9 | ☑ | **Phase 1 exit gate:** lib 56/56; harness 1132/1132 |

### Phase 1 lessons

- **Shared-binding scope-invalidation parity bug** (§1.8). Babel's
  `binding.path.remove()` invalidates scope in-place; Rust's deferred
  removal initially left bindings queryable, double-pushing rules.
  Fix: `mark_for_removal` clears the cached string value; locked
  by `mark_for_removal_invalidates_subsequent_lookup`.
- `swc_common::errors::HANDLER` for plugin-side throws (raw `panic!()`
  is wrapped as `plugin failed to invoke plugin on '<filename>'` —
  the original message is dropped).
- `extractStylesToDirectory` writes use `process.cwd()` as the WASI
  `/cwd` preopen — host responsibility (Phase 4 §4.7 OUT OF SCOPE).
- **Phase 7 breadcrumb — `/*#__PURE__*/` duplicates after CC-replacement.**
  SWC codegen multi-span emit path emits two PURE annotations even
  when the leading-comment store has one. Harness collapses via
  `/(\/\*#__PURE__\*\/\s+)\1+/g`; Phase 7 should fix in codegen.

---

## Phase 2 — `babel-plugin` scaffold + dispatcher ☑

> **Goal:** stand up the visitor skeleton + state setup. Pass-through
> byte-equal before any handler logic.
> **Phase exit gate:** ☑ — 477 fixtures, 954/954 pass-through parity.

| ID | Status | Checkpoint |
|---|---|---|
| §2.0 | ☑ | Extract 477 fixtures from `packages/babel-plugin/src/**/__tests__/*.test.ts` |
| §2.1 | ☑ | Port `types.rs`, `constants.rs` (data only) |
| §2.2 | ☑ | Build parity harness `engines.ts` + `harness.test.ts` |
| §2.3 | ▶ | `lib.rs` entry + dispatcher with stubbed handlers (§2.3(a) ☑; §2.3(b) deferred) |
| §2.4 | ☑ | State struct with `MutationRecorder::apply` as only mutator |
| §2.5 | ☑ | **Phase 2 exit gate:** 954/954 pass-through full corpus |

### §2.3(b) follow-up (dangling sub-checkpoint)

Two AST/comment-store mutations marked `// §2.3(b):` in
`crates/babel-plugin/src/babel_plugin.rs`:

1. `path.remove()` of classic-pragma `jsx` specifier (upstream's
   `findClassicJsxPragmaImport.path.remove()`).
2. Filter the matched JSX-pragma comment out of
   `comments.get_leading(first_body_item.span.lo)`.

Bundle with the first §6.5 css-prop fixture that surfaces pragma
divergence. Likely shape: extend `state.queue_cleanup` to accept
richer `CleanupAction` variants (specifier-remove with node identity,
comment-filter at BytePos), drained in `Program::exit`.

Other §2.3-region work that's gated:
- `Program::exit` `appendRuntimeImports` ☑ (§6.8a).
- `ImportDeclaration` specifier removal — same channel as §2.3(b).
- `is_compiled.rs` / `is_jsx_function.rs` / `normalize_props_usage.rs`
  predicate ports — gated until first handler that consumes them.

---

## Phase 3 — Hash compatibility ☑

> **Goal:** prove Rust `hash` is byte-identical to JS `@compiled/utils.hash`.
> **Phase exit gate:** ☑ — 10037 entries, zero divergence.

| ID | Status | Checkpoint |
|---|---|---|
| §3.1 | ☑ | Confirm `crates/compiled-utils` exposes `pub fn hash(input: &str) -> String` |
| §3.2 | ☑ | Build hash test-vector corpus (10037 entries: 4 real call shapes + ~33 categorical + 10K random) |
| §3.3 | ☑ | Diff Rust hash vs JS hash over corpus |
| §3.4 | ☑ | **Phase 3 exit gate:** zero divergence |

---

## Phase 4 — `buildCss` + direct synchronous `transformCss` Rust call ☑

> **Goal:** port `utils/css-builders.ts` and link Rust `transform_css`.
> Single-pass plugin, no scan/apply.

| ID | Status | Checkpoint |
|---|---|---|
| §4.1 | ☑ | `transform_css` integration parity test (120/120 byte-equal) |
| §4.2 | ☑ | Build `COMPAT_GENERATOR_COVERAGE.md` (55 fixtures across 5 axes) |
| §4.3 | ☑ | Port `compat/generator.rs` (55/55 byte-exact, zero skips, ~1640 LOC across 10 files) |
| §4.4 | ☑ | Port `utils/css_builders.rs` SHELL (4 hash-call-shape sites end-to-end; evaluate/resolve/visitCssMap stubs) |
| §4.5 | ☑ | Port `utils/{transform_css_items,build_css_variables}.rs` |
| §4.6 | ☑ | Wire `transform_css` into visitor (PARTIAL + bridge tail; SHELL stubs deleted; ScopeIndex threading) |
| §4.7 | OUT OF SCOPE | Parcel wrapper — downstream-host concern, not a deliverable |
| §4.8 | ☐ | **Phase 4 exit gate:** keyframes/css/cssMap byte-clean — gated on Phase 6 ship |

### Phase 4 lessons

- **AFM `.browserslistrc` is the production pin**, not `chrome 100`.
  `BROWSERSLIST_CONFIG=crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`.
- **Env-var test races are silent and look exactly like drift.**
  `EnvPin` mutates process-global env vars; cargo parallelises tests
  in the same binary. Confine `EnvPin` to single tests.
- **Bun isolated layout bypasses `package.json#overrides`** for
  transitive deps. Top-level promotion required (the §4.2 lesson).
- **§4.3 SWC↔Babel comment-storage quirk.** Same-line comments
  between two tokens are keyed in SWC as TRAILING of the previous
  token, NOT as leading of the next. Object-property iteration
  queries both positions before printing the first prop.
- **§4.4 `Metadata` reborrow shape.** Babel's `{...meta, context, key}`
  spread maps to `Metadata::reborrow_with_context(&mut self, ctx)`.
  Every `extract_*` takes `&mut Metadata<'_>` so child calls can
  reborrow.
- Keyframes name (#1), object-expression catch-all (#2), template-
  literal catch-all (#3) wired through `compat::generator` →
  `compiled_utils::hash`. Site #4 is in `crates/css`.

---

## Phase 5 — In-plugin resolver + expression evaluator ☑

> **Goal:** port `utils/resolve_binding.rs` + `traverse_expression/`
> + `utils/evaluate_expression.rs` + the compat/* layer using
> `oxc_resolver` for module resolution.

| ID | Status | Checkpoint |
|---|---|---|
| §5.0 entry-gate | ☑ | Audit + parity corpora + pin guards + #[ignore]'d Rust gates seeded; Q1/Q2/Q3 locks recorded in `COMPAT_SCOPE_AUDIT.md` |
| §5.0a | ☑ | Port `compat/scope.rs` + `compat/globals.rs` (~1100 + 140 LOC; 23/23 byte-parity) |
| §5.0b | ☑ | Port `compat/path.rs` (~960 LOC; AST-mutating `scope_push` replacing §5.0a stub) |
| §5.0c | ☑ | Port `compat/evaluation.rs` (~600 LOC; full line-by-line `path/evaluation.js`; 45/45 byte-parity) |
| §5.0d | ☑ | Compat infra extensions absorbed by §5.5 closure (`register_new_scope`; rest unneeded) |
| §5.1 | ☑ | Re-confirm `STATE_MUTATIONS.md` (zero new variants since 2026-05-02) |
| §5.2 | ☐ | Land consumer-monorepo refactor (zero outside-cwd includes) — §0.10 dependent |
| §5.3 | ☑ | Port `utils/cache.rs` Layer-1 + Layer-2 postcard `cache.bin` (NOT yet wired into State; gated on consumer) |
| §5.4a entry-gate | ☑ | Resolver matrix + `RESOLVER_SPEC_PART_TWO.md` schema; closes §0.11 |
| §5.4b | ☑ | Resolver engine (`resolver/{mod,config,default,engine}.rs`; `oxc_resolver = "11"`; 4/4 byte-parity) |
| §5.4c | ☑ | `resolver/transforms.rs` — 5-op `packageJsonTransforms` engine + `TransformingFileSystem` |
| §5.4d | ☑ | `resolver/prefer_first.rs` — match-by-prefix dispatcher |
| §5.4e | ☑ | Port `utils/resolve_binding.rs` + bundled `traversers/*.rs`; `Binding::import_info`; `imported_module: Arc<Module>` for cross-file scope-swap parity |
| §5.5 | ☑ | Port entire `traverse_expression/` subtree (14 leaves; `register_new_scope` + `Metadata::own_scope_override`) |
| §5.6 | ☑ | Port `utils/evaluate_expression.rs` (~600 LOC + 14 unit tests; cross-file scope-swap; namespace-import preflight) |
| §5.7 | ☐ | Wire `includedFiles` → `<callScratch>/included-files.json` sidecar |
| §5.8 | ☐ | Promote `scripts/audit-included-files.ts` to CI guardrail |
| §5.9 | ☐ | **Phase 5 exit gate:** module-traversal + expression-evaluation byte-clean — gated on §6.8 |

### Phase 5 closure highlights

- **Vendored data deps** (pinned in `crates/PARITY_VERSIONS.md`):
  `@babel/traverse@7.29.0` (audit reference);
  `@babel/helper-globals@7.28.0` (vendored — 13 lower + 49 upper
  builtins, count-locked);
  `enhanced-resolve@5.18.3` + `resolve@1.22.12` (oracles only).
- **`@babel/generator@7.23.0` + `@babel/parser@7.29.2`** (AFM-resolved
  under `@compiled/babel-plugin@0.36.1` commit `16a62b8`).
- **`Binding::init_expr` / `Binding::import_info`** — single-purpose
  shape extensions added per §5.0c / §5.4e precedent. Future agents
  should follow the same pattern (one new field, gated population).
- **§5.0a Findings 1–8** — see `plugins/COMPAT_SCOPE_AUDIT.md` for
  full upstream-trace details (stored `Binding.constant` bool;
  pattern-skip walk in `getBinding`; `var` hoist through ForStatement;
  `isInitInLoop` auto-reassign; `Scope.parent` key/decorators skip;
  AST-mutating `Scope.push`; eager pre-index intentional delta;
  vendored helper-globals).
- **§5.4e cross-file scope-swap drift fix.** `PartialBindingWithMeta::imported_module:
  Option<Arc<Module>>` carries the parsed AST forward; §5.6 builds
  a fresh `ScopeIndex` at the recursive-fold boundary so deep
  cross-file constant chains (`const a = b` where `b` is another
  binding in the imported file) fold correctly instead of deopting.
- **Raw-pointer dispatcher recursion in §5.6** — the SAFETY comment
  at module head enumerates leaf access discipline. Avoids
  `Rc<RefCell<ScopeIndex>>` (overlapping-borrow panic in
  `traverse_call_expression`'s borrow_mut), thread-local `Cell<*mut>`
  (same aliasing model), and hand-inlining (drift risk).

---

## Phase 6 — Per-API handlers ▶

| ID | Status | Checkpoint |
|---|---|---|
| §6.1 | ☑ | `keyframes` cleanup-only handler (~330 LOC + 12 unit; two-step queue+drain pattern) |
| §6.2 | ☑ | `css` (utility) cleanup-only handler (~95 LOC + 6 unit; reuses §6.1 drain) |
| §6.3 | ☑ | `cssMap` handler — first that emits real CSS + writes back into AST (~270 + 370 + 430 LOC + 24 unit; `process_selectors`) |
| §6.4 | ☑ | `xcss-prop` handler — first that consumes `state.css_map` (~470 LOC + 13 unit) |
| §6.5 | ☑ | `css-prop` handler + MutationRecorder threading through entire `build_css` call graph; real `generate_cache_for_css_map` body |
| §6.6 | ☑ | `<ClassNames>` handler — render-prop pattern with two-pass sub-traversal (~510 LOC + 7 unit) |
| §6.7 | ☑ | `styled` handler + verbatim `@emotion/is-prop-valid@1.4.0` table + `build_styled_component` (~470 + 770 + 470 LOC + 30 unit) |
| §6.8 | ▶ | **Phase 6 exit gate** — full-corpus byte-clean. See "§6.8 active state" at top of file. |

### Phase 6 closure highlights

- **§6.3 SWC↔Babel divergence:** `Ident` can't hold spaces/parens;
  upstream `t.identifier('@media screen and (min-width: 500px)')`
  becomes a string-literal key. Bytes equal because consumers read
  via `get_key_value`.
- **§6.5 `is_css_prop_disabled` stub returns `false`** — see "Standing
  bug-parity flags" above.
- **§6.6 dispatch order in `visit_mut_jsx_element`:** `<ClassNames>`
  FIRST (replaces entire element with wrapper); xcss/css-prop AFTER
  (no-op on wrapper).
- **§6.7 hand-built JSX vs `@babel/template`-driven.** Same printed
  bytes after prettier. `compat/template.rs` placeholder remains
  unused.
- **§6.7 `decls[0].id`-bug-parity preserved.** Upstream's
  `findParent(isVariableDeclaration)` uses the FIRST declarator's id
  regardless of which one triggered. Rust mirrors via
  `visit_mut_var_decl` pre-walk.

---

## Phase 7 — Comment placement and `Program::exit` ordering

| ID | Status | Checkpoint |
|---|---|---|
| §7.1 | ☐ | Build comment-shape diff tool |
| §7.2 | ☐ | Hunt comment-placement divergences (banner, `preserveLeadingComments`, `appendRuntimeImports` order, `@compiled-disable-*`) |
| §7.3 | ☐ | **Phase 7 exit gate:** zero comment divergence |

---

## Phase 8 — Corpus diff at scale and rollout gate

| ID | Status | Checkpoint |
|---|---|---|
| §8.1 | ☐ | Run parity harness across 100k+ Compiled call sites |
| §8.2 | ☐ | Stand up `cargo-fuzz` targets |
| §8.3 | ☐ | Shadow-mode CI — alarm on hash divergence |
| §8.4 | ☐ | **Phase 8 exit gate:** sustained zero divergence |

---

## Phase 9 — Rollout

| ID | Status | Checkpoint |
|---|---|---|
| §9.1 | ☐ | Engine flag default = Babel |
| §9.2 | ☐ | Ship Rust artefacts via `napi build` per platform |
| §9.3 | ☐ | Internal opt-in via `COMPILED_TRANSFORMER=swc` |
| §9.4 | ☐ | Hash-shadow in production |
| §9.5 | ☐ | Flip default to SWC after sustained zero divergence |
| §9.6 | ☐ | Keep Babel pipeline ≥1 year as parity oracle |

---

## Cardinal rules conformance

These are the standing invariants. A checkpoint that violates one is
not "done" — it is rejected at review.

- **Bytes after prettier are the contract.** Not "looks right." Not
  "passes tests." Bytes.
- **CSS class names live inside string literals.** Hashing is part
  of the byte contract.
- **No filesystem access outside `/cwd`** inside the plugin. Ever.
- **No JS callbacks from the plugin.** Side effects go via sidecar
  JSON written to `/cwd/<callScratch>/...`.
- **Don't bump `@swc/core` casually.** ABI breaks. Coordinated
  `swc_core` bump + full corpus rerun required.
- **Bugs are features.** Behavioural differences under the parity
  harness are port defects, not bug-fix opportunities. See
  "Standing bug-parity flags" above for the live list.
- **1:1 file mapping is enforced.** PLAN.md constraint 4. If you
  feel the urge to deviate, stop and ask.
- **No half-baked compat shims.** If `compat/<name>.rs` is incomplete,
  it will break in production. Finish it or escalate.
- **Build with `RUSTFLAGS=""`** for any crate that pulls in proc-macro
  deps via `swc_core`. User-global `lto=thin` breaks proc-macro
  builds.
- **Drift detection.** If something OUTSIDE your work looks ported
  incorrectly, raise it immediately as `Drift detected in X — <why>`.
  Don't work around it. Don't patch it. Many small drifts compound
  into major divergence.
