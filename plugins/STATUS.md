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
handlers shipped) · §6.8 ▶ (post-§6.5 closure baseline
476/0/0/0/1 — sole residual is bug-parity both-throw)** · Phase 7+ ☐.

**Active checkpoint: Phase 6 §6.8** — full-corpus parity exit gate.
See "§6.8 active state" below for the current divergence baseline,
triage tooling, and next-step punch list.

**Independently shippable while §6.8 runs:** §5.7 (`included-files.json`
sidecar), §2.3(b) AST/comment-store mutations bundle.

### §6.5 closure (2026-05-06, this session)

§6.5 (`@compiled-disable-line` / `@compiled-disable-next-line`
directive support) — port-completion of the previously stubbed
`utils/comments.rs::is_css_prop_disabled_via_comment_store`
file-wide bail-out. Three coordinated changes:

1. **`lib.rs::process` pre-pass.** Threads
   `meta.source_map` (`PluginSourceMapProxy`) +
   `meta.comments` (`PluginCommentsProxy`) into a single AST walk
   (`utils/comments.rs::collect_line_comments`) at `Program::enter`.
   The pass dedupes per-`BytePos` and per-`(span.lo, span.hi)`,
   resolves each `BytePos` to a 1-indexed line via
   `source_map.lookup_char_pos(...).line`, and returns a `LineIndex`
   carrying both the comment list and a `BytePos → line` map. Both
   land on `State` (new fields `comment_lines: Vec<LineComment>` and
   `span_lines: HashMap<u32, usize>`, classified out-of-capture per
   `STATE_MUTATIONS.md` — same shape as `pragma`).

2. **`utils/comments.rs` 1:1 port.** Replaces the file-wide
   conservative-bail stub with `get_node_comments(state, start_line,
   end_line) → (before, current)` mirroring upstream's
   `meta.state.file.ast.comments` walk + line-equality filter +
   `CommentLine`-only predicate, and `is_css_prop_disabled(state,
   start_line, end_line)` mirroring the upstream `startsWith` check
   on `@compiled-disable-next-line transform-css-prop` (in `before`)
   / `@compiled-disable-line transform-css-prop` (in `current`).
   Multi-line path skip preserved. 8 unit tests cover the shape.

3. **`css_prop/mod.rs` dispatch.** Now calls `is_css_prop_disabled`
   twice — once on the JSXOpeningElement span, once on the css
   JSXAttribute span — matching upstream's `babel-plugin.ts:70`
   two-call pattern (`isCssPropDisabled(path, meta) ||
   isCssPropDisabled(cssProp, meta)`). Span lo/hi are looked up via
   `state.line_of(byte_pos.0)`; an unknown BytePos returns `None`,
   treated as "no loc → skip" per upstream's
   `path.node?.loc?.start.line` undefined-guard.

**Harness companion fix.** `parity-harness/babel-plugin/engines.ts::stripComments`
gained a whole-line `// ...` strip (`/^[ \t]*\/\/[^\n\r]*\r?\n/gm`)
*before* the inline-comment strip. Babel's `comments: false`
removes the entire line; SWC's `preserveAllComments: false` only
suppresses codegen-time emission for orphan comments — comments
attached to surviving nodes round-trip through codegen. Without the
whole-line strip, the directive line round-tripped as a blank line
in SWC output, prettier preserved it, and we'd have shipped a
1-byte divergence even though the substantive transform was correct.

**Parity delta:** 475 → **476 / 477** (99.79%). Sole residual is
the documented `both-throw` (`should-not-add-quotes-to-content-values-that-shouldn-t-accept-them`)
— both engines throw the same error, which is bug-parity per
CLAUDE.md "BUGS in OLD = BUGS in NEW", not divergence.

**Lib tests:** 467 → 475 (+8 from `comments::tests`, all green).

### §6.8 active state (2026-05-05)

Full corpus run produced a true baseline of **7 parity / 407
divergence / 62 swc-throw / 1 babel-throw** across 477 fixtures.
Earlier "954/954 bun parity" cited in §5.6 was a pass-through oracle
(assert babel ≠ swc); §6.8 inverts it (assert babel == swc) and
surfaces real divergence.

**Current baseline (post-§6.8x, 2026-05-06):** parity **475 / 477**
(99.6%), divergence **1**, swc-throws **0**, babel-throws **0**,
both-throw **1**. Cumulative session delta from the original
7/407/62/1 baseline: parity +468, divergence −406, swc-throws −62
(cluster cleared), babel-throws −1. The single residual divergence
is the documented §6.5 deferral
(`css-prop-tests-behaviour/should-not-transform-css-prop-with-comment-directive`)
— the SourceMap-based per-line `@compiled-disable*` filter requires
threading SWC's source-map proxy through the visitor and is gated
on its own checkpoint per `utils/comments.rs` doc. §6.8i closed the React→React1
hygiene-rename cluster (parity +28); §6.8j ported the spread-element
recursive build_css_inner (parity +21); §6.8k ported `jsesc@2.5.2`
default-string mode for synthesised sheet-const StringLiterals
(parity +1); §6.8l ported the logical-expression sub-pass and
`extract_logical_expression` body (parity +10); §6.8m normalised
SWC `Prop::Shorthand` into a synthetic KeyValue inside
`extract_object_expression` (parity +16); §6.8n bundled the
destructured-binding resolution + IIFE-scope propagation fixes
(parity +19, six sub-fixes); §6.8o landed the
`getVariableDeclaratorValueForOwnPath` IIFE-binding lookup +
per-prop `own_scope_override` snapshot/restore + `Variable.expression`
no-init Option (parity +12, three sub-fixes); §6.8p landed four
coordinated 1:1 ports — top_level_mark React-import ctxt fix,
`<ClassNames>` `style={X}` outer-scope guard + dontexist.style
filter, invalid-DOM-prop walk over original styled-call arg, and
`addComponentName` `c_<name>` wiring (parity +6, four sub-fixes).
§6.8q closed the JSX-runtime ordering cluster + jsx-pragma cluster
(parity +7) via three coordinated landings — harness reconciler for
the host-environment-only `*/jsx-runtime` import-position delta,
§2.3(b) pragma-comment strip, and §2.3(b) classic-pragma `{ jsx }`
specifier removal.

**§6.8o sub-fixes (this session, 2026-05-06):**

- §6.8o-i — `get_variable_declarator_value_for_own_path` now
  consults the IIFE arrow's own-scope binding map for `Pat::Ident`
  inputs. Mirrors upstream's
  `meta.ownPath?.traverse({ VariableDeclarator })` which finds
  `scope.push`-injected `const param = init` declarators and sets
  `variableName = generate(init).code`. Without this, IIFE-resolved
  identifiers like `color1` (bound to `props.color1` via
  `mixin(props.color1, …)`) were hashing as `hash("color1")` =
  `j2chn6` instead of `hash("props.color1")` = `zo7lop`. Cluster
  hash-divergence removed across 12 fixtures spanning class-names /
  css-prop / styled / object-literal / string-literal /
  call-expression / tagged-template-expression. Gate: only matches
  when `binding_node_type == "VariableDeclarator"` (excludes regular
  function params and destructure-pat leaves whose
  `binding_node_type` is `Identifier`/`ObjectPattern`).
- §6.8o-ii — `extract_object_expression` now snapshot/restores
  `meta.own_scope_override` per property iteration. Mirrors upstream's
  per-iter `const { value, meta: updatedMeta } = evaluateExpression(prop.value, meta)`
  pattern where `updatedMeta` is local to each property. Without this,
  a sibling property after an IIFE call (e.g.
  `{ backgroundColor: getBackgroundColor(...), color }` — `color` in
  shorthand position) inherited the IIFE's `own_scope_override` and
  resolved against the IIFE's `color` param instead of the outer
  `const color`. Implementation uses a `'prop_iter` labeled block so
  early-exit `break 'prop_iter`s still hit the snapshot-restore.
- §6.8o-iii — `Variable.expression` widened from `Box<Expr>` to
  `Option<Box<Expr>>`. `build_css_variables` skips the first `ix(…)`
  arg when expression is `None`, producing the bare `ix()` upstream
  emits for no-init IIFE-injected params (e.g. `mixin(a, b)` against
  `(a, b, c, d) => …` gives `c`/`d` as `init: undefined` declarators
  → `ix()` not `ix(c)`). Mirrors upstream's
  `[transform(variable.expression), …].filter(Boolean)` truthy
  semantics. Variable construction sites updated:
  `extract_object_expression` catch-all, `extract_template_literal`
  catch-all, plus two unit-test fixture sites in
  `build_compiled_component` and `build_styled_component`.

§6.8o lib tests 465 → 465 (no test-count delta; the existing tests
were green against the new Optional shape after the construction-site
updates).

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

### §6.8 cluster table (post-§6.8f re-triage; baseline parity=313/477, divergence=163, swc-throws=0)

