# `plugins/RESOLVER_SPEC.md` — Compiled resolver port (Rust, JSON-driven)

> **Status:** locked design. Supersedes the earlier `type: "atlassian"`
> proposal (see §6 "Rejected alternative" — kept so the reasoning is
> not re-derived).
>
> **Goal:** swap `"resolver": "@jira-dev/compiled-resolver"` in
> `.compiledcssrc` for a declarative JSON object the Rust plugin can
> interpret natively, producing **byte-identical** absolute paths for
> every `(from, request)` Compiled invokes.

## 1. Background

Compiled's resolver contract is one function:

```ts
interface Resolver { resolveSync(from: string, request: string): string; }
```

Today Jira sets `"resolver": "@jira-dev/compiled-resolver"` — a 20-line
JS wrapper around:

- `AtlassianResolver` from `@atlassian/resolver-core` (an
  `enhanced-resolve` configured with custom `mainFields`, an extension
  list, conditions, and the `AtlassianSourcesPlugin` package.json
  mutator).
- `localPlatformPackages` from `@jira-dev/local-platform-packages`
  (~1,585 prefixes used to decide "try sources first").

The Rust plugin cannot execute a JS module path, so the string form
must become a JSON object.

## 2. Design principle — Jira-agnostic library, Jira-specific JSON

The Rust plugin ships **one** generic resolver. It has **no**
`if (type === "atlassian")` branch and no awareness of `af:exports`,
`atlaskit:src`, `localPlatformPackages`, `@af/*`, or `@atlaskit/*`.
All Jira knowledge lives in the consumer's `.compiledcssrc`.

What is actually Jira-specific in the existing wrapper is **data plus
dialect**, not algorithm:

| Behaviour | Generic? | Jira-specific part |
|---|---|---|
| Probe extensions in fixed order | ✅ | the list (`.ts/.tsx/.mjs/.js/.jsx/.cjs/.json`) |
| Per-context `mainFields` priority | ✅ | the lists (`['browser','module','main']`, `['module','main']`) |
| Honour Node `exports` with conditions | ✅ | the conditions (`['exports']`) |
| Honour an extra exports-style field | ✅ | the field name (`af:exports`) |
| Promote `'./' → '.'` in an exports map | ❌ Jira quirk | yes |
| Promote `atlaskit:src` string into `'.'` of an exports map | ❌ Jira quirk | yes |
| Inject defaults `'.': './src/index.ts'`, `'./*': './src/*'` | ❌ Jira quirk (currently `false` in Jira) | yes |
| "Try sources first" prefix list | ✅ | the list (~1,585 prefixes) |

The three "quirks" are all package.json field promotions — a generic
concept (a package.json mutator) parameterised by Jira-specific data.

## 3. Library-level configuration (no Jira knowledge)

```jsonc
"resolver": {
  // 1. File-probing.
  "extensions": [".ts", ".tsx", ".mjs", ".js", ".jsx", ".cjs", ".json"],

  // 2. Node-style exports resolution. `fields` lists which package.json
  //    keys are treated as exports maps, in priority order. Node's spec
  //    only allows "exports"; enhanced-resolve allows multiple — we
  //    expose that.
  "exports": {
    "fields": ["exports"],
    "conditions": ["exports"]
  },

  // 3. Per-context mainFields. The plugin has no hard-coded
  //    "browser"/"node" assumptions beyond using `defaultContext`.
  "contexts": {
    "browser": { "mainFields": ["browser", "module", "main"] },
    "node":    { "mainFields": ["module", "main"] }
  },
  "defaultContext": "browser",

  // 4. Generic, declarative package.json transform — see §3.1.
  "packageJsonTransforms": [ /* ... */ ],

  // 5. Per-request "prefer this resolver path first" rule — see §3.2.
  "preferFirst": [ /* ... */ ],

  // 6. Optional extra mainFields prepended in any context.
  //    Replaces the hard-coded `useModule2019MainField` flag.
  "extraMainFields": []
}
```

### 3.1 The package.json transform DSL (replaces `AtlassianSourcesPlugin`)

Five named ops, applied in array order, after reading and before
exports resolution:

