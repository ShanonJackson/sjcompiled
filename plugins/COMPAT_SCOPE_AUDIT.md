# `plugins/COMPAT_SCOPE_AUDIT.md` — surface enumeration + feasibility for the §5.4/§5.5/§5.6 unblock

> **Author:** scaffolding agent (handed off from §5.3 closure).
> **Audience:** the agent picking up Phase 5 §5.0 (NEW). Read this
> before opening `crates/babel-plugin/src/compat/`.
> **Purpose:** resolve the `(a)` vs `(b)` decision left dangling in
> `STATUS.md` — port `compat/scope.rs` + `compat/path.rs` vs.
> escalate to human review. This file does NOT write code; it
> bounds the work.

## TL;DR

**Recommendation:** option (a) — port `compat/scope.rs` +
`compat/path.rs` + a **subset** of `compat/evaluation.rs`. Estimated
~900–1200 LOC including unit tests. Tractable, but with one hidden
drift surface (Babel's `path.evaluate()` partial-evaluator) that
the audit surfaces and a previous reading of STATUS.md missed.

The earlier "1.5–3k LOC" estimate in STATUS.md §5.4–§5.6 escalation
note conflated the **whole** `@babel/traverse` scope/path machinery
(reified live scope chain, full constant-folder, full visitor
runtime) with the **slice** the Compiled evaluator actually
exercises. The slice is materially smaller. Numbers below.

The blocker is NOT that the surface is unportable — it is that
the surface has **three concerns** the STATUS.md note bundled into
one, and one of them (`path.evaluate()`) is its own 1:1-port unit
that needs an explicit decision the same way `compat/generator.rs`
did before §4.3.

## Surface enumeration (all sites that touch scope/path)

Pulled by grep over `packages/babel-plugin/src/`. The §5.4/§5.5/§5.6
slice is the subset reachable from `evaluate-expression.ts` /
`resolve-binding.ts` / `traverse-expression/*`. Other slices
(`class-names/`, `xcss-prop/`, `babel-plugin.ts`,
`normalize-props-usage.ts`) are listed for completeness — they are
out of `§5.4–§5.6` scope but reuse the same `compat/scope.rs`
surface, so porting once unblocks all of them.

### Scope chain operations (8 distinct call shapes)

| Call shape | §5.4–§5.6 sites | Other reachers | Notes |
|---|---|---|---|
| `path.scope.getBinding(name)` | `evaluate-expression.ts:23`, `:34`; `resolve-binding.ts:201` | `babel-plugin.ts:199`, `:204`; `class-names/index.ts:52`, `:165`; `normalize-props-usage.ts:106` | Lexical-chain walk. The hottest call. Returns `Binding | undefined`. |
| `path.scope.getOwnBinding(name)` | `resolve-binding.ts:201`; `namespace-import.ts:21` | — | Same as `getBinding` but does NOT walk parents. Used for IIFE-isolated lookups. |
| `path.scope.hasOwnBinding(name)` | — | `class-names/index.ts:50`, `:159`, `:164`, `:178` | Boolean form. Trivial. |
| `path.scope.push({id, init, kind})` | `traverse-call-expression.ts:112`; `namespace-import.ts:22` | — | **Mutating.** Synthesises a new `const X = init` declarator at the scope's hoist point and registers the binding. Used to inject IIFE-local args. |
| `path.scope.generateUidIdentifier('')` | — | `hoist-sheet.ts:18` | Mints a fresh unused name — `_<n>` counter. Phase 4 §4.6 already shipped a per-pass version of this on `state.uid_counter`; full-fidelity (collision-aware against existing bindings) is gated here. |
| `path.scope.registerBinding(kind, newVariable)` | — | `hoist-sheet.ts:28` | Registers a manually-constructed declarator with the scope after a manual AST mutation. Pairs with `generateUidIdentifier`. |
| `path.scope.bindings` (raw map iter) | — | `normalize-props-usage.ts:193` | One read of the entire bindings map. Iteration order matches insertion. |
| `parentPath.scope` (read access) | `resolve-binding.ts:201`, `:211` | many | Walking to the scope owner of a path — cheap if scope is keyed by node-id. |

### Binding fields read (5 distinct fields)

| Field | Sites | Shape |
|---|---|---|
| `binding.path.node` | `evaluate-expression.ts:28`, `:39`; `resolve-binding.ts:121`, `:260`, `:419` | The AST node where the binding lives (typically `VariableDeclarator` / `ImportSpecifier` / `ImportDefaultSpecifier`). |
| `binding.path.parentPath` | `resolve-binding.ts:281` | One step up — used to detect `binding.path.parentPath.isImportDeclaration()`. |
| `binding.constant` | `evaluate-expression.ts:28`, `:39`; `resolve-binding.ts:125`, `:276`, `:402`, `:421` | `true` iff the binding has no `constantViolations`. The plugin treats non-const bindings as un-evaluable. |
| `binding.referencePaths` | `evaluate-expression.ts:32`; `normalize-props-usage.ts:106` | Array of `NodePath<Identifier>` for every reference to the binding. Length-equality check + per-element scope walk. |
| Synthesized `Binding` literal at `resolve-binding.ts:209-219` | — | Constructed when re-export `export { foo } from 'bar'` doesn't have a real scope binding. Six fields: `identifier`, `scope`, `path`, `kind: 'const'`, `referenced: false`, `references: 0`, `referencePaths: []`, `constant: true`, `constantViolations: []`. The plugin downstream only reads `path`, `constant`, `referencePaths` — the rest are interface fillers. |

### NodePath operations (10 distinct call shapes)

| Call shape | Sites | Mutating? |
|---|---|---|
| `path.node` | pervasive | no |
| `path.parentPath` | `resolve-binding.ts:226`, `:281`; `css-builders.ts` (many); `xcss-prop/index.ts` | no |
| `path.isImport*Declaration()` / `isImportSpecifier()` / `isImportDefaultSpecifier()` / `isImportNamespaceSpecifier()` / `isExportNamedDeclaration()` / `isObjectPattern()` / `isExpression()` / `isVariableDeclarator()` / `isReferencedIdentifier()` | resolve-binding.ts (15+); class-names; evaluate-expression | no — pure node-shape predicates |
| `path.get(field)` | `resolve-binding.ts:369` (`'specifiers'`); `css-builders.ts:690` (`'init'`) | no — returns child `NodePath` (or array) |
| `path.listKey` | `traverse-member-expression/index.ts:37` | no |
| `path.evaluate()` | `evaluate-expression.ts:93` | no — returns `{ confident: bool, value: any, deopt?: NodePath }` |
| `path.replaceWith(newNode)` returning `[newPath]` | `traverse-call-expression.ts:95` | **YES** — in-place node replacement; returns the new path. |
| `path.traverse(visitor, state?)` | `evaluate-expression.ts:59`; `normalize-props-usage.ts:94`, `:97`; `build-styled-component.ts:104` | no — runs a sub-visitor over the path's subtree, sharing the parent's scope chain |
| `path.stop()` (inside traverse callback) | `evaluate-expression.ts:65`; `traverse-function.ts:36`; `resolve-binding.ts:96` | no — terminates the in-flight sub-traversal |
| `path.buildCodeFrameError(msg)` | `ast.ts:31`, `:50` | no — error formatter; production callers throw |

### Special semantics (the two genuine wrinkles)

#### 1. `getPathOfNode(node, parentPath)` — `ast.ts:10-35`

Synthesises a `NodePath` for `node` against `parentPath.scope`. Used
3× in §5.4–§5.6: once in `evaluate-expression.ts:88`, twice in
`traverse-call-expression.ts:91`, `:98`. The implementation is:

```js
traverse(node, { enter(path) { foundPath = path; path.stop(); } },
         parentPath.scope, undefined, parentPath);
```

i.e. it kicks `@babel/traverse` against `node` with the parent's
scope already-resolved as the inherited scope, then captures the
first path emitted. **The Rust analog is constructive**: build a
`PathHandle { node_id, parent_id, scope_id, list_key: None }`
directly. No actual traversal is needed because we already know
the node and the parent.

#### 2. The IIFE in `traverseCallExpression` — `traverse-call-expression.ts:91-122`

This is the tightest dependency. Lines 95–122 do:

1. `getPathOfNode(expression, parentPath)` → `expressionPath`
2. `expressionPath.replaceWith(wrapNodeInIIFE(expression))` — splice
   the call into `(() => callExpr)()`, get the new path
3. `getPathOfNode(wrappingNodePath.node.callee, wrappingNodePath as any)`
   → `arrowFunctionExpressionPath`
4. For each `(param, evaluatedArg)`:
   `arrowFunctionExpressionPath.scope.push({id: param, init: evaluatedArg, kind: 'const'})`
   — registers `const param = evaluatedArg` inside the IIFE's scope
5. `meta.ownPath = arrowFunctionExpressionPath`

Subsequent identifier resolution under this `meta` checks
`ownPath.scope.getOwnBinding(name)` first, then falls back to
`parentPath.scope.getBinding(name)` (`resolve-binding.ts:201`).

**The Rust analog**: synthesise an `ArrowFnExpr` wrapping the call
expression, build a `Scope` for the arrow's body keyed by an
`IndexMap<String, Expr>` of the (param-name → evaluated-arg) pairs,
and thread it as `meta.own_path: PathHandle`. No actual mutation of
the surrounding AST is required — the arrow node is constructed in
isolation, evaluated against the synthetic scope, and discarded
once the call expression's value is folded. The "replaceWith" in
upstream is mechanical (Babel needs a path; we don't).

#### 3. `path.evaluate()` — Babel's partial constant-folder

This is **not in the Compiled source.** It's `@babel/traverse/lib/path/evaluation.js`, ~640 LOC of constant-folding for literal-typed expressions. Compiled calls it once, in
`evaluate-expression.ts:93`, wrapped in `try { ... } catch { return fallbackNode }`. When it succeeds and returns a string or number, the result becomes a `t.stringLiteral` / `t.numericLiteral` that flows into CSS values → `transform_css` → atomic class hash.

**This means a divergent partial-evaluator output is a divergent class name in production.** Same severity as `compat/generator.rs` was for §4.3.

The good news: the corpus only exercises a slice. From
`expression-evaluation.test.ts` (32 tests) and `module-traversal.test.ts` (49 tests), the constant-folding shapes the test
corpus actually reaches:

- String / numeric / boolean / null / undefined literals (no-op fold).
- Binary expressions over literals: `+`, `-`, `*`, `/`, `==`, `===`,
  `<`, `>`, `<=`, `>=`, `&&`, `||`, `??`, `??=`, string concatenation.
- Unary expressions: `-`, `+`, `!`, `void`, `typeof`.
- Conditional expressions where the test folds to a literal.
- Identifiers resolving to a `const` whose init folds.
- Member expressions on object literals where the property folds.
- Template literals with all-literal quasis.

What the corpus does NOT reach (not part of the parity surface):

- Arrow / function bodies (Compiled hits these via
  `traverseFunction`, not `path.evaluate()` — different code path).
- Loops, conditionals at statement level.
- Throw / try / catch.
- Tagged templates (the `keyframes\`...\`` shape goes through the
  CSS extractor, not `path.evaluate`).
- `Symbol`, `Reflect`, builtins.

**Estimated LOC for the slice**: 200–400 LOC (vs. Babel's 640 LOC
full impl). Concretely, the Rust port is a `match Expr { Lit(_) =>
..., Bin(_) => ..., ... }` recursive folder over the slice above,
returning `Option<EvaluatedValue>` where `None` means deopt.
**This is a separate, locked unit of work that the §5.0 implementer
must scope explicitly** — not roll into "compat/scope.rs" by
accident.

## Concerns the STATUS.md note bundled together

STATUS.md §5.4–§5.6 escalation says "approximately 1.5–3k LOC" for
the spike. That number is based on porting the WHOLE
`@babel/traverse` (`packages/traverse/lib/scope/index.js` is 1500
LOC alone, `path/index.js` + `path/evaluation.js` + `path/family.js`
adds another 2000). **The Compiled plugin does not exercise that
whole surface.** Concrete decomposition:

| Unit | Estimated Rust LOC | What it ports | Risk |
|---|---|---|---|
| `compat/scope.rs` | 250–350 | Scope-tree pre-pass + `Binding` shape (`path.node`, `constant`, `referencePaths`, `parentPath`) + `getBinding` / `getOwnBinding` / `hasOwnBinding` lexical walk. NO `bindings` map iter (deferred until `normalize-props-usage` ports). NO `generateUidIdentifier` (already on `State` per §4.6 stop-gap; tighten later). | LOW — pure data structure; no AST mutation. |
| `compat/path.rs` | 250–350 | `PathHandle` carrying `(node_ref, parent_ref, scope_ref, list_key)`. Predicate methods (`is_*`). `get(field)`. `parent_path()`. `replace_with()` for the IIFE single-use case (in-place mutation through a `&mut Module` borrowed by the visitor). `traverse()` — delegates to a `VisitMut` on the subtree with the path's scope as the inherited scope. | MEDIUM — `replace_with` requires careful borrow checking against the `&mut Module` the SWC `VisitMut` already holds. Likely doable via `std::mem::replace` on the `&mut Expr` field; the IIFE wrapper is an `Expr → Expr` substitution. |
| `compat/evaluation.rs` | 200–400 | The slice of Babel's `path.evaluate()` reachable from the test corpus. Unit-tested against a parity corpus seeded from upstream's evaluator output (see "Verification" below). | HIGH — same severity as `compat/generator.rs` for §4.3. **A separate gate, not folded into the scope port.** |
| `utils/resolve_binding.rs` (§5.4) | 350–450 | 1:1 port of `resolve-binding.ts`. Calls into `compat/scope.rs` + `compat/path.rs`. Adds `oxc_resolver` for the import resolution path (the JS `resolve.sync()` fallback at `:185-189` AND the `resolver.resolveSync` injected path at `:191-193` collapse into one call — see PLAN.md §1 constraint 2). | MEDIUM — resolver parity is its own gate (PLAN.md §0 task 7's "resolver difference matrix"). |
| `utils/traverse_expression/*.rs` (§5.5) | 350–500 | 1:1 port of the 9-file `traverse-expression/` subtree. Pure layout port once `compat/*` is in place. | LOW once compat/* lands. |
| `utils/evaluate_expression.rs` (§5.6) | 200–300 | 1:1 port of the 200-LOC `evaluate-expression.ts`. | LOW once compat/* lands. |
| **Total Phase 5 scope-tree work** | **1600–2300 LOC** | | |

Of which `compat/*` is **700–1100 LOC**. The remaining 1000+ LOC
(§5.4 / §5.5 / §5.6) is the work the previous agent already scoped
under those checkpoint numbers; STATUS.md was conflating it.

## What the existing strip-runtime `compat/scope.rs` covers (and doesn't)

`crates/babel-plugin-strip-runtime/src/compat/scope.rs` (352 LOC,
already shipped) is a **flat module-binding index** for ONE narrow
lookup: `get_string_binding(name) → Option<&str>`. It indexes
top-level `var/let/const X = "literal"` and supports `mark_for_removal` + `apply_removals` for the strip-runtime CC/CS site dispatcher.

It is NOT a usable analog for the evaluator's needs because:

1. No lexical chain — it indexes only top-level. Compiled's
   evaluator walks into function bodies (param resolution),
   block-scoped declarations, and IIFE-injected synthetic scopes.
2. No `Binding` shape — it returns `Option<&str>` (the cached
   string value) directly. The evaluator needs the AST node
   (`binding.path.node`), the constness, and the reference paths.
3. No path analog — strip-runtime never queries `parentPath` /
   `replaceWith` / `get(field)` / `traverse`.

The strip-runtime impl SHOULD stay where it is (its narrowness is
a feature — single-lookup, single-purpose, ~352 LOC of locked
parity). The babel-plugin port needs a separate `compat/scope.rs`
that lives in `crates/babel-plugin/src/compat/`. The two do not
share a crate today; if a future refactor folds them into a
shared `crates/compat-scope/` crate, that's a Phase 7+ cleanup.

## Upstream-trace findings (2026-05-04)

The original audit traced **from consumers** (grep over
`evaluate-expression.ts` / `resolve-binding.ts` / `traverse-expression/*`).
That gives the methods called but doesn't independently verify the
transitive surface those methods reach inside `@babel/traverse`.
Spot-check landed before §5.0a opens, against the AFM-pinned source
at `node_modules/.bun/@babel+traverse@7.29.0/node_modules/@babel/traverse/lib/{scope/index.js,scope/binding.js}`.
Verified pin: `@babel/traverse@7.29.0`, resolved at
`node_modules/.bun/@babel+traverse@7.29.0/...`. Matches
`crates/PARITY_VERSIONS.md`.

Findings the consumer-trace missed, in order of severity:

### Finding 1 — `Binding.constant` IS a stored bool, NOT computed dynamically

`scope/binding.js:7-31` defines `Binding`'s constructor as:

```js
this.constantViolations = [];
this.constant = true;
// ...
reassign(path) {
  this.constant = false;
  this.constantViolations.push(path);
}
```

The §5.5/§5.6 owner's "load-bearing" concern that Babel computes
`constant` dynamically from `constantViolations.length` is **incorrect
for `@babel/traverse@7.29.0`**. The Rust port models `constant` as a
struct field set during pre-index crawl; mutations between two
`getBinding()` calls in the evaluator cannot make a stored-bool
diverge from a computed-from-violations bool because `reassign()` is
the single setter for both fields and they update atomically.

**Action**: §5.0a stores `constant: bool` directly on the Rust
`Binding` struct. No "should this be a method?" debate needed.
Behavior parity with 7.29.0 is what we ship.

### Finding 2 — `Scope.getBinding(name)` has a pattern-skip rule the audit missed

`scope/index.js:809-824`:

```js
getBinding(name) {
  let scope = this; let previousPath;
  do {
    const binding = scope.getOwnBinding(name);
    if (binding) {
      if (previousPath?.isPattern() && binding.kind !== "param" && binding.kind !== "local") {
        // SKIP — keep walking up
      } else {
        return binding;
      }
    } else if (!binding && name === "arguments" && scope.path.isFunction() && !scope.path.isArrowFunctionExpression()) {
      break;  // arguments stops at non-arrow function boundary
    }
    previousPath = scope.path;
  } while (scope = scope.parent);
}
```

Two non-obvious behaviors:

1. **Pattern-skip**: if the lookup walked through a `Pattern` scope
   (e.g. function-param destructuring), and the binding found in the
   parent is NOT `param`/`local`, SKIP it and keep walking. Handles
   `function f({ x }) { return x; }` — `x` resolves to the inner
   pattern binding even when an outer scope has a same-named
   `const` / `let`. **Reachable** from the §5.4 ports
   (resolve-binding for member-access on destructured params).
2. **`arguments` early-return**: in a non-arrow Function scope, if
   `arguments` is requested and there's no own-binding, return
   `undefined` rather than continuing up to module scope. Compiled
   would only hit this if a user wrote
   `function f() { return css({ ...arguments }); }` — confirmed
   ZERO matches across the 477-fixture corpus. Mark as
   evidenced-unreachable for the babel-plugin port.

**Action**: §5.0a's `get_binding()` MUST implement the pattern-skip
walk. New fixture `pattern-skip-getBinding-walks-past-pattern`
added to `compat-scope/fixtures.json`. The `arguments` short-circuit
is documented as evidenced-unreachable but the §5.0a impl should
still mirror it 1:1 (cheap, ~3 LOC); panic-with-citation is overkill
when the parity port is trivial.

### Finding 3 — `var` declarations hoist through ForStatement/ForXStatement init

`scope/index.js:191-200` (`ForStatement` collector) and `:222-233`
(`ForXStatement` collector):

```js
ForStatement(path) {
  const declar = path.get("init");
  if (declar.isVar()) {
    const parentScope = scope.getFunctionParent() || scope.getProgramParent();
    parentScope.registerBinding("var", declar);
  }
}
```

A `for (var x = …; …; …)` registers `x` at the **enclosing function
or module scope**, NOT at the `ForStatement`'s block. Same for
`for (var x of arr)` and `for (var x in obj)`. The Rust pre-index
must mirror this: `var` declarations hoist to the nearest function
or program scope; `let` / `const` / `using` / `await using` register
at the immediate block scope.

**Action**: §5.0a's pre-index crawl walks `var`-hoist sites and
registers them at the function/program parent. New fixture
`var-in-for-loop-hoists-to-function-scope` added to
`compat-scope/fixtures.json`.

### Finding 4 — `Binding` constructor auto-reassigns for `var`/`hoisted` in loops

`scope/binding.js:27-29`:

```js
if ((kind === "var" || kind === "hoisted") && isInitInLoop(path)) {
  this.reassign(path);
}
```

Where `isInitInLoop` walks parents looking for a `ForXStatement` `left`
position or a `Loop` `body` position with an init. Triggered when:

- `for (var x of arr) { … }` — `x` is auto-marked non-constant.
- `while (cond) { var y = 1; }` — `y` is auto-marked non-constant
  (function-decl `hoisted` kind only fires when `path.node.init` is
  set — pure `var x;` without init is exempt; see `isFunctionDeclarationOrHasInit`).

Compiled's evaluator reads `binding.constant` at
`evaluate-expression.ts:28` / `:39`. A `var` inside a loop body in
user code WOULD produce `binding.constant === false` and short-circuit
evaluation — so the Rust port must replicate. Likely rare in
CSS-value position but parity-significant.

**Action**: §5.0a's `Binding::new` mirrors `isInitInLoop`. New
fixture `var-in-while-loop-is-non-constant` added.

### Finding 5 — `Scope.parent` is a getter with pattern-skip semantics

`scope/index.js:347-359`:

```js
get parent() {
  let parent, path = this.path;
  do {
    const shouldSkip = path.key === "key" || path.listKey === "decorators";
    path = path.parentPath;
    if (shouldSkip && path.isMethod()) path = path.parentPath;
    if (path?.isScope()) parent = path;
  } while (path && !parent);
  return parent?.scope;
}
```

The parent chain skips:
- ObjectProperty/Method `key` positions (the key isn't in scope of
  the method body's lexical chain).
- Decorator `decorators` list positions (decorators evaluate in the
  enclosing scope, not the decorated method's).

The Rust pre-index's parent-pointer map MUST bake these skips in at
build time — OR the parent-walk code must mirror the skip logic.
Building it in is cleaner (single check on insert vs. repeated
checks on every walk).

**Action**: §5.0a's `ScopeIndex::build` checks `key` / `listKey`
when registering parents. Documented in compat/scope.rs at the
`fn parent_of(scope_id) -> Option<ScopeId>` impl.

### Finding 6 — `Scope.push({id, init, kind})` mutates the AST, not just the bindings map

`scope/index.js:717-756`. The simple "register a binding" mental model
is wrong. Push:

1. Walks the path up to a valid push-target (`BlockStatement` /
   `Program`; if currently in a Pattern, walk to pattern parent;
   if in a SwitchStatement, walk to function/program parent).
2. For loop / catch / function paths: calls `ensureBlock()` to
   force a block body, then descends to it.
3. Computes a `dataKey = "declaration:${kind}:${blockHoist}"` and
   reuses an existing declaration block at that key if present
   (collapses repeated pushes into one `VariableDeclaration` node).
4. Synthesises a `VariableDeclarator(id, init)` AST node.
5. Calls `unshiftContainer("body", [declar])` — the new decl lands
   at the TOP of the body.
6. Calls `registerBinding(kind, declarPath.get("declarations")[len - 1])`
   to wire up the binding.

For the §5.0b IIFE site (Q2 lock — single-site `&mut Expr`), the
synthesised arrow's body MUST receive these `const param = init`
declarators as actual AST nodes, NOT a side-table the Rust port
keeps separate. Otherwise downstream visitors that walk the arrow's
body (e.g. `path.traverse(visitor)` on the arrow) won't see the
injected bindings.

**Action**: §5.0b's `scope_push()` is a 1:1 port of this method.
Tracking issue: the IIFE wrapping in
`traverse-call-expression.ts:95-122` constructs a fresh Arrow with
empty body, then `scope.push`'s declarators into it. The Rust
analog inserts `VarDecl`s into the synthesised `BlockStmt` and
re-runs `ScopeIndex::register_local_var(arrow_scope_id, name,
declarator_ref)` for each.

### Finding 7 — `Scope.crawl()` is lazy via `init()`; eager pre-index is a deliberate semantic delta

`scope/index.js:658-716`:

```js
init() { if (!this.inited) { this.inited = true; this.crawl(); } }
```

Babel triggers `init()` from inside `getBinding`-family methods so
the first lookup forces a full subtree walk. The Rust port (Q1
lock) walks the entire Module on `Program::enter` regardless of
whether anything queries the scope.

**Functional equivalence**: Compiled queries scope very heavily
(every CSS value goes through identifier resolution), so lazy
crawl ends up walking the whole tree anyway. Eager pre-index is
trivially equivalent for our use case AND simplifies the borrow
model (Q1 reasoning).

**Action**: §5.0a documents this as an INTENTIONAL semantic delta,
not drift. Future agents reading the port should not "fix" it by
introducing lazy crawl; the difference is observed nowhere because
Compiled's evaluator forces full coverage anyway.

### Finding 8 — `Scope.globals` and `Scope.contextVariables` come from `@babel/helper-globals`

`scope/index.js:14-15`, `:940-941`:

```js
const globalsBuiltinLower = require("@babel/helper-globals/data/builtin-lower.json"); // 13 entries
const globalsBuiltinUpper = require("@babel/helper-globals/data/builtin-upper.json"); // 49 entries
Scope.globals = [...globalsBuiltinLower, ...globalsBuiltinUpper];
Scope.contextVariables = ["arguments", "undefined", "Infinity", "NaN"];
```

Reachable from `hasBinding(name, { noGlobals: false })` and from
`isPure(node, constantsOnly)`. The §5.0c evaluator's `Identifier`
branch resolves `undefined` / `NaN` / `Infinity` to their global
values — both lists must be vendored into the Rust port verbatim.

**Action**: §5.0a vendors both JSON files as `const` slices. Pin
`@babel/helper-globals@7.28.0` in `crates/PARITY_VERSIONS.md`
(transitively pulled by `@babel/traverse@7.29.0` today; promote
to top-level overrides per §4.2 lesson if it ever floats). Add a
schema-lock test asserting the entry count matches (`13 + 49`)
so a future `@babel/traverse` bump that changes globals fails the
gate immediately.

### Findings deferred — not in §5.4–§5.6 reach

- `Scope.rename()` — used by Babel's `_renamer`, not the evaluator.
  Out of scope.
- `Scope.hoistVariables()` — `var` hoisting on `for-of` rewrite,
  used by `@babel/plugin-transform-block-scoping`. Out of scope.
- `Scope.toArray()` — array iteration helper, used by spread-rewrite.
  Out of scope.
- `Scope.isPure()` / `Scope.isStatic()` — purity checks, called
  from constant-folding of `Symbol.for(...)` etc. Reachable from
  `path.evaluate()`'s `MemberExpression` branch — the §5.0c port
  needs `isPure` for the `Symbol.for("x")` case it covers. Add a
  fixture in compat-evaluation if `isPure` is non-trivial; for the
  current corpus it's a one-line `false` for any non-Symbol input.
- `Scope.removeBinding()` — strip-runtime's territory, not
  babel-plugin's evaluator. Out of scope.
- `Binding.setValue` / `clearValue` / `deoptValue` / `dereference` —
  internal mutation methods called only from inside `@babel/traverse`'s
  crawl pass, not consumer-visible. The Rust port's pre-index handles
  this internally during the build pass.

## Summary of audit-doc updates from upstream-trace

| Finding | Audit-table change | New fixture | Pin doc change |
|---|---|---|---|
| 1 | Confirmed: `constant` is stored bool. No surface change; reasoning recorded. | — | — |
| 2 | New row: `getBinding` pattern-skip walk. New row: `getBinding` arguments-early-return (evidenced-unreachable). | `pattern-skip-getBinding-walks-past-pattern` | — |
| 3 | New row: `var` hoist through `ForStatement`/`ForXStatement` init. | `var-in-for-loop-hoists-to-function-scope` | — |
| 4 | New row: `Binding` constructor `isInitInLoop` auto-reassign. | `var-in-while-loop-is-non-constant` | — |
| 5 | Updated `parent_path` row: pattern-skip key/decorators positions. | (covered by existing fixtures + Finding 2 fixture) | — |
| 6 | Major update to `scope.push` row: AST-mutating, not bindings-only. | (existing `scope-push-iife` fixture suffices) | — |
| 7 | New note: eager pre-index is intentional semantic delta. | — | — |
| 8 | Updated `Scope.globals` row: vendor `@babel/helper-globals@7.28.0` JSON. | (cover via §5.0c fixtures already present for `undefined`/`NaN`/`Infinity`) | NEW pin row |

The audit's surface table grows from "8 scope-chain methods + 5
binding fields + 10 NodePath operations" to:
- **9 scope-chain methods** (added: `getProgramParent` walk semantic
  used by `var` hoist, made implicit by Finding 3).
- **5 binding fields** (unchanged).
- **10 NodePath operations** (unchanged).
- **3 transitive semantic rules** (NEW row class — pattern-skip,
  var-hoist, isInitInLoop auto-reassign).
- **1 vendored data dependency** (`@babel/helper-globals` JSON).

LOC estimate update: `compat/scope.rs` 250–350 → 300–400 (the
extra ~50 LOC covers the three semantic rules + the helper-globals
vendor). Within Q1's budget.

---

## Three architectural questions — RESOLVED 2026-05-04

Locked by the §5.0 owner in response to this audit. Recorded here
so the implementer reads the answers, not the open questions.

**Q1 — Pre-index. ✓** SWC's `&mut self` visitors don't compose with
live-scope mutation; the borrow chain explodes. Pre-index on
`Program::enter` (binding map + parent-pointer map + reference-paths
map) gives read-only navigation during the visit pass. The only
"live" requirement is invalidate-on-replace, which is local.
Architecturally consistent with §5.3's record-then-replay cache
model.

**Q2 — Scoped `&mut Expr` for the IIFE site only. ✓** The single
`replaceWith` site (IIFE wrap in `traverseCallExpression`) gets
`&mut Expr` passed down explicitly. The rest of
`evaluate_expression` returns a `Resolved` value (computed
expression or replacement node) and stays read-only. **Don't
propagate `&mut Expr` through the whole evaluator to serve one
site.** One mutation-bearing call shape, the rest of the call
graph stays clean.

**§5.0b SPEC LOCK — `scope.push({id, init, kind})` is
AST-mutating** (Finding 6 above). Implementation contract:

**§5.0a → §5.0b handoff state (logged 2026-05-04 by §5.0a
implementer):** §5.0a shipped `scope_push_synthetic` as a
**binding-table-only stub** — registers the synthetic binding in
the scope index but does NOT touch the AST. This is correct for
§5.0a per the Q1 (pre-index, read-only) and Q2 (mutation confined
to one site) locks; §5.0a's deliverable is the read-only binding
index, not AST mutation. The stub passes the
`scope-push-iife` corpus fixture today because the fixture only
asserts post-push binding-shape observables (`getOwnBinding`
result, kind, parent scope reachability), NOT post-push AST
shape.

**§5.0b CLOSED (2026-05-04, this session).** The replacement
landed at `crates/babel-plugin/src/compat/path.rs::scope_push`
(`pub fn scope_push(&mut ScopeIndex, ScopeId, PushOpts, &mut
BlockStmt)`), per the 7-step contract below. Behaviour realised:
unshifts a `VariableDeclaration` into the target block's
`body[0]`, coalescing same-kind same-blockHoist pushes into one
declaration via the `dataKey` reuse rule, and registers the new
declarator's binding via the new
`ScopeIndex::register_synthetic_binding` helper.

The "push then traverse, observe new VarDecl" round-trip
(`scope_push_inserts_var_decl_into_arrow_body_visible_to_traverse`
in `compat/path.rs`'s `tests` module) is green. The stub
(renamed `scope_push_synthetic`, now a thin
binding-only delegate to `register_synthetic_binding`) is
retained ONLY for the §5.0a parity-gate fixture, which asserts
binding shape without AST observation. Production callers MUST
use `compat::path::scope_push`.

**Why both APIs coexist.** Removing `scope_push_synthetic`
outright would force a rewrite of the §5.0a integration test's
scope-push-iife runner (which works against `&Module` and a
synthetic span). The wrapper keeps that gate stable while making
the production path unambiguous. If a future cleanup migrates the
fixture to call `compat::path::scope_push` directly, delete the
wrapper — it has no production reach.



1. The IIFE construct in `traverse-call-expression.ts:95-122`
   synthesises an arrow `(() => callExpr)()` and pushes evaluated
   args as `const param = evaluatedArg` into the arrow's body
   scope.
2. The Rust port's `scope_push(arrow_path: &mut PathHandle,
   PushOpts { id, init, kind })` MUST:
   - Walk to a valid push-target per upstream's logic
     (`BlockStatement` / `Program`; pattern parents redirect to
     pattern-parent's body; switch-statement parents redirect to
     function/program parent).
   - On loop / catch / function paths: synthesise an empty
     `BlockStmt` if absent (`ensureBlock` analog), descend to it.
   - Compute `dataKey = "declaration:{kind}:{blockHoist}"`. Reuse
     an existing declaration block at that key if present, so
     repeated pushes collapse into one `VariableDeclaration`.
   - Synthesise a `VarDeclarator { name: id, init }` and
     `unshiftContainer`-equivalent the new `VarDecl` onto
     `block.stmts` (i.e. INSERT AT INDEX 0).
   - Re-run `ScopeIndex::register_local_var(arrow_scope_id, name,
     declarator_ref)` so subsequent `get_own_binding()` lookups
     against the arrow's scope find the injected binding.
3. Downstream `path.traverse(visitor)` on the arrow's subtree
   MUST observe the injected `VarDecl` nodes as ordinary AST.
   This is the byte-parity contract — a bindings-map-only
   update would silently diverge here.

The §5.0b implementer's first cargo unit test should be a
"push then traverse, observe the new VarDecl" round-trip; if
that test passes, the AST-mutation contract holds. If it
returns a bindings-map but no AST node, the model is wrong —
even if `get_own_binding` succeeds in isolation.

**§5.5/§5.6 trip-wire on lazy-crawl semantic delta** (Finding 7
above): if the §5.5/§5.6 ports ever reach a
`getBinding → mutate → getBinding` shape (i.e. the same
binding is queried before AND after a mutation in the same
evaluation pass), STOP and verify the eager pre-index sees the
mutation. Today's audit shows none of the §5.4/§5.5/§5.6
sources exercise this shape — but if a future fixture surfaces
it, the eager-vs-lazy delta becomes observable and the Rust
port must invalidate-and-re-crawl the affected scope, not
serve a stale binding from the pre-index.

**§5.5/§5.6 IMPLEMENTER ACTION — plant a breadcrumb at every
dispatch site that calls `get_binding()` / `get_own_binding()`.**
When §5.5 (`utils/traverse_expression/*.rs`) and §5.6
(`utils/evaluate_expression.rs`) land, every call into the
scope index gets a one-line comment:

```rust
// If a fixture surfaces lazy-crawl observability here
// (getBinding → mutate → getBinding diverging from upstream),
// see plugins/COMPAT_SCOPE_AUDIT.md Finding 7.
let binding = scope.get_binding(name);
```

The breadcrumb is grep-discoverable, points back to the
authoritative reasoning, and prevents the exact failure mode
CLAUDE.md forbids: a future agent encountering a divergence,
re-deriving the eager-vs-lazy decision badly, and patching
around it instead of escalating. Cost is one comment per
dispatch site (~10 sites across §5.4–§5.6 per the surface
table); benefit is durable provenance on a non-obvious
architectural choice.

The §5.5/§5.6 owner verifies this discipline at PR time —
grep for `get_binding\|get_own_binding` in `utils/` and confirm
each call carries the breadcrumb, OR is exempt because it sits
inside a sub-helper whose enclosing function already carries
one. No exemptions for "obviously not affected" — the whole
point of the breadcrumb is that observability is non-obvious.

**Q3 — Full port of `path.evaluate()`. ✓** Reverses the
partial-port recommendation in this audit. Reasons:
1. "BUGS in OLD = BUGS in NEW" is the cardinal rule (CLAUDE.md).
   Partial-port-with-deferred-list defers the rule to "Phase 8
   corpus diff will catch gaps" — i.e. ship a known-incomplete
   port and trust a few-hundred-fixture corpus to surface what
   the 10M-LOC consumer monorepo will actually hit. Coverage gap
   is real.
2. `path.evaluate()` is bounded. Read `@babel/traverse/lib/path/evaluation.js`,
   port line-by-line. Several hundred LOC, readable and finite.
3. Phase 8 §8.1 catching missing shapes is the most expensive
   blast radius for the latest possible unblock. Pay the cost
   now, not under integration-time pressure.

**Concession (in-scope):** if the line-by-line port surfaces a
node-type branch that genuinely cannot reach Compiled call sites
(e.g. JSXElement evaluation, async/generator-specific branches),
`unimplemented!("path.evaluate() unreachable from Compiled — see
<survey-file>")` is acceptable **iff** the survey citing every
caller is in-tree and referenced from the panic message. That's
bounded by evidence, not deferred by hope. Same shape as the
existing §6.3 / §5.6 stub citations.

The §4.3 precedent (`compat/generator.rs` deferring `flow.js` /
`typescript.js`) does NOT generalise here. Those are out-of-language
branches that parsing rejects before reaching the generator —
genuinely unreachable. `path.evaluate()`'s deferred branches in a
partial-port would all be reachable JavaScript. Not analogous.

---

The original open questions, retained for reference:

### Q1. Single-pass live scope or two-pass pre-index?

**Babel's model**: scope is built lazily during traversal.
`path.scope.getBinding('x')` triggers `Scope.crawl()` if the scope
hasn't been crawled yet, which walks the immediate subtree under
the scope owner registering all bindings.

**Rust option (a) — one-pass live**: maintain a `ScopeCtx` stack
inside the `VisitMut` impl. On `visit_mut_module`, push a new
scope; on `visit_mut_function`/`visit_mut_arrow`/`visit_mut_block`,
push a nested scope; on `visit_mut_var_declarator`, register the
binding into the top-of-stack. On exit, pop. Lookups walk the stack.

**Rust option (b) — two-pass pre-index**: before the visitor runs,
walk the whole `Module` once with `Visit` (read-only) and build a
flat `IndexMap<NodeId, ScopeId>` plus `IndexMap<ScopeId, Bindings>`.
The visitor then queries this index via cheap pointer lookups.
Bindings registered DURING the visitor (the IIFE case) are layered
into a per-call scratch scope.

**Recommendation**: **(b)**. Two reasons:
1. The `referencePaths` field requires a complete walk — you can't
   know all references to a binding until the whole subtree has
   been visited. Babel handles this by triggering `crawl()` on
   `getBinding`'s first call (which forces the full scope walk
   anyway). Doing the walk explicitly up-front simplifies the borrow
   model.
2. SWC's `VisitMut` borrows `&mut` of every node it visits.
   Maintaining a live scope stack that needs to look up nodes by
   id — while those nodes are in the `&mut` chain — gets nasty fast.
   Pre-indexing (read-only `Visit`) decouples the index from the
   mutation pass.

The cost is a doubled traversal cost on each transform call.
Empirically: SWC plugins are already 5–20× faster than Babel for
parse + transform; doubling the babel-plugin pass is well within
the performance margin. The cache (Phase 5 §5.3) hit-rate amortises
this across rebuilds.

### Q2. How does `path.replaceWith` work against an SWC `&mut Module`?

The IIFE construct (the only `replaceWith` site in §5.4–§5.6) needs
to substitute a `CallExpr` with `(() => CallExpr)()` mid-traversal.

**Borrow-checker reality**: in `VisitMut::visit_mut_call_expr(&mut self, node: &mut CallExpr)`, replacing `node` with a *different* expression kind (e.g. `Expr::Call` with `Expr::Arrow` wrapping a call) is not possible from within `visit_mut_call_expr` — the function only sees the inner `CallExpr`, not the enclosing `Expr`.

**Rust option**: take the path one level higher. The §5.4–§5.6
ports never directly visit the call expression-to-replace; they
visit the `Expr` that contains it (typically a tagged template
arg, an object property value, or an array element). At that
level the field is `&mut Box<Expr>` or `&mut Expr`, which CAN
be replaced via `*expr = new_expr`.

**Action item for §5.0**: trace through the IIFE call chain from
`css-builders.rs`'s `extract_keyframes` / `extract_object_expression`
/ `extract_template_literal` (the only callers of `evaluate_expression`
on Compiled-handled Expr fields). Confirm the `&mut Expr` is reachable.
If not — escalate; the IIFE may need to be modeled as a synthetic
scope without an actual AST mutation (as suggested in "Special
semantics" §2 above).

### Q3. How much of `path.evaluate()` to port?

**Recommendation**: a **separate, gated checkpoint** —
`Phase 5 §5.0c (NEW): compat/evaluation.rs`. Treated like
`compat/generator.rs` was for §4.3:
1. Build a coverage manifest (input shapes that reach
   `evaluate-expression.ts:93`).
2. Build a parity corpus (input AST → JS-oracle `path.evaluate()`
   output as JSON) over the corpus from `expression-evaluation.test.ts`.
3. Land the Rust port that passes the corpus byte-equal.
4. Gate Phase 5 §5.6 on that corpus passing.

Do NOT bundle this work into `compat/scope.rs`. Treating it as a
separate ~300 LOC port keeps the review-surface and risk
isolated.

## Verification corpus — bound the unknown the same way §4.3 did

The §4.3 success pattern was:

1. `parity-harness/compat-generator/oracle.mjs` runs upstream
   `@babel/generator@7.23.0` against 55 fixtures, emitting
   expected-output bytes into a corpus JSON.
2. `crates/babel-plugin/tests/compat_generator_integration.rs`
   reads the JSON, runs the Rust port, asserts byte-equality.

For §5.0 the analog is **two corpora**:

1. `parity-harness/compat-scope/` — synth fixtures exercising every
   call shape in the surface table. Oracle runs `@babel/traverse`
   bindings/getBinding/etc. against each fixture, captures the
   expected `binding.path.node.type`, `binding.constant`,
   `binding.referencePaths.length`, etc. as JSON. Rust integration
   test asserts the index produces the same shape.

2. `parity-harness/compat-evaluation/` — input AST + opts → JS
   oracle's `path.evaluate()` output (as `{ confident, value }`).
   Sub-corpus for the slice declared above (literals, binary,
   unary, ternary, identifier-to-const, member-on-literal).

Both corpora are gitignored, regenerated by `bun parity-harness/compat-scope/oracle.mjs` etc. The Rust gates fail fast on
divergence — same shape as the §3 hash-parity gate and the §4.3
generator gate.

## Why option (a) over (b)

STATUS.md §5.4–§5.6 escalation framed (b) as "let JS keep this
slice via an out-of-band oracle call". That violates PLAN.md
constraint 1 (no JS callbacks from the WASI plugin). The only way
(b) works is by running the JS evaluator at host level (Parcel
transformer wrapper) and threading results back through plugin
config — which means a two-pass scan/apply protocol that PLAN.md
§3.3 / §3.5 explicitly killed when constraint 3 (Rust CSS port)
landed.

(b) is therefore not actually a fallback — it's a re-introduction
of the architecture the spec already rejected. The remaining
choices are:

- **(a)** Port `compat/*` per the breakdown above.
- **(c)** Escalate to the user — but only after the spike (this
  document) shows the work is bounded.

This document's role is to convert STATUS.md's "1.5–3k LOC,
unknown" into "700–1100 LOC compat layer + a separate ~300 LOC
evaluation port, bounded and gated like §4.3". With that
re-framing, (a) is the right call. (c) is unnecessary.

If the §5.0 implementer hits a wall — particularly on Q2
(`replaceWith` reachability) — that's the time to escalate. Not now.

## Concrete next-step plan for the §5.0 implementer

1. **Read this file end-to-end.** Cross-reference STATUS.md §5.3
   closure summary (the cache machinery is locked and ready for
   the typed-T choices the evaluator dictates).
2. **Lock answers to Q1, Q2, Q3** in a new `STATUS.md` §5.0 entry
   before writing code. Treat them like the §4.3 entry-gate
   coverage manifest: explicit, dated, signed.
3. **Split into THREE checkpoints**:
   - §5.0a `compat/scope.rs` (the binding index + lexical chain)
   - §5.0b `compat/path.rs` (the `PathHandle` + `replaceWith` +
     `traverse` + `get(field)`)
   - §5.0c `compat/evaluation.rs` (the `path.evaluate()` slice,
     gated on its own parity corpus the way §4.3 was)
4. **Build the parity corpora FIRST.** Same pattern as §4.3 —
   the corpus is the contract; the port lands against it.
5. **Then proceed with §5.4 / §5.5 / §5.6 in order.** Each is a
   1:1 file-port once `compat/*` is in place.
6. **Total realistic calendar**: 2–3 sessions for `compat/*` +
   corpora (mostly corpus generation + careful review against
   `@babel/traverse` source); 1–2 sessions for the §5.4–§5.6
   ports. Compare to STATUS.md's open-ended escalation framing.

## Files not edited this session

This document is **research only**. No code, no `STATUS.md` edit,
no `Cargo.toml` change. The next agent owns:

- Adding a `Phase 5 §5.0` row to STATUS.md.
- Updating STATUS.md's "Resume here" / "Next checkpoint" pointer
  if they accept the recommendation.
- Citing this file from the new §5.0 row so the reasoning is
  traceable.

If the next agent disagrees with this recommendation — particularly
on Q1 (live vs pre-index) or Q3 (slice vs full evaluator) —
record the dissent in STATUS.md and proceed accordingly. The
critical thing is that the decision is made deliberately, not
tacitly.

## Open question — RESOLVED

The Q3 framing originally proposed partial-port-with-deferred-list.
Owner reversed: full port. See "Q3 — Full port of `path.evaluate()`"
above for the locked decision. No remaining open questions for
human review at the §5.0 entry-gate level.
