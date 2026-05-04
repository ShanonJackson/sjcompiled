# `State` mutation enumeration — `packages/babel-plugin`

> **Phase 0 architecture-discovery artefact.** Source of truth for the
> `StateDiff` enum shape in `crates/babel-plugin/src/mutation_recorder.rs`.
> Every mutation site below maps to exactly one variant. Adding a new
> mutation site in the Rust port (or in upstream Babel) requires either
> mapping to an existing variant or adding a new one with explicit plan
> amendment.

Generated 2026-05-02 from `packages/babel-plugin/src/`. Re-run
`grep -rEn 'state\.(includedFiles|compiledImports|sheets|cssMap|
ignoreMemberExpressions)\b'` and `'meta\.state\.(...)'` over the same
tree before each Phase 5 commit (Phase 5 task 0 reconciliation).

**Last reconciled:** 2026-05-04 (Phase 5 §5.1). Outcome: zero new
mutation sites, zero new `StateDiff` variants required. Two line
numbers drifted in upstream (`:325` → `:321`, `:725` → `:707`) —
documented inline at the affected sites. The 5-variant `StateDiff`
enum in `mutation_recorder.rs` remains the complete set. Reach of the
Phase 5 §5.5/§5.6 subtree (`evaluate-expression.ts`,
`traverse-expression/`, `traversers/`) into `state.*` writes is:
exactly one site at `traversers/set-imported-compiled-imports.ts:23`,
which writes `state.importedCompiledImports` — explicitly listed under
"Sites OUT of capture" (per-file scaffolding, written before any
Layer 2 lookup, no replay needed).

## State fields under capture

From `packages/babel-plugin/src/types.ts:140-207`:

| Field | Type | Purpose |
|---|---|---|
| `compiledImports?` | `{ ClassNames?, css?, keyframes?, styled?, cssMap?: string[] }` | API names imported from Compiled, keyed by API |
| `sheets` | `Record<string, t.Identifier>` | Hoisted sheet → identifier map |
| `includedFiles` | `string[]` | Files statically evaluated this pass (HMR invalidation set) |
| `cssMap` | `Record<string, string[]>` | Evaluated `cssMap()` calls, keyed by binding name |
| `ignoreMemberExpressions` | `Record<string, true>` | Bindings known not to be cssMap (negative cache) |

`pragma`, `usesXcss`, `importedCompiledImports`, `pathsToCleanup`,
`cache`, `resolver`, `opts`, `file` are also `State` members but are
either (a) per-file scaffolding written exactly once, or (b) not part
of the cross-file caching contract and therefore out of scope for
`StateDiff` capture. They live behind the same `pub(self)` visibility
so the compile-time encapsulation (§3.9.8) covers them too.

---

## Mutation sites (8 total)

Each row's "StateDiff variant" maps to the §3.9.8 sketch unless flagged
NEW.

### 1. `babel-plugin.ts:141` — pragma init

```ts
state.compiledImports = {};
```

Whole-object replacement (init from `undefined` to `{}`) on
`jsxImportSource` pragma detection. **No diff capture needed** — this
is `Program::enter` setup, not an evaluation-time mutation, and the
cache replays at evaluation time. Documented for completeness.

**StateDiff variant:** none (out of cache scope; happens before any
Layer 2 lookup can fire).

### 2. `babel-plugin.ts:151` — classic JSX pragma init

```ts
state.compiledImports = {};
```

Same shape as #1; classic `@jsx` pragma path. Same disposition: not
captured.

**StateDiff variant:** none.

### 3. `babel-plugin.ts:266` — `ImportDeclaration` enter

```ts
state.compiledImports = state.compiledImports || {};
```

Idempotent init at first import-decl that resolves to a Compiled
module. Same disposition as #1/#2: pre-evaluation setup.

**StateDiff variant:** none.

### 4. `babel-plugin.ts:282-284` — API name registration

```ts
const apiArray = state.compiledImports[apiName] || [];
apiArray.push(specifier.node.local.name);
state.compiledImports[apiName] = apiArray;
```

For each Compiled API found in an import (`styled`, `ClassNames`,
`css`, `keyframes`, `cssMap`), append the local binding name to the
matching bucket in `state.compiledImports`. **THIS IS THE FIRST
EVALUATION-VISIBLE MUTATION** — but it fires inside the
`ImportDeclaration` visitor, before any css/styled/etc. handler runs.
Cache replay at handler time must see it.

**StateDiff variant:** `CompiledImportsAppend { api: ApiKind, local_name: String }`
where `ApiKind` is an enum over the 5 known APIs.

(Note: §3.9.8's sketched `CompiledImportsSet { key, value }` was a
whole-bucket-replacement model; the actual mutation is an append into
a per-API bucket. **Variant rename.** §3.9.8 is updated below in the
"Reconciliation against §3.9.8" section.)

### 5. `css-map/index.ts:115` — cssMap result publish

```ts
meta.state.cssMap[path.parent.id.name] = totalSheets;
```

After evaluating a `cssMap({...})` call, store the resulting `string[]`
of sheets keyed by the variable name the call was assigned to.
Per-binding, whole-array publish (no in-place mutation of
`totalSheets`).

**StateDiff variant:** `CssMapInsert { binding: String, sheets: Vec<String> }`.

(§3.9.8 sketched `CssMapAdd { key, value: SerializedValue }`; rename
to `CssMapInsert` and constrain the value type from "any serialized
value" to `Vec<String>` because that's the actual shape — bounding
serialized size at the type level helps the §3.9.10 byte-cap.)

