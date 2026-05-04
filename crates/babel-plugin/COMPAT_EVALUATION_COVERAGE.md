# `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`

> Phase 5 §5.0c entry-gate manifest. Survey of which `@babel/traverse`
> `path.evaluate()` branches the Compiled call graph can actually
> reach. Used to:
>
> 1. Cite from `unimplemented!("…")` panic messages on
>    evidenced-unreachable branches in `crates/babel-plugin/src/compat/evaluation.rs`.
> 2. Drive the `parity-harness/compat-evaluation/fixtures.json`
>    corpus shape so the gate exercises every reachable branch.
>
> Same role `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` plays
> for §4.3. The methodology is the same: grep the in-tree corpus +
> read the upstream call graph; emit a manifest before any port
> code lands.

## Methodology

The §5.4–§5.6 evaluator reaches `path.evaluate()` at exactly one
site: `packages/babel-plugin/src/utils/evaluate-expression.ts:93`.
The `node` it passes is whatever flows in from the visitor —
typically a CSS-value expression inside a `css({...})` /
`styled.div\`…\`` / `cssMap({...})` / `<ClassNames>` / xcss site.

Coverage is therefore "what AST shapes can the Compiled visitor
hand to `evaluate-expression.ts:82`'s `babelEvaluateExpression`?"
The answer is bounded by:

1. The 477 fixtures already extracted under
   `parity-harness/babel-plugin/fixtures/` — every `@compiled/babel-plugin`
   test that ever shipped, snapshotted as `(input, opts) → output`.
2. The Compiled-handler entry points (`css-prop`, `class-names`,
   `styled`, `xcss-prop`, `cssMap`, `keyframes`) — these are the
   ONLY paths into the evaluator. Anything not reachable from a
   handler is not reachable from `path.evaluate()`.

## Survey results — 2026-05-04

Grepped the 477-fixture corpus and the upstream source for each of
the candidate branches.

### Confirmed unreachable (panic-with-citation permitted)

| Branch | Evidence | Panic citation target |
|---|---|---|
| **Flow type-cast (`TypeCastExpression`, `TypeAlias`, `TypeParameter*`, `OpaqueType`, etc.)** | Single grep hit across 477 fixtures: `0450-…-preserve-comments-…-runti.json` carries `// @flow strict-local` as a *comment header*. No fixture parses Flow as syntax. The `parserBabelPlugins` default in `@compiled/utils/DEFAULT_PARSER_BABEL_PLUGINS` does not include `'flow'` — Flow type nodes never enter the AST. | `unimplemented!("compat::evaluation: Flow type-cast unreachable from Compiled — parser config does not enable @babel/plugin-syntax-flow; see crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md §Flow")`. |
| **JSX-as-evaluable (`JSXElement`, `JSXFragment`, `JSXText` evaluation)** | Babel's evaluator folds JSX text/elements to their string forms in some branches. The Compiled evaluator never reaches a JSX node via `path.evaluate()` — JSX is consumed by per-handler dispatch (`css-prop` / `ClassNames` / xcss); CSS-value expressions inside `css({...})` are arbitrary `Expr` but never `JSXElement` (would be a syntax error in object property position). Zero hits across the corpus. | `unimplemented!("compat::evaluation: JSX evaluation unreachable from Compiled — JSX flows through per-handler dispatch, not the constant-folder; see COMPAT_EVALUATION_COVERAGE.md §JSX")`. |
| **`SequenceExpression` (comma operator)** | Zero hits across 477 fixtures for `"type":"SequenceExpression"` and zero source-text hits for the bare `(a, b)` shape in CSS-value position. Compiled CSS values land in object-property values / template quasis / call args — none of which are SequenceExpr-reachable in real production code. Compiled never wraps user code in a `(a, b, c)` rewrite. | `unimplemented!("compat::evaluation: SequenceExpression unreachable from Compiled — comma operator never appears in CSS-value position across 477-fixture corpus; see COMPAT_EVALUATION_COVERAGE.md §SequenceExpression")`. User-confirmed acceptable 2026-05-04 (extreme edge case: any actual production reach would surface in Phase 8 corpus diff). |
| **`TaggedTemplateExpression` evaluation** | Tagged templates that ARE Compiled (`keyframes\`...\``) are short-circuited at `evaluate-expression.ts:184` before reaching `path.evaluate()`. Tagged templates that are NOT Compiled (e.g. user `gql\`...\``) would be returned as-is by the evaluator's identifier-resolution layer (the binding's `path.node` is the `TaggedTemplateExpression` itself, not foldable). Babel's evaluator's `TaggedTemplateExpression` branch is reachable only via the literal `String.raw\`...\`` builtin, which Compiled does not consume. | `unimplemented!("compat::evaluation: TaggedTemplateExpression evaluation unreachable from Compiled — Compiled tagged templates short-circuit at evaluate-expression.ts:184; user tagged templates are returned as fallback; see COMPAT_EVALUATION_COVERAGE.md §TaggedTemplate")`. |