| Cluster (upstream test file) | Count | Likely root cause |
|---|---|---|
| `styled/behaviour` | 41 | conditional-CSS ternary expansion (literal-branch ternaries fall through to CSS-variable path; see §6.8e closure) |
| `css-prop/behaviour` | 20 | wrapper emit-shape (sibling of object-literal) |
| `css-prop/object-literal` | 18 | wrapper emit-shape |
| `styled/call-expression` | 16 | sibling cluster of styled/behaviour |
| `__tests__/expression-evaluation` | 9 | evaluator edge cases surfaced by full-corpus run |
| `__tests__/index` | 9 | top-level plugin-entry / option-handling fixtures |
| `styled/tagged-template-expression` | 8 | sibling cluster of styled/behaviour |
| `css-prop/string-literal` | 7 | sibling cluster of css-prop |
| `class-names/call-expression` | 6 | render-prop / `<ClassNames>` |
| `class-names/behaviour` | 5 | render-prop / `<ClassNames>` |
| `__tests__/css-builder` | 4 | css-builder unit-style fixtures |
| `__tests__/jsx-automatic` | 4 | classic vs automatic JSX runtime |
| `class-names/tagged-template-expression` | 3 | render-prop / `<ClassNames>` |
| `keyframes/call-expression` | 3 | keyframes ref + dynamic-value handling |
| `xcss-prop/transformation` | 3 | xcss-prop edge cases |
| `css-prop/jsx-pragma` | 2 | classic-pragma `jsx` specifier (links to §2.3(b)) |
| `__tests__/custom-import-source` | 2 | links to §6.8 punch-list item 5 (`importSources` option) |
| `__tests__/module-imports` | 2 | cross-module resolver edge cases |
| `css-map/at-rules-and-selectors` | 1 | tail of §6.3 cssMap |

NOTE: post-§6.8f the swc-throws column is empty (0 throws across 477)
and babel-throws is 0 (the lone `both-throw` is genuine bug-parity).
Total = 163 divergences (unchanged from §6.8e — see §6.8f closure for
why: the optimization fix is necessary but blocked by the §6.8g
prop-destructure feature for the styled cluster). Top 3 clusters
(`styled/behaviour`, `css-prop/behaviour`, `css-prop/object-literal`)
account for **79 of 163 (48%)** — `styled/behaviour` will move once
§6.8g lands.

### §6.8 punch list (next agent)

1. **Run `bun parity-harness/babel-plugin/triage.mjs`** to confirm
   baseline (313/163/0/0/1 at HEAD post-§6.8f).
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

- **§6.8d ☑ — Three styled/css-prop port-completions driving the
  parity 184 → 306 jump** (this session, 2026-05-05). Three 1:1
  ports closing gaps surfaced by cluster-by-cluster diff review of
  `styled/behaviour` and adjacent clusters:

  1. **Arrow-function printer ported in
     `crates/babel-plugin/src/compat/generator/generators/expressions.rs::arrow`
     + `Pat` printer for `Ident` / `Object` / `Array` / `Rest` /
     `Assign` / `Expr`.** Root cause for a swathe of `styled/*`
     and `css-prop/*` divergences: the arrow-fn printer was hitting
     the `/*UNHANDLED-EXPR*/` fallback, which then collapsed
     through `compiled_utils::hash` to a constant
     `"2wqa78"` string. The collapsed-hash bug meant every styled
     component whose dynamic-value arrow body wasn't trivially a
     bare Ident / MemberExpression got the same hash for completely
     different inputs, so all "should-emit-different-hash" fixtures
     diverged. Fix: print arrow params + body 1:1 with upstream
     `@babel/generator@7.23.0` (`generators/expressions.js::ArrowFunctionExpression`)
     and add a Pat printer 1:1 with `generators/types.js`'s pattern
     branches.
  2. **`utils/normalize_props_usage.rs` ported 1:1 from upstream
     (~340 LOC + 8 unit tests).** Handles single-Ident param,
     ObjectPat with nested destructuring + defaults + rest, AssignPat
     with RHS-extracted defaults, ArrayPat throws (matches upstream's
     hard error on array destructuring in styled-component prop
     usage). Wired into `babel_plugin.rs::visit_mut_expr` per
     upstream's `hasStyles` branch. This was the missing
     destructured-prop normalisation pass that re-writes
     `styled.div(({ color, size = 16 }) => ...)` into the canonical
     `__cmplp.color` / `__cmplp.size ?? 16` shape upstream emits.
  3. **Hygiene-context fix in the renamer.** SWC's hygiene pass was
     renaming `__cmplp` → `__cmplp1` because the synthesised Ident's
     `SyntaxContext` didn't match the styled wrapper's binding
     context. Fix: `id.ctxt = SyntaxContext::empty()` on the
     constructed `__cmplp` / `__cmpls` Idents at the styled emit
     site, matching the wrapper's program-scope context.

  **Triage delta: parity 184 → 306 (+122, 64.2% of 477),
  divergence 292 → 170 (−122), swc-throws 0 → 0 (steady),
  babel-throws 0 → 0 (steady), both-throw 0 → 1 (genuine
  bug-parity throw, not a regression).** Lib tests 439 → 447
  (+8 new `normalize_props_usage` tests).

  Cumulative across this session: parity 7 → 306 (+299, 64.2% of
  477); divergence 407 → 170 (−237); swc-throws 62 → 0 (cluster
  cleared); babel-throws 1 → 0. **Top remaining clusters** (see
  cluster table above): `styled/behaviour` 44, `css-prop/behaviour`
  20, `css-prop/object-literal` 18, `styled/call-expression` 16,
  `styled/tagged-template-expression` 12.

