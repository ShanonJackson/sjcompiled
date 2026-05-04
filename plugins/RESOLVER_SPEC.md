Here is a re-framed write-up, split clearly into two viewpoints — the Consumer (someone editing .compiledcssrc in Jira / a downstream product) and the Implementer (the team building the Rust port of @compiled/babel-plugin). All consumer-facing configuration is nested under a single resolver: { ... } object so it is a one-for-one swap with the current "resolver": "@jira-dev/compiled-resolver" string.

1. Background (shared context)
   Today .compiledcssrc contains:

"resolver": "@jira-dev/compiled-resolver"
@jira-dev/compiled-resolver is a 20-line wrapper around two real building blocks:

•  AtlassianResolver from @atlassian/resolver-core, which is itself an enhanced-resolve configured with custom mainFields, an extension list, conditional exports, and a package-json-mutating plugin (AtlassianSourcesPlugin).
•  localPlatformPackages from @jira-dev/local-platform-packages, which is platformWorkspaceNames.concat(postOfficeWorkspaceNames) — currently ~1,145 + ~440 ≈ 1,585 package names, used to decide “try sources first” per request.
Compiled's resolver contract is a single function:

interface Resolver { resolveSync(from: string, request: string): string; }
The Rust port cannot accept a JS module path, so the existing string form must be replaced with a declarative JSON object that the Rust plugin can interpret natively.

2. CONSUMER perspective

Audience: anyone editing .compiledcssrc in Jira, Confluence, or any other product that uses the new Rust @compiled/babel-plugin. Goal: swap "resolver": "@jira-dev/compiled-resolver" for a declarative object that produces byte-identical output.
2.1 New shape — everything nested under resolver: { ... }

// .compiledcssrc  (Jira — exact equivalent of today's "@jira-dev/compiled-resolver")
{
"addComponentName": true,
"extract": true,
"inlineCss": false,
"parserBabelPlugins": ["typescript", "jsx"],

"resolver": {
// Selects the built-in resolver implementation that ships
// inside the Rust port of @compiled/babel-plugin.
// (Replaces having to point at a JS module on disk.)
"type": "atlassian",

    // Mirrors `new AtlassianResolver({ exportsConditions: ['exports'] })`.
    // Internally adds 'exports' to BOTH exportsFields and conditionNames.
    "exportsConditions": ["exports"],

    // Resolver knobs (defaults match @atlassian/resolver-core defaults).
    "implicitSrcDirectory": false,
    "useModule2019MainField": false,

    // Extension probing order — must stay exactly this list & order.
    "extensions": [".ts", ".tsx", ".mjs", ".js", ".jsx", ".cjs", ".json"],

    // mainFields per resolution context. The "source" resolver
    // intentionally has no mainFields — it goes through af:exports.
    "mainFields": {
      "browser": ["browser", "module", "main"],
      "node":    ["module", "main"]
    },

    // af:exports synthesis (mirrors AtlassianSourcesPlugin behaviour).
    "afExports": {
      "fields": ["af:exports", "exports"],
      "promoteRootSlash": true,
      "useAtlaskitSrcAsDot": true,
      "implicitDefaults": {
        ".":   "./src/index.ts",
        "./*": "./src/*"
      }
    },

    // Replaces `localPlatformPackages.some(p => request.startsWith(p))`.
    // When the import specifier starts with any prefix in this list,
    // the resolver tries the "source" resolver FIRST and falls back to
    // mainFields/exports if sources don't resolve.
    //
    // Inline a list, OR point at a generated file (preferred):
    "sourcesForPackages": {
      "fromFile": "./dev-tooling/generated/local-platform-packages.json"
    },

    // Default Compiled call site is browser. Jira wrapper never sets it.
    "defaultContext": "browser"
},

"transformerBabelPlugins": [
["@atlaskit/tokens/babel-plugin", { "shouldUseAutoFallback": true, "shouldForceAutoFallback": false }]
],
"importSources": ["@atlaskit/css"],
"sortAtRules": true,
"sortShorthand": true
}
2.2 What the consumer needs to know

•  Drop-in semantic replacement. The above object is byte-equivalent to today's "@jira-dev/compiled-resolver". No JS module is required.
•  All resolver behaviour is configured under resolver: { ... }. No keys leak into the top-level config.
•  type: "atlassian" picks the bundled, Rust-native implementation. Other values (e.g. "node") can be added later, but in Jira always use "atlassian".
•  sourcesForPackages.fromFile is the only “moving” part: it points at a generated JSON file (committed by tooling) listing every workspace under /platform and /post-office. The consumer never edits it by hand — yarn dev regenerates it whenever workspaces change.

