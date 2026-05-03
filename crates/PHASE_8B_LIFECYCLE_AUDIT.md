# Phase 8b — `transformCss` plugin lifecycle audit

> Authoritative classification of the postcss lifecycle hooks for every
> plugin reachable from `packages/css/src/transform.ts:32-100`. Read together
> with `crates/STATUS.md` (`Phase 8b prep` + `Phase 8b scope discovery`),
> `crates/EXECUTION_PLAN.md`, `crates/PARITY_VERSIONS.md`, and the
> "Lifecycle ordering — load-bearing" docblock in
> `crates/css/src/sort.rs:39-61`.
>
> Drift detection follows the CLAUDE.md rule: any divergence between the
> JS source and the existing Rust port is flagged inline with the
> "**DRIFT RISK:**" prefix. The compose agent must NOT silently work
> around such items — they are escalations, not patches.

---

## Postcss lifecycle primer (recap — applies to every plugin below)

Postcss 8 runs a single `process()` invocation as **three rounds** over
the AST in plugin-array order:

1. **Once round.** Every plugin's `Once(root)` hook fires in array order
   before any walking starts. A plugin can mutate the tree freely here.
2. **Walk round.** A single depth-first walk visits each node. At every
   node, every plugin's matching visitor (`Declaration`, `Rule`,
   `AtRule`, `Comment`, `Root`, `RootExit`, `DeclarationExit`,
   `RuleExit`, `AtRuleExit`, `CommentExit`) fires in plugin-array order.
   Mutations performed during the walk re-trigger the index-walk for
   newly-inserted siblings (this is what `decl.replaceWith(nodes)` in
   `expand-shorthands` relies on).
3. **OnceExit round.** Every plugin's `OnceExit(root)` fires in array
   order **after** the walk has fully drained. A plugin can mutate the
   tree freely here, but the walk does not re-fire.

`prepare(result) -> { OnceExit, ... }` is sugar: postcss treats the
returned object as the plugin's hook map, with `prepare` running once
per `process()` invocation just before the lifecycle starts. Functionally
equivalent to declaring those hooks at the top level for ordering
purposes — they slot into the same Once / walk / OnceExit rounds.

The single most common porting bug, per the `sort.rs` docblock, is to
treat the plugin array as "iterate and apply": that fires hooks in the
wrong order whenever any plugin mixes hook types. The Rust composition
**must** interleave by lifecycle round, not by plugin position.

---

## Plugin 1 — `discardDuplicates` (LOCAL)

- **Source:** `packages/css/src/plugins/discard-duplicates.ts:6-27`
- **postcssPlugin name:** `'discard-duplicates'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once(root)` | **YES** (lines 9-25) | Iterates `root.each((node) => …)` collecting top-level `decl` nodes by `prop` into a `Record<string, Declaration[]>`. Then walks the map: for every prop with N occurrences, calls `.remove()` on the first N-1. Effect: keep only the **last** occurrence of each top-level declaration. |
| Per-node visitors | none | — |
| `OnceExit` | none | — |

### Mutation profile

Tree-mutating during `Once` (decl removal). Because removal happens
inside `Once`, before the walk starts, the walk sees the post-mutation
tree.

### Returns to caller

No callback, no result emission — pure tree mutation.

### opts consumption

`discardDuplicates()` takes **no arguments**. No `opts` consumed.

### Cross-plugin ordering hazards

- This is the very first plugin in the array; its `Once` is the first
  thing to run end-to-end. No upstream constraints.
- It only inspects **top-level** decls (siblings of root). Nested rules
  are not de-duped here — that's important because at this point
  `parentOrphanedPseudos` and `postcss-nested` have not yet fired, so
  any decls nested inside rules are untouched.
- **DRIFT RISK / clarification:** the `for (const key in decls)` JS
  iteration order is *insertion order* for string keys (V8 spec). The
  Rust port must use an `IndexMap` or equivalent — not a `HashMap`.
  Although the `.remove()` calls don't reorder visible output (the kept
  declaration is whichever was last in the array, which is independent
  of map iteration order), `for-in` order leaks if anyone ever changes
  the body. Flag in code comment.

---

## Plugin 2 — `discardEmptyRules` (LOCAL)

- **Source:** `packages/css/src/plugins/discard-empty-rules.ts:9-23`
- **postcssPlugin name:** `'discard-empty-rules'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once` | none | — |
| `Declaration(node)` | **YES** (lines 12-21) | If `isValueEmpty(node.value)` (value is literal `'undefined'`, `'null'`, or trims to empty), capture `node.parent`, `node.remove()`, then if the parent is a rule and its `nodes.length === 0`, remove the parent rule too. |
| `OnceExit` | none | — |

### Mutation profile

