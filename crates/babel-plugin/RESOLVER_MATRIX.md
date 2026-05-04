# `crates/babel-plugin/RESOLVER_MATRIX.md`

> Phase 5 §5.4a entry-gate manifest. Bounds the resolver port (§5.4b–§5.4e)
> the same way `COMPAT_EVALUATION_COVERAGE.md` bounds §5.0c. Cited from:
>
> 1. The `#[ignore]`'d Rust gate at
>    `crates/babel-plugin/tests/resolver_matrix_integration.rs`
>    (un-ignored once §5.4b lands).
> 2. The corpus shape in
>    `parity-harness/resolver-matrix/{fixtures.json,oracle.mjs}`.
> 3. `unimplemented!("…")` panics that may be added on
>    evidenced-unreachable resolver branches in
>    `crates/babel-plugin/src/resolver/`.
>
> Same role `crates/babel-plugin/COMPAT_GENERATOR_COVERAGE.md` plays for
> §4.3, and `COMPAT_EVALUATION_COVERAGE.md` plays for §5.0c. The
> methodology is identical: enumerate every reachable input shape,
> snapshot the JS oracle's output, ship the snapshot as the byte-parity
> contract.

## Scope

**This document covers Layer-1 (default-config) parity only** — the
behaviour the plugin must produce when `.compiledcssrc` does **not**
contain a `resolver` key. The §5.4 owner uses this matrix to verify
that `oxc_resolver` configured per `crates/babel-plugin/src/resolver/default.rs`
produces the same absolute path as the JS plugin would have produced
through `createDefaultResolver(config)` with `config.resolve = {}`.

**Out of scope for this document** (covered by §5.4c–§5.4d corpora):

- The declarative `resolver: { ... }` JSON schema from
  `plugins/RESOLVER_SPEC_PART_TWO.md` §2.1 (canonical schema reference).
- The 5-op `packageJsonTransforms` engine (§5.4c — own corpus).
- The `preferFirst` dispatcher (§5.4d — own corpus).
- The `resolve_binding.rs` 1:1 port itself (§5.4e — gated by the
  existing `parity-harness/babel-plugin/` `module-traversal` fixtures).

The corpora layer up. Layer-1 below is the foundation; Layer-2/3/4
corpora exercise the engine + transforms + preferFirst against the
same default-config baseline established here.

## Why this is a hard gate

Per `plugins/PLAN.md` §1 constraint 2 and §3.9.14 #9 (the original
resolver-matrix Phase 0 task that was deferred), resolver divergence
is a **class-hash-affecting bug**:

```
divergent resolved path
  → divergent file read
    → divergent parsed AST
      → divergent evaluator output
        → divergent CSS-value bytes
          → divergent atomic class hash
            → renamed classes in production
```

The blast radius is the same severity tier as `compat/generator.rs`
(§4.3) and `compat/evaluation.rs` (§5.0c) — both of which got their
own coverage manifests + parity corpora before any Rust landed. This
file is the equivalent for the resolver.

CLAUDE.md "minor drift unacceptable" applies: a single resolver branch
where `oxc_resolver` returns a different path multiplies across every
consuming `resolve-binding.ts:185-189` / `:191-193` call site. Catch
it now, not at Phase 8 §8.1 corpus-diff time at AFM scale.

## Methodology

Three resolvers, one corpus, one diff:

| Resolver | Role | Source-of-truth pin |
|---|---|---|
| **`enhanced-resolve@5.x`** | The library that production callers wrap via `createDefaultResolver` (see `plugins/PARCEL_USAGE_EXAMPLE.md`). Configured per `createDefaultResolver(config)` with `config.resolve = {}` — i.e. only `extensions` is overridden, everything else is enhanced-resolve's bare defaults; `useSyncFileSystemCalls: true`; `CachedInputFileSystem` IS configured by the JS oracle (matches production) but is not a parity surface — the resolved path is the same with or without the cache. | exact version pinned in `crates/PARITY_VERSIONS.md` (added by §5.4a). Verified via `bun -e "console.log(require('enhanced-resolve/package.json').version)"`. |
| **`resolve@1.x` (npm `resolve.sync`)** | The fallback path in `packages/babel-plugin/src/utils/resolve-binding.ts:185-189`, used when no `state.resolver` is injected — i.e. when the host wrapper has not been configured at all. Called as `resolve.sync(id, { extensions })` where `id = request[0] === '.' ? join(dirname(filename), request) : request`. | `resolve@1.22.12` (already in `node_modules/.bun/resolve@1.22.12`). |
| **`oxc_resolver`** | The Rust replacement, configured per `crates/babel-plugin/src/resolver/default.rs` to mirror enhanced-resolve's no-config defaults + `extensions = config.extensions ?? DEFAULT_CODE_EXTENSIONS`. | crate dep in `crates/babel-plugin/Cargo.toml` (added by §5.4b). Pinned version recorded in `crates/PARITY_VERSIONS.md` once §5.4b selects it. |

For each fixture: capture `enhanced-resolve` output (the production
oracle), capture `npm resolve.sync` output (the fallback oracle),
diff. **The corpus's `expected` column is `enhanced-resolve`'s output
unless explicitly overridden by axis** — `npm resolve.sync` is
captured for diff visibility only; production callers go through the
host wrapper, not the fallback.

`oxc_resolver` is run by the Rust gate
(`crates/babel-plugin/tests/resolver_matrix_integration.rs`) and must
match `expected` byte-for-byte. Divergences are handled per the
"Divergence action protocol" section below.

## Default-config baseline

The §5.4b implementer wires `resolver/default.rs` to mirror exactly
this:

```js
// What createDefaultResolver(config) produces when config.resolve === {}
ResolverFactory.createResolver({
  fileSystem: new CachedInputFileSystem(fs, 4000),
  extensions: config.extensions,         // i.e. user-passed; defaults to DEFAULT_CODE_EXTENSIONS
  useSyncFileSystemCalls: true,
});
```