Each panic message MUST cite this file by exact section name. If a
future fixture ever surfaces one of these branches, the panic fires
with a clear breadcrumb pointing back to the survey row that ruled
it unreachable — so the survey can be updated and the branch
ported, not silently fall through to wrong output.

### Reachable branches (full port required)

The remaining `@babel/traverse/lib/path/evaluation.js` branches
must be ported line-by-line per the Q3 lock in `COMPAT_SCOPE_AUDIT.md`.

The expected reachable surface (drawn from
`expression-evaluation.test.ts` + `module-traversal.test.ts` +
ad-hoc grep of CSS-value shapes across the 477 fixtures):

- **Literals**: `StringLiteral`, `NumericLiteral`, `BooleanLiteral`,
  `NullLiteral`, `BigIntLiteral` (NB: BigInt is rare but legal in
  numeric CSS values like `calc(0n + 1px)` — port even if
  unobserved; the `evaluation.js` source covers it).
- **`Identifier`** — resolves through scope binding; `undefined` /
  `Infinity` / `NaN` are special-cased to their global values per
  `evaluation.js`.
- **`UnaryExpression`** — `-`, `+`, `!`, `void`, `typeof`, `~`.
- **`BinaryExpression`** — all arithmetic, all comparison, all
  bitwise, `instanceof`, `in`, string concatenation.
- **`LogicalExpression`** — `&&`, `||`, `??`.
- **`ConditionalExpression`** — ternary fold when test is confident.
- **`MemberExpression`** — non-computed property access on
  `ObjectExpression` literals; computed access only when the key
  is a confident literal.
- **`ArrayExpression`** — fold each element; emit `[…]` value if
  every element is confident.
- **`ObjectExpression`** — fold each prop value; emit `{…}` value if
  every prop is confident and every key is a confident literal.
- **`TemplateLiteral`** — fold quasis + expressions; emit a
  concatenated string if every expression is confident.
- **`TypeCastExpression`** — TS-only (`@babel/plugin-syntax-typescript`):
  `expr as Type` and `<Type>expr` are folded by passing through to the
  inner expression. Compiled's parser config DOES enable TS, so this is
  reachable. (Distinguish from Flow TypeCastExpression — different node
  type despite the name; Babel's TS plugin uses `TSAsExpression` /
  `TSTypeAssertion`.)
- **`ParenthesizedExpression`** — pass-through.

Per the Q3 concession, every line of `evaluation.js` covering
these branches gets ported. Branches outside this list go to
`unimplemented!` with a citation back to this file.

### Resolver-driven evaluation (cross-file imports)

Compiled's evaluator additionally walks across module boundaries
via `resolve-binding.ts`. That path is NOT `path.evaluate()` —
it's `evaluate-expression.ts`'s identifier-traversal layer feeding
nodes parsed from a different file back into the same evaluator
recursively. Cross-file evaluation reuses the same `compat::evaluation` reachable-branch list above.

The `compat::evaluation` port does NOT need to know about the
file boundary — it just folds whatever expression it's given. The
cross-file glue is in `utils/resolve_binding.rs` (§5.4) and
`utils/evaluate_expression.rs` (§5.6).

## Maintenance

When a future Phase 5/6 port lands a fixture that fires one of the
four unreachable-branch panics:

1. Add the fixture's source under `parity-harness/compat-evaluation/fixtures.json`.
2. Move the row from "Confirmed unreachable" to "Reachable
   branches" with a note: "Surfaced by fixture <name>; ported in
   §<NNN>".
3. Land the Rust port for that branch.
4. Re-run the parity gate.

Per the §5.4–§5.6 owner's directive (2026-05-04): **defer-by-hope
is not acceptable; defer-by-evidence is**. This file is the
evidence ledger.