```jsonc
// (a) renameKey: rename a top-level package.json key.
{ "op": "renameKey", "from": "atlaskit:src", "to": "af:exports",
  "ifTargetMissing": true,
  "wrap": { "as": "object", "key": "." } }

// (b) ensureObject: ensure a key exists and is an object.
{ "op": "ensureObject", "key": "af:exports" }

// (c) renameMapEntry: inside an object-valued key, rename one entry.
{ "op": "renameMapEntry", "in": "af:exports", "from": "./", "to": ".",
  "ifTargetMissing": true, "deleteSource": true }

// (d) setDefault: inside an object-valued key, set defaults if missing.
{ "op": "setDefault", "in": "af:exports",
  "entries": { ".": "./src/index.ts", "./*": "./src/*" } }

// (e) deleteKey: remove a key once promoted/copied elsewhere.
{ "op": "deleteKey", "key": "atlaskit:src" }
```

The library implements those five ops and only those five. Every
`AtlassianSourcesPlugin` behaviour is a particular sequence of these
ops applied to particular field names the consumer chooses.

### 3.2 The `preferFirst` rule (replaces `localPlatformPackages` + `includeSources`)

`includeSources(req) = startsWith(prefix)` is just "for matching
specifiers, try resolver path X first, then fall back". Generic shape:

```jsonc
"preferFirst": [
  {
    "match": { "specifierStartsWith": { "fromFile": "./local-platform-packages.json" } },
    "use":   { "exportsFields": ["af:exports", "exports"], "mainFields": [] }
  }
]
```

The library doesn't know what `local-platform-packages.json` is or
what `af:exports` means — both are strings the consumer supplies.

## 4. CONSUMER perspective (Jira `.compiledcssrc`)

```jsonc
{
  "addComponentName": true,
  "extract": true,
  "inlineCss": false,
  "parserBabelPlugins": ["typescript", "jsx"],
  "transformerBabelPlugins": [
    ["@atlaskit/tokens/babel-plugin", { "shouldUseAutoFallback": true, "shouldForceAutoFallback": false }]
  ],
  "importSources": ["@atlaskit/css"],
  "sortAtRules": true,
  "sortShorthand": true,

  "resolver": {
    "extensions": [".ts", ".tsx", ".mjs", ".js", ".jsx", ".cjs", ".json"],
    "exports": { "fields": ["exports"], "conditions": ["exports"] },
    "contexts": {
      "browser": { "mainFields": ["browser", "module", "main"] },
      "node":    { "mainFields": ["module", "main"] }
    },
    "defaultContext": "browser",

    "packageJsonTransforms": [
      { "op": "ensureObject",   "key": "af:exports" },
      { "op": "renameMapEntry", "in": "af:exports", "from": "./", "to": ".",
                                "ifTargetMissing": true, "deleteSource": true },
      { "op": "renameKey",      "from": "atlaskit:src", "to": "af:exports",
                                "ifTargetMissing": true,
                                "wrap": { "as": "object", "key": "." } },
      { "op": "deleteKey",      "key": "atlaskit:src" }
      // implicitSrcDirectory=false in Jira — no setDefault needed today.
    ],

    "preferFirst": [
      {
        "match": { "specifierStartsWith": { "fromFile": "./dev-tooling/generated/local-platform-packages.json" } },
        "use":   { "exportsFields": ["af:exports", "exports"], "mainFields": [] }
      }
    ]
  }
}
```

### Migration steps

1. Run the generator (Build Infra) to produce
   `dev-tooling/generated/local-platform-packages.json`. Commit it.
2. Open `.compiledcssrc`.
3. Replace `"resolver": "@jira-dev/compiled-resolver"` with the
   `resolver: { ... }` block above.
4. Remove `@jira-dev/compiled-resolver` from any package's deps.
5. Re-run the build — output CSS should be byte-identical.

### Things that look like they should work but won't

- ❌ `"resolver": { "rewrite": { "from": "@atlaskit/x", "to": ".../src/index.ts" } }` —
  cannot represent fs-probing, exports, conditions, or `includeSources`.
- ❌ Pointing `resolver.type` at a JS module path — Rust plugin does
  not execute JS.
- ❌ Inventing context names other than those declared in `contexts`
  unless `defaultContext` references them.

## 5. IMPLEMENTER perspective

### 5.1 Runtime contract

```rust
fn resolve_sync(from: &str, request: &str) -> Result<PathBuf, ResolveError>;
```

`Ok` returns an absolute file path that exists on disk; `Err` matches
JS `if (err) throw err` behaviour Compiled relies on.

