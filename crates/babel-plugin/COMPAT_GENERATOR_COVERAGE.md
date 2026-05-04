# `compat::generator` coverage manifest (Phase 4 §4.2)

> **Purpose.** Enumerate every AST shape that
> `crates/babel-plugin/src/compat/generator.rs` (Phase 4 §4.3) must
> produce byte-for-byte against `@babel/generator@7.23.0`. Each row
> below corresponds to one or more fixtures in
> `parity-harness/compat-generator/fixtures.json`. A divergence on
> any of them at §4.3 ship time silently renames classes in
> production — the parity gate at
> `crates/babel-plugin/tests/compat_generator_integration.rs` is
> what catches that before we ship.

## Why this manifest exists

`packages/babel-plugin/src/utils/` calls `@babel/generator` from
**five distinct sites**. Three of them feed `compiled-utils::hash`
with NO prettier downstream — byte-exact match is mandatory.
Two of them emit into source that prettier round-trips, but we
lock byte exactness identically (drift today = drift tomorrow,
per the §4.2 hand-off).

`swc_ecma_codegen` is **not** byte-equivalent to
`@babel/generator@7.23.0`. Confirmed divergence axes (per the §4.2
hand-off + spot-checks on the generated corpus):

- Whitespace around binary / logical operators (`a+b` vs `a + b`).
- Paren policy at precedence boundaries (`(a && b) || c` vs
  `a && b || c` — Babel drops redundant parens; SWC keeps them).
- Default quote style (heuristic vs `"double"` enforced).
- Trailing-comma policy in arrays / object literals.
- Property-shorthand collapsing (`{ a: a }` → `{ a }`).
- Semicolon-after-class-body and `do {} while ()`.
- **Comment attachment** — Babel preserves attached positions for
  inline `/* ... */` blocks, ESLint directives, JSDoc, and PURE
  annotations; SWC stores them in separate leading/trailing slots
  via `Comments` and emits them with default whitespace policy.
  Confirmed by the corpus: Babel emits `/* yes */'a-class'`
  (no space after the comment); SWC emits `/* yes */ 'a-class'`.

Every site in the upstream code that calls `generate(...)` must be
reachable from at least one fixture; every divergence axis above
must be exercised by at least one fixture. This manifest is the
ledger.

## The five call sites (1:1 with `packages/babel-plugin/src/utils/`)

| `call_site` axis | Upstream call site | Hashing? | Notes |
|---|---|---|---|
| `keyframes-expression` | `css-builders.ts:464` — `hash(generate(expression).code)` for `keyframes({...}) \| keyframes\`...\`` | **YES** (strict-byte) | The hardest bar. No prettier downstream; bytes feed directly into class-name hash. |
| `generic-expression` | `css-builders.ts:280` — `let variableName = generate(node).code` | **YES** (strict-byte, indirect) | `variableName` is hashed at `:639` / `:869`. Same byte bar as keyframes. |
| `variable-init` | `css-builders.ts:298` — `variableName = init ? generate(init).code : node.name` | **YES** (strict-byte, indirect) | Same chain as `:280` — `variableName → hash(variableName)` at `:639`. |
| `jsx-key-attribute` | `build-compiled-component.ts:30` — `<CC ${keyAttribute ? generate(keyAttribute).code : ''}>` | no | Output is JSX-template-interpolated; SWC parses the result and prettier round-trips before any byte assertion. Locked byte-exact defensively. |
| `conditional-classname-item` | `build-styled-component.ts:133` — `conditionalClassNames += \`${generate(item).code}, \`` | no | Output is concat'd into a runtime args list; same prettier round-trip as above. Locked byte-exact defensively. |

## Coverage axes per call site

The fixtures in `parity-harness/compat-generator/fixtures.json` are
organised so every (call_site × shape) cell is covered. "Shape"
includes both the AST node kind and the comment-shape axis the
user flagged (eslint-disable, ternary-inner, pure annotations).

### `keyframes-expression` (11 fixtures)