In Rust (no caching per WASI constraint — see CLAUDE.md "Never edit
packages/*" + "WASI/WASM Compilation" sections; SWC tears down the
WASI instance between calls per `plugins/PLAN.md` §3.9.4):

```rust
// crates/babel-plugin/src/resolver/default.rs
pub fn build_default(extensions: &[String]) -> oxc_resolver::Resolver {
    oxc_resolver::Resolver::new(oxc_resolver::ResolveOptions {
        extensions: extensions.iter().cloned().collect(),
        // every other field: oxc_resolver default
        ..Default::default()
    })
}
```

The corpus axes below verify that `oxc_resolver`'s defaults match
`enhanced-resolve`'s defaults for every behaviour the JS plugin
reaches. Where they don't, the matrix surfaces the divergence and
§5.4b ships a configuration adjustment OR escalates per the protocol.

The DEFAULT_CODE_EXTENSIONS list is locked at
`packages/babel-plugin/src/constants.ts` and ports verbatim to
`crates/babel-plugin/src/utils/constants.rs` (already shipped):
`['.js', '.jsx', '.ts', '.tsx']`. The matrix exercises both
"extensions present" and "extensions absent" paths.

## Corpus axes — 9 axes, ~30–50 fixtures per axis

The §0.11 deferral framing in `plugins/PLAN.md` §3.9.14 #9 is
adopted verbatim. Every fixture lives under
`parity-harness/resolver-matrix/fixtures-source/<axis>/<fixture-name>/`
as a synthesized npm-package skeleton (real `package.json` +
real source files on disk) so both JS oracles can be run against
file-system reality.

### Axis 1 — `package.json#main`

Plain CommonJS-era resolution. Every node resolver supports this; the
matrix verifies oxc_resolver/enhanced-resolve default `main` priority
is identical when both `main` and an extension-less file exist.

Fixtures: `main` only, `main` + `module`, `main` + `browser`, missing
`main` (falls back to `index.<ext>` directory probe), `main` pointing
at a non-existent file (error class match), `main` pointing at a
relative path with no extension.

### Axis 2 — `package.json#exports` with conditions

Modern Node-style exports resolution. The richest divergence surface:
`exports` can be a string, an array, an object with conditions
(`import` / `require` / `node` / `default` / arbitrary), nested
patterns, subpath imports.

Fixtures: bare `"exports": "./entry.js"`, `"exports": { ".":
"./entry.js" }`, conditional `{ "import": "...", "require": "..." }`,
nested conditional, subpath patterns (`./*` → `./src/*`), `null`
condition (block resolution), array fallback chain, missing `default`
condition (must error consistently across resolvers).

### Axis 3 — `tsconfig` paths

If `tsconfig.json` is present with `compilerOptions.paths`,
enhanced-resolve picks it up via the `tsconfig-paths-plugin` ONLY if
the wrapper config includes the plugin. `createDefaultResolver(config)`
with `config.resolve = {}` does **NOT** include it. Verify
`oxc_resolver` defaults likewise do not honour tsconfig paths in the
no-config case.

Fixtures: `paths: { "@app/*": ["./src/*"] }` should NOT resolve
through paths in default config (negative test); presence of
`tsconfig.json` should not affect resolution; verify both resolvers
return the same `MODULE_NOT_FOUND`-equivalent error for an unrelated
specifier when tsconfig is present.

### Axis 4 — Symlink realpath (pnpm-style stores)

Resolution must follow symlinks consistently. `enhanced-resolve` has
a `symlinks` option (default `true`) that resolves symlinked package
paths to the symlink target's realpath. `oxc_resolver`'s default must
match.

Fixtures: package installed via symlink (mimic pnpm's
`.pnpm/<name>@<ver>/node_modules/<name>` layout); resolved path must
be the realpath, not the symlink path. Verify across deep symlink
chains (symlink → symlink → real). On Windows, junction points
substitute for symlinks; the corpus skips this axis on
non-Linux/Darwin runners with a CI-conditional.

### Axis 5 — Browser-field

`package.json#browser` field is honoured by enhanced-resolve when
`mainFields` includes `'browser'`. Default config does NOT include
`'browser'` in `mainFields` (`createDefaultResolver` with
`config.resolve = {}` inherits enhanced-resolve's defaults: `['main']`
or `['module', 'main']` depending on enhanced-resolve version). The
matrix verifies `oxc_resolver`'s default `mainFields` matches.

Fixtures: package with `main` + `browser`, default config should
prefer `main`; package with `module` + `main` + `browser`, verify
priority order.

### Axis 6 — Extension order

When a request has no extension and matches both `foo.js` and
`foo.ts` on disk, the resolved path depends on the `extensions`
order. With `DEFAULT_CODE_EXTENSIONS = ['.js', '.jsx', '.ts', '.tsx']`,
`foo.js` wins.

Fixtures: bare specifier matching multiple extensions, verify .js
wins; relative path matching multiple, verify .js wins; specifier
matching .ts only, verifies .ts is found; specifier with explicit
extension overrides probe order.

### Axis 7 — Directory index resolution

When the request resolves to a directory, the resolver probes
`index.<ext>` for each extension in order. Same priority rules as
Axis 6.

Fixtures: directory with `index.js` only; directory with both
`index.js` and `index.ts`; nested directory probes; directory
without any index (must error consistently).

### Axis 8 — Scoped packages

`@scope/pkg` and `@scope/pkg/subpath` resolution. Verifies both
resolvers parse the scope prefix identically, walk `node_modules`
identically, and apply `exports` / `main` rules identically.

Fixtures: bare scoped package import, scoped package deep import,
scoped package with conditional exports, missing scoped package
(error path).

### Axis 9 — Deep imports + `node_modules` walk

The resolver walks up the directory tree looking for `node_modules`.
Verify both resolvers walk identically (same parent-directory order,
same stop conditions at filesystem root) and that deep-import
specifiers (`pkg/subpath`) are resolved through `exports` (when
present) before falling back to file-on-disk probing.

Fixtures: package found at immediate `node_modules/`; package found
at parent `node_modules/`; package not found anywhere (error class
match); deep import where subpath exists in `exports` map; deep
import where subpath bypasses `exports` (legacy-Compatibility surface
— enhanced-resolve has had bugs here, verify oxc_resolver replicates).

## Per-fixture corpus shape

```jsonc
// parity-harness/resolver-matrix/fixtures.json
[
  {
    "name": "axis-2-exports-import-condition",
    "axis": "package.json-exports-conditions",
    "fromFile": "fixtures-source/axis-2/exports-import-condition/consumer.js",
    "request": "@parity/axis2-pkg",
    "extensions": [".js", ".jsx", ".ts", ".tsx"],
    "expected": {
      "enhancedResolve": {
        "kind": "ok",
        "path": "/abs/path/to/fixtures-source/axis-2/exports-import-condition/node_modules/@parity/axis2-pkg/dist/import.mjs"
      },
      "npmResolve": {
        "kind": "err",
        "errorClass": "MODULE_NOT_FOUND",
        "errorMessage": "Cannot find module '@parity/axis2-pkg' from '...'"
      }
    }
  },
  // ...
]
```

Notes on the shape:

- `fromFile` is path-relative-to-corpus-root; resolved to absolute at
  oracle time so the corpus is portable across machines.
- `expected.enhancedResolve.path` is the **production-oracle truth**.
  `oxc_resolver` must match this byte-for-byte.
- `expected.npmResolve` is captured for diagnostic diff only; some
  axes (modern `exports`) are never reachable via `resolve.sync`.
- `errorClass` matches Node's `code` field on the thrown error
  (e.g. `MODULE_NOT_FOUND`, `ERR_PACKAGE_PATH_NOT_EXPORTED`); error
  *messages* are checked loosely (presence of substring) since they
  drift between resolver versions.

## Divergence action protocol

When `cargo test -p babel-plugin --test resolver_matrix_integration`
fails on a fixture (oxc_resolver diverges from
`expected.enhancedResolve`), the §5.4 implementer applies one of:

1. **Match — adjust `oxc_resolver` configuration in
   `resolver/default.rs`** to close the gap. Most divergences fall
   here (different default for `mainFields`, `conditionNames`,
   `extensionAlias`, etc.). Adjust and re-run; document the option
   set chosen inline at the call site.
2. **Port shim — write a small wrapper in `resolver/default.rs`** if
   the gap is structural (e.g. enhanced-resolve has special-case
   logic that oxc_resolver doesn't replicate). Cite this file at the
   shim site.
3. **Escalate — update this file with a new "Confirmed unreachable"
   row** if the divergence is on a branch that demonstrably never
   reaches the Compiled call graph (e.g. `imports` field — Compiled
   never imports relative-to-package paths). Land an
   `unimplemented!("compat::resolver: <branch> unreachable from
   Compiled — see crates/babel-plugin/RESOLVER_MATRIX.md
   §<section>")` and update this file. Same protocol as
   `COMPAT_EVALUATION_COVERAGE.md`'s four unreachable branches.

**Defer-by-hope is not acceptable; defer-by-evidence is** — the same
discipline §5.0c locked. If §5.4b can't decide between (1)/(2)/(3)
for a given divergence, the divergence is escalated to user review,
not patched-around.

## Confirmed unreachable

(Populated as §5.4b lands and surfaces evidenced-unreachable
branches. Empty at §5.4a entry-gate.)

## Maintenance

When a future Phase 5/6/8 fixture surfaces a resolver path this
document didn't anticipate:

1. Add the fixture under
   `parity-harness/resolver-matrix/fixtures-source/<axis>/`.
2. Re-run the JS oracle to update `fixtures.json`.
3. Run the Rust gate; one of the three actions above applies.
4. Update this file's axis section with the new shape if the axis
   itself grew.

The §5.4 owner verifies the corpus is byte-clean (`cargo test
--test resolver_matrix_integration` zero failures) before
flipping the §5.4 row in `plugins/STATUS.md` from `▶` to `☑`.