Mutates during walk: removes the visited declaration and (conditionally)
its parent rule. Parent removal during a walk is supported by postcss's
index-walk (the walker tracks indices), but it does shorten the sibling
list. Because Plugin 2's `Declaration` is the only walk-time visitor for
plugins 1-3, the interleave with later plugin visitors only becomes an
issue once `expand-shorthands` (#6) and `normalize-current-color`
(inside #5) join.

### Returns to caller

No callback, pure mutation.

### opts consumption

No arguments.

### Cross-plugin ordering hazards

- Runs before nesting (#3, #4), so `node.parent` here is whatever the
  parser produced — typically the rule that lexically contained the
  decl. Empty rules created by **later** plugins (e.g. nested unwrapping
  emptying a rule shell) will **not** be cleaned up here, by design,
  because this plugin's walk has already finished.
- The "parent.type === 'rule'" check intentionally excludes at-rules:
  an empty `@media` is not removed here.

---

## Plugin 3 — `parentOrphanedPseudos` (LOCAL)

- **Source:** `packages/css/src/plugins/parent-orphaned-pseudos.ts:21-44`
- **postcssPlugin name:** `'parent-orphened-pseudos'` (note typo
  preserved upstream — JS file misspells "orphaned" → "orphened" in the
  plugin name string. **DRIFT RISK:** any Rust port must replicate the
  typo verbatim; postcss compares plugin names byte-for-byte for
  ordering and result merging.)

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once(root)` | **YES** (lines 24-42) | Calls `root.walkRules`. For each rule, splits `rule.selectors`; for any selector starting with `:` runs `selectorParser` to walk pseudos and prepend a nesting (`&`) selector via `parent.insertBefore`. Reassigns `rule.selectors = …`. |
| Per-node visitors | none | — |
| `OnceExit` | none | — |

### The "Once instead of Rule" comment is load-bearing

Lines 19-20 of the JS file: *"Requires the use of Once over Rule else it
runs into conflicts with the postcss-nested plugin"*. The reason is
ordering: in array index 3 the plugin's `Once` fires **before**
`postcss-nested`'s walk-time `Rule` visitor runs (Plugin 4, see below).
If this plugin used a `Rule` visitor, it would interleave with
`postcss-nested`'s `Rule` visitor at every node, and the order in which
sibling-nesting and pseudo-promotion happen would silently change.

**This is precisely the hazard the sort.rs docblock warns about.** The
Rust composition must keep `parentOrphanedPseudos` strictly in the
Once round, before the walk.

### Mutation profile

Mutates inside `Once`: only rewrites `rule.selectors` strings. No node
addition/removal. Walk indices unaffected.

### Returns to caller

No callback, pure mutation.

### opts consumption

No arguments.

### Cross-plugin ordering hazards

- **Must run before postcss-nested.** This is the explicit upstream
  comment.
- Walks the **entire** tree via `root.walkRules`, so it catches deeply
  nested rules even though it's a Once hook (walkRules is recursive).

---

## Plugin 4 — `postcss-nested@5.0.6` (npm)

- **Source:** `crates/_vendor/postcss-nested-5.0.6/index.js:127-213`
- **postcssPlugin name:** `'postcss-nested'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once` | none | — |
| `Rule(rule, { Rule })` | **YES** (lines 144-211) | For each rule visited during the walk, iterates its child nodes and unwraps nested rules / at-rules / declarations using `selectors()`, `pickDeclarations()`, `pickComment()`, and `atruleChilds()`. Manipulates `rule.raws.semicolon` and may `rule.remove()` the original wrapper if `unwrapped && preserveEmpty !== true && rule.nodes.length === 0`. |
| Per-node visitors (other) | none | — |
| `OnceExit` | none | — |

### Mutation profile

**Heavy walk-time mutation.** Inserts new sibling rules via
`after.after(child)`, `after.after(parent)`, `after.after(nodes)`;
removes the parent rule when emptied; reassigns `child.selectors`;
re-parents via clone + append.

Postcss's index-walk semantics handle the new siblings: they enter the
walk after the current node. This is the entire reason `postcss-nested`
works as a `Rule` visitor instead of `Once` — it relies on the walker
to revisit unwrapped children.

### Returns to caller

No callback, pure mutation.

### opts consumption

Reads its own constructor opts:

- `opts.bubble` — appended to the default `['media', 'supports']`
  (lines 128). transform.ts passes `['container', '-moz-document',
  'layer', 'else', 'when', 'starting-style']` (transform.ts:45-55).
- `opts.unwrap` — appended to the default `['document', 'font-face',
  'keyframes', '-webkit-keyframes', '-moz-keyframes']` (lines 130-139).
  transform.ts passes `['color-profile', 'counter-style',
  'font-palette-values', 'page', 'property']` (transform.ts:56).
- `opts.preserveEmpty` — defaults to falsy. transform.ts does not set
  it.

The `atruleNames(defaults, custom)` helper (lines 113-125) builds a
`{ name: true }` lookup. Custom names with leading `@` are stripped:
`i.replace(/^@/, '')`.

### Cross-plugin ordering hazards

- Fires during the walk round, after all Once hooks (1-3 and 5's inner
  Once chain — see plugin 5) have completed.
- Within the walk, plugin 4's `Rule` visitor will fire **before** any
  Rule visitor from later plugins at each node (none of the remaining
  plugins have a `Rule` visitor; `expand-shorthands` (#6) has only
  `Declaration`; `normalize-current-color` is the same; etc.). So in
  practice plugin 4 has the walk-round Rule slot to itself.
- **DRIFT RISK / Anomaly #1 cross-reference:** `PARITY_VERSIONS.md`
  Anomaly #1 says v5→v6 changed selector merging semantics. The Rust
  port at `crates/postcss-nested` must be the v5 algorithm. The
  bubble/unwrap arrays passed from `transform.ts:45-56` are the v5-era
  workaround config — re-verify they propagate to the Rust plugin
  through the compose layer.

---

## Plugin 5 — `normalizeCSS(opts)` spread (LOCAL composer)

- **Source:** `packages/css/src/plugins/normalize-css.ts:58-81`
- **postcssPlugin names:** the **N child plugins** spread by `...` —
  the composer itself does not wrap them.

This entry is structurally different from every other entry. The JS
spreads `normalizeCSS(opts)` into the postcss array, so the outer
pipeline sees **N flat plugin objects**, not one. Each child has its
own postcss lifecycle hook(s).

### Composition (per `normalize-css.ts` and STATUS.md "Phase 6 BAND ship")

The spread expands to:

1. **The cssnano-preset-default@5.2.14 sub-plugins**, in
   *cssnano-preset-default's source order* (Anomaly #7 in
   PARITY_VERSIONS.md), filtered to those whose `postcssPlugin` name is
   in `BASE_PLUGINS ∪ (optimizeCss ? PROD_PLUGINS : [])`.
2. **`normalizeCurrentColor()`** appended **only when `optimizeCss !==
   false`** (`normalize-css.ts:76-78`).

The 14 cssnano sub-plugins (after filter, in BASE-or-PROD union) are:
`postcss-discard-comments`, `postcss-minify-gradients`,
`postcss-reduce-initial`, `postcss-convert-values`,
`postcss-normalize-url`, `postcss-normalize-positions`,
`postcss-normalize-string`, `postcss-normalize-timing-functions`,
`postcss-minify-params`, `postcss-normalize-unicode`, `postcss-colormin`,
`postcss-ordered-values`, `postcss-minify-selectors`, `postcss-calc`.
**The actual position of each in the outer pipeline is determined by
cssnano-preset-default's source order — not the order in the
BASE_PLUGINS / PROD_PLUGINS arrays.** `crates/cssnano-preset-default`
is the source of truth.

### Hooks (per child plugin)

Verified by grepping `postcssPlugin|OnceExit|Once|prepare|Declaration|
Rule|AtRule` in each plugin's `src/index.js`:

| Plugin | Hooks |
|--------|-------|
| `postcss-discard-comments@5.1.2` | `OnceExit(css, { list })` only |
| `postcss-minify-gradients@5.1.1` | `OnceExit(css)` only |
| `postcss-reduce-initial@5.1.2` | `prepare(result) -> { OnceExit(css) }` |
| `postcss-convert-values@5.1.3` | `OnceExit(css)` only |
| `postcss-normalize-url@5.1.0` | `OnceExit(css)` only |
| `postcss-normalize-positions@5.1.1` | `OnceExit(css)` only |
| `postcss-normalize-string@5.1.0` | `OnceExit(css)` only |
| `postcss-normalize-timing-functions@5.1.0` | `OnceExit(css)` only |
| `postcss-minify-params@5.1.4` | `OnceExit(css)` only |
| `postcss-normalize-unicode@5.1.1` | `prepare(result) -> { OnceExit(css) }` |
| `postcss-colormin@5.3.1` | `prepare(result) -> { OnceExit(css) }` |
| `postcss-ordered-values@5.1.3` | `prepare() -> { OnceExit(css) }` |
| `postcss-minify-selectors@5.2.1` | `OnceExit(css)` only |
| `postcss-calc@8.2.4` | `OnceExit(css, { result })` only |
| `normalize-current-color` (LOCAL) | `Declaration(declaration)` only |

**Net result for the outer pipeline:** the entire `normalizeCSS` spread
contributes exactly **one walk-round visitor** (`normalize-current-color
.Declaration`) plus **N OnceExit hooks** — one per filtered cssnano
sub-plugin. No cssnano sub-plugin contributes to the Once round or to
the walk round.

### Mutation profile

- Per-plugin: each cssnano sub-plugin mutates the tree at OnceExit time
  (decls replaced/removed, comments stripped, etc.). All operate on the
  full root after the walk has drained.
- `normalizeCurrentColor`: mutates `declaration.value` in place inside
  the walk. No node addition/removal, no walk-index disruption.

### Returns to caller

None of these plugins emit anything via callback. They mutate the tree;
their effect is felt downstream by extractStyleSheets → sheets array.

### opts consumption

- `normalizeCSS({ optimizeCss })` reads `opts.optimizeCss` at the
  composer level (lines 59, 63, 76). Default = `true`. The flag only
  affects WHICH cssnano plugins are included and whether
  `normalizeCurrentColor` is appended — **not** the lifecycle hooks of
  any individual plugin.
- Each cssnano sub-plugin is invoked as `creator()` (lines 66-70) — i.e.
  default options. Per Anomaly #8 those defaults must match the pinned
  version exactly.

### Internal lifecycle status (Phase 6 BAND)

Per STATUS.md "Phase 6 BAND ship", `crates/compiled-css/src/plugins/
normalize_css.rs` already implements the inner lifecycle correctly:
runs `normalize-current-color` first as a Declaration walk, then
iterates the preset in source-defined order applying OnceExit. **For
the outer transform.ts pipeline composition**, this internal correctness
is necessary but not sufficient — the outer composition must still
correctly interleave the spread's contributions with the rest of the
pipeline. See "Composition recipe" below.

### Cross-plugin ordering hazards

- **DRIFT RISK / call-out:** the existing Phase 6 BAND `normalize_css.rs`
  treats the spread as an *all-OnceExit-at-once* unit (since 13/14
  cssnano plugins are OnceExit and `normalize-current-color` is the
  only walker). When composing the OUTER pipeline, the compose agent
  MUST NOT lift `normalize_css` as a single OnceExit boundary — that
  would put cssnano OnceExits *before* later plugins' (atomicifyRules,
  increaseSpecificity, extractStyleSheets) OnceExits. They actually
  belong in array order. The N OnceExits must each take their array-
  position slot in the global OnceExit round.
- The `normalizeCurrentColor.Declaration` visitor must fire **at array
  index ≈ 4+13 (the position of normalize-current-color in the spread)
  during the walk round** — NOT before plugin 4's `Rule` visitor at
  each node. The walk visits each node once; at that node, all visitors
  for that node-type fire in array order. Since `normalizeCurrentColor
  .Declaration` and `expandShorthands.Declaration` (plugin 6) are both
  Declaration visitors, **`normalizeCurrentColor` must fire first per
  node** because it sits earlier in the array.

---

## Plugin 6 — `expandShorthands` (LOCAL)

- **Source:** `packages/css/src/plugins/expand-shorthands/index.ts:71-108`
- **postcssPlugin name:** `'expand-shorthands'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once` | none | — |
| `Declaration(decl)` | **YES** (lines 74-106) | Looks up `shorthands[decl.prop]` (`margin`, `padding`, `place-content`, `place-items`, `place-self`, `overflow`, `flex`, `flex-flow`, `outline`, `text-decoration`, `background`). If no entry → return early. Else parses `decl.value` with `postcss-values-parser`, bails if any node is `func && isVar`. Calls the conversion fn → array of `{ prop, value }` longforms. If single longform with `prop === undefined` → return (preserve original). Otherwise clones decl per longform (`decl.clone({ ...val, value: \`${val.value}\` })`) and `decl.replaceWith(nodes)`. |
| `OnceExit` | none | — |

### Mutation profile

**Walk-time replaceWith.** Each matching decl is replaced with N new
sibling decls. Postcss's index-walk semantics will visit the new
siblings as the walk progresses. **The new decls themselves match
shorthand props (e.g. `margin-top`)? No — they are longforms (per the
expansion table comments) which are NOT in the `shorthands` keys, so
they don't recursively re-match this plugin.** No infinite loop.

### Returns to caller

No callback, pure mutation.

### opts consumption

`expandShorthands()` takes **no arguments** (the function literally
ignores any caller-passed object — there is no parameter). No `opts`
consumed.

### Cross-plugin ordering hazards

- Within walk: fires after `normalizeCurrentColor.Declaration` at each
  decl node (because position in the array is later). The expansion
  reads `decl.value` — `normalizeCurrentColor` may have just rewritten
  `currentcolor` → `currentColor` in that value. Order matters for any
  `decl.value` that contains the literal string `currentcolor`/
  `current-color`, but in practice no shorthand expansion key reads
  color tokens specifically. Still, the order is `normalizeCurrentColor`
  THEN `expandShorthands`.
- Throws `'Longform properties were not returned!'` if a conversion fn
  returns falsy. This propagates up through postcss's run loop — the
  outer try/catch in `transformCss` wraps it in `createError(...)`.

---

## Plugin 7 — `atomicifyRules` (LOCAL)

- **Source:** `packages/css/src/plugins/atomicify-rules.ts:282-318`
- **postcssPlugin name:** `'atomicify-rules'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once` | none | — |
| Per-node visitors | none | — |
| `OnceExit(root)` | **YES** (lines 291-316) | Iterates `root.each((node) => …)`. For each top-level node: <br>• `atrule` → if `canAtomicifyAtRule` returns true, `node.replaceWith(atomicifyAtRule(node, opts))`. <br>• `rule` → `node.replaceWith(atomicifyRule(node, opts))` (returns array of new rules). <br>• `decl` → `node.replaceWith(atomicifyDecl(node, opts))`. <br>• `comment` → `node.remove()`. <br>The recursive atomicify helpers throw on nested `rule` (forces normalization upstream) and on unknown/forbidden at-rules. |

### Mutation profile

Mutates at OnceExit time only. Operates on the post-walk tree. Calls
`opts.callback(fullClassName)` from `buildAtomicSelector` (line 113) for
every selector × atomicClassName produced.

### Returns to caller

**Yes — via `opts.callback`.** transform.ts:62 wires
`callback: (className: string) => classNames.push(className)`. Every
generated atomic class name is pushed during OnceExit. **The Rust
NAPI bridge must surface this as an output of transformCss.**

### opts consumption

```ts
opts.classNameCompressionMap?: Record<string, string>
opts.callback?: (className: string) => void
opts.classHashPrefix?: string
// internal during recursion: selectors, atRule, parentNode
```

The factory throws on construction (lines 283-287) if
`classHashPrefix` is set and fails the
`/^[a-zA-Z\-_]+[a-zA-Z\-_0-9]*$/` regex. This validation runs at plugin
creation time, NOT at OnceExit time — i.e. it fires before postcss
even starts processing.

**DRIFT RISK / hash entry point:** `atomicClassName` (lines 38-47)
calls `hash()` from `@compiled/utils` (see CLAUDE.md "Never" list —
that package is immutable). The Rust port must wire to a byte-identical
hash impl for class-name byte parity. `crates/compiled-css` should
already cover this — re-verify before Phase 8b ships.

### Cross-plugin ordering hazards

- Runs at OnceExit. Position 7 (counting `normalizeCSS`'s spread as one
  block — but **see Plugin 5's hazard call-out: the spread expands into
  N positions**, so atomicify's actual array-index slot depends on
  spread length).
- **Critical ordering:** atomicify's OnceExit replaces top-level `rule`
  nodes with `Rule[]` arrays via `replaceWith`. Anything that runs at
  OnceExit *after* atomicify in the array sees the atomicized tree.
  In particular: `increaseSpecificity` (#8), `sortAtomicStyleSheet`
  (#9), `autoprefixer` (#10), `postcss-normalize-whitespace` (#11),
  `extractStyleSheets` (#12) all run after — perfect, since they need
  the atomic shape.
- The 14 cssnano sub-plugins inside `normalizeCSS` run at OnceExit
  **before** atomicify (because they're earlier in the array). So
  cssnano operates on un-atomicized CSS. This matters for selectors
  (cssnano-minify-selectors works on the raw selector strings) and for
  declarations (postcss-ordered-values reorders decl values pre-
  atomic).

---

## Plugin 8 — `increaseSpecificity` (LOCAL, conditional)

- **Source:** `packages/css/src/plugins/increase-specificity.ts:24-40`
- **postcssPlugin name:** `'increase-specificity'`
- **Conditional gate:** `transform.ts:65` — `...(opts.increaseSpecificity
  ? [increaseSpecificity()] : [])`. Runs only when
  `opts.increaseSpecificity` is truthy. The Rust composition must
  conditionally include this plugin in the OnceExit round.

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once` | none | — |
| Per-node visitors | none | — |
| `OnceExit(root)` | **YES** (lines 27-38) | `root.walkRules((rule) => { rule.selectors = rule.selectors.map(selector => …) })`. For each selector: if it contains the substring `'._'` (i.e. a Compiled-generated atomic class), runs `parser.astSync(selector).toString()`. The shared `parser` (lines 5-11) walks classes and inserts a `:not(#\#)` pseudo via `parent.insertAfter(node, pseudo({ value: INCREASE_SPECIFICITY_SELECTOR }))`. |

`INCREASE_SPECIFICITY_SELECTOR` lives in `@compiled/utils` (immutable
package) — Rust must use the same constant string.

### Mutation profile

Mutates at OnceExit: rewrites `rule.selectors`. No node addition/
removal, only string substitution.

### Returns to caller

No callback, pure mutation.

### opts consumption

`increaseSpecificity()` takes **no arguments**. The conditional gate
itself reads `opts.increaseSpecificity` from the outer `TransformOpts`.

### Cross-plugin ordering hazards

- **MUST run after `atomicifyRules`** (the JS comment lines 17 says so:
  *"This rule should run after CSS declarations have been atomicized"*).
  Both are OnceExit; OnceExit fires in array order; atomicify is at
  pos 7, increaseSpecificity at pos 8 — correct.
- **MUST run before `sortAtomicStyleSheet`** so the sort sees final
  selectors? Not actually — sortAtomicStyleSheet uses `Once`, not
  OnceExit, so it actually runs **before** any OnceExit (including
  atomicify's). See Plugin 9.
- Selector matching uses substring `'._'` to filter Compiled classes
  (line 30). This is a fragile lexical match — any pre-existing rule
  whose selector contains `._` will be rewritten too. **DRIFT RISK:**
  the Rust port must use the exact same substring check (no regex
  optimization, no anchoring).

---

## Plugin 9 — `sortAtomicStyleSheet` (LOCAL)

- **Source:** `packages/css/src/plugins/sort-atomic-style-sheet.ts:43-114`
- **postcssPlugin name:** `'sort-atomic-style-sheet'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once(root)` | **YES** (lines 52-112) | Partitions root.nodes into `catchAll`, `rules`, `atRules` buckets via `root.each`. Optionally calls `sortShorthandDeclarations` on each bucket (when `sortShorthandEnabled`). Sorts rules via `sortPseudoSelectors`. Optionally sorts atRules via `atRules.sort(sortAtRules)` (when `sortAtRulesEnabled`). For each at-rule, recursively `sortAtRulePseudoSelectors`. Reassigns `root.nodes = [...catchAll, ...rules, ...atRules.map(a => a.node)]`. |
| Per-node visitors | none | — |
| `OnceExit` | none | — |

The doc comment lines 41-42: *"Using Once due to the catchAll
behaviour"*.

### **CRITICAL ORDERING HAZARD — confirms the audit's main thesis**

This plugin's `Once` fires in the **Once round** at the very start of
processing — i.e. before plugins 4-8's walk visitors and OnceExits.
**That means sortAtomicStyleSheet sorts the un-atomicized, un-nested
tree.** That makes no semantic sense in isolation — until you realize
that postcss-nested, atomicifyRules, and the 14 cssnano OnceExits all
mutate the tree AFTER sort runs, undoing the sort.

This is **the same drift hazard that bit `sort.ts`** per
`crates/css/src/sort.rs:39-61`. In `sort()`, the array is `[discardDup,
mergeDupAtRules, sortAtomic]` — the naive composition would put sort
last, but the actual postcss order puts sort FIRST (Once round) and
discardDup LAST (OnceExit round). The compose agent must not be
fooled by array position when `Once` is in the mix.

**Ahhh, but wait** — let me re-read transform.ts carefully. Yes,
`sortAtomicStyleSheet` is at array index 9 (in the 12-plugin sequence).
In postcss's lifecycle, its `Once` fires in the global Once round in
array order — i.e. AFTER `discardDuplicates.Once` (pos 1),
`parentOrphanedPseudos.Once` (pos 3), but BEFORE the walk and BEFORE
any OnceExit.

So the ordering is:

```
ONCE ROUND (in plugin-array order):
  1. discardDuplicates.Once          (de-dup top-level decls)
  3. parentOrphanedPseudos.Once       (rewrite selectors)
  9. sortAtomicStyleSheet.Once        (sort non-atomic root)
WALK ROUND:
  4. postcss-nested.Rule              (unwrap nested rules)
  5.normalize-current-color.Declaration (currentcolor canonicalization)
  2. discardEmptyRules.Declaration    (drop empty values)
  6. expandShorthands.Declaration     (expand shorthands)
  — at each node these fire in array order: 2,4,5,6
ONCEEXIT ROUND (in plugin-array order):
  5. (14 cssnano sub-plugins)         (cssnano normalization)
  7. atomicifyRules.OnceExit          (build atomic classes — emits classNames)
  8. increaseSpecificity.OnceExit     (conditional :not(#\#))
  10. autoprefixer.OnceExit           (vendor prefixes)
  11. postcss-normalize-whitespace.OnceExit  (raws cleanup)
  12. extractStyleSheets.OnceExit     (emit sheets via callback)
```

Wait — that means sort runs **before atomicify**? That breaks every
mental model. Let me sanity-check by reading the JS doc comment again
("Only top level CSS rules will be sorted") and looking at what
`sortPseudoSelectors` actually requires…

The plugin partitions `root.each((node) => …)` and reassigns
`root.nodes = [...catchAll, ...rules, ...atRules.map(a => a.node)]`.
After this Once finishes, the walk starts. During the walk,
postcss-nested may unwrap rules (creating new top-level siblings),
expand-shorthands may replace decls. After the walk,
**atomicifyRules.OnceExit replaces every top-level rule with a Rule
or Rule[] from `atomicifyRule`**. So atomicify creates a new shape
that the earlier sort has not seen.

**This means the actual sort that ships in the output bytes is
performed implicitly by atomicify's deterministic generation order plus
the ordering of cssnano's OnceExit operations.** sortAtomicStyleSheet
running in the Once round only sorts the pre-atomicized input — its
visible output is "tree shape that the rest of the pipeline assumes
is the start state."

**DRIFT RISK / CRITICAL — flag this loudly:** I can imagine a future
maintainer reading the array `[…, sortAtomicStyleSheet, autoprefixer,
…]` and assuming sort runs after the atomic transform. It does not.
This is **exactly the load-bearing detail** that the sort.rs docblock
warns about, replicated in transform.ts.

The Rust composition must call the local `sort_atomic_style_sheet`
function in the Once round (right after `parent_orphaned_pseudos`),
NOT after atomicify. Putting it after atomicify silently produces
different byte output even though the array order makes "after"
look correct.

### Mutation profile

Mutates `root.nodes` wholesale during Once. Also walks at-rules
recursively to sort their child pseudos.

### Returns to caller

No callback, pure mutation. **Note:** the user's question speculated it
"emits via mutation" — yes, it mutates root.nodes; no callback.

### opts consumption

```ts
config.sortAtRulesEnabled?: boolean    // default true
config.sortShorthandEnabled?: boolean  // default true
```

Defaults applied via `?? true` (lines 47-48).

### Cross-plugin ordering hazards

See the entire critical-ordering-hazard discussion above. The Rust
composition in transform.rs **must** invoke this in the Once round, in
array-order with the other Once hooks (after #1 and #3, since those
are earlier in the array). The existing `sort.rs` Rust port for the
`sort()` function already gets this right (see `sort.rs:65-71` —
`sortAtomicStyleSheet` is called first); transform.rs must follow the
same pattern.

---

## Plugin 10 — `autoprefixer@10.4.14` (npm, conditional)

- **Source:** `crates/_vendor/autoprefixer-10.4.14/package/lib/
  autoprefixer.js:116-147`
- **postcssPlugin name:** `'autoprefixer'`
- **Conditional gate:** `transform.ts:70` — `...(process.env.AUTOPREFIXER
  === 'off' ? [] : [autoprefixer()])`. Disabled only when the env var
  AUTOPREFIXER is exactly the string `'off'`. The Rust composition
  must read the same env var via `std::env::var("AUTOPREFIXER")`.

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `prepare(result)` | **YES** (lines 119-136) | Loads/caches a `Prefixes` engine via `loadPrefixes({ from: result.opts.from, env: options.env })`. Returns an inner hook map. |
| `OnceExit(root)` (returned by prepare) | **YES** (lines 126-134) | Calls `timeCapsule(result, prefixes)` (warning emit, no tree mutation). If `options.remove !== false`, runs `prefixes.processor.remove(root, result)`. If `options.add !== false`, runs `prefixes.processor.add(root, result)`. |
| Top-level `Once`/per-node | none | — |

`prepare()` is postcss sugar — for ordering purposes this slots into
the OnceExit round.

### Mutation profile

OnceExit-time. The `processor.add`/`remove` operations rewrite vendor
prefixes across rules and declarations. No callback.

### Returns to caller

No callback (warnings go through `result.warn` which is postcss's
warning channel — not on the transformCss return shape).

### opts consumption

`autoprefixer()` is called with no arguments in transform.ts:70.
Defaults:
- `options.remove` defaults to true (i.e. removal is enabled).
- `options.add` defaults to true.
- `options.overrideBrowserslist` undefined → uses browserslist resolution.
- `options.env`, `options.stats`, `options.ignoreUnknownVersions`
  undefined.

`brwlstOpts = { ignoreUnknownVersions, stats, env }` is passed to the
Browsers constructor — all undefined → defaults.

### Cross-plugin ordering hazards

- Runs at OnceExit, after atomicify and increaseSpecificity. Operates
  on the atomic shape.
- Runs **before** `postcss-normalize-whitespace` (#11). That order
  matters: autoprefixer adds new declarations, and whitespace
  normalization will then strip the raws on those new decls. If the
  order were swapped, the inserted prefixed decls would have unstripped
  whitespace.
- **DRIFT RISK / Anomaly #3 cross-reference:** caniuse-lite is pinned
  at `1.0.30001766`. The Rust autoprefixer port at `crates/autoprefixer`
  must consume exactly that data snapshot. Verify via
  `crates/caniuse-db` build.rs output — already done per Phase 7.
- **DRIFT RISK / Anomaly #4 cross-reference:** browserslist 4.24.2
  defaults must match. Phase 8b composition should pin
  `BROWSERSLIST=defaults` (or whatever the AFM consumer uses) to avoid
  per-machine browserslist resolution drift. STATUS.md "Browserslist
  parity" section already addressed this for the cssnano band — apply
  the same pin to the full pipeline.

---

## Plugin 11 — `postcss-normalize-whitespace@5.1.1` (npm)

- **Source:** `node_modules/.bun/postcss-normalize-whitespace@5.1.1+…/
  node_modules/postcss-normalize-whitespace/src/index.js:46-105` (also
  vendored at `crates/_vendor/postcss-normalize-whitespace-5.1.1/` per
  PARITY_VERSIONS.md, but I confirmed the bun-installed copy is
  identical via the resolved version string)
- **postcssPlugin name:** `'postcss-normalize-whitespace'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once`/per-node | none | — |
| `OnceExit(css)` | **YES** (lines 50-103) | Calls `css.walk((node) => …)` over the entire tree. For decl/rule/atrule with `node.raws.before`, strips whitespace via regex. For decl: rewrites `node.raws.important = '!important'`, normalizes `\9` IE hack, parses+walks value via `valueParser` to call `reduceWhitespaces` (cached by string), handles `--` custom prop empty-value `' '` rule, strips semicolons in `raws.before` for non-rule prevs, sets `raws.between = ':'`, `raws.semicolon = false`. For rule/atrule: sets `raws.between = ''`, `raws.after = ''`, `raws.semicolon = false`. Final: `css.raws.after = ''`. |

### Mutation profile

Pure raws/value mutation at OnceExit. Walks the entire tree but does
not add/remove nodes. The `Map`-based value cache (line 51) is local
to one OnceExit invocation — Rust port should use a `HashMap<String,
String>` reset per call (NOT a global lazy_static).

### Returns to caller

No callback, pure mutation.

### opts consumption

Takes **no arguments** (the upstream factory is `pluginCreator()`).
transform.ts:71 calls `whitespace()` with no args.

### Cross-plugin ordering hazards

- Runs after autoprefixer (#10) so it can normalize the raws on the
  newly-inserted prefixed decls.
- Runs **before** `extractStyleSheets` (#12) so the extracted sheet
  strings have whitespace already normalized. **This ordering is the
  whole reason the output bytes are tight.** Swapping these two would
  emit pre-normalize whitespace into sheets.
- The `valueParser` cache uses pre-normalize key (`const value =
  node.value` *before* normalization). Rust port must replicate this
  exactly to keep cache hit/miss byte-equivalent.
- **DRIFT RISK / Anomaly #2 cross-reference:** v5 vs v4/v6 differ. The
  Rust port at `crates/postcss-normalize-whitespace` must be the v5
  algorithm.

---

## Plugin 12 — `extractStyleSheets` (LOCAL)

- **Source:** `packages/css/src/plugins/extract-stylesheets.ts:6-15`
- **postcssPlugin name:** `'extract-style-sheets'`

### Hooks

| Hook | Present? | What it does |
|------|----------|--------------|
| `Once`/per-node | none | — |
| `OnceExit(root)` | **YES** (lines 9-13) | `root.each((node) => opts?.callback(node.toString()))`. For every top-level node (rule, atrule, decl, comment) calls the callback with the stringified form of that single node. **Does not mutate.** |

### Mutation profile

**No mutation.** Read-only OnceExit.

### Returns to caller

**Yes — via `opts.callback`.** transform.ts:72 wires
`callback: (sheet: string) => sheets.push(sheet)`. Each top-level node
becomes one entry in `sheets`. **The Rust NAPI bridge must surface
this list as the `sheets` field of the TransformResult.**

The trailing `result.css;` access on transform.ts:78 is a no-op-looking
property read whose only purpose is to force postcss to materialize
the result — touching `.css` triggers the lazy stringify, which is what
makes all OnceExit hooks fire. (postcss processing is lazy; without
some access, OnceExits would never run.) **DRIFT RISK:** the Rust
composition must explicitly drive the stringify (or at minimum, the
OnceExit round) — there is no lazy postcss equivalent in Rust, so this
is moot for the Rust port, but the comment is here for completeness.

### opts consumption

```ts
opts?: { callback: (sheet: string) => void }
```

The factory's `?` makes opts optional but the plugin checks `opts?.
callback` before invoking — so passing no opts produces a no-op
plugin.

### Cross-plugin ordering hazards

- Runs LAST in the OnceExit round. Sees the fully-processed tree.
- Stringify happens here (`node.toString()`) — every prior plugin's
  raws/value mutations crystallize into bytes at this moment.
- **The order of `sheets` in the output is determined by `root.each`
  order**, which is post-everything-mutates root.nodes order. The
  earlier `sortAtomicStyleSheet.Once` partitioned root into
  `[catchAll, rules, atRules]`, but then nesting/atomicify rebuilt the
  tree, and cssnano possibly removed/reordered. The actual extraction
  order is whatever the OnceExit-round mutations end up producing.
  This is implicit, version-stable, and load-bearing for hash
  determinism.

---

## Composition recipe — round-by-round execution order

This is the canonical order the Rust port must reproduce. Each round
fires its hooks in plugin-array order; the spread of `normalizeCSS`
expands inline. Conditional plugins (8, 10) are included or skipped
based on opts.

### Plugin-array layout (after `normalizeCSS` spread expansion)

For the `optimizeCss = true` case (default), the array `transform.ts`
hands to `postcss(...)` is — listing each filtered cssnano sub-plugin
in cssnano-preset-default's source order (per Anomaly #7):

```
INDEX  PLUGIN                          HOOK SET
   1   discardDuplicates               Once
   2   discardEmptyRules               Declaration
   3   parentOrphanedPseudos           Once
   4   postcss-nested                  Rule
   5a  postcss-discard-comments        OnceExit
   5b  postcss-minify-gradients        OnceExit
   5c  postcss-reduce-initial          prepare→OnceExit
   5d  postcss-convert-values          OnceExit
   5e  postcss-normalize-url           OnceExit
   5f  postcss-normalize-positions     OnceExit
   5g  postcss-normalize-string        OnceExit
   5h  postcss-normalize-timing-fns    OnceExit
   5i  postcss-minify-params           OnceExit
   5j  postcss-normalize-unicode       prepare→OnceExit
   5k  postcss-colormin                prepare→OnceExit
   5l  postcss-ordered-values          prepare→OnceExit
   5m  postcss-minify-selectors        OnceExit
   5n  postcss-calc                    OnceExit
   5o  normalize-current-color         Declaration
   6   expandShorthands                Declaration
   7   atomicifyRules                  OnceExit  (callback → classNames)
   8   increaseSpecificity             OnceExit  (only if opts.increaseSpecificity)
   9   sortAtomicStyleSheet            Once
  10   autoprefixer                    prepare→OnceExit  (only if env AUTOPREFIXER != 'off')
  11   postcss-normalize-whitespace    OnceExit
  12   extractStyleSheets              OnceExit  (callback → sheets)
```

The 5a–5n ordering is **cssnano-preset-default's source order**, NOT
the order in `BASE_PLUGINS`/`PROD_PLUGINS`. The exact source-order
sequence is dictated by `crates/cssnano-preset-default/src/lib.rs`'s
`default_preset()` function (the Phase 6h port already encoded this
1:1 with `cssnano-preset-default@5.2.14`'s `src/index.js`). Do not
re-derive it from `normalize-css.ts`.

For the `optimizeCss = false` case, the spread reduces to `5i`
(postcss-minify-params) and `5m` (postcss-minify-selectors) only
(BASE_PLUGINS), and `normalize-current-color` is **not** appended.

### Round 1 — Once round (plugin-array order)

```
discardDuplicates.Once               // top-level decl dedup
parentOrphanedPseudos.Once           // pseudo→nested selector rewrite
sortAtomicStyleSheet.Once            // catchAll/rules/atRules partition + sort + reassign root.nodes
```

Three Once hooks. They fire **before any walk and before any
OnceExit**, in that array order.

> **Critical:** `sortAtomicStyleSheet.Once` must fire here, in the Once
> round — NOT later just because it sits at array index 9. This is the
> exact same drift hazard documented in `crates/css/src/sort.rs:39-61`.
> See Plugin 9 above.

### Round 2 — Walk round (single DFS, visitors fire per node in array order)

At each visited node (depth-first from root), the following visitors
fire in array order if their target type matches:

```
At each Declaration node:
   discardEmptyRules.Declaration       (#2)        [removes node + maybe parent]
   normalize-current-color.Declaration (#5o)       [in-place value rewrite]
   expandShorthands.Declaration        (#6)        [decl.replaceWith(N decls)]

At each Rule node:
   postcss-nested.Rule                 (#4)        [unwraps nested children]
```

There are **no** `AtRule`, `Comment`, or `Root` visitors anywhere in
the pipeline. The walk-round visitor traffic is exclusively
Declaration (3 plugins) + Rule (1 plugin).

Mutation re-entrancy:
- `discardEmptyRules` removes the visited decl and possibly its parent
  rule. Postcss adjusts walk indices.
- `normalize-current-color` is in-place; no walk impact.
- `expandShorthands` replaces a decl with N siblings. The new siblings
  enter the walk; they are NOT shorthand props, so they don't re-match
  this plugin.
- `postcss-nested` does heavy `after.after(...)` insertions. The walk
  resumes on the new siblings.

### Round 3 — OnceExit round (plugin-array order)

```
postcss-discard-comments.OnceExit          (#5a)
postcss-minify-gradients.OnceExit          (#5b)
postcss-reduce-initial.OnceExit            (#5c via prepare)
postcss-convert-values.OnceExit            (#5d)
postcss-normalize-url.OnceExit             (#5e)
postcss-normalize-positions.OnceExit       (#5f)
postcss-normalize-string.OnceExit          (#5g)
postcss-normalize-timing-functions.OnceExit (#5h)
postcss-minify-params.OnceExit             (#5i)
postcss-normalize-unicode.OnceExit         (#5j via prepare)
postcss-colormin.OnceExit                  (#5k via prepare)
postcss-ordered-values.OnceExit            (#5l via prepare)
postcss-minify-selectors.OnceExit          (#5m)
postcss-calc.OnceExit                      (#5n)
atomicifyRules.OnceExit                    (#7) — emits classNames via callback
increaseSpecificity.OnceExit               (#8, conditional)
autoprefixer.OnceExit                      (#10, conditional, via prepare)
postcss-normalize-whitespace.OnceExit      (#11)
extractStyleSheets.OnceExit                (#12) — emits sheets via callback
```

Note that `atomicifyRules.OnceExit` runs AFTER all 14 cssnano OnceExits.
This means cssnano operates on un-atomicized CSS. After cssnano runs
and atomicify runs, increaseSpecificity (conditional) sees atomic
classes; autoprefixer (conditional) inserts prefix decls; whitespace
normalization tightens raws; finally extractStyleSheets stringifies
each top-level node into one entry of the output `sheets` array.

The exact 5a–5n sub-plugin labels above are illustrative —
**the compose agent must consult `crates/cssnano-preset-default/src/
lib.rs::default_preset()` for the authoritative source-order list**
and slot only those whose `postcssPlugin` name matches BASE ∪ PROD
filter into the OnceExit round in that order. This filter is
implemented in `crates/compiled-css/src/plugins/normalize_css.rs`
already — re-use that filter logic; do NOT re-derive it.

---

## Cross-cutting hazards summary (read this before writing transform.rs)

1. **Once-round vs OnceExit-round confusion is the #1 drift risk.**
   `sortAtomicStyleSheet` is a `Once` plugin sitting at array index 9.
   Naive composition would call it after atomicify; postcss calls it
   first. This is identical to the `sort.rs` hazard.

2. **`normalizeCSS` is a SPREAD, not a unit.** Its 14 cssnano sub-
   plugin OnceExits each occupy their own slot in the global OnceExit
   round, in cssnano-preset-default's source order. `normalize-current-
   color`'s Declaration visitor occupies its own slot in the global
   walk round. **Do not lift the spread as a single OnceExit
   boundary** — that puts cssnano OnceExits before atomicify's
   OnceExit, which is the JS order, but it also forces all 14 to fire
   contiguously, which is also the JS order — so this might actually
   be safe. Re-verify carefully:
   - JS: cssnano sub-plugins appear at array positions 5a..5n.
     atomicify is at position 7. Therefore cssnano OnceExits all fire
     before atomicify's OnceExit. ✓
   - The spread's contiguity is preserved.
   - **Exception:** `normalize-current-color.Declaration` must fire in
     the WALK round, not lifted into a contiguous spread block.
     Concretely: at every Declaration node, the per-node visitor order
     is: `discardEmptyRules` (pos 2) → `normalize-current-color` (pos
     5o) → `expandShorthands` (pos 6). Lifting the entire normalizeCSS
     into one OnceExit unit erases this interleave and produces drift.
   - **Therefore the safe lift is "all 14 cssnano OnceExits as one
     contiguous block within the OnceExit round" only.** The
     Declaration visitor stays in the walk round at its array-position
     slot. The existing Phase 6 BAND `normalize_css(root, opts)`
     function in Rust appears to do BOTH inside one call (Declaration
     walk + cssnano OnceExits) — that's structurally fine for
     standalone use, but **for transform.rs composition the compose
     agent must NOT call it as a single function**. Decompose into:
     - one walk-round step that runs `normalize-current-color`'s
       Declaration visitor logic (or interleave it manually with
       `discardEmptyRules` + `expandShorthands` + `postcss-nested`'s
       Rule visitor in a single hand-rolled walk),
     - one OnceExit-round block that calls each of the 14 cssnano sub-
       plugin OnceExit functions in source order.
   - **DRIFT RISK / FLAG TO COMPOSE AGENT:** if `compiled-css::plugins
     ::normalize_css::normalize_css(...)` is called as-is from
     transform.rs, the result will diverge from JS because (a) it
     packages the Declaration visitor and the OnceExit batch into one
     call, and (b) it runs both before any of the rest of transform.rs's
     pipeline can interleave its own visitors / OnceExits at array-
     adjacent positions. The Rust composition must invoke the
     individual cssnano sub-plugin entry points one-by-one in source
     order during the global OnceExit round, and the
     `normalize_current_color` Declaration logic during the global
     walk round.

3. **Walk-round visitor interleaving is per-node, not per-plugin.**
   At each Declaration node, three visitors fire in array order. A
   naive Rust composition might call `discardEmptyRules` over the
   entire tree, then `normalize-current-color` over the entire tree,
   then `expandShorthands` over the entire tree. **That is wrong.**
   Postcss does ONE walk; at each node, all matching visitors fire
   before moving to the next node. The order of subtree-mutation
   effects (e.g. `expandShorthands` replacing a decl with N siblings;
   the next decl visited is one of those N) depends on per-node
   interleave. The Rust composition should mimic postcss's walk by
   running a single DFS and at each node firing the relevant visitors
   in array order — same pattern as `sort.rs`'s
   `mergeDuplicateAtRules.visit` but extended.

4. **Two callback emissions; both must be plumbed to the NAPI return.**
   - `atomicifyRules` (pos 7) emits class names via
     `opts.callback(className)` during its OnceExit. Multiple emits
     per call. transform.ts wraps with `unique(classNames)` (line 82)
     before returning. **Rust must apply the same dedup.**
   - `extractStyleSheets` (pos 12) emits one sheet per top-level node
     via `opts.callback(sheet)` during its OnceExit. transform.ts
     returns the raw `sheets` array (no unique). **Rust must NOT dedup
     sheets.**

5. **Conditional plugins.**
   - Plugin 8 `increaseSpecificity` only runs when
     `opts.increaseSpecificity` is truthy.
   - Plugin 10 `autoprefixer` only runs when
     `process.env.AUTOPREFIXER !== 'off'`. Rust must read the env var
     identically (string equality vs `'off'`).

6. **Error path.** transform.ts wraps all plugin processing in a
   try/catch (lines 39-99). On any thrown error, it constructs a
   `createError('css', 'Unhandled exception')(message)` with the input
   CSS embedded in the error string (lines 86-98). The Rust port must
   replicate the message format byte-for-byte if any consumer parses
   error strings (low likelihood, but flagged).

7. **`result.css` access.** transform.ts:78 reads `result.css` to
   force lazy stringify. In Rust this is moot — the composition is
   eager — but it is a reminder that the OnceExit round MUST be
   driven; nothing fires it for free.

8. **Plugin name typo.** `parent-orphened-pseudos` — typo preserved
   upstream (lines 23, 5). Rust must replicate the typo if any
   `postcssPlugin` name comparison is on the hashing path (it is
   not directly, but result.warn / processor namespacing may surface
   it).

9. **Hash impl.** `atomicifyRules` calls `hash()` from
   `@compiled/utils`. That package is immutable per CLAUDE.md.
   Phase 8b assumes `crates/utils` (or wherever the hash is ported)
   already byte-matches. Re-verify.

10. **Browserslist pin.** `autoprefixer` and the 5 browserslist-aware
    cssnano sub-plugins (`postcss-colormin`, `postcss-minify-params`,
    `postcss-reduce-initial`, `postcss-convert-values`,
    `postcss-normalize-unicode`) all need a stable browserslist
    resolution. STATUS.md "Phase 6 BAND ship → Browserslist parity"
    section already pins `BROWSERSLIST=chrome 100` for the cssnano
    band corpus run. Phase 8b should pin the same env var (or
    whatever AFM uses) for both engines during parity testing.

11. **postcss-nested bubble/unwrap config plumbing.** transform.ts:45-
    56 sets specific bubble and unwrap arrays. Rust composition must
    pass these through to `crates/postcss-nested`'s entry function.
    The default `['media', 'supports']` for bubble and `['document',
    'font-face', 'keyframes', '-webkit-keyframes', '-moz-keyframes']`
    for unwrap are added inside the plugin via `atruleNames(defaults,
    custom)`. The Rust port must replicate the merge logic, including
    the `i.replace(/^@/, '')` strip on custom names (none of the
    transform.ts entries currently start with `@`, but that's
    incidental).

---

## Open questions for the compose agent

1. **Does `crates/compiled-css/src/plugins/normalize_css.rs::
   normalize_css(...)` expose the cssnano sub-plugin OnceExits and the
   `normalize_current_color` Declaration visitor as separately callable
   entry points?** If not, decomposing the function (without modifying
   the immutable `packages/css/src/plugins/normalize-css.ts`) is a
   pre-req for the Phase 8b composition to be lifecycle-correct.
   FLAG to compose agent: read the function and confirm.

2. **Is `crates/postcss-nested` callable as a `Rule` walk-time visitor
   (i.e. exposing a per-Rule processing function), or only as a "run
   over root" function?** If only the latter, the Rust composition will
   need to treat `postcss-nested.Rule` as a degenerate per-node walk
   that visits every rule itself — not a single OnceExit-style sweep,
   because the JS plugin relies on postcss's index-walk for
   re-entrancy. Verify against `crates/postcss-nested/src/lib.rs`.

3. **Is `crates/autoprefixer` callable with the same `(root, result)`
   pair semantics, including caching and env reads?** Phase 7 closed
   per `crates/autoprefixer/AGENT_6_DONE.md`; verify the public API
   matches what transform.rs needs.

4. **Does `crates/postcss-normalize-whitespace` reset its valueParser
   cache per call?** Per the JS source line 51 (`const cache = new
   Map()` inside the OnceExit body), the cache is per-invocation. Rust
   port must match — a global cache would produce different bytes on
   the second invocation if the value-string population differed.

5. **Does the Rust postcss-core support `replaceWith(nodes)` in a
   walk-time Declaration visitor, with new siblings re-entering the
   walk?** This is non-negotiable for `expandShorthands` to work.
   Verify against `crates/postcss-core`.

These are not blockers — just items the compose agent must verify
before assuming the parts compose correctly.