▪  For tests/forks you can use "prefixes": ["@af/foo", ...] inline instead.
•  Defaults are intentionally Jira-shaped. A consumer that wants Jira behaviour can just write "resolver": { "type": "atlassian", "sourcesForPackages": { "fromFile": "..." } } and rely on defaults for the rest.
2.3 Migration steps for a consumer

1.  Run the new generator (added by Build Infra) to produce dev-tooling/generated/local-platform-packages.json. Commit it.
2.  Open .compiledcssrc.
3.  Replace the line "resolver": "@jira-dev/compiled-resolver", with the resolver: { ... } block above.
4.  Remove the @jira-dev/compiled-resolver package from dependencies of any package that depended on it directly.
5.  Re-run the build — output CSS should be byte-identical to the previous run.
    2.4 Things that look like they should work but won't

•  ❌ "resolver": { "rewrite": { "from": "@atlaskit/x", "to": ".../src/index.ts" } } — a rewrite map cannot represent fs-probing, package.json-driven exports, condition matching, or the includeSources decision.
•  ❌ Pointing resolver.type at a JS module on disk — the Rust plugin does not execute JS.
•  ❌ Mixing in arbitrary mainFields outside the browser/node keys — only those two contexts are supported (matching AtlassianResolver).

3. IMPLEMENTER perspective

Audience: the team writing the Rust port of @compiled/babel-plugin. Goal: consume the JSON shape above and produce, for every (from, request) pair, the exact same absolute path that @jira-dev/compiled-resolver produces today.
3.1 Required runtime contract
Compiled today calls:

resolver.resolveSync(context: string /* absolute path */,
request: string /* import specifier */): string;
The Rust resolver must expose an equivalent FFI/ABI entry that:

•  Accepts (from: &str, request: &str).
•  Returns Result<PathBuf, ResolveError> whose Ok branch is an absolute file path that exists on disk.
•  Throws/returns an error on miss (matches the JS if (err) throw err behaviour Compiled relies on).
3.2 Internal architecture (ports of existing pieces)
The Rust port should be split into the same three pieces as @atlassian/resolver-core:

1.  A Rust port of enhanced-resolve (or a thin wrapper around an existing crate such as oxc_resolver / nodejs-resolver) supporting:

•  extensions (ordered list).
•  mainFields (ordered list).
•  exportsFields (ordered list, ≥ 1; both af:exports and exports).
•  conditionNames (set; from exportsConditions).
•  Sync resolution with fs-cache (default on; safe for a single Babel/Rust pass).
2.  AtlassianSourcesPlugin equivalent — a package.json mutator that runs before exports resolution. Logic must match atlassian-sources-plugin.ts exactly:

afExports = { ...descriptionFileData['af:exports'] || {} }
if ('./' in afExports && !('.' in afExports)) {
afExports['.'] = afExports['./']
}
delete afExports['./']
if (atlaskit:src && !('.' in afExports)) {
afExports['.'] = atlaskit:src
}
if (implicitSrcDirectory) {
afExports['.']   ??= './src/index.ts'
afExports['./*'] ??= './src/*'
}
delete descriptionFileData['atlaskit:src']
descriptionFileData['af:exports'] = afExports
Driven by resolver.afExports.{fields, promoteRootSlash, useAtlaskitSrcAsDot, implicitDefaults} plus resolver.implicitSrcDirectory.
3.  A dispatcher mirroring AtlassianResolver.resolveSync:

if request[0] == '.' {
fromDir = isDir(from) ? from : dirname(from)
return source.resolveSync(fromDir, request)
}
if includeSources(request) {
if let Ok(p) = source.resolveSync(from, request) { return p }
}
return contextResolvers[defaultContext].resolveSync(from, request)
includeSources(request) = sourcesForPackages.prefixes.iter().any(|p| request.starts_with(p)). Load prefixes once from the inline list or fromFile at plugin init; do not re-read on each call.
3.3 Building the three internal resolvers from JSON
Pseudocode that the implementer should follow on plugin init:

let exportsFields_extra = cfg.exportsConditions.iter().map(|_| "exports").collect();
let conditionNames      = cfg.exportsConditions.clone(); // ['exports']

let source = build_resolver(BuildOpts {
extensions:     cfg.extensions.clone(),
exportsFields:  prepend("af:exports", exportsFields_extra),  // ['af:exports','exports']
conditionNames: conditionNames.clone(),
mainFields:     vec![],
plugins:        vec![AtlassianSourcesPlugin::new(&cfg.afExports, cfg.implicitSrcDirectory)],
syncFs:         true,
});

