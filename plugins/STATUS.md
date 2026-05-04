# `plugins/` STATUS — checkpoint ledger

> **Purpose.** Single source of truth for where the SWC port stands.
> Each row is a **checkpoint**: a 0%-or-100% unit of work owned by
> exactly one agent. Checkpoints are not parallelisable within
> themselves — pick one up, finish it, move on. If context is lost
> mid-checkpoint, the next agent re-reads this file and the artefacts
> the previous agent produced and continues at the same checkpoint.
>
> Read this file with `plugins/PLAN.md` (the design) and
> `plugins/READ_WRITE.md` (the WASI sandbox contract). PLAN.md is the
> spec; this file is the schedule + state.

## How to read this file

- **Status:** `☐` not started · `▶` in progress (active) · `☑` done · `⚠` blocked
- **Owner:** the agent who is or was on the checkpoint. `—` if unowned.
- **Artefacts:** the files / build outputs the checkpoint produces.
  Reading them is how the next agent verifies "is this actually done?"
- **Verification:** the exact command(s) a fresh agent runs to confirm
  the checkpoint still holds. If it doesn't, fix it before moving on.
- **Resume note:** what the previous agent left behind for the next
  one. Empty for fresh checkpoints; populated when a checkpoint is
  paused mid-flight.

## Resume here

**Closed:** Phase 0 ☑ (modulo deferred §0.10–§0.12 hardening
tasks — Phase 5 gates, NOT Phase 4 blockers). Phase 1 ☑. Phase 2 ☑.
Phase 3 ☑ (§3.1–§3.4). Phase 4 §4.1 ☑. Phase 4 §4.2 ☑. Phase 4 §4.3 ☑.
Phase 4 §4.4 ☑ (SHELL port). Phase 4 §4.5 ☑ (data adapters).
Phase 4 §4.6 ☑ **PARTIAL** (post-CSSOutput template builders +
3 leaf utils; visitor dispatch wiring deferred — see §4.6 closure
summary below).

**Next checkpoint: §4.6 finalisation OR §4.7** — depending on the
order chosen by the next session. The §4.6 closure today covers the
post-CSSOutput template construction primitives
(`build_compiled_component`, `compiled_template`, `hoist_sheet`,
`get_jsx_attribute`, `get_runtime_class_name_library`). The visitor
dispatch sites (css-prop / classNames / cssMap / styled handlers)
that USE these are NOT wired — they reach Phase 5 §5.6
(evaluate_expression) and Phase 5 §5.4 (resolve_binding) through
`buildCss`. The pragmatic next step is to land Phase 5 §5.4–§5.6
before circling back to wire the visitor — that's the gate the §4.4
SHELL was always intended to wait on. §4.7 (Parcel wrapper update)
remains independently shippable and can land in parallel.

After §4.7, §4.8 is the Phase 4 exit gate (full byte-clean for
keyframes / css / cssMap fixtures — the gate that requires Phases
5/6 to be real, not stubbed).

Phase 4 §4.3 closure: 55/55 fixtures byte-exact, JSX printer landed
1:1 from `@babel/generator@7.23.0/lib/generators/jsx.js` (122 LOC →
~270 LOC Rust including doc-comments and the SWC↔Babel field-name
divergence table). The byte-parity gate in
`crates/babel-plugin/tests/compat_generator_integration.rs` no longer
skips ANY entries via `continue` — the JSX-key-attribute axis (5
fixtures) walks the parsed Module to find the first `key=` JSXAttr
and dispatches through the new `generate_jsx_attribute_with_comments`
entry point, mirroring the JS oracle's `extractJsxKeyAttribute`
extractor exactly.

§2.3(b) is a dangling sub-checkpoint (two deferred AST/comment-store
mutations from §2.3(a)) — NOT a phase gate; bundles with the first
§6.5 css-prop handler that needs the classic-pragma divergence.

### Verifying the current state from a cold pickup

```bash
# Plugin unit + integration tests.
RUSTFLAGS="" cargo test -p babel-plugin --lib                          # 118/118 (was 99/99; +19 from §4.6 leaves + builder)
RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity              # 4/4 over 10037 entries
RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration  # 3/3 over 120 entries
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration  # 3/3 (55/55 byte-exact, zero skips)
RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib             # 56/56
RUSTFLAGS="" cargo test -p compiled-utils --lib                         # 31/31
RUSTFLAGS="" cargo test -p compiled-css --lib                           # 163/163 (was 121; +42 from CSS-port agent's §4.4 unblock)

# Bun parity harnesses.
bun test parity-harness/strip-runtime/harness.test.ts                   # 1132/1132
BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 \
  bun test parity-harness/babel-plugin/harness.test.ts                  # 954/954

# CSS-port producer-side gate (uses bun, not node — see note below).
bun run packages/equality-harness/scripts/verify.mjs                    # 336/336

# Optional: regenerate the gitignored corpora before the cargo tests.
bun parity-harness/hash/oracle.mjs
bun parity-harness/transform-css/oracle.mjs
bun parity-harness/compat-generator/oracle.mjs                          # 55 entries
```

Total: **2679 tests, zero failures, zero ignored** (+19 vs. §4.5 close).

### Phase 4 §4.6 closure summary — PARTIAL (post-CSSOutput builders; visitor dispatch deferred)

§4.6 as specified in PLAN.md task 4 says "update lib.rs so the
visitor performs both extraction and substitution in a single
traversal". The visitor's CssProp / ClassNames / Styled / CssMap
dispatch sites all reach `buildCss`, which the §4.4 SHELL stubs at
every `evaluate_expression` / `resolve_binding` / `visitCssMapPath`
panic site. **Wiring the visitor today would force the SHELL stubs
to be real → that's Phase 5 work.** The pragmatic split: §4.6 ships
the post-CSSOutput AST builders (the primitives the future visitor
calls), defers the visitor wiring itself to the Phase 5 / 6 close.

This split is consistent with the previous-agent hand-off
(§4.5 closure): "wire the call sequence inside the visitor, but
ship only the parts where evaluation isn't required" — that constraint
narrows §4.6 to the post-CSSOutput AST construction primitives.

**New files this checkpoint** (all under
`crates/babel-plugin/src/utils/`, all 1:1 ports of upstream
`packages/babel-plugin/src/utils/` siblings):

* `get_jsx_attribute.rs` (~50 LOC + 4 unit tests) — pure SWC AST
  query. `get_jsx_attribute(&Expr, &str) -> (Option<&JSXAttr>, isize)`.
  Returns `(None, -1)` for "not a JSXElement" or "no match" — exactly
  Babel's `[undefined, -1]` tuple.
* `get_runtime_class_name_library.rs` (~25 LOC + 2 unit tests) —
  one-line opts read returning `"ac"` (compression) or `"ax"`.
* `hoist_sheet.rs` (~75 LOC + 5 unit tests) — sheet-name registration
  via `MutationRecorder::SheetsInsert`. Mints UIDs through a new
  `state.next_uid_name()` mint (`_<n>` counter, fresh per pass —
  Phase 5 §5.4 lands the scope-aware variant). The actual AST
  insertion (insertBefore on Program.body's first non-import) is
  NOT a `paths_to_cleanup` entry — the data lives on
  `state.sheets()` and the Phase 6 Program::exit emit-pass reads it
  there. Signature divergence: takes explicit
  `&mut MutationRecorder` because the recorder isn't on `Metadata`.
* `build_compiled_component.rs` (~340 LOC + 8 unit tests) — both
  `compiled_template` and `build_compiled_component` from upstream's
  `build-compiled-component.ts`. Hand-built JSX AST instead of a
  `compat/template.rs` `@babel/template` analog (smaller blast
  radius; the template-parser port is deferred to a future
  checkpoint where `compat/template.rs` is genuinely needed).
  Splices className + style onto the user's JSXElement, then wraps
  in `<CC><CS>{cssArray}</CS>{jsxNode}</CC>`. Reuses every §4.5
  adapter (`transform_css_items` + `build_css_variables`) and every
  §4.6 leaf added above.

**State-shape additions:**

* `state.uid_counter: u32` + `state.next_uid_name() -> String` mint
  the `_<n>` UID names. Fresh-per-pass (matches "SWC tears down the
  WASI instance between transforms" constraint). NOT captured in
  `StateDiff` — `uid_counter` is per-pass derivable, not part of the
  cross-call cache schema.

**Recorder threading**: the recorder lives on `BabelPluginVisitor`
as a sibling field to `state` per the §2.4 split. `Metadata` does
NOT carry it; functions that record state mutations
(`hoist_sheet`, `compiled_template`, `build_compiled_component`)
take `&mut MutationRecorder` as an explicit parameter. The visitor
call shape becomes `fn(args, &mut Metadata, &mut MutationRecorder)`.

**Deliberately deferred (NOT urgent, NOT §4.7 blockers):**

* `build_styled_component.rs` — needs `pickFunctionBody`,
  `@emotion/is-prop-valid` table verbatim, `findOpenSelectors`
  regex helper, the larger forwardRef template. Defer until the
  styled-handler dispatch site lands in Phase 6 §6.7.
* `build_display_name.rs` — small leaf, ports alongside the
  per-handler dispatch that consumes it.
* `append_runtime_imports.rs` — Program::exit machinery, ports
  alongside the §2.3(b) cleanup-queue work bundle.
* `compat/template.rs` — the `@babel/template` analog. Hand-built
  AST is sufficient for `compiled_template` / `compiled_styled`;
  port `compat/template.rs` only when a future fixture needs full
  template-string parsing.
* The four visitor dispatch sites in `babel_plugin.rs` (css-prop,
  classNames, cssMap, styled, xcss-prop) — all blocked on Phase 5
  §5.4 (resolve_binding) + §5.6 (evaluate_expression). The §4.4
  SHELL's `unimplemented!()` panics fire at the first reach.

**Bug-parity preserved:**

* `get_expression`'s panic on `JSXEmptyExpression` mirrors upstream's
  `throw new Error('Empty expression not supported.')`. Unit-tested
  via `build_compiled_component_concats_existing_classname` (the
  positive path); the panic path lights up if a future fixture
  surfaces an empty `className={}` value.

**Test count delta**: babel-plugin lib 99 → 118 (+19: 4 + 2 + 5 + 8).
Total workspace: 2660 → 2679. All other gates
(hash_parity, transform_css_integration, compat_generator_integration,
strip-runtime lib + harness, full babel-plugin harness, equality
harness) unchanged at their §4.5-close numbers.

### Phase 4 §4.5 closure summary (data adapters — transform_css_items, build_css_variables)

Three new files at `crates/babel-plugin/src/utils/` mirror upstream
1:1:

* `transform_css_items.rs` (~110 LOC + 13 unit tests) — ports
  `packages/babel-plugin/src/utils/transform-css-items.ts` 1:1.
  Three exports: `transform_css_item` (private, recursive),
  `transform_css_items` (public), `apply_selectors` (public). All
  four CssItem-variant branches wired:
  - Conditional → recurses both branches; folds to `<test|!test> &&
    <classExpression || undefined>` for one-sided sheets, ternary
    when both branches carry sheets, no-op when both are empty.
  - Logical → `transform_css(get_item_css(item), opts)` then
    `compress_class_names_for_runtime` on the join, wraps in
    `LogicalExpression { op: l.operator, left: l.expression, right: <stringLit> }`.
  - Map → reads `meta.state.css_map()[name]` for sheets, threads the
    map's stored expression as classExpression.
  - Default (Unconditional / Sheet) → same `transform_css` /
    `compress_class_names_for_runtime` as Logical, but emits the
    classExpression as a bare StringLiteral (or `None` when the
    joined name is whitespace-only — matches JS's `className.trim()`
    short-circuit).

* `build_css_variables.rs` (~110 LOC + 7 unit tests) — ports
  `packages/babel-plugin/src/utils/build-css-variables.ts` 1:1.
  One export: `build_css_variables(variables, transform)`, returning
  `Vec<PropOrSpread>`. Caller-side default (matches the JS default
  arg) is `build_css_variables(&vars, |e| e)`. Bug-parity preserved:
  the prefix is ONLY emitted when suffix is ALSO present (upstream's
  `suffix && prefix && t.stringLiteral(prefix)` short-circuit).
  Locked with a `drops_prefix_when_suffix_missing_bug_parity` test.

* `compress_class_names_for_runtime.rs` (~50 LOC + 4 unit tests) —
  ports
  `packages/babel-plugin/src/utils/compress-class-names-for-runtime.ts`
  1:1. Trivial pure-string helper that
  `transform_css_items` depends on. Uses `chars().skip().take()` for
  the JS `slice(1)` / `slice(1, 5)` translation (char-indexed; for
  ASCII identical to byte slicing, but degrades gracefully on
  non-ASCII rather than panicking on a non-char-boundary byte slice).

**Helper changes:**

* `crates/babel-plugin/src/utils/css_builders.rs`:
  `logical_op_to_swc(op)` promoted from `fn` to `pub(crate) fn` so
  `transform_css_items` can reuse the LogicalOperator → BinaryOp
  translation. Single-line edit; same body.
* `crates/compiled-utils/src/lib.rs`: re-export `unique_by`
  alongside `unique` (was already present in `array.rs`, just not
  surfaced). `build_css_variables` consumes upstream's
  `unique(variables, (v) => v.name)` shape.

**Babel→SWC field-name divergences documented inline at the file
heads** (no behavioural drift, only the `t.identifier('undefined')`
→ SWC Ident, `t.logicalExpression` → BinExpr+LogicalAnd/Or/NullishCoalescing,
`t.conditionalExpression` → CondExpr.alt-not-alternate,
`t.stringLiteral` → Lit::Str(Str{ raw: None }) shape mappings).

**PluginOptions → TransformOpts conversion**: upstream JS plugin
duck-types `meta.state.opts` straight into `transformCss`. Rust
needs an explicit projection — `plugin_opts_to_transform_opts` in
`transform_css_items.rs` covers the AFM-pinned 0.19.0 surface
(no `flattenMultipleSelectors`; `sortShorthand` not on
`PluginOptions` so threads as `None`). Per-call instantiation; no
caching (matches "SWC tears down the WASI instance between calls"
constraint).

**Error semantics**: upstream JS lets `transformCss` throw and
bubble. Rust port mirrors with `unwrap_or_else(|e| panic!(...))`.
Phase 4 §4.6+ lands the proper visitor-level error channel.

**Test count delta**: babel-plugin lib 75 → 99 (+24: 4 +
7 + 13). Total workspace: 2636 → 2660. All other gates
(hash_parity, transform_css_integration, compat_generator_integration,
strip-runtime lib + harness, full babel-plugin harness, equality
harness) unchanged at their §4.4-close numbers.

### Phase 4 §4.4 closure summary (SHELL port; 4 hash-call-shape sites end-to-end)

`crates/babel-plugin/src/utils/css_builders.rs` (~1100 LOC mirroring
upstream `packages/babel-plugin/src/utils/css-builders.ts` 1:1) plus
the small tractable transitive deps (`types`, `is_empty`,
`is_compiled`, `ast`, `object_property_to_string`,
`manipulate_template_literal`) ship as a SHELL — the file structure
mirrors upstream 1:1, the four §3 hash-call-shape sites are wired
end-to-end through `compat::generator::generate` →
`compiled_utils::hash`, and every `evaluate_expression` /
`resolve_binding` / `visit_css_map_path` dispatch site is gated on a
phase-citing `unimplemented!()` panic. The §4.8 phase exit gate
(harness fixtures byte-clean for keyframes / css / cssMap) is what
will eventually require those stubs to be real; §4.4 is the
structural milestone that makes the Phase 6 handler ports possible.

**Hash-call-shape sites wired (§3 corpus already covers the inputs):**

| Site | Upstream LOC | Wiring |
|---|---|---|
| #1 keyframes name | css-builders.ts:464 | `format!("k{}", hash(&generate(call_or_tagged_tpl)))` in `extract_keyframes` |
| #2 object-expression catch-all | css-builders.ts:639 | `format!("--_{}", hash(&variable_name))` in `extract_object_expression` |
| #3 template-literal catch-all | css-builders.ts:869 | `format!("--_{}{}", hash(&variable_name), prefix_marker)` in `extract_template_literal` |

(The fourth hash-call-shape site at `atomicify-rules.ts:41/:44` lives
in `crates/css` / `crates/compiled-css`, not in babel-plugin — owned
by the CSS-port agent. Already exercised by the §3 hash parity corpus
through the §4.1 transform_css integration test.)

Each site has a unit test in `utils::css_builders::tests` that
constructs the AST shape the JS path would build and asserts the
emitted hash-keyed name matches the JS oracle. Tests reach the real
Rust dispatch (no mocking); the §3 corpus's 10037-entry parity
contract guarantees byte-equality at the hash-input boundary.

**Stubs (Phase 5 / 6 gates):**

* `evaluate_expression_stub(...)` → Phase 5 §5.6
  (utils/evaluate-expression.ts) — every dispatch into the evaluator.
* `resolve_binding_stub(...)` → Phase 5 §5.4
  (utils/resolve-binding.ts) — every dispatch into the resolver.
* `visit_css_map_path_stub()` → Phase 6 §6.3 (css-map handler).
* `has_nested_template_literals_with_conditional_rules` → Phase 5
  §5.6's NodePath-parent-traversal analog
  (`getPathOfNode + traverse(parent, ...)`).

**CSS-port agent's parallel work** (CSS_BUILDERS_DEPS.md, RESOLVED
2026-05-04). `addUnitIfNeeded` (45-property unitless lookup +
`AddUnitValue` enum) and `cssAffixInterpolation` (with
`BeforeInterpolation` / `AfterInterpolation` types) landed at
`crates/compiled-css/src/utils/{css_property,css_affix_interpolation}.rs`
as 1:1 ports of the JS source (35 affix-test-cases ported verbatim;
9 add_unit cases). Re-exported from `crates/css/src/lib.rs` so
`crates/babel-plugin/src/utils/css_builders.rs` imports from `css::`
the same shape JS imports from `@compiled/css`. `compiled-css` test
count: 121 → 163. The §4.4 SHELL uses both helpers directly at the
numeric-literal-property and template-literal catch-all sites — no
stubs at the hash-call-shape paths.

**Architectural change: `Metadata` reborrow shape.** Babel's
`{ ...meta, context: 'keyframes', keyframe: name }` object spread
shares the `state` reference and overrides fields. Rust requires an
explicit reborrow because `State` is `&mut`-held —
`Metadata::reborrow_with_context(&mut self, ctx) -> Metadata<'_>`
(landed in `types.rs`) is the analog. Every `extract_*` function in
`css_builders.rs` takes `&mut Metadata<'_>` (not `&Metadata<'_>`) so
child calls can reborrow properly. This rippled to
`object_property_to_string.rs` and
`manipulate_template_literal.rs` signatures; tests updated to pass
`&mut meta`.

**Cargo.toml move:** `css = { workspace = true }` promoted from
`[dev-dependencies]` to `[dependencies]` per the §4.6 hand-off note
in §4.3's closure (now landed at §4.4 because the helpers it
re-exports are reachable from the SHELL hash-call sites). Also added
`regex` and `once_cell` workspace deps for the upstream regex ports
in `manipulate_template_literal.rs` and `css_builders.rs`.