| Shape | Why it matters | Fixture label(s) |
|---|---|---|
| `ObjectExpression { from, to }` — basic | The single most common shape in real consumer code. | `keyframes-expr/object-from-to` |
| `ObjectExpression` with string property keys | Percentage keyframes (`'0%'`, `'50%'`, `'100%'`) require quoted keys; quote-style policy must match. | `keyframes-expr/object-string-keys` |
| `ObjectExpression` with comma keys (`'from, 25%'`) | Stress-tests Babel's string-key passthrough. | `keyframes-expr/object-comma-key` |
| `TaggedTemplateExpression` (no interpolation) | The CallExpression \| TaggedTemplateExpression dual of `extractKeyframes`. | `keyframes-expr/template-literal-tagged` |
| `TaggedTemplateExpression` with interpolations | Identifier-vs-MemberExpression placement inside `${...}`. | `keyframes-expr/template-with-interpolation` |
| Deeply nested object | Nested ObjectExpression formatting. | `keyframes-expr/nested-object-deep` |
| `ConditionalExpression` inside property value | Ternary-in-context — comment & paren policy. | `keyframes-expr/conditional-value` |
| Leading comment on property | Comment-attachment axis. | `keyframes-expr/comment-leading-property` |
| Trailing comment on property | Comment-attachment axis (rare site, easy to drift). | `keyframes-expr/comment-trailing-property` |
| `eslint-disable-next-line` directive | Real consumer pattern; must survive byte-exact. | `keyframes-expr/comment-eslint-disable` |
| Comment inside ternary branch | The user's specific concern (`a /* x */ ? b : c`). | `keyframes-expr/comment-inside-ternary` |

### `generic-expression` (25 fixtures)

Highest-coverage axis because the upstream call signature is
`(node: t.Expression)` — anything reachable through normal
expression shapes can land here.

| Shape group | Fixtures |
|---|---|
| `Identifier`, `MemberExpression` (dot / chain / computed) | `generic-expr/identifier`, `generic-expr/member-expression`, `generic-expr/member-expression-deep`, `generic-expr/member-computed` |
| `CallExpression` (no args, with args) | `generic-expr/call-expression`, `generic-expr/call-with-args` |
| `ArrayMember` | `generic-expr/array-member` |
| Literals — string (single + double), numeric (int + decimal), bool, null | `generic-expr/string-literal-{double,single}`, `generic-expr/numeric-{literal,decimal}`, `generic-expr/boolean-literal`, `generic-expr/null-literal` |
| `TemplateLiteral` (static, with interpolation) | `generic-expr/template-literal-static`, `generic-expr/template-with-interpolation` |
| `BinaryExpression` (precedence, parens) | `generic-expr/binary-add`, `generic-expr/binary-precedence`, `generic-expr/parenthesized` |
| `ConditionalExpression` (simple, nested) | `generic-expr/conditional-simple`, `generic-expr/conditional-nested` |
| **Comment axis** | `generic-expr/comment-leading-identifier`, `generic-expr/comment-trailing-identifier`, `generic-expr/comment-inside-ternary`, `generic-expr/comment-eslint-disable-line`, `generic-expr/comment-pure-annotation` |

### `variable-init` (6 fixtures)

The parser output is the same as `generic-expression` (the upstream
site has already drilled into `VariableDeclarator.init` before
calling `generate`, so we feed bare init expressions here). The
axis label is preserved for failure-report grouping.

| Shape | Fixture |
|---|---|
| String literal | `variable-init/string-literal` |
| `MemberExpression` | `variable-init/member-expression` |
| `CallExpression` | `variable-init/call-expression` |
| `ObjectExpression` | `variable-init/object-literal` |
| `ConditionalExpression` | `variable-init/conditional` |
| `TemplateLiteral` with interpolation | `variable-init/template-with-interpolation` |

### `jsx-key-attribute` (5 fixtures)

The upstream call generates on a `JSXAttribute` node (extracted via
`getJSXAttribute(node, 'key')`), not the whole JSXElement. Each
fixture's `input_source` is the surrounding `<div key={...} />`
form; oracle + Rust gate both walk to the matching `JSXAttribute`
node and call `generate` on that. Phase 4 §4.3 must dispatch on
JSXAttribute as a separate case from `Expr`.

| Attribute value shape | Fixture |
|---|---|
| `StringLiteral` (`key="static"`) | `jsx-key/string-literal-attr` |
| `NumericLiteral` in expression container | `jsx-key/numeric-expr-attr` |
| `MemberExpression` in expression container | `jsx-key/member-expr-attr` |
| `TemplateLiteral` in expression container | `jsx-key/template-expr-attr` |
| `ConditionalExpression` in expression container | `jsx-key/conditional-expr-attr` |