let browser = build_resolver(BuildOpts {
extensions:     cfg.extensions.clone(),
exportsFields:  exportsFields_extra.clone(),
conditionNames: conditionNames.clone(),
mainFields:     prepend_if(cfg.useModule2019MainField, "module:es2019",
cfg.mainFields["browser"].clone()),
plugins:        vec![],
syncFs:         true,
});

let node = build_resolver(BuildOpts {
extensions:     cfg.extensions.clone(),
exportsFields:  exportsFields_extra.clone(),
conditionNames: conditionNames.clone(),
mainFields:     prepend_if(cfg.useModule2019MainField, "module:es2019",
cfg.mainFields["node"].clone()),
plugins:        vec![],
syncFs:         true,
});
3.4 Field-by-field mapping (JSON → internal resolver options)

JSON path under resolver
Maps to
type
Selector for which Rust resolver to instantiate ("atlassian" is the only one initially).
exportsConditions
Adds entries to both exportsFields and conditionNames of every internal resolver.
implicitSrcDirectory
Passed into AtlassianSourcesPlugin to enable the ./src/index.ts / ./src/* defaults.
useModule2019MainField
Prepends "module:es2019" to browser and node resolvers' mainFields.
extensions
extensions of all three resolvers. Order matters.
mainFields.browser
mainFields of the browser resolver.
mainFields.node
mainFields of the node resolver.
afExports.fields
exportsFields for the source resolver (in priority order; first wins).
afExports.promoteRootSlash
Toggles the './' → '.' promotion in AtlassianSourcesPlugin.
afExports.useAtlaskitSrcAsDot
Toggles the atlaskit:src → '.' promotion.
afExports.implicitDefaults
Defaults injected when implicitSrcDirectory=true.
sourcesForPackages.prefixes / .fromFile
The includeSources(request) prefix list.
defaultContext
Picks browser vs node for non-relative imports when includeSources=false.
3.5 Conformance / parity testing
Implementer must add a parity test suite that, for a fixed corpus of (from, request) pairs, asserts the Rust resolver returns the identical absolute path as the existing JS @jira-dev/compiled-resolver:

•  Relative imports (./foo, ../bar/baz) starting from both files and directories.
•  Bare imports of platform packages whose prefix is in sourcesForPackages (must hit af:exports/source).
•  Bare imports of non-platform packages (must hit mainFields/exports).
•  Packages with each combination of af:exports, atlaskit:src, exports, module, main.
•  Wildcard af:exports patterns (./* → ./src/*).
•  Packages where './' is present but '.' is not (promotion).
•  Packages where both './' and '.' are present (no promotion).
•  Subpath imports that miss every routing rule (must fall back to fs probing using the extension list).
A simple harness: snapshot the JS resolver's output on a representative request set in CI, then diff against Rust output. CI must fail on any divergence.
3.6 Generator that the implementer must ship alongside
The implementer also owns a small Node script (or Rust binary that shells out to node -e) executed via yarn dev:

// dev-tooling/scripts/generate-local-platform-packages.js
const { localPlatformPackages } = require('@jira-dev/local-platform-packages');
const fs = require('fs');
fs.writeFileSync(
'dev-tooling/generated/local-platform-packages.json',
JSON.stringify({ prefixes: localPlatformPackages }, null, 2),
);
Hook it into the existing codegen pipeline so local-platform-packages.json is regenerated whenever /platform or /post-office workspaces change. This is the only dynamic part of the old behaviour, and decoupling it from the Rust plugin keeps the plugin pure-data.
3.7 Non-goals / things NOT to implement (parity hazards)

•  Do not support arbitrary user-supplied JS callbacks under resolver.* — that re-introduces the JS-module problem we're escaping.
•  Do not invent a rewrite: { from, to } shape — it cannot replicate the fs-probing/exports/condition behaviour and will silently diverge.
•  Do not attempt to support contexts other than browser and node initially — AtlassianResolver only has those two.
•  Do not skip the atlaskit:src deletion step in the package-json mutator — leaving it in place changes downstream behaviour for other consumers of the same package.json cache.

4. Summary

•  Consumer change in .compiledcssrc: replace "resolver": "@jira-dev/compiled-resolver" with a single nested resolver: { type: "atlassian", ... } object whose subkeys map 1:1 onto the inputs of AtlassianResolver + AtlassianSourcesPlugin + the local-platform-packages prefix list.
•  Implementer change in the Rust plugin: ship a built-in "atlassian" resolver that mirrors @atlassian/resolver-core exactly (three internal enhanced-resolve-equivalent resolvers, an AtlassianSourcesPlugin port, and a per-request dispatcher), plus a generator script that emits the prefix list as JSON for the plugin to consume.
•  This is the only design that can be byte-equal to the current @jira-dev/compiled-resolver while removing all JS execution from the Rust plugin's resolution path.