### 6. `utils/css-builders.ts:321` — included-files push

```ts
meta.state.includedFiles.push(next.state.file.loc.filename);
```

Every time the static evaluator opens and parses another file, the
filename is appended to the pass's HMR invalidation set. **Highest-
frequency mutation in this list.**

**StateDiff variant:** `IncludedFilesPush { path: String }`. Matches
§3.9.8 exactly.

(Line drift watch: was `:325` at Phase 0 capture; reconfirmed as `:321`
on the Phase 5 §5.1 reconciliation pass. Surrounding code unchanged —
the upstream edit was a few lines of comment-only churn above this
mutation site.)

### 7. `utils/css-builders.ts:707` — negative-cache mark

```ts
meta.state.ignoreMemberExpressions[node.name] = true;
```

Mark a binding name as "known-not-cssMap" so subsequent lookups
short-circuit. Boolean key set; insertion order doesn't affect output
bytes (it's a presence check).

**StateDiff variant:** `IgnoreMemberExprMark { name: String }`. **NEW**
relative to §3.9.8 (which omitted this field — the encapsulation enum
must add it).

(Line drift watch: was `:725` at Phase 0 capture; reconfirmed as `:707`
on the Phase 5 §5.1 reconciliation pass. Surrounding code unchanged.)

### 8. `utils/hoist-sheet.ts:32` — hoisted sheet record

```ts
meta.state.sheets[sheet] = sheetIdentifier;
```

Records that the literal stylesheet `sheet: string` has been hoisted
to the top of the module under identifier `sheetIdentifier`. The
identifier is `t.Identifier` — a Babel AST node. **This is the
trickiest variant to serialize.** Two options:

  a) Serialize the identifier `name: String` only and rebuild
     `t.identifier(name)` on replay. The hoist code at lines 18-30
     uses `scope.generateUidIdentifier('')` which produces fresh
     unique names per scope — the *string* is what matters; the
     identity of the AST node is recreated each pass.
  b) Re-run the hoist on cache hit. Defeats the cache.

Option (a) is correct and matches the existing JS replay shape: on
cache hit, the plugin needs only the *string name* of the hoisted
sheet so it can rebuild the reference identifier locally.

**StateDiff variant:** `SheetsInsert { sheet_text: String, hoisted_name: String }`.

(§3.9.8 sketched `SheetsAdd { value: String }`; rename to
`SheetsInsert` and split into `(sheet_text, hoisted_name)` because
both halves of the map entry are needed for replay.)

---

## Reconciliation against §3.9.8

§3.9.8's sketched enum:

```rust
enum StateDiff {
    IncludedFilesPush { path: String },                              // ✓ matches site 6
    CompiledImportsSet { key: String, value: SerializedValue },      // ✗ rename + reshape
    SheetsAdd { value: String },                                     // ✗ rename + add field
    CssMapAdd { key: String, value: SerializedValue },               // ✗ rename + reshape
}
```

Reconciled enum (Phase 0 final):

```rust
enum StateDiff {
    IncludedFilesPush { path: String },
    CompiledImportsAppend { api: ApiKind, local_name: String },
    SheetsInsert { sheet_text: String, hoisted_name: String },
    CssMapInsert { binding: String, sheets: Vec<String> },
    IgnoreMemberExprMark { name: String },                           // NEW
}

enum ApiKind { ClassNames, Css, Keyframes, Styled, CssMap }
```

**Five variants, not four.** §3.9.8 missed `IgnoreMemberExprMark`
entirely. The CSS-builders flow short-circuits via this set — without
replay, a Layer 2 hit would re-evaluate and re-decide for every
non-cssMap binding the consumer touches, which (a) costs perf and
(b) could pull in transitive dep entries that the cached entry's
`transitive_deps` did not record. **This is exactly the kind of
discovery this Phase 0 task exists to surface.**

The §3.9.8 `SerializedValue` placeholder is now gone — replaced with
concrete bounded types (`String`, `Vec<String>`, `ApiKind`). Bounding
at the type level lets the §3.9.10 byte-cap math be tighter.

PLAN.md §3.9.8 should be amended in a follow-up edit to use this
reconciled enum. The architecture isn't broken — the variant count
went from 4 to 5 and shapes are more precise — but the doc should
reflect what's actually being built before Phase 2 starts.

---

## Encapsulation: how this gets enforced

Per §3.9.8 the `State` struct's mutable fields are `pub(self)`. The
only public mutator is `MutationRecorder::apply(diff: StateDiff,
state: &mut State)`. Outside `state.rs`, there is no syntactic way to
mutate state without going through a diff.

Pre-commit lint (Phase 0 task #4 in §3.9.8 numbering):

```bash
grep -rnE 'state\.[a-z_]+\.(push|set|add|insert|remove|extend)' \
    crates/babel-plugin/src --include '*.rs' \
    | grep -v 'src/state\.rs\|src/mutation_recorder\.rs'
```

Returning zero matches is a Phase 5 exit gate.

---

## Sites OUT of capture (deliberately)

`state.pragma.*`, `state.usesXcss`, `state.importedCompiledImports.css`,
`state.pathsToCleanup`, and the bare `state.file` are written before
any evaluation that Layer 2 caches. They cannot be inputs to a cache
hit because they're set during `Program::enter` / `ImportDeclaration`
and read during the visitor body — which always runs after enter, so
no replay scenario can read them prematurely.

If a future change reads `pragma.*` during static evaluation of an
imported file (it does not today), `pragma` joins this list and gets
its own `StateDiff` variant.