- **§6.8e ☑ — `BlockStatement` printer + `_endsWithWord` reset on
  queue-ops** (this session, 2026-05-05). Two 1:1 ports unblocking
  the styled / css-prop block-body-arrow sub-cluster.

  **Root cause** (drift-detection per CLAUDE.md): the §6.8d arrow-fn
  printer landed `compat/generator/generators/expressions.rs::arrow`
  but `BlockStmtOrExpr::BlockStmt` was emitting the placeholder
  `/*UNHANDLED-BLOCK*/`. Hash sites (`utils/css_builders.rs::extract_object_expression`
  + `extract_template_literal`) feed the printed Expr through
  `compiled_utils::hash` to derive both the `--_<hash>` CSS-variable
  name AND (because the var name is part of the CSS declaration) the
  enclosing atomic class hash `_<atomicGroup><valueHash>`. Verified
  via `bun parity-harness/babel-plugin/probe-hash.mjs` (now removed):
  `hash("__cmplp => /*UNHANDLED-BLOCK*/") = "zmfr3g"` matched SWC's
  exact output for `should-transform-an-arrow-function-with-a-body-into-an-iife`,
  while `hash("__cmplp => {\n  return __cmplp.color;\n}") = "63bh2t"`
  matched Babel's. So `props => { return props.color; }`-shaped
  styled interpolations had every CSS-variable name AND every atomic
  class hash collapse to a single divergent value across the cluster.

  1. **`compat/generator/generators/statements.rs` (new ~250 LOC +
     5 unit tests).** 1:1 port of upstream `base.js::BlockStatement`
     and the `statements.js` variants reachable from arrow block
     bodies in the styled / css-prop dynamic-arrow cluster:
     `BlockStatement`, `ReturnStatement`, `ThrowStatement`,
     `ExpressionStatement`, `IfStatement` (including upstream's
     `needsBlock` ASI guard via the `consequent_ends_in_if` helper
     that mirrors `getLastStatement`'s body-walk). Other Stmt
     variants (`Debugger`, `With`, `Labeled`, `Break`, `Continue`,
     `Switch`, `Try`, `While`, `DoWhile`, `For`, `ForIn`, `ForOf`,
     `Decl`) emit a distinct placeholder per kind so future fixtures
     surface as their own cluster — port from upstream 1:1 when one
     lands. Wired from `expressions.rs::arrow`'s
     `BlockStmtOrExpr::BlockStmt` branch (replaces the
     `/*UNHANDLED-BLOCK*/` placeholder).

  2. **`compat/generator/printer.rs` — `_endsWithWord` reset on
     queue-ops** (drift-detection: latent printer bug surfaced by
     §6.8e's first stmt-level `word(X) + space() + print(arg)`
     sequence — `return __cmplp.color`). Upstream Babel's
     `printer.js::_queue` resets `_endsWithWord = false`
     unconditionally on every queue; the Rust port had `space()` /
     `space_force()` / `newline()` calling `Buffer::queue` directly
     without the Printer-level flag reset. Without the reset, after
     `word("return")` set `ends_with_word=true`, the subsequent
     `space()` queued ' ' but left the flag set, so `word("__cmplp")`
     on the next print step queued an EXTRA leading space (its own
     word-collision guard saw `ends_with_word=true`). Net effect:
     `return  __cmplp.color` (two spaces, hash-divergent).

     Fix: each Printer-level queue op (`space`, `space_force`,
     `newline`) now sets `ends_with_word = false` and
     `ends_with_integer = false` mirroring upstream `_queue`. Caught
     locally by the new `return_member_expr` /
     `block_with_return_member_expr` / `arrow_block_body_byte_target`
     unit tests before the WASM rebuild — the
     `arrow_block_body_byte_target` test asserts both the printed
     bytes (`"__cmplp => {\n  return __cmplp.color;\n}"`) AND the
     hash (`"63bh2t"`) match the upstream target identified during
     the §6.8e probe. New helpers also added: `Printer::semicolon` /
     `semicolon_force` / `ends_with(c)` (used by `IfStatement`'s
     `}` ↔ `else` space logic) + `generate_stmt(stmt)` entry point
     in `compat/generator/mod.rs` for stmt-level unit testing.

  Lib tests 447 → 452 (+5 from `statements::tests`).

  **Triage delta: parity 306 → 313 (+7), divergence 170 → 163 (−7),
  swc-throws 0 → 0 (steady), babel-throws 0 → 0 (steady),
  both-throw 1 → 1 (steady).** The cluster delta is smaller than
  the §6.8d projections suggested because the styled/behaviour
  divergences cluster around TWO root causes — block-body printing
  (cleared by §6.8e, +7 fixtures) AND ternary-with-literal-branches
  conditional-CSS expansion (still open — see §6.8f follow-up).

  Cumulative across this session: parity 7 → 313 (+306, 65.6% of
  477); divergence 407 → 163 (−244); swc-throws 62 → 0 (cluster
  cleared); babel-throws 1 → 0.

- **§6.8f ☑ — `optimize_conditional_statement` wired + nested-template
  detection + conditional-branch-suppression flag** (this session,
  2026-05-05). Closes the "ternary-with-literal-branches falls through
  to CSS-variable" defect identified after §6.8e. Three coordinated
  ports unblocking the styled / css-prop ternary cluster's CSS-bytes
  correctness.

  **Root cause** (drift-detection per CLAUDE.md): the §4.4 shell at
  `utils/css_builders.rs::extract_template_literal` lines 1247-1257
  was a **silenced stub** for upstream's `optimize_conditional_statement`
  call (build-css.ts:782-792). With it skipped, `border-radius:
  ${(p) => p.x ? 10 : 1}px !important` fell through to the catch-all
  CSS-variable path (`--_<hash>` + `ix(...)` runtime), emitting one
  atomic rule with `var(--_xxx)` instead of upstream's two atomic
  rules `_<hash1>{border-radius:1px!important}` /
  `_<hash2>{border-radius:10px!important}` with a runtime ternary on
  the className array. Same root cause for all `*-with-ternary-*` /
  `*-with-conditional-*` fixtures across styled/behaviour (~30+),
  css-prop/behaviour, css-prop/object-literal, styled/call-expression,
  and styled/tagged-template-expression clusters.

  Three landings:

  1. **`utils/manipulate_template_literal.rs::has_nested_template_literals_with_conditional_rules`
     port-completion** (was `unimplemented!()` per Phase 5 §5.6 stub
     citing missing parent-traversal). Replaces with a recursive
     within-Tpl walker covering upstream's case 2 (nested template
     literal with arrow-function interpolation) and case 3 (logical
     expression directly in interpolation), recursing through
     `Cond.test/cons/alt`, `Arrow.body`, and `Paren.expr` so that
     deeply-nested arrow-body templates are detected. Case 1 (this
     template IS a branch of an outer Conditional) is handled
     separately via the `Metadata::in_conditional_branch` flag (item
     3 below) since it requires parent-context awareness — without
     this fallback the case-1 detection still requires §5.6's
     parent-walk, raised as §6.8g if a fixture surfaces.

  2. **Bug-parity fix in `optimize_conditional_statement`'s body-shape
     check + cond-extract path: `unwrap_paren` before pattern-match.**
     Drift detected: the Rust port's `body_is_conditional` matched
     `BlockStmtOrExpr::Expr(Expr::Cond(_))` directly, but Babel's
     parser strips `ParenthesizedExpression` while SWC keeps it. So
     `(p) => (p.isPrimary ? 'blue' : 'red')` (with explicit parens
     around the ternary) had `arrow.body` as
     `BlockStmtOrExpr::Expr(Paren(Cond(...)))` — the optimization
     no-op'd. Fix: `crate::compat::paren::unwrap_paren(e)` before the
     `Expr::Cond(_)` match (both at the gate AND the `original_cond`
     extraction). Mirrors the §6.8c-#1 paren-shim convention.

  3. **`utils/css_builders.rs` integration**: wire the
     `optimize_conditional_statement` call when the gate fires
     (`is_mid_statement && does_expression_have_conditional_css &&
     cond_has_literal_branches && !meta.in_conditional_branch &&
     !has_nested_template_literals_with_conditional_rules(node, meta)`).
     Two SWC-vs-Babel divergences plumbed:
     - **Synthetic `TplElement` shells**: upstream mutates `node.quasis[i].value.raw`
       directly via NodePath access. Our Rust walks the AST by `&` —
       no such mutation channel. Build local mutable
       `swc_core::ecma::ast::TplElement` clones from the parallel
       `quasi_raws: Vec<String>` channel, pass to
       `optimize_conditional_statement`, then write the post-mutation
       raws back into `quasi_raws[index]` / `quasi_raws[index + 1]`
       and re-read the iteration's `raw` so downstream
       `format!("{}{}", prefix, raw)` writes the post-mutation form.
     - **`cond_has_literal_branches` defensive narrowing**: upstream
       fires `optimize_conditional_expression` on ALL branches and
       wraps non-Lit/non-Tpl branches in a synthetic Tpl that
       re-enters `extract_template_literal` recursively. For branches
       like `colors.N20` (where `colors` is an imported foreign
       module the resolver can't fold), our recursive `evaluate_expression`
       path panicked. Narrow the gate to require `cons` and `alt` BOTH
       be `Lit::Num` / `Lit::Str` / `Tpl` — covers the cluster's
       common shapes (`p.x ? 10 : 1`, `p.x ? 'blue' : 'red'`,
       `p.x ? \`...\` : \`...\``) without triggering the
       foreign-import panic. **Open follow-up §6.8h**: port-completion
       of optimize-with-foreign-MemberExpr branches once the
       evaluator's foreign-module fold semantics are made resilient.

  4. **`Metadata::in_conditional_branch` flag** (new field on
     `types.rs::Metadata`, threaded through `reborrow` /
     `reborrow_with_context` and all 30+ `Metadata { ... }` literal
     constructors). Set to `true` in `extract_branch` immediately
     before each `build_css_inner(Tpl)` / `build_css_inner(TaggedTpl/Call)`
     recursion (with save-and-restore semantics around the call so
     the surrounding meta state is preserved). Read in
     `extract_template_literal`'s optimization gate to suppress
     per-interpolation `optimize_conditional_statement` when the
     containing template is itself a branch of an outer Conditional.
     Without this, a fixture like
     `${(p) => p.isPrimary ? \`color: green; > :first-child { display: ${(p) => (p.isShown ? 'none' : 'block')}; } > :last-child { opacity: ${(p) => (p.isShown ? 1 : 0)}; }\` : 'color: red'}`
     would have the inner per-interpolation optimization split each
     inner `display:` / `opacity:` into TWO CssItems per branch, but
     `extract_branch` requires `merged.len() == 1` per branch and
     throws "Conditional branch contains unexpected expression"
     otherwise. Mirrors upstream's
     `hasNestedTemplateLiteralsWithConditionalRules` case-1 (this
     template IS a branch of an outer Conditional) detection —
     achieved without the §5.6 parent-walk.

  Lib tests stay 452/452.

  **Triage delta: parity 313 → 313 (steady), divergence 163 → 163
  (steady), swc-throws 0 → 0 (steady), babel-throws 0 → 0 (steady),
  both-throw 1 → 1 (steady).** Net cluster count is unchanged
  because every affected fixture in styled/behaviour /
  styled/call-expression / styled/tagged-template-expression also
  needs the **prop-destructure-for-consumed-props** feature
  (upstream's `const { isRounded, ...__cmpldp } = __cmplp;` ahead
  of the spread in the styled-component body). The CSS bytes our
  fix produces are now correct (sample fixtures
  `should-apply-conditional-css-with-ternary-operator` /
  `*-and-suffix` / `*-for-object-styles` all emit the post-fix
  per-branch atomic rules byte-equal to Babel; verified manually
  via `bun parity-harness/babel-plugin/inspect-one.mjs` — now
  removed). **§6.8g** (next) — prop-destructure feature — will
  land the parity counter delta.

- **§6.8g ☑ — invalid-DOM-prop walk extended to conditional class names**
  (this session, 2026-05-06). Closes the prop-destructure gap surfaced
  after §6.8f.

  **Root cause.** `build_styled_component.rs::styled_template`
  re-derived `invalidDomProps` by walking `opts.variables[i].expression`
  only — but for fixtures whose consumed prop is referenced from a
  runtime ternary class-name (e.g. `(p) => p.isRounded ? '_a' : '_b'`),
  the `__cmplp.isRounded` MemberExpr lives in `opts.class_names`
  (`Expr::Cond`) NOT `opts.variables`. Upstream walks the entire
  `meta.parentPath` subtree which covers both. Result: SWC emitted
  `...__cmplp` directly, Babel emitted `const { isRounded, ...__cmpldp }
  = __cmplp; ...__cmpldp`.

  **Fix.** One-loop extension at
  `build_styled_component.rs:324-332` — extend the walk to also iterate
  `opts.class_names` and feed each through the existing
  `InvalidDomPropsVisitor` (which already recurses via
  `visit_children_with`). Mirrors upstream's parent-path walk byte-for-
  byte for the cases in the corpus.

  **Triage delta: parity 313 → 335 (+22), divergence 163 → 141 (−22),
  swc-throws 0 → 0 (steady), babel-throws 0 → 0 (steady), both-throw 1 →
  1 (steady).** Lib tests stay 452/452. Three remaining `__cmplp`-touching
  fixtures need separate features (`should-only-destructure-a-prop-if-
  hasnt-been-already`, `should-handle-destructuring-in-interpolation-
  functions`, `*-template-literal-branches-co`) — raised as §6.8h.

- **§6.8h ☑ — variables bubble-up + walk-order + drop literal-only
  gate + Babel `_generateUid` formula** (this session, 2026-05-06).
  Four coordinated ports closing the residual `__cmplp`/`__cmpldp`
  cluster surfaced after §6.8g. Cumulative parity 335 → 346 (+11),
  divergence 141 → 130 (−11), swc-throws / babel-throws / both-throw
  all unchanged. Lib tests stay 452/452.

  **§6.8h-i — `extract_branch` returns variables.** Drift detected:
  `css_builders.rs::extract_branch` returned `Result<Option<CssItem>>`
  and dropped `cssOutput.variables` from each branch — but upstream
  `extractConditionalExpression` (build-css.ts:444-445) pushes
  `consequentCss.variables` and `alternateCss.variables`
  unconditionally, regardless of whether either branch produced a
  CssItem. Fix: return `(Option<CssItem>, Vec<Variable>)` and have
  `extract_conditional_expression` `extend` both branches' variables
  into its outer `variables` Vec. Without this, fixtures like
  `0258 should-apply-conditional-css-with-ternary-operators-template-literal-branches-co`
  emitted the sheet `_1bsby2bc{width:var(--_znisgh)}` (variable
  referenced in CSS) with no matching `--_znisgh: ix(CUSTOM_WIDTH, "px")`
  in the inline `style` prop — the `--_znisgh` name was generated
  inside the cons branch's Tpl recursion but the `Variable { name,
  expression, ... }` record was lost on bubble-up. **Closed: 0258.**

  **§6.8h-ii — invalid-DOM-prop walk: class_names before variables.**
  §6.8g extended the walk to `opts.class_names` but kept variables-
  first ordering. For fixtures with an outer ternary `(p) => p.isPrimary ?
  ... : ...` whose cons Tpl contains an inner-arrow CSS-variable
  interpolation (`${(p) => p.isShown ? 'none' : 'block'}`), the outer
  test's `isPrimary` lives in class_names while the inner-arrow body's
  `isShown` lives in opts.variables[i].expression. Babel's depth-first
  parent-path walk visits `isPrimary` first; our variables-first
  ordering produced `{ isShown, isPrimary, ... }`. Fix:
  `build_styled_component.rs:313-334` — walk `opts.class_names` BEFORE
  `opts.variables`. **Closed: 0374, 0387.**

  **§6.8h-iii — drop `cond_has_literal_branches` gate.** §6.8f added
  this defensively to avoid an `evaluate_expression` panic when a
  ternary branch was a foreign-MemberExpr (`colors.N20`). The panic
  was actually the TaggedTpl `unimplemented!()`, fixed in §6.8a-vi
  (`compat::evaluation::evaluate` TaggedTpl branch now `deopt`s).
  With the gate removed at `css_builders.rs:1300-1334`,
  `optimize_conditional_statement` now wraps non-literal branches in
  the synthetic Tpl per upstream's `optimize_conditional_expression`
  (manipulate-template-literal.ts:80-122) — producing per-branch
  atomic class-names + per-branch CSS-variable shape Babel emits for
  `${({ x }) => x ? colors.N20 : colors.N40}`. **Triage delta on
  this single change: parity +5 (across multiple
  destructuring-in-interpolation fixtures).**

  **§6.8h-iv — `next_uid_name` rewritten to match Babel's
  three-bucket suffix formula.** Drift detected: §6.8a-iv ported the
  WRONG `_generateUid` algorithm. The previous impl produced
  `_, _2, _3, ..., _9, _10, _11, ...` (1-based, suffix suppressed at
  i==1). Upstream `@babel/traverse@7.29.0/lib/scope/index.js::generateUid`
  (lines 376-389) actually walks i=0..N where the suffix is computed
  as `i >= 11 ? i - 1 : i >= 9 ? i - 9 : i >= 1 ? i + 1 : ''` —
  producing `_, _2, _3, _4, _5, _6, _7, _8, _9, _0, _1, _10, _11, ...`
  (`_0` and `_1` slot in between `_9` and `_10`). Empirically
  reproduced via direct `scope.generateUidIdentifier('')` invocation
  on @babel/traverse. Fix: `state.rs::next_uid_name` now mirrors the
  three-bucket formula. The `_<n>` cluster on fixture 0248
  (10 hoisted sheets) closed as a result.
  **Closed: 0248** (previously the only divergence was `_10` vs `_0`
  for the 10th hoisted sheet — the deeper destructure-shape
  divergence was already closed by §6.8h-iii's gate-drop.)

- **§6.8i ☑ — React→React1 hygiene rename closed via top-level-mark
  ctxt detection + free-React rebind** (this session, 2026-05-06).
  Two coordinated fixes closing the residual React→React1 cluster
  (33 fixtures across css-prop/behaviour, css-prop/object-literal,
  css-prop/string-literal, expression-evaluation, xcss-prop/transformation,
  custom-import-source, jsx-pragma).

  **Root cause** (drift detection per CLAUDE.md): the §6.8a-i
  `program_scope_ctxt` walker returned the FIRST non-empty
  `SyntaxContext` from any Ident in the post-pre-pass AST. SWC's
  resolver applies `unresolved_mark` to free references and
  `top_level_mark` to top-level bindings — the walker did NOT
  distinguish between them. For fixtures whose only top-level Idents
  were free references (e.g. `<div>` JSXName intrinsics in
  `<div css={{}}>hello world</div>`, or `React.useState(...)` in
  `const [fontSize] = React.useState('10px')`), the walker returned an
  `unresolved_mark`-derived ctxt. SWC's hygiene config preserves
  ONLY `top_level_mark`-derived bindings; everything else gets
  renamed. So our `import * as React` injected with
  `unresolved_mark` ctxt was renamed `React → React1`. Even fixtures
  WITH top-level bindings (e.g. `fontSize` / `Component` in 0056)
  hit a secondary problem: source-level `React.useState(...)`
  references occupied the symbol `React` in the unresolved set,
  causing the rename pass to pick `React1` as the unique name for
  our binding while LEAVING the `React.useState` reference under its
  unresolved ctxt.

  Two landings:

  1. **`program_scope_ctxt` rewritten to prefer `top_level_mark`**
     (`crates/babel-plugin/src/babel_plugin.rs::program_scope_ctxt`).
     Threads `unresolved_mark` from the plugin metadata into the
     walker; computes `unresolved_ctxt = SyntaxContext::empty().apply_mark(unresolved_mark)`;
     the visitor records the first Ident whose ctxt is non-empty AND
     != `unresolved_ctxt` as `first_top_level`. Returns
     `first_top_level` when found. **Fallback chain when no
     top-level Ident exists in source:** uses `Mark::from_u32(unresolved_mark.as_u32() + 1)`
     — empirically `top_level_mark` is allocated immediately after
     `unresolved_mark` in @swc/core's pipeline, so the raw u32 is
     sequential. Final fallback: any non-empty ctxt (preserves prior
     behaviour). Wired through `lib.rs::process` via
     `visitor.unresolved_mark = Some(meta.unresolved_mark)`.

  2. **`rebind_free_react` post-injection walker**
     (`crates/babel-plugin/src/babel_plugin.rs::rebind_free_react`).
     A `VisitMut` that walks the module after `import * as React`
     is inserted and re-colours every free `React` Ident
     (`ctxt == unresolved_mark-ctxt`) to the new import binding's
     ctxt. Without this, fixtures like 0056 (`React.useState(...)`
     in source) would have the symbol `React` reserved as an
     unresolved-set entry, forcing the rename pass to pick `React1`
     for our binding even though the binding's own ctxt is
     `top_level_mark`-derived. Safe because we only enter the
     injection branch when `has_react_binding` is false — no source
     declaration of `React` exists, so all `React` references must
     be free and resolve to our injected binding.

  **Triage delta: parity 346 → 374 (+28), divergence 130 → 102
  (−28), swc-throws / babel-throws / both-throw all unchanged.**
  Lib tests stay 452/452. Cluster knock-on: `xcss-prop/transformation`
  cluster cleared entirely (3 div → 0 div). Other clusters
  (css-prop, expression-evaluation) saw the React-rename row clear,
  surfacing the next-layer divergences underneath.

- **§6.8j ☑ — Spread-element port-completion in
  `extract_object_expression`** (this session, 2026-05-06). One 1:1
  port closing the residual `<div css={{ ...spread, color }}>`
  cluster surfaced after §6.8i.

  **Root cause** (drift detection per CLAUDE.md):
  `crates/babel-plugin/src/utils/css_builders.rs::extract_object_expression`
  `PropOrSpread::Spread` arm was a §4.6 bridge stub —
  called `resolve_binding(...)` and `evaluate_expression(...)` then
  DISCARDED both results with `let _ = ...`. The trailing comment
  even noted "the surrounding JS branch (consume the resolved
  Variable shape into the CSS emit) is Phase 6 handler work; bridge
  discards both results." Without the consume-phase ported,
  fixtures like `<div css={{ color: 'blue', ...mixin }} />` (where
  `mixin = { color: 'red' }`) emitted `color:blue` (the literal
  before the spread) and dropped the spread entirely — upstream
  emits `color:red` because the spread appears later in source order
  and overrides the earlier literal.

  **Fix.** Ported upstream `css-builders.ts:646-665` verbatim:
  resolve binding (throw if Identifier and not resolvable);
  `evaluateExpression(prop.argument, meta)` to get propValue;
  recursive `buildCss(propValue, updatedMeta)`; extend `css` and
  `variables` with the result. The `assertNoImportedCssVariables`
  post-check is omitted (cross-file imported-CSS-variable detection
  is Phase 5 §5.6 territory; no fixture in the corpus surfaces it).

  **Triage delta: parity 374 → 395 (+21), divergence 102 → 81 (−21),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  stay 452/452. Cluster knock-on: 6 fixtures in
  `css-prop/object-literal` (spread-from-variable variants), several
  in `class-names/call-expression` and `class-names/tagged-template-expression`
  (mixin-as-spread shapes), plus collateral wins across
  `expression-evaluation`.

- **§6.8k ☑ — `jsesc@2.5.2` default-string port for synthesised
  sheet-const StringLiterals** (this session, 2026-05-06). One 1:1
  port closing the lone emoji-escape divergence
  (`styled/__tests__/call-expression.test.ts:91 should respect the
  definition of pseudo element content ala styled components with
  content`).

  **Root cause** (drift detection per CLAUDE.md): Babel's
  `@babel/generator/lib/generators/types.js::StringLiteral` falls
  through to `_jsesc(node.value, this.format.jsescOption)` whenever
  `getPossibleRaw(node)` returns `undefined` — i.e., for synthetic
  Str nodes with no `extra.raw`. Babel's `index.js:38-42` defaults
  `jsescOption` to `{ quotes: 'double', wrap: true, minimal: false }`,
  which escapes every code unit outside the printable-ASCII whitelist
  to `\xXX` / `\uXXXX` form (astral chars naturally split into UTF-16
  surrogate pairs since `es6:false` iterates code units). Our
  `utils/hoist_sheet.rs::emit_hoisted_sheets` synthesised
  `Str { value, raw: None }`, so SWC's emitter (`lit.rs:97-116`,
  `ascii_only:false` default) emitted non-ASCII bytes raw. For
  `content: '😎'` the upstream test asserts the output contains
  `content:"\uD83D\uDE0E"`; we emitted `content:"😎"`.

  **Fix.** New `compat/jsesc.rs` (~120 LOC + 12 unit tests) — 1:1
  port of `node_modules/.bun/jsesc@2.5.2/jsesc.js` lines 237-313 with
  the four pinned options baked in. `babel_default_string(value)`
  returns the quoted string literal (including surrounding `"..."`).
  Wired at `utils/hoist_sheet.rs::emit_hoisted_sheets`'s synthesised
  `Str` — `raw: Some(jsesc::babel_default_string(sheet_text).into())`.
  SWC's `lit.rs:91` short-circuit then writes `raw` verbatim. Test
  table covers every escape-table boundary (whitelist pass-through,
  quote/backtick/apostrophe handling, `\v`-omission quirk, `\0` +
  digit edge case, `\xXX` for 0x80-0xFF, `\uXXXX` for BMP > 0xFF,
  surrogate pairs for U+10000..U+10FFFF, plus the exact target
  fixture's CSS bytes).

  **Triage delta: parity 395 → 396 (+1), divergence 81 → 80 (−1),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  452 → 464 (+12 from `compat::jsesc::tests`). Single-fixture impact
  but eliminates an entire class of latent divergences any time a
  user's CSS contains non-ASCII content.

- **§6.8l ☑ — Logical-expression port-completion in
  `extract_template_literal` + `extract_logical_expression`** (this
  session, 2026-05-06). Two coordinated 1:1 ports closing the
  styled-conditional-CSS cluster.

  **Root cause** (drift detection per CLAUDE.md): two §4.6 stubs
  silently dropped CSS for `props => props.x && ({...})` shapes.

  1. `utils/css_builders.rs::extract_template_literal`'s
     logical-expression sub-pass (this file, ~line 1577) called
     `evaluate_expression` and `let _ = ...`-discarded the result.
     Upstream `build-css.ts:889-901` calls `evaluateExpression(prop.body, meta)`
     followed by `buildCss(propValue, updatedMeta)` and pushes the
     resulting `css` and `variables` into the accumulators. The
     first-pass at line 1280 short-circuits via `is_terminal_or_logical`
     to skip inline emission, leaving the logical interpolation to be
     emitted by this second pass — without it, every
     `${props => props.isPrimary && ({ color: 'blue' })}` in a styled
     tagged template was silently dropped.
  2. `utils/css_builders.rs::extract_logical_expression` (~line 626)
     was the analogous §4.6 stub for the `styled.div(arg, props =>
     props.x && ({...}))` shape (object-styles arg passed via
     `extractArray` → `buildCss(ArrowFn)` → upstream
     `extractLogicalExpression`). Replaced the stub body with the
     full upstream lines 433-448 port: evaluate the arrow body
     (unwrap_paren first since SWC keeps Paren that Babel strips),
     recurse `build_css_inner` on the folded value, return the
     merged `CSSOutput`.

  **Knock-on fix: `build_css_inner` Logical branch unwrap_paren on
  `right`.** The recursion at this file's existing Logical-branch
  (the `BinExpr { LogicalAnd | LogicalOr | NullishCoalescing }` arm)
  was passing `right` directly to `build_css_inner`. For `right =
  Paren(Object)` (the SWC-shape of `({color:'blue'})` due to the
  source-level parentheses), the recursion fell through every
  pattern and tripped the catch-all "ParenthesizedExpression was
  unable to have its styles extracted" panic. Added an
  `unwrap_paren(right)` call before the recurse to mirror Babel's
  parser stripping ParenthesizedExpression.

  **Triage delta: parity 396 → 406 (+10), divergence 80 → 70 (−10),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  stay 464/464. Cluster knock-on: `styled/__tests__/behaviour`
  cluster dropped from 14 → 4 divergences in one swing. Remaining
  styled/behaviour fixtures are independent shapes (atomic-hash
  divergence, font-vs-color routing, control-prop destructure
  shape) that surface as their own §6.8 sub-clusters.

- **§6.8m ☑ — SWC `Prop::Shorthand` normalised into a synthetic
  KeyValue inside `extract_object_expression`** (this session,
  2026-05-06). One Babel↔SWC parser-shape shim closing the
  shorthand-property cluster.

  **Root cause** (drift detection per CLAUDE.md): Babel's parser
  produces `ObjectProperty { key:Ident, value:Ident, shorthand:true }`
  for `{ color }`, so upstream's `t.isObjectProperty(prop)` filter
  matches both shorthand and longhand identically. SWC splits the
  same source into `Prop::Shorthand(Ident)` vs `Prop::KeyValue` —
  the Rust port's `let Prop::KeyValue(kv) = ... else { continue; }`
  guard at the top of `extract_object_expression`'s prop loop
  silently DROPPED every shorthand property. The pre-existing
  doc-comment ("Shorthand / Method / Setter / Getter / Assign —
  upstream's `t.isObjectProperty(prop)` filter matches only
  KeyValue") was wrong: shorthand IS ObjectProperty in Babel.

  **Fix.** The match arm now normalises `Prop::Shorthand(id)` into
  a synthetic `KeyValue { key: PropName::Ident(id), value:
  Box::new(Expr::Ident(id)) }` and proceeds with the rest of the
  prop walk unchanged. `Prop::Method`, `Setter`, `Getter`, `Assign`
  remain skipped (none are ObjectProperty in Babel).

  **Triage delta: parity 406 → 422 (+16), divergence 70 → 54 (−16),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  stay 464/464. Cluster knock-on broad: `css-prop/object-literal`
  9 → 5, `css-prop/string-literal` 5 → 5 (mixed), `css-prop/behaviour`
  7 → 3, `keyframes/call-expression` 3 → 2, `class-names/behaviour`
  4 → 3, `class-names/call-expression` 5 → 3, plus collateral wins
  across `__tests__/css-builder` and `__tests__/expression-evaluation`.

- **§6.8n ☑ — Destructured-binding resolution + IIFE-scope
  propagation bundle** (this session, 2026-05-06). Six sub-fixes
  landed together because they share a single root cause: Compiled's
  `resolveBinding` path handles destructured LHS by walking the
  pattern + source object pair, but the §5.0a/§5.0c port narrowed
  this to const-with-Pat::Ident only.

  **Sub-fixes:**

  - **§6.8n-i — `init_expr` gate widened from `Const + Pat::Ident`
    to `Pat::Ident` (any kind).** `compat/scope.rs::register_var_declarator`
    now populates `init_expr` for `let x = 20` / `var x = 20`
    bindings whose LHS is a plain Identifier. Babel's
    `evaluation.js:120-123` deopts only on
    `binding.constantViolations.length > 0` (i.e. observed
    reassignment), NOT on `kind`. The runtime `binding.constant`
    gate is checked at use sites
    (`evaluation.rs:445`, `traverse_identifier`); `var` deopts
    unconditionally at `evaluation.rs:457` so a populated
    `init_expr` for `var` is harmless.

  - **§6.8n-ii — Added `destructured_pat` / `destructured_init`
    fields to `Binding`.** Populated for `Pat::Object` LHS by
    `register_var_declarator`. Mirrors `resolve-binding.ts:263-269`
    which reads `binding.path.node.id` (the pattern) and
    `binding.path.node.init` (the source) at resolve time.

  - **§6.8n-iii — `Prop::Shorthand` normalisation in
    `get_object_property_value`.** Same root cause as §6.8m — the
    `Prop::KeyValue` guard silently dropped shorthand properties.
    Now `({ color })` accessed via `.color` correctly returns the
    Ident("color") value (which then folds via the recursive
    evaluator).

  - **§6.8n-iv — IIFE site registers `Pat::Object` params with
    `destructured_pat` / `destructured_init`.** `traverse_call_expression`
    previously skipped ObjectPattern params with a `documented as a
    follow-up` comment; now mirrors
    `arrowFunctionExpressionPath.scope.push({ id: <ObjectPattern>,
    init, kind: 'const' })` — registers each leaf name with the
    whole pattern + evaluated arg.

  - **§6.8n-v — `compat::evaluation::evaluate`'s Ident branch handles
    destructured bindings inline.** When the recursive evaluator
    descends into a folded ObjectExpression (e.g. an arrow body
    being walked by `babelEvaluateExpression`), it bypasses
    Compiled's resolve-binding wrapper. The Ident branch now walks
    `destructured_pat` via `getDestructuredObjectPatternKey` to
    recover the source key, then walks `destructured_init` (an
    ObjectLit at IIFE-call time — chained Ident/Member sources are
    handled by the higher-level resolve path) for the matching
    KeyValue or Shorthand, and recurses on the value with the same
    scope.

  - **§6.8n-vi — Stop restoring `meta.own_scope_override` after
    the IIFE call.** Mirrors JS upstream's
    `({ value, meta: updatedMeta } = evaluateExpression(callee, updatedMeta))`
    shape: the meta with `ownPath = arrowFunctionExpressionPath`
    propagates to the caller, so the spread branch in
    `css_builders.rs` processes the folded ObjectLit with the
    IIFE scope active. The pre-§6.8n eager restore was the
    last-mile blocker for `<div css={{ ...mixin({ color1: ... }, ...) }} />`
    shapes — without it, the outer scope's `color1` (a homonym
    of the destructured param) shadowed the IIFE binding. Leak
    bound: per-visit `Metadata` constructed at the plugin entry
    (css_prop / styled / class_names) ensures no cross-visit
    leakage. Updated the
    `iife_site_registers_param_binding_and_swaps_own_scope_override`
    unit test to assert propagation instead of restoration.

  **Triage delta: parity 422 → 441 (+19), divergence 54 → 35 (−19),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  464 → 465. Cluster knock-on broad — every `argument-arrow-function-variable`
  fixture across css-prop / styled / class-names cleared, plus the
  expression-evaluation destructuring cluster (5 → 1) and the
  member-expression-of-arrow-call shapes (`mixin().color`,
  `mixin.foo()` patterns).

- **§6.8p ☑ — Four coordinated 1:1 ports** (this session, 2026-05-06).
  Cumulative parity 453 → 459 (+6), divergence 23 → 17 (−6), swc /
  babel / both-throw unchanged. Lib tests stay 465/465.

  - **§6.8p-i — drop the unsound first-non-unresolved-Ident walker;
    always derive `top_level_mark` ctxt for the React-import inject
    via `Mark::from_u32(unresolved_mark.as_u32() + 1)`.** Drift detected:
    the §6.8i `program_top_level_ctxt` walker grabbed the FIRST Ident
    whose `ctxt != unresolved_ctxt`. SWC's resolver assigns a
    *function-scope* mark to function/arrow params and their inner
    references, which ALSO satisfy "non-empty + != unresolved" — so
    fixtures whose only such Idents lived inside arrow bodies (e.g.
    `import '@compiled/react'; ['x'].map((str) => <div>{str}</div>)`)
    grabbed the function-scope ctxt and SWC's hygiene then renamed
    our injected `import * as React` to `React1`. Fix: the
    `Program::exit` injection now ALWAYS uses
    `unresolved_mark + 1` (= `top_level_mark`, empirically reliable
    across SWC's pipeline). Fixtures cleared:
    `css-prop/behaviour::should-retain-keys-for-mapped-react-components`,
    `__tests__/index::should-compress-conditional-class-names`. (+2)

  - **§6.8p-ii — `<ClassNames>` `style={dontexist.style}` filter +
    `style={style}` outer-scope guard.** Drift detected against
    upstream `class-names/index.ts:153-188`. Two issues:
    (a) The Member arm replaced ANY `<x>.style` with the variables-built
    style value. Upstream gates on
    `t.isIdentifier(obj) && scope.hasOwnBinding(obj.name)` — when
    obj is an Ident NOT bound at the children-fn scope (e.g.
    `dontexist`), the replacement is skipped. Fix: collect every
    binding name introduced by the children-fn's first parameter
    (Ident param OR ObjectPat destructure) into a `bound_names`
    HashSet on `StyleRefReplacer`; gate the Member-arm replacement
    on `bound_names.contains(obj.name)`. Fixture cleared:
    `class-names/behaviour::should-not-transform-object-property-access-from-invalid-style-prop`.
    (b) The Ident arm replaced `style={style}` whenever
    `rename.original("style") == Some("style")` — true on every
    fixture (we seed identity entries for the css-call dispatch).
    Upstream gates on `scope.hasOwnBinding('style')` so an outer-scope
    `style` reference (e.g. `({ style }) => <ClassNames>{({ css }) =>
    <span style={style}>...`) passes through unchanged. Fix: gate
    the Ident replacement on `bound_names.contains(id.sym)`.
    Fixture cleared:
    `class-names/behaviour::should-not-transform-style-identifier-when-its-coming-from-outer-scope`.
    (+2)

  - **§6.8p-iii — extend invalid-DOM-prop walk to original styled-call
    arg expression.** Drift detected: the §6.8g/h walk operated only
    on POST-extraction `opts.class_names` and `opts.variables`. For
    fixtures where both branches of a conditional produce no CSS
    (e.g. `styled.div({ color: props => props.isPrimary ? undefined : null })`),
    the conditional class-name doesn't make it into `opts.class_names`
    — but `props.isPrimary` IS still referenced in the styled call's
    original argument subtree. Babel's `meta.parentPath` walk catches
    it; ours missed. Fix: thread the original `css_node_expr` from the
    styled handler through `build_styled_component` →
    `StyledTemplateOpts.original_css_node`, and run the
    InvalidDomPropsVisitor over it alongside class_names/variables.
    Fixture cleared:
    `styled/behaviour::should-apply-no-classes-when-both-conditional-branches-contains-empty-values`.
    (+1)

  - **§6.8p-iv — wire `addComponentName` opt's `c_<name>` className
    emit.** Drift detected: `derive_component_name` was a stub
    returning `None`. Upstream uses
    `meta.parentPath.findParent(VariableDeclaration)` to read the
    surrounding `const X = styled...` binding name; we already capture
    that name in `visit_mut_var_decl` for the displayName queue, so
    plumb the same value forward. Fix: add
    `current_styled_var_name: Option<String>` field on
    `BabelPluginVisitor`, set it pre-children-walk in
    `visit_mut_var_decl` (matching the displayName name capture's
    `decls[0].id` bug-parity), thread through `try_visit_styled` →
    `build_styled_component` → `StyledTemplateOpts.declared_var_name`
    → `derive_component_name_from_opts`. Fixture cleared:
    `__tests__/index::should-add-component-name-if-addcomponentname-is-true`.
    (+1)

- **§6.8q ☑ — JSX-runtime ordering reconciler + §2.3(b) pragma
  comment-strip + classic-pragma specifier removal** (this session,
  2026-05-06). Three coordinated landings closing the JSX-runtime
  ordering cluster (4 fixtures), the jsx-pragma cluster (2 fixtures),
  and the custom-import-source automatic-pragma fixture (1 fixture).

  - **§6.8q-i — Harness-only reconciler for `*/jsx-runtime`
    ordering delta.** Drift detected as a fundamental host-
    environment behaviour (NOT plugin drift): Babel's preset-react
    inserts the jsx-runtime import via
    `@babel/helper-module-imports::addNamed` which lands the import
    AFTER existing imports; SWC's `swc_ecma_transforms_react::Jsx`
    injects via `prepend_stmt`
    (`swc_ecma_utils:371`) which puts the import at body[0] (after
    directives only). WASM plugins always run BEFORE SWC's react
    transform — there is no `before/after` hook — so our
    `Program::exit` cannot see the jsx-runtime import to reorder it.
    Our `appendRuntimeImports` is 1:1 with upstream
    (`unshiftContainer('body', ...)` → `body.insert(0, ...)`); the
    delta is purely the post-plugin react transform's injection
    strategy. Fix: `parity-harness/babel-plugin/engines.ts`
    `reconcileJsxRuntimeOrdering(a, b)` — strips a matching
    `*/jsx-runtime` import line from BOTH outputs before byte-
    comparison. Conservative (only strips when both sides have the
    same SOURCE and same SET of specifiers — sorted, since SWC and
    Babel emit specifiers in different orders within the braces);
    real divergences (one-sided import, different sources) still
    surface. Wired into `harness.test.ts` and `triage.mjs`. Cleared
    4 fixtures: all of `__tests__/jsx-automatic`. (+4)

  - **§6.8q-ii — `@jsxImportSource` pragma comment-strip
    (§2.3(b)).** Drift detected: `scan_jsx_pragma_comments` was
    recognition-only; the upstream
    `babel-plugin.ts:157-181` pragma-comment removal was deferred.
    Without the strip, SWC's react transform reads the pragma and
    emits `import { jsx } from "<pragma-source>/jsx-runtime"`;
    Babel's preset-react (deprived of the comment by upstream's
    strip) falls back to default `react/jsx-runtime`. Bug-parity
    (per CLAUDE.md "BUGS in OLD! Need to be BUGS In NEW") —
    upstream's intent is to avoid a double-import noted at
    `babel-plugin.ts:162-165`. Fix: track the last-matched
    pragma comment's `Span` during the scan, then `take_leading(pos)`
    + filter + `add_leading_comments(pos, kept)` to remove only the
    matched comment. Sibling comments at the same anchor (e.g.
    leading copyright banners) survive the filter; non-matching
    `@jsxImportSource <other-source>` pragmas pass through to SWC's
    react transform unmolested. Three new unit tests. Cleared 2
    fixtures (one jsx-pragma, one custom-import-source-automatic).
    (+2)

  - **§6.8q-iii — Classic-pragma `{ jsx }` specifier removal
    (§2.3(b)).** Drift detected: `scan_classic_jsx_pragma_import`
    was recognition-only; the upstream `findClassicJsxPragmaImport`
    `path.remove()` on the matched specifier was deferred. Without
    it, `import { jsx } from '@compiled/react'` survives into the
    SWC output; Babel's preset-react never sees the specifier
    (upstream removed it during `Program::enter`) so Babel emits
    no such import. Fix: change signature from `&Program` to
    `&mut Program`, use `retain` on `decl.specifiers` to drop the
    matched `jsx` specifier; rely on the existing
    `remove_empty_compiled_imports` exit-time cleanup to drop the
    now-emptied `import {} from '@compiled/react'` shell when no
    sibling specifiers remain. One new unit test
    (`classic_pragma_drops_matched_jsx_specifier_only`); the
    pre-existing `classic_pragma_does_not_mutate_ast` test renamed
    + inverted to assert removal. Cleared 1 fixture
    (`should-transform-css-prop-using-jsx-pragma`). (+1)

  **Triage delta: parity 459 → 466 (+7), divergence 17 → 10 (−7),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  465 → 467 (+2 new pragma-strip tests, +1 classic-pragma removal
  test, −1 reframed test). Bun harness 954/954 (no regressions).
  Cluster knock-on: `__tests__/jsx-automatic` 4 → 0,
  `css-prop/jsx-pragma` 2 → 1, `__tests__/custom-import-source`
  2 → 1.

- **§6.8r ☑ — Member-on-member destructure resolution** (this
  session, 2026-05-06). Two coordinated fixes closing the
  expression-evaluation `statically-evaluates-deconstructed-values-from-deeply-nested-objects`
  fixture.

  **Root causes (drift detection per CLAUDE.md):** two distinct
  porting bugs in `resolve_object_pattern_value_node`
  (`crates/babel-plugin/src/utils/resolve_binding.rs`):

  1. The `Expr::Member(_)` arm collapsed two distinct JS branches
     into one. JS has separate `t.isMemberExpression(expression) &&
     t.isMemberExpression(expression.object)` (member-on-member,
     evaluator-required) and `t.isMemberExpression(expression) &&
     t.isIdentifier(expression.object)` (single-Member,
     identifier-recursion) checks. The §5.4e Rust port used
     `if let Expr::Member(_)` which matches BOTH and unconditionally
     hit the evaluator-only path — so single-Member inits like
     `const { small } = theme.fonts` returned `None` even though
     the identifier-recursion path below would have resolved them.
  2. The `Expr::Object` arm walked only top-level properties. JS
     uses `traverse(expression, { ObjectProperty: { exit ... } })`
     with `path.stop()` on first match — a recursive DFS that
     surfaces nested matches. So `resolveObjectPatternValueNode(
     theme_object, 'small')` finds `theme.fonts.small` via the
     deep walk; our top-level-only port returned `None`. Bug-parity
     note: when the key is ambiguous (e.g. `small` exists both at
     `theme.fonts.small` AND `theme.foo.small`), the JS deep-DFS
     returns the first traversal-order match — same shape our
     pre-order DFS produces. No fixture in the corpus exercises the
     ambiguous-multi-match case.

  **Fixes.**

  - **§6.8r-i — split the Member arm.** Inner arm now matches
    `member.obj` against `Expr::Member(_)` to decide
    member-on-member vs single-Member. Member-on-member returns
    None without an evaluator (existing behaviour); single-Member
    falls through to the identifier-recursion branch unchanged.
  - **§6.8r-ii — recursive ObjectExpression walk.** New
    `deep_find_object_expression_property` helper walks
    PropOrSpread::Prop trees pre-order DFS, matching `Prop::KeyValue`
    with `Ident` key and recursing into `kv.value` when it's an
    Object. Also handles `Prop::Shorthand` (matches local sym; JS
    sees `ObjectProperty { key, value }` both Identifier).
  - **§6.8r-iii — Member-on-member fallback in `traverse_identifier`.**
    Mirrors the JS member-on-member branch (`evaluateExpression`-
    folds the chain) at the leaf where the closure is in scope —
    the public `resolve_binding` surface takes `&Metadata`/
    `&ScopeIndex` and threading mutable refs through 12+ call sites
    would be invasive. The leaf already holds the closure, so a
    targeted fallback covers the case without restructuring the
    call graph: when the binding has destructured_pat +
    destructured_init AND init is member-on-member, fold the chain
    via the local closure, then walk the folded ObjectExpression
    for the destructure key.

  **Triage delta: parity 466 → 467 (+1), divergence 10 → 9 (−1),
  swc-throws / babel-throws / both-throw all unchanged.** Lib tests
  stay 467/467. Bun harness 954/954 (no regressions). Single-fixture
  impact (the deeply-nested-objects expression-evaluation case) but
  the underlying ports — split branches, deep walk, leaf-level
  member-on-member fallback — close the structural gap upstream's
  resolveObjectPatternValueNode covers. Future fixtures with this
  shape will route through the ported path automatically.

- **§6.8s ☑ — Host-environment-only SWC param hygiene-rename
  reconciler** (this session, 2026-05-06). SWC's resolver+hygiene
  pass renames a function parameter to `<name><N>` when the param
  shadows a free reference of the same name elsewhere in the module
  (`(fromColor, toColor) => ...` becomes `(fromColor1, toColor) =>
  ...` when `fromColor` is also referenced at module scope). Babel's
  generator preserves source identifier names verbatim. Repro
  confirmed without our plugin loaded — purely host (SWC) behavior.

  Same shape as §6.8q (jsx-runtime ordering): fixed in the harness,
  not the plugin. New `reconcileSwcParamHygieneRenames(a, b)` in
  `parity-harness/babel-plugin/engines.ts` walks both outputs in
  lockstep; the ONLY divergences allowed are insertions of a
  digit-suffix on an identifier in `b` (SWC) where `a` (Babel) has
  the un-suffixed identifier, with surrounding context byte-equal.
  Renames apply globally as `\b<name><digits>\b` substitution in
  the SWC output. Wired into both `triage.mjs` and
  `harness.test.ts` after the §6.8q reconciler.

  Cleared the keyframes-shadowed-values cluster (2 fixtures): both
  `dynamic-keyframe-with-shadowed-values--applied-to-a-single-element`
  and `*-applied-to-multiple-elements`.

  **Triage delta: parity 467 → 469 (+2), divergence 9 → 7 (−2).**
  Lib tests stay 467/467. Bun harness 954/954.

- **§6.8t ☑ — `Pat::Array` LHS init_expr population** (this session,
  2026-05-06). Drift detected in `compat/scope.rs:976-987`: the
  §6.8n landing populated `init_expr` for `Pat::Ident` only; `Pat::Array`
  was paired with `Pat::Object` as "destructure deopt". This is
  incorrect for ArrayPattern — Babel's `path.evaluate()`
  (`@babel/traverse/path/evaluation.js:162-168`) doesn't slot-extract
  for ANY LHS shape; it just folds the whole init via
  `binding.path.get('init')`. For `const [color] = ['blue']`, Babel
  returns `Value::Array(['blue'])`, which the
  template-literal/binary-`+` quasi-concat path then string-coerces
  via `Array.prototype.toString = elements.join(',')` → `"blue"`.

  Compiled's resolve-binding wrapper at `resolve-binding.ts:263`
  also doesn't slot-extract ArrayPattern (only ObjectPattern), so
  the whole-array shape is what the upstream pipeline observes
  end-to-end. The Rust fix matches: extend the `init_expr_for_const_ident`
  match to also cover `Pat::Array`. ObjectPattern bindings continue
  to use the `destructured_pat` / `destructured_init` pair (init_expr
  stays None) so the §6.8n slot-extract branch is the one that fires.

  Cleared `css-prop-behaviour--should-concat-explicit-use-of-style-prop-on-an-element-when-destructured-templat`
  (the `const [color] = ['blue']` + `\`${color}\`` template fixture).

  **Triage delta: parity 469 → 470 (+1), divergence 7 → 6 (−1).**
  Lib tests stay 467/467.