### `conditional-classname-item` (8 fixtures)

The upstream filter is
`t.isLogicalExpression(item) || t.isConditionalExpression(item)`.

| Shape | Fixture |
|---|---|
| `LogicalExpression` (`&&`) | `conditional-classname/logical-and` |
| `LogicalExpression` (`\|\|`) | `conditional-classname/logical-or` |
| `LogicalExpression` (`??`) | `conditional-classname/nullish-coalescing` |
| `ConditionalExpression` (basic) | `conditional-classname/conditional-expr` |
| `ConditionalExpression` with null branch | `conditional-classname/conditional-with-null` |
| Nested logical (paren policy) | `conditional-classname/nested-logical` |
| Comment between ternary branches | `conditional-classname/comment-between-branches` |
| Leading `eslint-disable` directive | `conditional-classname/comment-eslint-disable` |

## Real divergences captured at §4.2 corpus generation

Spot-checked from the corpus (run `bun parity-harness/compat-generator/oracle.mjs`
and inspect `crates/babel-plugin/tests/compat_generator_corpus.json`):

| Input | Babel output | SWC default | What §4.3 must do |
|---|---|---|---|
| `cond ? /* yes */ 'a-class' : 'b-class'` | `cond ? /* yes */'a-class' : 'b-class'` | `cond ? /* yes */ 'a-class' : 'b-class'` (extra space) | Suppress whitespace between block-comment and following expression. |
| `(a && b) \|\| c` | `a && b \|\| c` | `(a && b) \|\| c` (paren retained) | Drop redundant parens at precedence boundary. |
| `'a-class'` (single-quote source) | `'a-class'` | `"a-class"` (re-quoted) | Preserve source quote style. |
| `/* eslint-disable-next-line */ cond ? 'a' : 'b'` | `/* eslint-disable-next-line */cond ? 'a' : 'b'` | leading whitespace on next line | Same as comment-attachment rule above. |

Every one of these is ALREADY present in the corpus's
`expected_code`, so when §4.3 lands its assertion will fire on any
shortcut implementation.

## Adding fixtures

1. Edit `parity-harness/compat-generator/fixtures.json` — add a
   `{ label, call_site, input_source }` row. `label` must be unique;
   `call_site` must be one of the five axes above.
2. Run `bun parity-harness/compat-generator/oracle.mjs` to
   regenerate the cargo-readable corpus. The pin guard in the
   oracle fail-fasts if `@babel/generator` or `@babel/parser`
   floats off the AFM-pinned versions.
3. Run `cargo test -p babel-plugin --test compat_generator_integration`
   to confirm the new entry parses cleanly under SWC. The
   `corpus_input_sources_parse_under_swc` gate fires immediately;
   the parity gate stays `#[ignore]`d until §4.3.

## Out of scope (deliberately)

- **Full `@babel/generator@7.23.0` surface coverage** — the port
  matches what the **5 upstream call sites** actually feed in;
  per CLAUDE.md "1:1 file mapping", no future-proofing for unused
  AST node kinds. Adding coverage requires a new call site upstream.
- **Decorators, pipeline operator, record/tuple, Flow, stage-3
  proposals** — none are reachable from real Compiled consumer
  code feeding `css-builders.ts:464`. If a future fixture surfaces
  a parse-shape divergence between `@babel/parser@7.29.2` and
  `swc_core@54.0.0` on the constrained subset, treat it as a
  separate Drift event.
- **Cosmetic prettier-tolerated drift on sites 4 and 5** — locked
  byte-exact identically to sites 1–3, per the §4.2 hand-off.
  Don't relax the assertion just because prettier round-trips it.

## Verification

```bash
# Regenerate the JS-locked corpus.
bun parity-harness/compat-generator/oracle.mjs
# → wrote 55 entries (...) -> crates/babel-plugin/tests/compat_generator_corpus.json
# → pin guard: @babel/generator=7.23.0, @babel/parser=7.29.2

# §4.2 gates (corpus shape + SWC parse coverage). Both should be GREEN at §4.2 ship.
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration

# §4.3 gate (the actual byte-parity assertion). Currently #[ignore]d;
# §4.3 lands the port and removes the ignore.
RUSTFLAGS="" cargo test -p babel-plugin --test compat_generator_integration -- --ignored
```