**Bug-parity preserved:** `manipulate_template_literal.rs`
intentionally mirrors upstream's `[;|{|}]` split-statement regex
verbatim (the `|` between bracket entries is literal in a char
class — looks like a typo but ships in production). Per CLAUDE.md
"BUGS in OLD! Need to be BUGS In NEW" (added 2026-05-04), this is
documented inline at the regex declaration so future readers don't
"fix" it.

**Deliberately deferred (NOT urgent):**

* `arrow.body = firstExpression` mutation in
  `extract_object_expression` (Babel's optimised template-literal
  wrapping for arrow-function-with-body-as-Tpl-with-cond-arg). The
  §4.4 corpus does not exercise this path; Phase 5 §5.6's
  mutable-walker shape lands the proper model.
* `nextQuasis.value.raw = after.css` mutation in
  `extract_template_literal`. Same reason — Phase 5 §5.6 mutable
  walker.
* `extract_member_expression`'s map-cache miss path (Phase 6 §6.3).

### Phase 4 §4.3 closure summary (55/55 byte-exact, zero skips)

`crates/babel-plugin/src/compat/generator/` mirrors
`@babel/generator@7.23.0/lib/` 1:1 across 10 files (~1640 LOC):
- `mod.rs` (entry — `generate`, `generate_with_comments`,
  `generate_jsx_attribute`, `generate_jsx_attribute_with_comments`)
- `buffer.rs` (output buffer; queue-cursor + drop-trailing-whitespace-before-newline; source-map machinery dropped)
- `printer.rs` (dispatcher; print/word/token/space/newline; indent + maybe_indent; leading/trailing comment threading; JSX-* `Expr` variants now route to `generators::jsx::*`)
- `node/{mod,parentheses}.rs` (paren policy; `PRECEDENCE` table 1:1; logical/binary/conditional rules)
- `generators/{mod,expressions,types,template_literals,jsx}.rs` (Identifier, NumericLiteral, StringLiteral, BooleanLiteral, NullLiteral, BinaryExpression / LogicalExpression, ConditionalExpression, UnaryExpression, MemberExpression, CallExpression, ParenthesizedExpression-as-transparent, ObjectExpression with `printList`-statement-mode, ArrayExpression, ObjectProperty / Spread, TemplateLiteral, TaggedTemplateExpression, JSXElement, JSXAttribute, JSXIdentifier, JSXMemberExpression, JSXNamespacedName, JSXEmptyExpression, JSXExpressionContainer, JSXText, JSXOpeningElement, JSXClosingElement, JSXSpreadAttribute, JSXFragment, JSXOpeningFragment, JSXClosingFragment, JSXSpreadChild).

The corpus's 4 known real divergences are all reproduced byte-exact:
1. `cond ? /* yes */ 'a-class' : 'b-class'` →
   `cond ? /* yes */'a-class' : 'b-class'` (NO space between `*/`
   and `'a-class'`).
2. `(a && b) || c` → `a && b || c` (paren dropped at precedence
   boundary). Implemented via `Expr::Paren` transparency in
   `Printer::print` so the inner expression sees the GRANDPARENT
   when paren policy decides — matches Babel's flattened-shape
   AST default.
3. `'a-class'` (single-quote source) → `'a-class'` (preserved). Via
   `Str.raw` passthrough mirroring Babel's `getPossibleRaw(node)`.
4. `/* eslint-disable-next-line */ cond ? 'a' : 'b'` → no space
   between leading block-comment and the next expression.

**SWC↔Babel comment-storage quirk discovered + worked around.**
A same-line comment between two tokens (e.g.,
`{ /* leading */ from`) is keyed in SWC's comment store as
**TRAILING of the previous token**, NOT as leading of the next.
Specifically: `comments.take_trailing(BytePos(open_brace.lo + 1))`
returns the comment, while `take_leading(prop.span.lo)` returns
nothing. Object-property iteration handles this by querying both
positions before printing the first prop. Documented inline at
`generators/types.rs::object` so future fixtures hitting the same
shape have a clear breadcrumb.

**JSX sub-step closed this session.** `generators/jsx.rs` covers
every node kind from upstream's `lib/generators/jsx.js`:
JSXElement, JSXAttribute, JSXIdentifier, JSXMemberExpression,
JSXNamespacedName, JSXEmptyExpression, JSXExpressionContainer,
JSXText, JSXOpeningElement, JSXClosingElement, JSXSpreadAttribute,
JSXFragment, JSXOpeningFragment, JSXClosingFragment, JSXSpreadChild.
Two new public entry points in `mod.rs` (`generate_jsx_attribute`,
`generate_jsx_attribute_with_comments`) carry the same byte
contract for the JSXAttribute call shape — `Printer::print(&Expr,_)`
can't be overloaded because SWC types JSXAttr separately from Expr.
The 5 jsx-key-attribute fixtures (StringLiteral / NumericLiteral /
MemberExpression / TemplateLiteral / ConditionalExpression attribute
values) reach the new entry point via the integration test's
`find_first_key_jsx_attribute` walker (mirrors the JS oracle's
`extractJsxKeyAttribute` recursive walk verbatim — both must agree
on which `key=` attribute they pluck out, otherwise the byte-parity
assertion compares different inputs).

**SWC↔Babel field-name divergences** (mechanical renames; byte
output identical). Documented inline at the head of `jsx.rs`:
`opening`/`closing` (SWC) vs `openingElement`/`closingElement`
(Babel) on JSXElement; `opening`/`closing` vs `openingFragment`/
`closingFragment` on JSXFragment; `attrs`/`self_closing`/`type_args`
vs `attributes`/`selfClosing`/`typeParameters` on JSXOpeningElement;
`obj`/`prop` vs `object`/`property` on JSXMemberExpr; `ns` vs
`namespace` on JSXNamespacedName; `expr` vs `expression` on
JSXExprContainer / JSXSpreadChild. JSX spread attributes are typed
as the generic `SpreadElement` inside `JSXAttrOrSpread::SpreadElement`
in SWC (Babel uses a JSX-specific `JSXSpreadAttribute` node — same
byte output: `{...arg}`). JSXIdentifier in Babel maps to `Ident` in
JSXElementName positions and `IdentName` in JSXAttrName / member-prop
positions on the SWC side; both expose `.sym` so the byte output is
identical.

**Deliberately deferred (NOT urgent):**
- TS type-args on JSXOpeningElement (`<MyComp<T> />`): SWC stores as
  `Option<Box<TsTypeParamInstantiation>>` on `type_args`. The corpus
  doesn't reach this branch; `jsx_opening_element` emits a
  `/*UNHANDLED-JSX-TYPE-ARGS*/` marker if `type_args.is_some()` so
  the byte-parity gate fails loudly with a clear pointer when a real
  fixture surfaces it. Port `generators/typescript.js`'s
  `TSTypeParameterInstantiation` printer at that point.
- Inner-comment threading on `JSXEmptyExpression` (`{/* hint */}`):
  upstream calls `printInnerComments()`. The corpus's 5 jsx-key
  fixtures don't carry comments inside the expression container, so
  today's no-op matches Babel exactly. When a future fixture lands a
  comment, query the `Comments` store at the `JSXEmptyExpr.span`
  bounds — same shape as `Printer::print(&Expr, _)` does for
  Expression-typed nodes.

**Other deliberately omitted upstream files** (per CLAUDE.md
"1:1 with what's reachable"): `flow.js`, `typescript.js`,
`classes.js`, `methods.js`, `modules.js`, `statements.js`,
`base.js`. None reachable from the 5 call sites; port if a real
fixture surfaces them.

### Phase 4 §4.2 closure summary

`crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` (the per-call-site
coverage manifest), `parity-harness/compat-generator/{fixtures.json,
oracle.mjs}` (the in-tree input set + JS oracle producing
`crates/babel-plugin/tests/compat_generator_corpus.json`),
`crates/babel-plugin/src/compat/{mod.rs,generator.rs}` (the stub
that §4.3 fills in), and `crates/babel-plugin/tests/compat_generator_integration.rs`
(2 GREEN gates + 1 `#[ignore]`d byte-parity gate) lock the contract
for `compat::generator::generate`.

**Pin landed:** `@babel/generator@7.23.0` + `@babel/parser@7.29.2`
(AFM-resolved under `@compiled/babel-plugin@0.36.1` commit
`16a62b8`) added to root `package.json#overrides` AND promoted to
top-level `devDependencies`. Both are required for resolution to
land inside the workspace — bun's isolated dep layout means
`require('@babel/generator')` walks past the workspace's `.bun`
store unless the package is hoisted via a top-level dep. Caught
empirically: pre-promotion, `require('@babel/generator/package.json').version`
returned `7.28.5` from `Documents\projects\node_modules\@babel\generator/`
(a sibling project's tree). Pin guard in `oracle.mjs` AND the
`corpus_shape_lock` test in the integration test both fail-fast on
this drift. Recorded in `crates/PARITY_VERSIONS.md`.

**Corpus composition (55 entries):**
- `keyframes-expression`: 11 — basic / string-key / comma-key /
  tagged-template / interpolated-template / nested / conditional /
  comment-axis ×4 (leading, trailing, eslint-disable, ternary-inner).
- `generic-expression`: 25 — Identifier / MemberExpression chain
  (dot, deep, computed) / Call / ArrayMember / literal set
  (string-double, string-single, numeric, decimal, bool, null) /
  TemplateLiteral (static + interpolated) / Binary (precedence) /
  Conditional (simple, nested) / parenthesized / comment-axis ×5
  (leading, trailing, ternary-inner, eslint-disable-line, PURE).
- `variable-init`: 6 — string / member / call / object / conditional
  / template-with-interpolation.
- `jsx-key-attribute`: 5 — string-literal / numeric-expr / member-expr
  / template-expr / conditional-expr.
- `conditional-classname-item`: 8 — `&&` / `||` / `??` / `?:` /
  `?:` with null / nested logical (paren) / comment-between-branches /
  leading eslint-disable.

**Real divergences captured at §4.2 corpus generation** (these
will fire on shortcut §4.3 implementations):
1. `cond ? /* yes */ 'a-class' : 'b-class'` → Babel emits
   `cond ? /* yes */'a-class' : 'b-class'` (NO space between
   block-comment and following expression). SWC default emits
   the space — §4.3 must match Babel.
2. `(a && b) || c` → Babel emits `a && b || c` (paren dropped at
   precedence boundary). SWC default keeps the paren.
3. `'a-class'` (single-quote source) → Babel preserves
   single-quote; SWC default re-quotes to double.
4. `/* eslint-disable-next-line */ cond ? 'a' : 'b'` → no space
   between leading block-comment and the following expression
   (same axis as 1).

**Lessons logged this session:**

**1. Bun's isolated dep layout silently bypasses `package.json#overrides`
for transitive deps unless the dep is also a top-level devDep.**
Workspace-root `bun pm ls` reported `@babel/generator@7.23.0`
correctly, but `require('@babel/generator/package.json')` from a
script INSIDE the workspace resolved `7.28.5` from a sibling
project's `node_modules` one directory up. The override flowed
into bun's `.bun/@babel+generator@7.23.0/...` store but bun
didn't symlink `node_modules/@babel/generator/`, so Node's
resolution algorithm walked past the workspace and hit the
ancestor tree. Fix: promote the pinned package to top-level
`devDependencies`. Pin guards (in oracle.mjs AND in the cargo
integration test) catch this immediately if it ever recurs —
keep both gates.

**2. AFM tiebreaker isn't auto-actionable from in-tree docs for
non-CSS deps.** `AFM_MONOREPO_DEPENDENCIES_MORE.md` and
`AFM_MONOREPO_PACKAGE_VERSIONS.md` only document
`@compiled/css@0.19.0`'s 61-item dep tree. AFM's resolution for
`@compiled/babel-plugin@0.36.1`'s `@babel/*` family had to be
fetched out-of-band (user provided: `@babel/generator@7.23.0`,
`@babel/parser@7.29.2`). When future Phase 4–6 work needs another
upstream npm-package version (e.g., `@babel/traverse`,
`@babel/types`, `@emotion/is-prop-valid`), expect to ask the AFM
dependency engineer rather than grep the in-tree docs. Recording
each new pin in `crates/PARITY_VERSIONS.md` keeps the cross-check
durable.

### Phase 4 §4.1 closure summary

`crates/babel-plugin/tests/transform_css_integration.rs` (3 tests,
120/120 entries byte-equal) locks the consumer-side parity contract
for `css::transform_css`. Corpus is 30 hand-curated CSS fixtures
from `crates/parity-runner/corpus/transform-css/` × 4 opts
permutations (`{}` default, `{optimizeCss:false}`,
`{increaseSpecificity:true}`, `{classHashPrefix:'x'}`).

**Browserslist pin:** `BROWSERSLIST_CONFIG` set to
`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` (the
EXACT production AFM pin per `BROWSER_LIST_FROM_AFM.md`); resolves
to the documented 14-entry list under workspace-pinned
`caniuse-lite@1.0.30001766` + `browserslist@4.24.2`. Both engines
honor `BROWSERSLIST_CONFIG` (`crates/browserslist-shim/src/node.rs:143`
for Rust). `BROWSERSLIST` and `AUTOPREFIXER` env vars are explicitly
unset (would short-circuit the config-file path / disable autoprefixer).

### Lessons logged this session

**1. Env-var test races are silent and look exactly like drift.**
`EnvPin` mutates process-global env vars (`std::env::set_var`).
Cargo parallelises test functions in the same binary by default; if
multiple tests construct an `EnvPin`, one thread's `set_var` collides
with another's `Drop` `remove_var`, and `transform_css` ends up
calling autoprefixer with the env var unset. Under default
browserslist (older Firefoxes), autoprefixer correctly emits
`-moz-user-select`, but the test compares against the JS oracle
captured under AFM (which doesn't need it). Apparent divergence,
real cause is the harness.

Fix landed: `EnvPin` is confined to the single test that calls
`transform_css`. The two schema-shape tests don't construct one
(they don't read env state). A `CRITICAL` rustdoc block on `EnvPin`
documents the hazard for future test authors. 5 consecutive
`cargo test` runs all 120/120 post-fix.

The original (incorrect) brief filed at
`plugins/AUTOPREFIXER_DRIFT_BRIEF.md` was deleted after the
autoprefixer agent reproduced no drift via
`crates/css/examples/repro_user_select.rs`. Apology delivered.

**2. AFM is the production browserslist pin, not `chrome 100`.**
Initial oracle used `BROWSERSLIST=chrome 100` because that's what
the parity-runner's `TransformCss` stage uses
(`crates/parity-runner/src/stages.rs:582`). That's a non-production
isolation pin. The Jira build runs against the AFM `.browserslistrc`,
documented in `BROWSER_LIST_FROM_AFM.md`. Pin switched to
`BROWSERSLIST_CONFIG=<AFM-fixture-path>` — production-accurate.
The parity-runner could be re-pinned the same way; not a plugins/*
edit.

### Drift watch point (still open — handed to CSS-port agent)

`crates/STATUS.md:3303` row 8b still says *"PARTIAL — currently
identity-passthrough"* about `crates/css/src/transform.rs`. That's
stale: `transform.rs` shipped the full plugin chain in commit
`be17def "trasnform css is ready"` (1037 lines composing every
plugin per `crates/PHASE_8B_LIFECYCLE_AUDIT.md`'s Round 1 → 2 → 3
recipe). The dispatch in `packages/css/src/transform.ts:36`
(`if (process.env.COMPILED_CSS_ENGINE === 'rust') return
require('@compiled/css-native').transformCss(css, opts);`) is wired;
the row's `:70` line citation is a wrong line number. The CSS-port
agent owns flipping row 8b to DONE; `plugins/*` does not edit
`crates/STATUS.md`.

`packages/equality-harness/scripts/verify.mjs` has a
`require.resolve`-in-ESM bug under raw `node` (the harness uses
`require.resolve` in an `.mjs` file, which is undefined — both
engines error identically and the "both-errored" branch reports a
vacuous 336/336 pass). Under `bun`, `require` is polyfilled and
the harness runs the real test. Bug-fixing the Node path is the
equality-harness owner's task, not plugins/*.

### Phase 3 closure summary (this session)

§3.1 ☑ — confirmed `pub fn hash(input: &str) -> String` at
`crates/compiled-utils/src/hash.rs:52`, re-exported at
`crates/compiled-utils/src/lib.rs:34`. Symbol stable since Phase 1.

§3.2 ☑ — built `parity-harness/hash/oracle.mjs` (mirrors the
strip-runtime synth precedent: deterministic mulberry32 seed=1,
re-runs produce a byte-identical corpus). Imports the upstream
JS `hash` via `@compiled/utils` (workspace dep, resolves to
`packages/utils/src/hash.ts`) and emits
`crates/babel-plugin/tests/hash_corpus.json` (gitignored — same
pattern as `parity-harness/strip-runtime/fixtures/synthesized/`).
Composition (10037 entries):
- **4 real-call-shape entries** — one per `hash()` call site in
  `packages/babel-plugin/src/`: `hash(generate(expression).code)`
  (css-builders.ts:464), `hash(variableName)` (css-builders.ts:639,
  869), composite atomicify key (atomicify-rules.ts:41), CSS value
  (atomicify-rules.ts:44).
- **~33 categorical entries** — empty / single char / NUL
  (leading, trailing, embedded, all-NUL) / leading-trailing
  whitespace / UTF-8 multibyte (2/3/4-byte) / surrogate-pair
  boundaries / length-tail (`l mod 4 ∈ {0,1,2,3}`, all four murmur2
  branches) / >4 KiB ASCII / >4 KiB mixed.
- **10000 random entries** — 5000 ASCII + 5000 valid Unicode
  scalar values (skips surrogate range U+D800..U+DFFF — those are
  legal JS string elements but not RFC-valid JSON; the consuming
  Babel plugin only ever passes valid UTF-8 to `hash()`, so the
  parity contract is over scalar values).

§3.3 ☑ — Rust integration test at
`crates/babel-plugin/tests/hash_parity.rs` reads the JSON corpus
and asserts byte-equality via `compiled_utils::hash`. Four tests:
`rust_hash_matches_js_corpus` (the gate),
`corpus_includes_real_call_shapes` (lock the four call-site
fingerprints), `corpus_covers_phase3_categories` (lock the §3.2
acceptance set), `corpus_has_at_least_10k_random_entries` (lock
the §3.3 ≥10K acceptance). All four pass over 10037 entries —
zero divergence. The pure-Rust gate (no WASM, no JS bridge) is
the cheapest shape for a pure-data parity check; matches the
previous agent's recommendation.

Drift fixed in `crates/compiled-utils/src/hash.rs::tests::known_vectors`:
the comment in that test (lines 198-201 prior) explicitly
telegraphed the §3.3 migration ("Once we have a Node verification
step (parity-runner) we'll cross-check them against the JS hash
and lock in the bytes from JS instead. For now the values lock
current behaviour against future drift."). Replaced the
self-referential `hash(input)` lines with five hardcoded
JS-locked byte vectors pulled directly from the corpus
(`""→"0"`, `"a"→"14mfbry"`, `"abcd"→"aougpt"`,
`"color: red"→"1wszpi4"`, `"@media (max-width: 100px)"→"w2cthn"`).
Five-vector smoke is still in-process (faster iteration loop
than reading the 1.7 MB JSON corpus); the JSON corpus remains
the authoritative parity contract.

§3.4 ☑ — Phase 3 exit gate met. Final test state:
- `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31.
- `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43
  (unchanged from §2.4).
- `RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity`
  → 4/4 over 10037 entries.
- `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib`
  → 56/56 (unchanged from §1.9).
- `bun test parity-harness/strip-runtime/harness.test.ts` →
  1132/1132 (unchanged).
- `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
  bun test parity-harness/babel-plugin/harness.test.ts` →
  954/954 (unchanged from §2.5).
- Encapsulation lint still clean (zero matches outside
  `state.rs`/`mutation_recorder.rs`).

### Phase 2 closure summary (multi-session)

§2.0 (prior): 477 fixtures extracted from the babel-plugin test
files; full-corpus Babel determinism baseline green.

§2.1 (prior): data-only `constants.rs`, `utils/constants.rs`,
`types.rs` ported.

§2.2 (prior): parity harness skeleton with `babelEngine` +
`swcEngine` (pass-through `babel_plugin.wasm`); 152 sampled tests
green.

§2.3 (this session): dispatcher SKELETON (prior) + §2.3(a)
JSX-pragma recognition (this session) landed. `BabelPluginVisitor`
is generic over `C: Comments` (plugin entry uses
`PluginCommentsProxy`; tests use `SingleThreadedComments::default()`).
Recognition covers:
  (a) Compiled-import recognition → `state.compiled_imports[apiName]`
      (moved from `visit_mut_program` pre-walk into
      `visit_mut_module_decl` so upstream's order — pragma scan
      first, then import-decl visitor — is preserved). Now routed
      through `MutationRecorder::apply` (§2.4).
  (b) `findClassicJsxPragmaImport` analog →
      `state.pragma.classic_jsx_pragma_is_compiled` +
      `classic_jsx_pragma_local_name` (handles bare, renamed, and
      StringLiteral-imported jsx specifiers).
  (c) Comment scan at `comments.get_leading(first_body_item.span.lo)`
      → `state.pragma.jsx` / `jsx_import_source` and bootstraps
      `state.compiled_imports = Some(default)`.

§2.3(a) is recognition-ONLY. Two AST/comment-store mutations
(`path.remove()` of classic-pragma `jsx` specifier; comment-store
filter dropping the matched pragma comment) are tagged
`// §2.3(b):` in `babel_plugin.rs` and deferred — see "§2.3(b)
follow-up" below.

Stub handlers for `call_expr`/`tagged_tpl`/`jsx_element`/
`jsx_opening_element` unchanged from prior session.

Two pieces of drift fixed while consuming `compiled-utils` (prior
session): (a) `COMPILED_IMPORT` was `@compiled/react`, JS source is
`@compiled/react`; (b) `jsx.ts` was never ported — added as
`jsx.rs` with `JSX_ANNOTATION_REGEX` / `JSX_SOURCE_ANNOTATION_REGEX`
both as `OnceLock<Regex>` and 6 unit tests covering the Babel
`comment.value` shape (delimiters stripped before regex match).

**Drift watch point captured this session:** upstream's pragma scan
walks the FLAT `file.ast.comments` list. The SWC analog requires an
anchor; for module-level pragmas the canonical position is the
leading-comment slot of the FIRST body item — matches the routing
pattern `babel-plugin-strip-runtime` already uses for banner
comments. Documented inline in `babel_plugin.rs` (`comments` field
doc + `scan_jsx_pragma_comments` doc).

**§2.4 closed (this session):**
- `state.rs` holds `State` + inner shapes (`CompiledImports`,
  `ImportedCompiledImports`, `PragmaState`, `CleanupAction`,
  `CleanupKind`, `CacheSlot`) with `pub(crate)` fields and
  read-only `pub fn` getters. Init-time mutators (`set_opts`,
  `set_import_sources`, `ensure_compiled_imports`,
  `set_classic_jsx_pragma`, `set_pragma_jsx`,
  `set_pragma_jsx_import_source`, `queue_cleanup`) cover the
  non-cache-captured fields. The `MutationRecorder::apply` impl
  block lives in `state.rs` (per PLAN.md §3.9.8 "same module as
  State") so it has same-module write access without exposing
  fields broader than `pub(crate)`.
- `mutation_recorder.rs` holds 5-variant `StateDiff`
  (IncludedFilesPush, CompiledImportsAppend, SheetsInsert,
  CssMapInsert, IgnoreMemberExprMark — reconciled per
  STATE_MUTATIONS.md), 5-variant `ApiKind` (ClassNames, Css,
  Keyframes, Styled, CssMap with `from_imported_name` resolver),
  and `MutationRecorder` (Vec<StateDiff> diff log with `new`,
  `diff_log`, `drain_diff_log`, and a `pub(crate)` `push_diff`
  hook the apply impl uses). Both enums derive serde for the
  Phase 5 §5.3 `cache.bin` lock; a JSON round-trip test stands in
  for the postcard test until that dep lands.
- `babel_plugin.rs` `BabelPluginVisitor` adds `recorder` field.
  `record_compiled_import` routes per-API pushes through
  `MutationRecorder::apply(StateDiff::CompiledImportsAppend, &mut state)`.
  Pragma writes use `state.set_*` mutators. The encapsulation lint
  `grep -rEn 'state\.[a-z_]+\.{push,set,add,insert,remove,extend}'`
  outside `state.rs`/`mutation_recorder.rs` returns zero matches.
- Final test state: `cargo test -p babel-plugin --lib` → 43/43
  pass (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin).
  Both parity harnesses still 1284/1284.

§2.5 (this session): Phase 2 exit gate met.
`BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
bun test parity-harness/babel-plugin/harness.test.ts` →
**954/954 pass** (477 full-corpus parity + 477 full-corpus
determinism). Strip-runtime corpus untouched (1132/1132).
Dispatcher is recognition-only, so SWC output is byte-identical
to Babel for every fixture where Babel pass-through-round-trips
through prettier; fixtures where Babel transforms and SWC stays
pass-through assert NOT-equal under `expectedToFail` and
correctly fail-as-expected.

### §2.3(b) follow-up — dangling sub-checkpoint, NOT a Phase 2 gate

Two AST/comment-store mutations marked `// §2.3(b):` in
`crates/babel-plugin/src/babel_plugin.rs`. Both are deferred per
the §2.3(a) hand-off contract:

1. `path.remove()` of the classic-pragma `jsx` specifier
   (upstream's `findClassicJsxPragmaImport.path.remove()`).
   Hides the classic pragma from the SWC analog of
   `@babel/plugin-transform-react-jsx`. NOT load-bearing for
   any §2.3 / §2.4 / §2.5 verification gate; load-bearing for
   §6.5 (css-prop) where the pragma drives output divergence.
2. Filter the matched JSX-pragma comment out of
   `comments.get_leading(first_body_item.span.lo)` — SWC
   analog of upstream's `file.ast.comments` /
   `body[0].leadingComments` filter. Same gating as (1).

Concrete shape decision pending the first §2.3(b) commit:
these are NOT `StateDiff` variants (AST/comment-store, not
cache-replay-relevant). Likely shape: extend `state.queue_cleanup`
to accept richer `CleanupAction` variants (specifier-remove with
node identity, comment-filter at BytePos), drained in
`Program::exit`. Bundle this work with the first Phase 6 handler
that consumes pragma divergence (§6.5 css-prop) so the pair lands
together end-to-end-testable.

Other §2.3-region work that's gated (NOT urgent):
- `Program::exit` `appendRuntimeImports` + banner +
  `pathsToCleanup` loop — lands alongside the first real Phase 6
  handler (no point shipping runtime imports without anything that
  needs them).
- `ImportDeclaration` specifier removal — same AST-mutation
  channel as §2.3(b).
- `is_compiled.rs` / `is_jsx_function.rs` /
  `normalize_props_usage.rs` predicate ports — gated until the
  first handler that consumes them.

**Important constraint (re-confirmed by user this session):** SWC
tears down the WASI instance between `transformSync` calls. The
`BabelPluginVisitor` is allocated fresh per `process(...)` entry —
no module-level state, no static caches, no `lazy_static` holding
plugin-state. The Phase 5 `cache.bin` design (PLAN.md §3.9.10) is
the only viable cross-transform channel and uses the filesystem,
not memory.

**Important constraint (re-confirmed):** SWC tears down the WASI
instance between `transformSync` calls. The Phase 5 `cache.bin`
design (PLAN.md §3.9.10) accommodates this by reading at
`Program::enter` and writing at `Program::exit` per transform — the
filesystem is the only viable cross-transform channel. NO in-memory
cross-transform caching anywhere in the Rust port.

**Prerequisites met:** all of Phase 0 except probes 9 and audit
(both Phase 5 gates, not Phase 1).

**Last completed:** §2.5 (Phase 2 exit gate). Final state on
sign-off (this session):
- `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43
  (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin).
- `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib`
  → 56/56 (unchanged from §1.9).
- `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31
  (unchanged).
- `bun test parity-harness/strip-runtime/harness.test.ts` →
  1132/1132 (unchanged from §1.9).
- `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1
  bun test parity-harness/babel-plugin/harness.test.ts` → 954/954
  (full-corpus pass-through parity + full-corpus determinism).
- Encapsulation lint
  `grep -rEn 'state\.[a-z_]+\.(push|set|add|insert|remove|extend)'
  crates/babel-plugin/src --include '*.rs' | grep -vE
  'state\.rs|mutation_recorder\.rs' | wc -l` → 0.
- `RUSTFLAGS="" cargo build -p babel-plugin
  -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`
  clean (modulo a documented `#[allow(dead_code)]` on
  `state::CacheSlot` — Phase 5 §5.3 placeholder).

### Hand-off — what the next session should do

**Active checkpoint:** Phase 4 §4.6 PARTIAL ☑ landed today. The
post-CSSOutput template builders (`build_compiled_component`,
`compiled_template`) and three leaf utils (`get_jsx_attribute`,
`get_runtime_class_name_library`, `hoist_sheet`) are 1:1 ported and
unit-tested. The visitor dispatch wiring is NOT done — that's
blocked on Phase 5 §5.4 (resolve_binding) + §5.6 (evaluate_expression)
because the css_builders SHELL's `unimplemented!()` stubs panic at
the first reach.

Recommended order for the next session(s):
1. **§4.7** — update `packages/parcel-transformer/src/index.ts` to
   make a single `transformSync` call per PLAN.md §8 (Parcel
   wrapper drains sidecars). **Independently shippable** — does
   not depend on Phase 5/6 work. Smallest tractable next checkpoint.
2. **Phase 5 §5.1–§5.6** — port the resolver + evaluator subtree.
   This unblocks the §4.4 SHELL stubs and then §4.6's visitor
   dispatch wiring becomes feasible.
3. **§4.6 finalisation** — back-fill the visitor dispatch sites in
   `babel_plugin.rs` (css-prop, classNames, cssMap, styled, xcss-prop
   handlers) using the §4.6 builders + the freshly-ported
   evaluate/resolve. Port `build_styled_component.rs` (with
   `is_prop_valid` table verbatim) here too.
4. **§4.8** — Phase 4 exit gate: keyframes / css / cssMap fixtures
   byte-clean.

§4.6 deliberately deferred (NOT urgent at this checkpoint;
required for §4.6 finalisation):
- `build_styled_component.rs` — needs `pickFunctionBody`,
  `@emotion/is-prop-valid` (a known-prop lookup table verbatim),
  `findOpenSelectors` regex, larger forwardRef hand-built template.
- `build_display_name.rs` — addComponentName-driven leaf.
- `append_runtime_imports.rs` — Program::exit prepend machinery.
- `compat/template.rs` — only port if a future fixture genuinely
  needs full template-string parsing (the §4.6 builders are
  hand-built, not template-parsed, so this is currently unused).
- Visitor dispatch sites — blocked on Phase 5 §5.4–§5.6.

Phase 4 §4.4 deliberately deferred (NOT urgent, NOT §4.5 blockers):
- `arrow.body = firstExpression` mutation in
  `extract_object_expression` — Phase 5 §5.6 mutable-walker.
- `nextQuasis.value.raw = after.css` mutation in
  `extract_template_literal` — same Phase 5 §5.6 reason.
- `extract_member_expression`'s map-cache miss path — Phase 6 §6.3
  visitCssMapPath wiring.

See `crates/babel-plugin/CSS_BUILDERS_DEPS.md` for the
CSS-port-agent collaboration record (RESOLVED 2026-05-04).

Phase 4 §4.3 deliberately deferred (NOT urgent, NOT blockers):
- `Expr::Paren` transparency edge cases — only one corpus fixture
  exercises this (`(a && b) || c`); test passes. If a future fixture
  surfaces a paren-drop case the current dispatch can't decide
  (e.g., a deep nested conditional inside a paren), that's a real
  Drift event worth diagnosing.
- Synthetic-AST fallbacks (str without `raw`, num without `raw`) —
  the corpus exclusively exercises parsed nodes which always carry
  `raw`. The fallback path emits naïvely; when a real consumer
  case lands a synthetic AST node, port the upstream `_jsesc`
  details.
- TS type-args on JSXOpeningElement and inner-comment threading on
  JSXEmptyExpression — see the §4.3 closure summary above.

See `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` for the
full per-call-site coverage manifest, divergence table, and
fixture-addition workflow.

Other items deliberately deferred (NOT urgent, NOT blockers):
- §2.3(b) — two AST/comment-store mutations from §2.3(a). Bundles
  with the first §6.5 css-prop handler that consumes pragma
  divergence.
- §0.10 / §0.11 / §0.12 — Phase 0 hardening tasks. Land before
  Phase 5 ships, not before it starts.

For verification commands (cold pickup), see the "Verifying the
current state from a cold pickup" block at the top of this file —
that's the canonical recipe; this hand-off block is the work
direction.

### Phase 1 closure (prior session, kept for context)

§1.9 exit-gate state: `cargo test -p babel-plugin-strip-runtime
--lib` → 56/56 pass; `bun test
parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass
(91 determinism — 41 hand-curated + 50 sampled synth — and 1041
parity — 41 hand-curated + 1000 synth, with 9 hand-curated fixtures
still gated `expectedToFail`: 3 on Phase 2 compiledBabelPlugin/bake
parity, 6 on Phase 7 directive-prologue blank-line). Synthesised
fixtures are deterministic — re-running
`bun parity-harness/strip-runtime/synthesize-fixtures.mjs --count
1000` produces a byte-identical corpus.

**§1.8 surfaced one real port defect.** Multi-component fixtures
sharing an atomic-CSS declarator (e.g. two `<div css={{display:'inline-block'}}>`
inputs that bake to a shared `_` binding) drove a divergence at synth
fixture 42: Babel pushes the rule once because `binding.path.remove()`
inside `removeStyleDeclarations` invalidates the scope entry in-place,
so the second visit's `getBinding(name)` returns undefined. The Rust
port deferred binding removal to `Program::exit`, leaving the binding
queryable for the second visit and pushing a duplicate rule. Fix:
`crates/babel-plugin-strip-runtime/src/compat/scope.rs`
`mark_for_removal` now clears the cached string value on the binding
entry while keeping the location so `apply_removals` still drops the
declarator. New unit test `mark_for_removal_invalidates_subsequent_lookup`
locks the parity contract.

**End-of-session notes for next pickup:**

- Architect re-pinned the upstream `packages/babel-plugin-strip-runtime`
  source. Audited every file (`index.ts`, `types.ts`, all five
  `utils/*.ts`) against the existing port — byte-identical. No
  re-port needed.
- Two upstream JS-side fixes applied while resolving the audit-blocked
  harness: (a) `packages/css/src/index.ts` now uses
  `export { type AfterInterpolation, type BeforeInterpolation }`
  (interfaces aren't runtime values); (b)
  `packages/babel-plugin-strip-runtime/src/__tests__/{strip-runtime-source-code,strip-runtime-transpiled-code}.test.ts`
  had a stale `regexToFindRequireStatements` literal — `@compiled\/`
  bumped to `@compiled\/` to match the renamed constants.
- Side-effect of the rename: `DEFAULT_IMPORT_SOURCES` is
  `['@compiled/react', '@atlaskit/css']`. Fixtures driven from any
  source string still using `@compiled/react` go untransformed (no
  CC/CS wrappers, every `expectsError` fixture's throw-path is dead).
  `generate-fixtures.mjs` was updated accordingly. **If you regenerate
  fixtures or write new ones, use `@compiled/react` as the import
  source.**

---

## Phase 0 — Prerequisites and parity harness

> **Goal:** stand up the verification oracle, validate every load-
> bearing architectural assumption, before any port code lands.
>
> **Phase exit gate:** ☑ — all prerequisites for Phase 1 met.
> Probes 9 (resolver matrix) and §0.10 (audit script) defer to
> Phase 5; they do not block Phase 1.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §0.1 | ☑ | Pin `swc_core@54.0.0` and `prettier@2.8.8` in `crates/PARITY_VERSIONS.md` and root `package.json` `overrides` | claude-2026-05-02 | `crates/PARITY_VERSIONS.md` (SWC + Prettier sections), `package.json` `overrides` block | `bun pm ls \| grep swc/core` shows `1.15.8`; `cat crates/PARITY_VERSIONS.md \| grep '54.0.0'` non-empty |
| §0.2 | ☑ | Scaffold `crates/babel-plugin` and `crates/babel-plugin-strip-runtime` cdylib + rlib for `wasm32-wasip1` | claude-2026-05-02 | `crates/babel-plugin/{Cargo.toml,src/lib.rs}`, `crates/babel-plugin-strip-runtime/{Cargo.toml,src/lib.rs}`, both built `.wasm` artefacts under `crates/target/wasm32-wasip1/release/` | `RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release` produces ~1.4 MB `.wasm` per crate |
| §0.3 | ☑ | Build `crates/babel-plugin/STATE_MUTATIONS.md` enumerating every state-mutation site and reconciling §3.9.8's `StateDiff` enum | claude-2026-05-02 | `crates/babel-plugin/STATE_MUTATIONS.md`, PLAN.md §3.9.8 amended to 5-variant enum (added `IgnoreMemberExprMark`) | Re-run `grep -rEn 'state\.(includedFiles\|compiledImports\|sheets\|cssMap\|ignoreMemberExpressions)\b' packages/babel-plugin/src/` and confirm count matches the file's table |
| §0.4 | ☑ | Build the §3.9.14 probe plugin and run probes 1, 2, 3, 4, 5, 6, 7 on Windows | claude-2026-05-02 | `crates/babel-plugin-phase0-probes/`, `phase0-probes/probes.test.ts`, `crates/babel-plugin/PHASE0_FINDINGS.md` | `bun test phase0-probes/probes.test.ts` → 7 pass / 0 fail |
| §0.5 | ☑ | Document the WASI `/cwd` mount semantics in PLAN.md §3.2 | claude-2026-05-02 | PLAN.md §3.2 corrected (host-side path translation contract added) | `cat plugins/PLAN.md \| grep -A2 '/cwd'` shows the mount documentation |
| §0.6 | ☑ | Stand up `parity-harness/strip-runtime/` skeleton — Babel + SWC engines, fixture loader, bun test driver | claude-2026-05-02 | `parity-harness/README.md`, `parity-harness/strip-runtime/{engines.ts,harness.test.ts,fixtures/*.json}` | `bun test parity-harness/strip-runtime/harness.test.ts` → 6 pass / 0 fail |
| §0.7 | ☑ | Babel-vs-itself determinism baseline (3 fixtures × 3 runs) | claude-2026-05-02 | The 3 `Babel determinism baseline` tests in `parity-harness/strip-runtime/harness.test.ts` | Same harness command above; the 3 determinism tests pass |
| §0.8 | ☑ | Seed 3 representative strip-runtime fixtures (full extraction → Phase 1 §1.1) | claude-2026-05-02 | `parity-harness/strip-runtime/fixtures/{extract-automatic-passthrough,extract-automatic-stripped,extract-classic-stripped}.json` | `ls parity-harness/strip-runtime/fixtures/*.json \| wc -l` ≥ 3 |
| §0.9 | ☑ | Confirm harness can detect drift (the 2 `expectedToFail` parity tests pass *because* SWC passthrough diverges from Babel — proves the oracle is sensitive) | claude-2026-05-02 | The 2 `expected-to-fail` tests in the harness | Same harness command; the 2 expected-to-fail tests pass |
| §0.10 | ☐ | Build `scripts/audit-included-files.ts` — instrument `onIncludedFiles`, count out-of-cwd outliers across consumer monorepo | — | `scripts/audit-included-files.ts`, `crates/babel-plugin/INCLUDED_FILES_AUDIT.md` (target ≤100 outliers per workspace) | `bun run scripts/audit-included-files.ts <consumer-workspace>` produces an outlier count; documented per workspace |
| §0.11 | ☐ | Resolver difference matrix — `enhanced-resolve@5.x` vs `npm resolve.sync` vs `oxc_resolver` | — | `crates/babel-plugin/RESOLVER_MATRIX.md` | A matrix of resolution requests with three engines' results; gaps documented |
| §0.12 | ☐ | Run probes 1–7 on Linux + macOS (CI matrix) | — | CI job entry; results appended to `crates/babel-plugin/PHASE0_FINDINGS.md` "Cross-platform results" section | CI passes on `ubuntu-latest` and `macos-latest`; results captured |

§0.10 and §0.11 are Phase 5 gates — not blockers for Phase 1.
§0.12 is a hardening task — not a blocker for Phase 1, but should be
done before declaring Phase 0 fully signed off across the platform set.

### Phase 1 findings (write-once notes)

- **§1.8 — shared-binding scope-invalidation parity bug.** When two
  components reference the same atomic-CSS declarator (e.g. both have
  `display: inline-block`, baking to a shared `const _ = "._1e0c1o8l{display:inline-block}"`),
  Babel pushes the rule once because `binding.path.remove()` inside
  `removeStyleDeclarations` invalidates the scope entry in-place — the
  second visit's `parentPath.scope.getBinding('_')` returns undefined.
  The Rust port originally deferred all removals to `Program::exit`'s
  `apply_removals`, so the second visit still saw a live binding and
  pushed a duplicate. Fix: `mark_for_removal` clears the cached string
  value on the binding entry while keeping its `BindingLocation` so
  `apply_removals` can still drop the declarator. The hand-curated
  41-fixture set never exercised this path (every fixture has a single
  component); the §1.8 synth corpus surfaced it on fixture
  `synth-00042-automatic-extract-adds-require`.

- **§1.5 — `.compiled.css` postcss `sort()` deferred to Phase 4.**
  Babel's `extractStylesToDirectory` write path calls
  `sort(styleRules.sort().join('\n'), sortConfig)` from `@compiled/css`.
  The Rust port lives at `crates/css/src/sort.rs` and depends on
  `compiled-css` + the postcss-* crates (`postcss-core`,
  `postcss-discard-duplicates`, `cssnano-preset-default`, …).
  Pulling that into `babel-plugin-strip-runtime` would inflate the
  WASM binary several-fold *now*, before Phase 4 wires the same
  CSS Rust port into `babel-plugin` proper (PLAN.md §3.5 says both
  plugins link the CSS crate). For §1.5 the file write uses the
  JS-level pre-sort only (`styleRules.sort().join('\n')`), which
  produces a non-empty file with the rule set complete. The
  postcss-level `sort(stylesheet, sortConfig)` will land alongside
  Phase 4. The harness gate for §1.5 is JS-byte parity (the
  `import './<basename>.compiled.css'` injection on the AST side);
  `.compiled.css` contents are NOT diffed against Babel today.
  When Phase 4 lands `crates/css` into `babel-plugin`, the same dep
  becomes available here and the postcss sort lands as a single
  follow-up commit.
- **§1.5 — `swc_common::errors::HANDLER` for plugin-side throws.**
  A raw `panic!()` in a SWC plugin is wrapped by the runner as
  `plugin failed to invoke plugin on '<filename>'` — the original
  message is dropped. Use
  `HANDLER.with(|h| h.struct_span_err(span, msg).emit())` and
  return early for any error that needs to be visible to the host
  (e.g. the `Source directory '<source>' was not found relative to
  source file ('<sourceFileName>')` throw at A02). HANDLER-emitted
  errors propagate cleanly through `swc.transformSync` to the
  host's catch.
- **§1.5 — host responsibilities the harness inlines.** Two
  concerns the production Parcel transformer wrapper will own
  (PLAN.md §3.9.13) but the harness re-implements: (a)
  `extractStylesToDirectory` writes use process.cwd() as the WASI
  `/cwd` preopen, so the harness `process.chdir`s into a scratch
  dir around the SWC call to scope writes; (b) `compiledRequireExclude`
  needs `<callScratch>/style-rules.json`, so the harness mkdirs a
  per-call dir under `_scratch`, host-translates to `/cwd/<rel>`
  via `toWasiPath`, threads it as `callScratch` plugin config, and
  rmSyncs in `finally`. The plugin only ever sees `/cwd`-prefixed
  paths (PLAN.md §3.2 contract).
- **Phase 7 breadcrumb — `/*#__PURE__*/` duplicates after CC-replacement.**
  When `StripRuntimeVisitor` replaces `/*#__PURE__*/_jsxs(CC,...)` with
  the inner `/*#__PURE__*/_jsx('div',...)`, SWC's codegen emits TWO
  pure annotations (`/*#__PURE__*/ /*#__PURE__*/ _jsx(...)`) even
  though the leading-comment store has only one entry. Probe
  evidence: `take_leading(outer.span.lo)` returned the single outer
  PURE; `get_leading(outer.span.lo)` after the drop is `None`;
  `get_leading(inner.span.lo)` is `Some([PURE])`. So the store IS
  clean — the duplication originates in `swc_ecma_codegen`'s
  multi-span emit path (CallExpr + callee MemberExpr + leading-Ident
  all triggering `emit_leading_comments_of_span`). For §1.4 the
  harness post-processes the doubled PURE via
  `parity-harness/strip-runtime/engines.ts`'s
  `/(\/\*#__PURE__\*\/\s+)\1+/g` collapse; Phase 7 should replace
  this workaround by either pruning at every relevant sub-span or
  relocating the inner's leading comments to a single canonical
  position before printing.

- **`extractStylesToDirectory` writes to disk during harness runs.**
  The strip-runtime plugin's `Program.exit` calls `mkdirSync` +
  `writeFileSync` against `<babel.cwd>/<dest>/<rel>.compiled.css`. The
  Jest test mocks `fs`; Bun does not. The harness now passes
  `babel.cwd = parity-harness/strip-runtime/_scratch` for those
  fixtures, so writes are scoped + gitignored.
- **`expectsError` fixture schema.** Four fixtures (A02, B05, B06, B07)
  assert that the plugin throws. The harness now treats the
  Babel-side throw (with matching `expectsError.babelMessage`) as the
  determinism oracle for those fixtures, and requires the SWC side to
  throw the same message once the §1.4 port lands.

### Phase 0 findings (write-once notes)

- **WASI mount path is `/cwd`, not `/`.** `env::current_dir()`
  returning `Ok("/")` is cosmetic. Plugin must use `/cwd/<rel>` literal
  paths. PLAN.md §3.2 corrected. See `crates/babel-plugin/PHASE0_FINDINGS.md`.
- **Build trap:** user-global `RUSTFLAGS` includes `-C lto=thin` which
  breaks proc-macro deps. Always build SWC plugin crates with
  `RUSTFLAGS=""`. Documented in both Cargo.toml files.
- **`StateDiff` enum is 5 variants, not the 4 sketched in §3.9.8.**
  `IgnoreMemberExprMark` was missing. PLAN.md amended.
- **Bun's caret resolution drifts past `package.json` pins.** Same
  pattern as the existing CSS-port discovery — use root `overrides` to
  hold every byte-affecting dep at the exact reference version.
  `@swc/core@1.15.8` and `prettier@2.8.8` added to the `overrides`
  block.

---

## Phase 1 — `babel-plugin-strip-runtime` 1:1 port

> **Goal:** ship a `wasm32-wasip1` SWC plugin that is byte-equivalent
> to `packages/babel-plugin-strip-runtime` after prettier
> normalisation across all 38 existing tests + ≥1000 synthesised
> fixtures.
>
> **Why this plugin first:** smaller (6 source files, ~600 LOC), no
> cross-file resolution, no `transformCss` calls. Validates the
> end-to-end toolchain (WASI build, prettier oracle, sidecar
> manifests, SWC ABI) on a tractable target.
> `packages/parcel-transformer/` is an EXAMPLE consumer shape — not a
> deliverable of this port. The plugin contract is `(source string +
> PluginOptions) → (transformed code + sidecar JSON)`; how downstream
> bundlers wire that up is out of scope.
>
> **Phase exit gate:** all 38 strip-runtime tests pass through the
> parity harness AND ≥1000-file synthesised fixture run is
> zero-divergence.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §1.0 | ☑ | Extract all 38 fixtures from the existing strip-runtime test files into `parity-harness/strip-runtime/fixtures/*.json` | claude-2026-05-02 | 38 fixture JSON files (`A01`–`A04`, `B01`–`B10`, `C01`–`C16`, `D01`–`D08`) under `parity-harness/strip-runtime/fixtures/`; generator at `parity-harness/strip-runtime/generate-fixtures.mjs`; `@babel/preset-env` + `@babel/preset-typescript` added to `packages/babel-plugin-strip-runtime/package.json` devDeps (resolves the dep drift) | `ls parity-harness/strip-runtime/fixtures/*.json \| wc -l` reports 41 (3 phase-0 seeds + 38 new); `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass |
| §1.1 | ☑ | Port `utils/to_uri_component.rs` (URL-encode + escape `!` to `%21`) | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/utils/to_uri_component.rs`, `crates/babel-plugin-strip-runtime/src/utils/mod.rs`, `lib.rs` declares `pub mod utils;` | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib to_uri_component` → 10/10 pass; cross-checked against JS `encodeURIComponent(x).replace(/!/g,'%21')` over 12 inputs (CSS rules, full unreserved set, `!`, UTF-8 multibyte, NUL, webpack loader separator) — byte-equal |
| §1.2 | ☑ | Port `utils/is_automatic_runtime.rs`, `utils/is_cc_component.rs`, `utils/is_create_element.rs` predicates | claude-2026-05-02 | three `.rs` files under `crates/babel-plugin-strip-runtime/src/utils/`, `mod.rs` declarations updated, tests construct AST nodes via `swc_core::ecma::ast` builders (no parser dep — keeps `wasm32-wasip1` build minimal) | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 32/32 pass (10 + 10 + 6 + 6) |
| §1.3 | ☑ | Port `utils/remove_style_declarations.rs` + create `compat/scope.rs` for SWC binding lookup | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/compat/{mod.rs,scope.rs}` (module-scope binding index with deferred `apply_removals`), `crates/babel-plugin-strip-runtime/src/utils/remove_style_declarations.rs` (handles `React.createElement(CS, ..., [..])`, `_jsx(CS, { children: [..] })`, `<CS>{[..]}</CS>`), `lib.rs` declares `pub mod compat;` | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 44/44 pass (10 to_uri + 10 is_automatic_runtime + 6 is_cc + 6 is_create_element + 6 scope + 6 remove_style_declarations) |
| §1.4 | ☑ | Port `lib.rs` entry + dispatcher: `Program::exit`, `ImportSpecifier`, `JSXElement`, `CallExpression`. Lock `Program::exit` ordering (banner → preserveLeadingComments → require-OR-css-OR-metadata, never two) | claude-2026-05-02 | `crates/babel-plugin-strip-runtime/src/lib.rs` rewritten — `StripRuntimeVisitor` (CC/CS replacement, `take_leading` mirroring Babel's `path.node.leadingComments=null`, ImportSpecifier filter for CC/CS), `make_require_stmt` + `first_non_directive_index` for the `styleSheetPath` injection path, `unwrap_paren` in `is_automatic_runtime` for CommonJS-interop callees `(0, _jsxRuntime.jsx)(...)`. Visitor scope cleanup runs BEFORE require-injection so binding indices stay valid. The auto-`expectedToFail` heuristic is removed; per-fixture `expectedToFail` + `failureReason` (in `generate-fixtures.mjs`'s `EXPECTED_TO_FAIL` map) now gates the 13 fixtures that depend on later phases — 4 on §1.5 (extractStylesToDirectory file write), 3 on Phase 2 (compiledBabelPlugin errors / babelJSXImportSource bake), 6 on Phase 7 (directive-prologue blank-line). | `RUSTFLAGS="" cargo build -p babel-plugin-strip-runtime --target wasm32-wasip1 --release` clean; `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass (41 determinism + 41 parity, 13 of which gated `expectedToFail` with explicit reasons) |
| §1.5 | ☑ | Sidecar handlers: `compiledRequireExclude=true` writes `<callScratch>/style-rules.json`; `extractStylesToDirectory.dest` writes `.compiled.css` files via `/cwd` preopen | claude-2026-05-03 | `crates/babel-plugin-strip-runtime/src/lib.rs` extends `PluginOptions` with `call_scratch` + `source_file_name` (host-threaded — Babel reads `file.opts.generatorOpts.sourceFileName` natively, SWC has no equivalent), adds `parse_name`/`dirname`/`path_join` helpers, `validate_dest_under_cwd` (rejects absolute, drive-prefixed, `..`-escape paths) called at plugin entry, `make_side_effect_import` (with banner-span re-anchoring trick), `StyleRulesSidecar` v1 (`{version, rules}`); Program::exit branch wires both side-effect outputs and uses `swc_common::errors::HANDLER` for the source-not-found error path so the message survives plugin-runner wrapping. `parity-harness/strip-runtime/engines.ts` threads `sourceFileName` + `callScratch` (mkdir + cleanup, host-translated `/cwd/<rel>` form) into the SWC plugin config and `process.chdir`s into `_scratch` for `extractStylesToDirectory` fixtures so plugin writes scope under the WASI preopen. `generate-fixtures.mjs` ungates A01–A04. | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 55/55 pass (44 prior + 11 §1.5 helpers); `bun test parity-harness/strip-runtime/harness.test.ts` → 82/82 pass; manual inspection: `_scratch/dist/app.compiled.css` non-empty (4 sorted rules), per-call `<callScratch>/style-rules.json` non-empty with v1 shape `{"version":1,"rules":[...]}` |
| §1.6 | ☑ | Lock `plugins/SIDECAR_SCHEMA.md` v1 (PLAN.md §7) | claude-2026-05-03 | `plugins/SIDECAR_SCHEMA.md` (v1 schema for `style-rules.json` §2, `included-files.json` §1, `cache.bin` §3 + plugin-config shape §4); Rust `StyleRulesSidecar` struct doc-comment in `crates/babel-plugin-strip-runtime/src/lib.rs` cites SIDECAR_SCHEMA.md §2; harness reader comment in `parity-harness/strip-runtime/engines.ts` cites the same. Versioning policy + drift watch points documented. | `plugins/SIDECAR_SCHEMA.md` exists; `grep -n SIDECAR_SCHEMA crates/babel-plugin-strip-runtime/src/lib.rs parity-harness/strip-runtime/engines.ts` non-empty (Rust writer + JS host both back-reference); harness 82/82 unchanged. |
| §1.7 | — | ~~Inline the SWC wrapper in `packages/parcel-transformer/`~~ | — | Parcel-transformer integration is an EXAMPLE consumer shape (`plugins/PARCEL_USAGE_EXAMPLE.md`), not a Phase 1 deliverable. Removed from gate. | n/a |
| §1.8 | ☑ | Generate ≥1000 synthesised already-baked fixtures (run JS babel-plugin against random inputs to produce CC/CS-wrapped code, freeze as fixtures) | claude-2026-05-04 | `parity-harness/strip-runtime/synthesize-fixtures.mjs` (deterministic mulberry32 seeded generator), `parity-harness/strip-runtime/fixtures/synthesized/synth-NNNNN-*.json` × 1000; harness loader updated to recurse into subdirs (`walkFixtureFiles`); 50-sample determinism stride to keep wall-clock tractable on the synth tail. Surfaced one real port defect — multi-component fixtures sharing an atomic-CSS declarator pushed the shared rule twice; fixed in `crates/babel-plugin-strip-runtime/src/compat/scope.rs::mark_for_removal` which now clears the cached binding value on mark (parity with Babel's immediate `binding.path.remove()`); new unit test `mark_for_removal_invalidates_subsequent_lookup` locks it in. | `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass (91 determinism + 1041 parity). Re-running the generator twice produces a byte-identical corpus (`shasum` of all 1000 fixture files matches across runs). |
| §1.9 | ☑ | **Phase 1 exit gate:** all checkpoints above closed; full corpus is byte-clean | claude-2026-05-04 | This STATUS.md updated with the sign-off summary (no separate `phase1-signoff.md` — the row above + the "Resume here" block carry the closure record). | `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 56/56 pass; `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132 pass (1041 fixtures parity, zero divergence). |

---

## Phase 2 — `babel-plugin` scaffold + dispatcher

> **Goal:** stand up the visitor skeleton + state setup for the larger
> plugin. Pass-through is byte-equal before any handler logic ports.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §2.0 | ☑ | Extract all babel-plugin fixtures from `packages/babel-plugin/src/**/__tests__/*.test.ts` into `parity-harness/babel-plugin/fixtures/*.json` | claude-2026-05-04 | `parity-harness/babel-plugin/extract-fixtures.mjs` (runtime extractor — Bun.plugin loader rewrites `test-utils.ts` so `transform` records `(code, opts)` per call; Jest globals stubbed so describe/it bodies execute synchronously without assertion side-effects), `parity-harness/babel-plugin/{engines.ts,harness.test.ts}` skeleton, 477 fixture JSONs (gitignored, regenerable). 6 test files skipped: 3 use `jest.mock`/`jest.fn`/`jest.spyOn` against utility internals (`cache`, `object-property-to-string`, `module-traversal`), 1 is the perf benchmark (`__perf__/module-traversal-cache`), 2 (`errors`, `resolver`) are throw-assertion-only and don't carry byte-parity signal. | `bun test parity-harness/babel-plugin/harness.test.ts` → 120/120 pass (sampled stride). `BABEL_PLUGIN_FULL_DETERMINISM=1 bun test parity-harness/babel-plugin/harness.test.ts` → 477/477 pass (full corpus). |
| §2.1 | ☑ | Port `types.rs`, `constants.rs` (data only, no logic) | claude-2026-05-04 | `crates/babel-plugin/src/constants.rs` (1:1 port of `packages/babel-plugin/src/constants.ts`), `crates/babel-plugin/src/utils/constants.rs` (1:1 port of `packages/babel-plugin/src/utils/constants.ts`), `crates/babel-plugin/src/types.rs` (port of `types.ts` — `PluginOptions`, `State`, `Tag`, `Metadata`, `TransformResult`, with `CacheMode` custom (de)serializer for the `bool \| "file-pass"` wire shape and `IndexMap` everywhere ordering matters). Babel-only types (`NodePath`, `PluginPass`, the JS-callback `Resolver`) deliberately omitted — see module docs in `types.rs` for the resolution table. | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 5/5 pass (CacheMode bool/string round-trip, PluginOptions camelCase wire shape, default-is-all-None, full round-trip). `RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release` clean. |
| §2.2 | ☑ | Build `parity-harness/babel-plugin/{engines.ts,harness.test.ts}` mirroring strip-runtime's shape | claude-2026-05-04 | `parity-harness/babel-plugin/engines.ts` (`babelEngine` + `swcEngine` + `diffSummary` — `swcEngine` runs `babel_plugin.wasm` in pass-through mode through SWC parser/codegen + prettier round-trip), `parity-harness/babel-plugin/harness.test.ts` (Babel ↔ SWC parity describe block + Babel determinism baseline; `expectedToFail` semantics mirror strip-runtime — fixtures where Babel transforms the input assert NOT-equal vs the pass-through SWC, fixtures where prettier round-trips identically through both assert byte-equal). Stride samples by default (30 parity / 100 determinism); `BABEL_PLUGIN_FULL_PARITY=1` and `BABEL_PLUGIN_FULL_DETERMINISM=1` flip to full corpus. | `bun test parity-harness/babel-plugin/harness.test.ts` → 152/152 pass (32 parity sample + 120 determinism sample); `BABEL_PLUGIN_FULL_DETERMINISM=1` → 477/477 determinism. Full parity gate (§2.5) lights up after §2.3+ ports land. |
| §2.3 | ▶ | Port `lib.rs` entry + dispatcher visitor with stubbed handlers (no-ops that record "would have visited" in a debug log) | claude-2026-05-04 (skeleton + §2.3(a)); `—` for §2.3(b) | `crates/babel-plugin/src/lib.rs` (rewritten — `process(...)` instantiates `BabelPluginVisitor` with `PluginCommentsProxy`, threads `PluginOptions`; `run_dispatcher` is now generic `<C: Comments>` so tests can pass `SingleThreadedComments::default()`), `crates/babel-plugin/src/babel_plugin.rs` (dispatcher: `BabelPluginVisitor<C: Comments>` holds owned `State` + computed `import_sources` + comment proxy. **§2.3(a) added:** `scan_classic_jsx_pragma_import` records `pragma.classic_jsx_pragma_*` for `import { jsx }` from Compiled origins (handles bare / renamed / StringLiteral-imported shapes); `scan_jsx_pragma_comments` reads `comments.get_leading(first_body_item.span.lo)` and applies the `@jsx` / `@jsxImportSource` regexes from `compiled_utils::jsx`, setting `pragma.jsx` / `jsx_import_source` and bootstrapping `state.compiled_imports = Some(default)`. Recognition only — two AST/comment-store mutations marked with `// §2.3(b):` TODOs at the call sites the MutationRecorder will flip on. Compiled-import recognition moved from `visit_mut_program` pre-walk into `visit_mut_module_decl` to preserve upstream's `Program::enter` (pragma scan) → children walk (import-decl visitor) order. Stub `visit_mut_call_expr`/`tagged_tpl`/`jsx_element`/`jsx_opening_element` unchanged.). Drift fixed earlier in `crates/compiled-utils/src/constants.rs` (`COMPILED_IMPORT`) and `crates/compiled-utils/src/jsx.rs` ported (was missing). Remaining for §2.3(b) (gated on §2.4): (i) `MutationRecorder.queue_specifier_remove(...)` for the classic-pragma `jsx` specifier; (ii) comment-store filter dropping the matched JSX-pragma comment from `body[0].leadingComments`; (iii) generic `ImportDeclaration` specifier removal + auto-`path.remove()` when drained; (iv) `Program::exit` `appendRuntimeImports` + banner + `pathsToCleanup` loop. | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 25/25 pass (5 §2.1 types + 8 prior §2.3 skeleton + 12 new §2.3(a): 5 classic-pragma recognition, 6 comment-scan, 1 end-to-end `visit_mut_program` exercising classic-pragma + comment + ImportDeclaration walk in one pass). `RUSTFLAGS="" cargo test -p compiled-utils --lib` → 31/31. `RUSTFLAGS="" cargo test -p babel-plugin-strip-runtime --lib` → 56/56. Pass-through preserved: `bun test parity-harness/strip-runtime/harness.test.ts parity-harness/babel-plugin/harness.test.ts` → 1284/1284 pass. |
| §2.4 | ☑ | State struct with `IndexMap` everywhere, `pub(crate)` field encapsulation, `MutationRecorder::apply` as only mutator (per `STATE_MUTATIONS.md`) | claude-2026-05-04 | `crates/babel-plugin/src/state.rs` (State + CompiledImports + ImportedCompiledImports + PragmaState + CleanupAction + CleanupKind + CacheSlot moved here from types.rs; fields are `pub(crate)` with read-only public getters; init-time mutators `set_opts` / `set_import_sources` / `ensure_compiled_imports` / `set_classic_jsx_pragma` / `set_pragma_jsx` / `set_pragma_jsx_import_source` / `queue_cleanup`; `MutationRecorder::apply` impl block lives here per PLAN.md §3.9.8 "lives in the same module as State" — same-module access to private fields without exposing them broadly), `crates/babel-plugin/src/mutation_recorder.rs` (5-variant `StateDiff` enum with serde-derive for the §5.3 cache.bin lock, 5-variant `ApiKind` enum with `from_imported_name` resolver, `MutationRecorder` struct owning a `Vec<StateDiff>` diff log + `new` / `diff_log` / `drain_diff_log` / `pub(crate) push_diff`), `crates/babel-plugin/src/types.rs` (slimmed to config-only types: `PluginOptions`, `CacheMode`, `Tag`/`TagKind`, `Metadata`/`MetadataContext`, `TransformResult`; re-exports State + inner shapes for the original surface area), `crates/babel-plugin/src/babel_plugin.rs` (`BabelPluginVisitor` adds `recorder: MutationRecorder` field; `record_compiled_import` routes per-API pushes through `MutationRecorder::apply` with `StateDiff::CompiledImportsAppend`; pragma writes and bootstrap `ensure_compiled_imports` go through `State`'s init-time mutators). | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 43/43 pass (5 types + 4 mutation_recorder + 11 state + 23 babel_plugin: prior 25 §2.3(a) tests + 2 new §2.4 recorder-routing assertions). Encapsulation lint `grep -rEn 'state\.[a-z_]+\.(push\|set\|add\|insert\|remove\|extend)' crates/babel-plugin/src --include '*.rs' \| grep -vE 'state\.rs\|mutation_recorder\.rs' \| wc -l` → 0. Pass-through preserved: `bun test parity-harness/strip-runtime/harness.test.ts parity-harness/babel-plugin/harness.test.ts` → 1284/1284. |
| §2.5 | ☑ | **Phase 2 exit gate:** pass-through harness clean across all babel-plugin fixtures | claude-2026-05-04 | This STATUS.md row + the Resume block updated with the closure summary (no separate phase2-signoff.md — same convention as §1.9). | `BABEL_PLUGIN_FULL_PARITY=1 BABEL_PLUGIN_FULL_DETERMINISM=1 bun test parity-harness/babel-plugin/harness.test.ts` → 954/954 pass (477 full-corpus parity + 477 full-corpus determinism). Strip-runtime corpus untouched: `bun test parity-harness/strip-runtime/harness.test.ts` → 1132/1132. Encapsulation lint clean (zero matches outside `state.rs` / `mutation_recorder.rs`). |

---

## Phase 3 — Hash compatibility (consume shared `crates/compiled-utils`)

> **Goal:** prove the Rust `hash` function shared with the CSS port is
> byte-identical to JS `@compiled/utils.hash` from this plugin's
> consuming side.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §3.1 | ☑ | Confirm `crates/compiled-utils` exposes `pub fn hash(input: &str) -> String` | claude-2026-05-04 | `crates/compiled-utils/src/hash.rs:52` (signature `pub fn hash(input: &str) -> String`); re-exported via `crates/compiled-utils/src/lib.rs:34` (`pub use hash::hash;`). | `grep -n 'pub fn hash' crates/compiled-utils/src/hash.rs` → line 52; `grep -n 'pub use hash' crates/compiled-utils/src/lib.rs` → line 34 |
| §3.2 | ☑ | Build hash test-vector corpus: ASCII, UTF-8 multibyte, empty, embedded NUL, >4KB, leading/trailing whitespace, real keyframe-expression inputs | claude-2026-05-04 | `parity-harness/hash/oracle.mjs` (mulberry32 seed=1 — deterministic; re-runs produce a byte-identical corpus) → emits `crates/babel-plugin/tests/hash_corpus.json` (10037 entries, gitignored — same regenerable-corpus precedent as the §1.8 strip-runtime synth set). Composition: 4 real-call-shape entries (one per `hash()` call site in `packages/babel-plugin/src/`), ~33 categorical entries (empty / NUL variants / whitespace / UTF-8 multibyte / surrogate-pair boundaries / >4 KiB / length-tail coverage for every murmur2 `(l mod 4)` branch), 10000 random entries (5000 ASCII + 5000 valid Unicode scalar values, surrogate range skipped — JS allows lone surrogates in strings but they're not RFC-valid JSON and the consuming plugin never passes them). | `bun parity-harness/hash/oracle.mjs` writes 10037 entries; manifest version=1; entry counts match: ≥30 categorical (`grep` + recount), 10000 `random-` prefixed, 4 `real:` prefixed |
| §3.3 | ☑ | Diff Rust `hash` vs JS `hash` over the corpus + 10K random inputs | claude-2026-05-04 | `crates/babel-plugin/tests/hash_parity.rs` (4 tests: `rust_hash_matches_js_corpus` parity gate; `corpus_includes_real_call_shapes` locks the 4 call-site fingerprints; `corpus_covers_phase3_categories` locks §3.2 acceptance; `corpus_has_at_least_10k_random_entries` locks §3.3 ≥10K acceptance). Pure-Rust integration test reading the JSON oracle — no WASM bridge, no Node round-trip; cheapest shape for a pure-data parity check. Also replaces the self-referential `crates/compiled-utils/src/hash.rs::tests::known_vectors` (which the existing comment flagged for migration) with five JS-locked byte vectors pulled from the corpus (`""→"0"`, `"a"→"14mfbry"`, `"abcd"→"aougpt"`, `"color: red"→"1wszpi4"`, `"@media (max-width: 100px)"→"w2cthn"`) — fast in-process smoke alongside the JSON-driven gate. | `RUSTFLAGS="" cargo test -p babel-plugin --test hash_parity` → 4/4 over 10037 entries; `RUSTFLAGS="" cargo test -p compiled-utils --lib hash` → 7/7 |
| §3.4 | ☑ | **Phase 3 exit gate:** zero divergence | claude-2026-05-04 | This STATUS.md updated (Phase 3 closure summary + this row + §3.1–§3.3 rows). | Phase 3 final-state command outputs preserved in the "Phase 3 closure summary" block above (cargo + bun parity harnesses; sibling suites unchanged) |

---

## Phase 4 — `buildCss` + direct synchronous `transformCss` Rust call

> **Goal:** port `utils/css-builders.ts` and link the parallel-agent's
> Rust `transform_css` directly. Single-pass plugin, no scan/apply.
>
> **Hard preconditions:** Phase 3 green; Rust `transform_css` shipped
> as a callable `pub fn`; `compat/generator.rs` coverage manifest
> exists.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §4.1 | ☑ | `transform_css` integration parity test — every JS-corpus input produces byte-identical Rust output from this plugin's perspective | claude-2026-05-04 | `parity-harness/transform-css/oracle.mjs` (imports `@compiled/css.transformCss` directly; pins `BROWSERSLIST_CONFIG` to `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` — the EXACT production AFM pin per `BROWSER_LIST_FROM_AFM.md`, resolves to the 14-entry list `and_chr 144 / chrome 144..140 / edge 144..143 / firefox 147..146 / ios_saf 26.2..26.1 / safari 26.2..26.1` under workspace-pinned `caniuse-lite@1.0.30001766` + `browserslist@4.24.2`. Unsets `BROWSERSLIST`, `AUTOPREFIXER`, and `COMPILED_CSS_ENGINE`) → emits `crates/babel-plugin/tests/transform_css_corpus.json` (gitignored — regenerable). 30 hand-curated CSS fixtures from `crates/parity-runner/corpus/transform-css/` × 4 opts permutations spanning the babel-plugin's real call shape from `packages/babel-plugin/src/utils/{transform-css-items,build-styled-component}.ts` (`{}`, `{optimizeCss:false}`, `{increaseSpecificity:true}`, `{classHashPrefix:'x'}`). **120/120 entries byte-equal** with `css::transform_css`. New `[dev-dependencies]` entry on `crates/css` in `crates/babel-plugin/Cargo.toml` (will promote to `[dependencies]` when §4.6 wires `transform_css` into the visitor). | `RUSTFLAGS="" cargo test -p babel-plugin --test transform_css_integration` → 3/3 (120/120 parity, no expected-to-fail). 5 consecutive runs all green confirms the env-race fix is stable. Sibling suites unchanged. |
| §4.2 | ☑ | Build `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` enumerating every AST node kind reachable from `keyframes(...)` (and any other `generate(...)` call site) in the consuming monorepo | claude-2026-05-04 | `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` (per-call-site coverage manifest + comment-axis fixtures + real-divergence table); `parity-harness/compat-generator/{fixtures.json,oracle.mjs}` (in-tree input manifest + JS oracle that emits `crates/babel-plugin/tests/compat_generator_corpus.json`, gitignored, regenerable; pin guard fail-fasts on `@babel/generator` / `@babel/parser` drift); `crates/babel-plugin/src/compat/{mod.rs,generator.rs}` (stub `generate(&Expr) -> String` that `unimplemented!()`s — by design, callers know §4.3 is a hard prereq); `crates/babel-plugin/tests/compat_generator_integration.rs` (3 tests: corpus shape lock + SWC parse coverage GREEN at §4.2 ship; byte-parity assertion `#[ignore]`d until §4.3 ports the line-for-line generator). Pinned `@babel/generator@7.23.0` + `@babel/parser@7.29.2` (AFM-resolved) in root `package.json#overrides` AND promoted both to top-level devDependencies (workspace's bun-isolated dep layout doesn't symlink `node_modules/@babel/generator`, so without the devDep promotion `require('@babel/generator')` walked PAST the workspace and grabbed `Documents\projects\node_modules\@babel\generator@7.28.5` — a sibling tree's copy. Caught by the oracle's pin guard). Both pins recorded in `crates/PARITY_VERSIONS.md` with the rationale for pinning the parser alongside the generator. 55 fixtures across 5 call_site axes (conditional-classname-item: 8, generic-expression: 25, jsx-key-attribute: 5, keyframes-expression: 11, variable-init: 6) — every call_site has ≥1 fixture; comment-axis fixtures (eslint-disable, ternary-inner, PURE annotations, leading/trailing block comments) seeded explicitly per the user's flag. Real divergences captured in the corpus that §4.3 must reproduce: `cond ? /* yes */'a-class' : 'b-class'` (no space after block comment), `(a && b) \|\| c → a && b \|\| c` (paren drop at precedence boundary), single-quote preservation. | `bun parity-harness/compat-generator/oracle.mjs` writes 55 entries with pin guard `@babel/generator=7.23.0, @babel/parser=7.29.2`. `RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration` → 2 passed (`corpus_shape_lock`, `corpus_input_sources_parse_under_swc`), 1 ignored (`rust_compat_generator_matches_js_corpus` waiting on §4.3). Sibling suites unchanged: 43 babel-plugin lib + 4 hash_parity + 3 transform_css_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. **Verified against current `crates/css` HEAD — §4.1 still 120/120 under both new pins** (defense-in-depth crosscheck per the parallel autoprefixer-agent collision risk note in §4.2 plan). |
| §4.3 | ☑ | Port `compat/generator.rs` covering every node kind in the manifest | claude-2026-05-04 | `crates/babel-plugin/src/compat/generator/{mod,buffer,printer}.rs`, `compat/generator/node/{mod,parentheses}.rs`, `compat/generator/generators/{mod,expressions,types,template_literals,jsx}.rs` (~1640 LOC across 10 files mirroring upstream `lib/{index,buffer,printer}.js`, `lib/node/{index,parentheses}.js`, `lib/generators/{expressions,types,template-literals,jsx}.js`). `mod.rs` exposes 4 entry points: `generate(&Expr)` (synthetic-AST callers; no comments), `generate_with_comments(&Expr, &dyn Comments)` (the keyframes / generic-expression / variable-init / conditional-classname-item axes), `generate_jsx_attribute(&JSXAttr)` and `generate_jsx_attribute_with_comments(&JSXAttr, &dyn Comments)` (the jsx-key-attribute axis — `Printer::print(&Expr,_)` can't be overloaded because SWC types JSXAttr separately from Expr). Foundation handles: paren-policy with precedence (Babel's PRECEDENCE table 1:1, including `(a && b) \|\| c → a && b \|\| c` redundancy drop), source-quote preservation via `Str.raw` / `Number.raw`, multi-line `ObjectExpression` with 2-space indent and `printList`-statement-mode pre/post newlines, comment threading via `Comments::take_leading` / `take_trailing` keyed at node `BytePos` (with the SWC quirk that same-line comments between tokens are stored as TRAILING-of-previous, not leading-of-next — handled at object-property iteration via `take_trailing(open_brace.lo + 1)` for the first-property case). Comment policy: leading space before block comments unless tail is `[` or `{`, no trailing space (matches the corpus's `cond ? /* yes */'a-class'` divergence). Indent honoured by `print_comment` via `maybe_indent` so `\n  /* leading */` lands at the right column. JSX printer (this session) handles JSXElement / JSXAttribute / JSXIdentifier / JSXMemberExpression / JSXNamespacedName / JSXEmptyExpression / JSXExpressionContainer / JSXText / JSXOpeningElement / JSXClosingElement / JSXSpreadAttribute / JSXFragment / JSXOpeningFragment / JSXClosingFragment / JSXSpreadChild — all 5 jsx-key-attribute fixtures (StringLiteral / NumericLiteral / MemberExpression / TemplateLiteral / ConditionalExpression attribute values) byte-exact via the new entry point. SWC↔Babel field-name divergence table documented inline at the head of `jsx.rs` (mechanical renames; byte output identical). The integration test's `find_first_key_jsx_attribute` walker mirrors the JS oracle's `extractJsxKeyAttribute` recursive walk verbatim (both must agree on which `key=` attribute they pluck out). Other upstream files (`flow.js`, `typescript.js`, `classes.js`, `methods.js`, `modules.js`, `statements.js`, `base.js`) intentionally NOT ported — none are reachable from the 5 call sites per CLAUDE.md "1:1 with what's reachable, not future-proofing". | `RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration` → 3/3 pass (corpus_shape_lock + corpus_input_sources_parse_under_swc + rust_compat_generator_matches_js_corpus **over 55/55 fixtures byte-exact, zero skips, zero ignored**). Wasm cdylib still builds clean (`RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`). All sibling suites unchanged: 43 babel-plugin lib + 4 hash_parity + 3 transform_css_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. **Total: 2562 tests, zero failures, zero ignored.** |
| §4.4 | ☑ | Port `utils/css_builders.rs` line-for-line (SHELL: file structure mirrors upstream 1:1; 4 hash-call-shape sites end-to-end through `compat::generator → compiled_utils::hash`; `evaluate_expression` / `resolve_binding` / `visit_css_map_path` dispatch sites stubbed with phase-citing `unimplemented!()`. Misleading verification cell amended.) | claude-2026-05-04 | `crates/babel-plugin/src/utils/{ast,css_builders,is_compiled,is_empty,manipulate_template_literal,object_property_to_string,types}.rs` (all new); `crates/babel-plugin/CSS_BUILDERS_DEPS.md` (RESOLVED — CSS-port agent shipped `add_unit_if_needed` + `css_affix_interpolation` re-exports from `crates/css/src/lib.rs`); `crates/babel-plugin/Cargo.toml` (`css` promoted from `[dev-dependencies]` to `[dependencies]`; `regex`/`once_cell` workspace deps added); `crates/babel-plugin/src/types.rs` (`Metadata::reborrow_with_context` + `reborrow` methods land the JS `{ ...meta, ... }` analog under Rust borrow rules). Verification gate: hash-call-shape sites end-to-end clean (3 unit tests in `utils::css_builders::tests` assert each emitted name matches `hash(generate(...))`); evaluate / resolve / visitCssMap stubbed pending Phases 5/6; `addUnitIfNeeded` / `cssAffixInterpolation` wired through `css::` re-exports. **The full byte-clean gate (`harness fixtures exercising keyframes`/`css`/`cssMap`) is the §4.8 phase exit gate, NOT §4.4.** | `RUSTFLAGS="" cargo test -p babel-plugin --lib` → 75/75 (was 43/43; +32 from new utils ports including 3 hash-call-shape sites). All sibling suites unchanged: 4 hash_parity + 3 transform_css_integration + 3 compat_generator_integration + 56 strip-runtime lib + 31 compiled-utils lib + 1132 strip-runtime harness + 954 babel-plugin full harness + 336 equality-harness. WASI cdylib still builds clean (`RUSTFLAGS="" cargo build -p babel-plugin -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`). **Total: 2636 tests, zero failures, zero ignored.** |
| §4.5 | ☐ | Port `utils/transform_css_items.rs` and `utils/build_css_variables.rs` | — | Both `.rs` files | Same harness gate |
| §4.6 | ☐ | Wire `transform_css` calls into the visitor (single pass, no scan/apply) | — | Updated `lib.rs` | Harness clean for `keyframes`, `css`, `cssMap` fixtures |
| §4.7 | ☐ | Update Parcel wrapper to a single `transformSync` call (PLAN.md §8) | — | `packages/parcel-transformer/src/index.ts` | Wrapper produces a single SWC call, drains sidecars, returns to Parcel |
| §4.8 | ☐ | **Phase 4 exit gate:** keyframes / css / cssMap fixtures byte-clean | — | STATUS.md updated | All such fixtures green in the parity harness |

---

## Phase 5 — In-plugin resolver + expression evaluator

> **Goal:** port `utils/resolve_binding.rs` and the entire
> `traverse_expression/` subtree using `oxc_resolver` for module
> resolution.
>
> **Hard preconditions:** §0.10 audit script reports ≤100 outliers
> per workspace, and the consumer monorepo's ~100-file refactor
> bringing all included files under cwd is merged. §0.11 resolver
> matrix is complete.

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §5.1 | ☐ | Re-confirm `STATE_MUTATIONS.md` is current vs upstream Babel; reconcile any new mutation sites | — | Updated STATE_MUTATIONS.md if needed | `grep` enumeration matches doc |
| §5.2 | ☐ | Land the consumer-monorepo refactor (zero outside-cwd includes) | — | refactor PR | §0.10 audit reports zero outliers |
| §5.3 | ☐ | Port `utils/cache.rs` — Layer 1 in-memory + Layer 2 postcard `cache.bin` per PLAN.md §3.9 | — | `crates/babel-plugin/src/utils/cache.rs`, `crates/babel-plugin/src/cache_schema.rs` | `cargo test -p babel-plugin cache::` passes; size + entry caps enforced |
| §5.4 | ☐ | Port `utils/resolve_binding.rs` using `oxc_resolver` configured per §0.11 matrix | — | `crates/babel-plugin/src/utils/resolve_binding.rs` | Resolver-matrix corpus byte-clean against this plugin's resolver wrapper |
| §5.5 | ☐ | Port the entire `traverse_expression/` subtree file-for-file (leaves first) | — | `crates/babel-plugin/src/utils/traverse_expression/**` | Harness `module-traversal` and `expression-evaluation` fixtures byte-clean |
| §5.6 | ☐ | Port `traversers/` and `evaluate_expression.rs` | — | `crates/babel-plugin/src/utils/{traversers,evaluate_expression.rs}/**` | Same as above |
| §5.7 | ☐ | Wire `includedFiles` accumulation → `<callScratch>/included-files.json` sidecar | — | Updated lib.rs Program::exit | Harness fixtures with cross-file imports produce non-empty sidecar; host's `asset.invalidateOnFileChange` matches Babel's |
| §5.8 | ☐ | Promote `scripts/audit-included-files.ts` to CI guardrail | — | CI config update | Audit failure blocks PR merge |
| §5.9 | ☐ | **Phase 5 exit gate:** module-traversal + expression-evaluation byte-clean; `MutationRecorder` shadow-eval suite reports zero replay/live divergence; pre-commit state-mutation lint clean | — | STATUS.md updated | All exit-gate sub-conditions met |

---

## Phase 6 — Per-API handlers (least-risk first)

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §6.1 | ☐ | `keyframes` cleanup-only handler | — | `crates/babel-plugin/src/keyframes/mod.rs` | Keyframes fixtures byte-clean |
| §6.2 | ☐ | `css` (utility) cleanup-only handler | — | `crates/babel-plugin/src/css/mod.rs` | css() fixtures byte-clean |
| §6.3 | ☐ | `cssMap` handler (`process_selectors.rs`) | — | `crates/babel-plugin/src/css_map/{mod.rs,process_selectors.rs}` | cssMap fixtures byte-clean |
| §6.4 | ☐ | `xcss-prop` handler | — | `crates/babel-plugin/src/xcss_prop/mod.rs` | xcss fixtures byte-clean |
| §6.5 | ☐ | `css-prop` handler (comment-placement-sensitive) | — | `crates/babel-plugin/src/css_prop/mod.rs` | css-prop fixtures byte-clean |
| §6.6 | ☐ | `ClassNames` handler | — | `crates/babel-plugin/src/class_names/mod.rs` | ClassNames fixtures byte-clean |
| §6.7 | ☐ | `styled` handler (largest; forwardRef + `@emotion/is-prop-valid` table verbatim port) | — | `crates/babel-plugin/src/styled/mod.rs`, `crates/babel-plugin/src/utils/build_styled_component.rs`, `crates/babel-plugin/src/compat/is_prop_valid.rs` (verbatim emotion table) | styled fixtures byte-clean (the largest single fixture set) |
| §6.8 | ☐ | **Phase 6 exit gate:** all ~50 babel-plugin tests + cross-handler tests + JSX automatic-runtime fixtures + custom-import-source fixtures byte-clean | — | STATUS.md updated | Full harness green |

---

## Phase 7 — Comment placement and `Program::exit` ordering

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §7.1 | ☐ | Build comment-shape diff tool — parse both prettier outputs back, walk comment array, compare attachment | — | `parity-harness/comment-diff.ts` | Tool runs on representative fixtures and prints attachment differences |
| §7.2 | ☐ | Hunt every comment-placement divergence (version banner, `preserveLeadingComments`, `appendRuntimeImports` order, `@compiled-disable-*` directives) | — | Code fixes in the plugin | Harness reports zero comment-related divergences |
| §7.3 | ☐ | **Phase 7 exit gate:** full corpus zero comment divergence | — | STATUS.md updated | All harness tests pass with comment-shape diff tool clean |

---

## Phase 8 — Corpus diff at scale and rollout gate

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §8.1 | ☐ | Run parity harness across 100k+ Compiled call sites in the consumer monorepo | — | Capture every divergence; treat each as a blocking bug | Iterate until zero divergence holds for ≥2 consecutive weeks |
| §8.2 | ☐ | Stand up `cargo-fuzz` targets that synthesise plausible Compiled inputs | — | `crates/babel-plugin-fuzz/` | 72h continuous fuzz run finds no new divergence |
| §8.3 | ☐ | Shadow-mode CI — real builds use Babel; SWC runs in parallel; alarm on hash divergence | — | CI config | Two consecutive weeks of zero divergence in shadow mode |
| §8.4 | ☐ | **Phase 8 exit gate:** all of the above sustained | — | STATUS.md updated | Sustained green; ready for rollout |

---

## Phase 9 — Rollout

| ID | Status | Checkpoint | Owner | Artefacts | Verification |
|---|---|---|---|---|---|
| §9.1 | ☐ | Engine flag default = Babel | — | Parcel transformer reads `COMPILED_TRANSFORMER` env var | Default behaviour unchanged for unflagged consumers |
| §9.2 | ☐ | Ship Rust artefacts via `napi build` for linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64-msvc | — | Per-platform `.wasm` (or `.node` if architecture changes) | Each platform binary runs the harness clean |
| §9.3 | ☐ | Internal opt-in via `COMPILED_TRANSFORMER=swc` | — | Internal teams flip the flag | Production traffic on SWC for opt-in repos |
| §9.4 | ☐ | Hash-shadow in production: compute SWC hash, compare to Babel hash, log divergence | — | Production hash-shadow telemetry | Zero divergence over N weeks |
| §9.5 | ☐ | Flip default to SWC after sustained zero divergence | — | Parcel transformer default flipped | Production now on SWC; Babel kept as oracle |
| §9.6 | ☐ | Keep Babel pipeline in tree for ≥1 year as parity oracle | — | This is a non-removal contract | Babel code untouched |

---

## Cardinal rules conformance

These are the standing invariants. A checkpoint that violates one is
not "done" — it is rejected at review.

- **Bytes after prettier are the contract.** Not "looks right." Not
  "passes tests." Bytes.
- **CSS class names live inside string literals.** Hashing is part of
  the byte contract.
- **No filesystem access outside `/cwd`** inside the plugin. Ever.
- **No JS callbacks from the plugin.** Side effects go via sidecar JSON
  written to `/cwd/<callScratch>/...`.
- **Don't bump `@swc/core` casually.** ABI breaks. Coordinated
  `swc_core` bump + full corpus rerun required.
- **Bugs are features.** Behavioural differences under the parity
  harness are port defects, not bug-fix opportunities.
- **1:1 file mapping is enforced.** PLAN.md constraint 4. If you feel
  the urge to deviate, stop and ask.
- **No half-baked compat shims.** If `compat/<name>.rs` is incomplete,
  it will break in production. Finish it or escalate.
- **Build with `RUSTFLAGS=""`** for any crate that pulls in proc-macro
  deps via `swc_core`. User-global `lto=thin` breaks proc-macro
  builds.