- **§6.8u ☑ — `importSources` relative-path resolution +
  filename-aware matcher** (this session, 2026-05-06). Closes the
  custom-import-source-relative deferral marked at
  `babel_plugin.rs::resolve_import_sources` ("Relative-path resolution
  from upstream is deferred to §5.4"). Upstream behaviour
  (`babel-plugin.ts:96-108` + `:243-259`):
  1. `importSources` entries starting with `.` get rewritten via
     `join(rootPath, origin)` where `rootPath = state.opts.root ??
     this.cwd` (Babel's cwd default).
  2. The `ImportDeclaration` handler matches userland imports against
     `this.importSources` first by exact match, then by a relative-
     path fallback: `userLand[0] === '.' && userLand.endsWith(basename(compiledOrigin))`
     gates `resolve(dirname(filename), userLand) === compiledOrigin`.

  Three landings, all 1:1 against upstream:
  1. **`PluginOptions::root: Option<String>`** in `types.rs`. The host
     wrapper threads `process.cwd()` (parity harness) or the project
     root (production Parcel transformer). The plugin runs in WASI
     with no `process.cwd()`, so this field is the only path-base
     channel — when `None`, relative entries pass through unchanged
     (preserves §2.3 pre-§6.8u behaviour).
  2. **Lexical path helpers in `babel_plugin.rs`** —
     `normalize_path` (Node `path.normalize`-equivalent: drops `.` /
     `..` lexically, strips empty components, normalises `\` to `/`
     for cross-platform string equality), `lexical_join`,
     `dirname`, `basename`. No filesystem access — WASI-safe.
  3. **`is_compiled_module_source_for_import(userland, sources, filename, root)`**
     mirrors the relative-path fallback at `babel-plugin.ts:243-259`.
     Used by `record_compiled_import` (the only upstream call site
     that does the fallback) and `remove_empty_compiled_imports` (so
     emptied relative-import shells get dropped end-to-end). Pragma
     scan continues to use the exact-only `is_compiled_module_source`
     to match upstream's `Array.includes(...)` shape at
     `babel-plugin.ts:49`.

  Harness wiring: `parity-harness/babel-plugin/engines.ts::swcEngine`
  injects `root: process.cwd().replace(/\\/g, '/')` so the SWC
  pipeline matches Babel's default cwd — Babel reads cwd
  automatically from its own state.

  Cleared `tests-custom-import-source/should-pick-up-custom-relative-import-source`.

  **Triage delta: parity 470 → 471 (+1), divergence 6 → 5 (−1).**
  Lib tests stay 467/467.

- **§6.8v ☑ — class-names in-body destructure rename + bound_names
  extension** (this session, 2026-05-06). Drift detected in
  `class_names/mod.rs`: the `RenameMap` was built from the
  children-fn's parameter list ONLY. Upstream's
  `class-names/index.ts:50-61` (renamed `c({...})` detection) and
  `:163-175` (renamed `style={styl}` detection) reach the rename via
  `path.scope.getBinding(name)` → `binding.path.node` →
  `resolveIdentifierComingFromDestructuring`, which also catches
  in-body destructure declarations like `(arg) => { const { css: c,
  style: styl } = arg; ... }`.

  Two landings:
  1. **`extend_rename_map_from_body(block, rename)`** walks the
     children-fn's top-level Block for `const { css: <local> } = ...`
     and `const { style: <local> } = ...` declarations, adding the
     (local, key) pairs to the rename map. Handles both `KeyValue`
     (rename) and `Assign` (shorthand) variants. Scope: top-level
     Block only — matches upstream's `path.scope.hasOwnBinding(...)`
     own-scope semantics.
  2. **`extend_bound_names_from_body(block, set)`** + new free-function
     `collect_pat_names(pat)` extends the `bound_names` set
     similarly. Without this, the §6.8p bound-names gate at
     `StyleRefReplacer` (`class_names/mod.rs:444-448`) would skip
     replacing `style={styl}` because `styl` wasn't in the param-
     only `bound_names` set.

  Cleared `class-names-behaviour--should-transform-style-and-css-renamed-prop-coming-from-local-variable`.

  **Triage delta: parity 471 → 472 (+1), divergence 5 → 4 (−1).**
  Lib tests stay 467/467.

- **§6.8w ☑ — Arrow value paren-unwrap + Tpl-body arrow swap**
  (this session, 2026-05-06). Two coordinated fixes for the
  styled-conditional-CSS cluster — paren-shim convention extended
  to two more paths in `extract_object_expression`'s Arrow handler.

  1. **Paren-unwrap before `Expr::Cond` match** (line 1053). SWC
     preserves `Expr::Paren` for `(p) => (cond ? a : b)` while
     Babel's parser strips it. Without unwrap, the explicit-parens
     shape (`color: (props) => (props.isPrimary ? 'blue' : 'red')`
     in fixture 0256) fell through to the catch-all CSS-variable
     path. Same shim shape as §6.8f-#2.
  2. **Apply upstream's `prop.value.body = firstExpression` mutation
     against a CLONED arrow** for the Tpl-with-Cond body shape
     (`({ isLast }) => \`${isLast ? 5 : 10}px\`` in fixture 0256
     marginRight). Upstream mutates `propValue.body` in place — our
     §4.4 stub left a comment saying the corpus didn't reach this
     path; §6.8w now does. Without the swap, the synthesised Tpl
     wraps an Arrow whose body is still the outer template, and the
     inner-arrow optimization gate at `extract_template_literal`
     never matches the Cond. Net effect: deopt to catch-all
     CSS-variable. Fix: clone the arrow, replace its body with the
     unwrapped first-expression (the Cond), then synthesize the Tpl.

  Cleared `styled-component-behaviour--should-apply-conditional-css-with-ternary-operator-for-object-styles`
  AND `*--should-apply-multi-conditional-logical-expression-with-different-props-lines-and`.

  **Triage delta: parity 472 → 474 (+2), divergence 4 → 2 (−2).**
  Lib tests stay 467/467.

- **§6.8x ☑ — `extract_branch` paren-unwrap** (this session,
  2026-05-06). Drift detected in `css_builders.rs::extract_branch`:
  the `match path_node` at line 557 didn't unwrap `Expr::Paren`. SWC
  preserves the paren wrapper around branches like
  `cond ? ({ color: 'blue' }) : ({ color: 'red' })`, while Babel's
  parser strips it. Without unwrap, paren-wrapped Object / String /
  Tpl / Cond / Member branches fell through to the catch-all
  `_ => None` arm and the entire conditional was DROPPED from the
  output (fixture 0280's arg2 `(props) => cond ? (obj) : (obj)`
  produced no atomic sheets, no conditional className).

  Fix: one-line paren-unwrap at the top of the match. Same shim
  shape as §6.8f / §6.8w.

  Cleared `styled-component-behaviour--should-apply-conditional-css-with-ternary-and-boolean-in-the-same-line`.

  **Triage delta: parity 474 → 475 (+1), divergence 2 → 1 (−1).**
  Lib tests stay 467/467. Bun harness 954/954. The single residual
  divergence is the documented §6.5 deferral
  (`should-not-transform-css-prop-with-comment-directive`) — gated
  on threading SWC's source-map proxy through the visitor so the
  per-line `@compiled-disable*` filter at
  `utils/comments.rs::is_css_prop_disabled_via_comment_store` can
  graduate from its always-false stub.

### Verifying the current state from a cold pickup

```bash
# Plugin unit + integration tests.
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 467/467 (post-§6.8x)
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
bun parity-harness/babel-plugin/triage.mjs                              # 475/1/0/0/1 (parity/div/swc-throw/babel-throw/both-throw) post-§6.8x

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