### 5.2 Implementation surface

1. **Generic Node-style resolver.** `extensions` + per-context
   `mainFields` + `exportsFields` (≥1) + `conditionNames` + sync
   resolution with fs cache. Use or wrap an existing crate (e.g.
   `oxc_resolver` / `nodejs-resolver`).
2. **Package.json transform engine.** Five named ops (§3.1).
   Add more only if a generic need surfaces.
3. **`preferFirst` dispatcher.** For each request, walk
   `preferFirst[]` in order; try the matching config first; fall back
   to the default `exports`/`contexts` config.

Dispatch shape:

```text
if request[0] == '.' {
  fromDir = isDir(from) ? from : dirname(from)
  return relative_resolver.resolveSync(fromDir, request)
}
for rule in preferFirst {
  if rule.match(request) {
    if let Ok(p) = rule_resolver.resolveSync(from, request) { return p }
  }
}
return contexts[defaultContext].resolveSync(from, request)
```

Load `preferFirst` prefix lists once at plugin init (inline list or
`fromFile`). Do not re-read on each call.

### 5.3 Parity / conformance testing

For a fixed corpus of `(from, request)` pairs, assert the Rust
resolver returns the **identical** absolute path as the existing JS
`@jira-dev/compiled-resolver`:

- Relative imports (`./foo`, `../bar/baz`) starting from both files
  and directories.
- Bare imports of platform packages whose prefix matches a
  `preferFirst` rule (must hit `af:exports`/source).
- Bare imports of non-platform packages (must hit
  `mainFields`/`exports`).
- Each combination of `af:exports`, `atlaskit:src`, `exports`,
  `module`, `main`.
- Wildcard `af:exports` patterns (`./*` → `./src/*`).
- Packages where `'./'` is present but `'.'` is not (promotion).
- Packages where both `'./'` and `'.'` are present (no promotion).
- Subpath imports that miss every routing rule (must fall back to
  fs-probe via the extension list).

Snapshot the JS resolver's output on a representative request set in
CI; diff against the Rust output. CI must fail on any divergence.

### 5.4 Generator the implementer ships alongside

```js
// dev-tooling/scripts/generate-local-platform-packages.js
const { localPlatformPackages } = require('@jira-dev/local-platform-packages');
const fs = require('fs');
fs.writeFileSync(
  'dev-tooling/generated/local-platform-packages.json',
  JSON.stringify({ prefixes: localPlatformPackages }, null, 2),
);
```

Hook into the existing codegen pipeline so the JSON regenerates
whenever `/platform` or `/post-office` workspaces change. This is the
only dynamic part of the old behaviour; decoupling it from the Rust
plugin keeps the plugin pure-data.

### 5.5 Non-goals (parity hazards)

- Do not support arbitrary user-supplied JS callbacks under
  `resolver.*` — re-introduces the JS-module problem.
- Do not invent a `rewrite: { from, to }` shape — cannot replicate
  fs-probing/exports/conditions; will silently diverge.
- Do not skip the `atlaskit:src` deletion step — leaving it changes
  downstream behaviour for other consumers of the same package.json
  cache.

## 6. Rejected alternative — `type: "atlassian"`

An earlier draft proposed `"resolver": { "type": "atlassian", ... }`
where the library shipped a hard-coded Atlassian-shaped resolver
selected by `type`. Rejected because:

- **Library-level Jira tax.** Every consumer (Confluence, Townsquare,
  green-field) would ship code paths that benefit only Jira.
- **Magic identifiers as back-channel.** `"atlassian"` and
  `"@jira-dev/compiled-resolver"` would act as flags into hardcoded
  behaviour — opaque to anyone reading the JSON.
- **Closed extensibility.** New Jira (or other-product) quirks would
  become new flags in the library instead of new entries in
  `packageJsonTransforms` / `preferFirst`.

The generic 5-op DSL + `preferFirst` reproduces every behaviour of
`@jira-dev/compiled-resolver` declaratively, with **no Jira/Atlassian
names anywhere in the library**. Parity criterion (identical absolute
paths for every `(from, request)`) is unchanged.

If a future agent revisits this and is tempted to add a `type` flag
to "simplify" the consumer config: don't. The simplification is
illusory — it just hides the same data behind a name and re-creates
the maintenance burden the rewrite was designed to eliminate.
