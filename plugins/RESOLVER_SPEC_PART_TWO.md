
You're right — I was sneaking Jira‑specific behaviour into the library by labelling it "type": "atlassian". That's a leak: the Rust plugin shouldn't ship a Jira‑shaped resolver, it should ship a generic resolver whose behaviour is fully described by JSON. Below is a rethink that keeps all Jira‑specific knowledge inside .compiledcssrc (i.e. the consumer’s config) and leaves the library implementation Jira‑agnostic.

1. Audit: what is actually Jira‑specific in @jira-dev/compiled-resolver?
   Looking again at the 20‑line wrapper, the “Jira‑ness” of it is not an algorithm. It is data plus dialect:

Behaviour
Generic?
Jira‑specific?
Probe extensions in a fixed order
✅ generic — every resolver does this
the list (.ts/.tsx/.mjs/.js/.jsx/.cjs/.json) is a Jira choice
Pick a mainFields priority per context
✅ generic
the lists (['browser','module','main'], ['module','main']) are choices
Honour Node exports with custom conditions
✅ generic — Node spec
the conditions (['exports']) are a choice
Honour an additional exports‑style field besides "exports"
✅ generic — Node’s exports already supports this concept
the extra field (af:exports) is a Jira choice
Promote './' to '.' in an exports map
❌ Jira‑specific quirk inherited from atlaskit:src history
yes
Promote a string field (atlaskit:src) into '.' of an exports map
❌ Jira‑specific quirk
yes
Inject default '.': './src/index.ts' and './*': './src/*'
❌ Jira‑specific quirk (implicitSrcDirectory)
yes — but currently false in Jira anyway
Decide “try sources first” based on a list of package‑name prefixes
✅ generic — “for these packages, prefer source over build output” is a perfectly reasonable cross‑product feature
the list (~1,585 platform/post‑office packages) is a Jira choice
Two important observations:

1.  The only items that look truly Jira‑specific (the three “quirks” in red above) are all package.json field promotions — i.e. “before applying Node exports resolution, rewrite the package.json this way”. That’s a generic concept (a package.json mutator) with Jira‑specific data.
2.  Everything else is just standard resolver knobs with Jira‑specific values.
    So the right shape isn’t “a Jira resolver picked by type: "atlassian"”. It’s a generic resolver whose every behaviour is parameterised in JSON, with Jira putting all of its choices into the JSON file.

2. Reframed design — Jira‑agnostic library, Jira‑specific JSON
   2.1 Library‑level concepts (zero Jira knowledge)
   The Rust plugin exposes a single resolver implementation with these generic configuration primitives. None of them mention Atlassian, Jira, @af, @atlaskit, af:exports, atlaskit:src, or platform/post‑office:

"resolver": {
// 1. File‑probing (already generic in every JS resolver).
"extensions": [".ts", ".tsx", ".mjs", ".js", ".jsx", ".cjs", ".json"],

// 2. Node‑style exports resolution.
//    `fields` lists which package.json keys are treated as exports maps,
//    in priority order. (Node's spec only allows one — "exports" — but
//    enhanced-resolve has always supported multiple. We expose that.)
//    `conditions` is the standard Node conditions set.
"exports": {
"fields": ["exports"],
"conditions": ["exports"]
},

// 3. Per-context mainFields. The plugin only knows about contexts named
//    in this object — it has no hard‑coded "browser"/"node" assumptions
//    beyond using `defaultContext` to pick one.
"contexts": {
"browser": { "mainFields": ["browser", "module", "main"] },
"node":    { "mainFields": ["module", "main"] }
},
"defaultContext": "browser",

// 4. A generic, declarative *package.json transform*. This is the key
//    mechanism: the consumer describes how to mutate a package.json
//    before exports/mainFields resolution runs. There is no
//    Atlassian-specific code path in the library — only this rewrite DSL.
"packageJsonTransforms": [ /* see §2.2 */ ],

// 5. Per‑request "prefer this resolver path first" rule.
//    Generic: "if the import specifier matches one of these patterns,
//    try this exports field first, then fall back."
//    Again — no Jira names in the library, just a pattern list.
"preferFirst": [ /* see §2.3 */ ],

// 6. (Optional) extra mainFields prepended in any context. Lets consumers
//    add legacy/custom mainFields without the library knowing them.
//    Replaces the hard‑coded `useModule2019MainField` flag.
"extraMainFields": []
}
That entire block is content‑free w.r.t. Jira. It would be a perfectly fine resolver config for Confluence, Townsquare, or a green‑field consumer.
2.2 The package.json transform DSL (replaces AtlassianSourcesPlugin)
AtlassianSourcesPlugin is just three small mutations on a package.json object before exports resolution. We can express each one as a generic, named operation. None of them mention af:exports, atlaskit:src, or './src/...' in the library — those are just parameter values the consumer supplies.
Operation primitives (all Jira‑agnostic):

// Inside "packageJsonTransforms": each entry is one rewrite rule.
// Operations are applied in array order, after reading and before
// exports resolution.

// (a) renameKey: rename a top-level package.json key.
{ "op": "renameKey", "from": "atlaskit:src", "to": "af:exports", "ifTargetMissing": true,
"wrap": { "as": "object", "key": "." } }

// (b) ensureObject: ensure a key exists and is an object.
{ "op": "ensureObject", "key": "af:exports" }

// (c) renameMapEntry: inside an object-valued key, rename one entry.
{ "op": "renameMapEntry", "in": "af:exports", "from": "./", "to": ".",
"ifTargetMissing": true, "deleteSource": true }

// (d) setDefault: inside an object-valued key, set defaults if missing.
{ "op": "setDefault", "in": "af:exports", "entries": {
".":   "./src/index.ts",
"./*": "./src/*"
}
}

// (e) deleteKey: remove a key once it has been promoted/copied elsewhere.
{ "op": "deleteKey", "key": "atlaskit:src" }
The library implements those five generic ops — and only those five. Every Atlassian‑specific behaviour in AtlassianSourcesPlugin is just a particular sequence of those ops applied to particular field names that the consumer chooses.
2.3 The “prefer first” rule (replaces localPlatformPackages + includeSources)
includeSources: foo.startsWith(prefix) is just a generic “for matching specifiers, try resolver path X first, then fall back to the default path” feature. Expressed generically:

"preferFirst": [
{
// Match by import-specifier prefix (or globs/regex if you prefer).
"match": { "specifierStartsWith": { "fromFile": "./local-platform-packages.json" } },

    // Which resolver path to try first. "useExportsFields" overrides the
    // default `exports.fields` for matched requests only.
    "use": { "exportsFields": ["af:exports", "exports"], "mainFields": [] }
}
]
The library doesn't know what local-platform-packages.json is or what af:exports means — both are just strings the consumer supplies.
2.4 The Jira .compiledcssrc becomes a pure data file
Putting it all together, Jira’s .compiledcssrc is now the only place where Jira‑isms live:

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

    "exports": {
      "fields": ["exports"],
      "conditions": ["exports"]
    },

    "contexts": {
      "browser": { "mainFields": ["browser", "module", "main"] },
      "node":    { "mainFields": ["module", "main"] }
    },
    "defaultContext": "browser",

    // Replaces AtlassianSourcesPlugin, expressed as generic ops.
    "packageJsonTransforms": [
      { "op": "ensureObject",     "key": "af:exports" },
      { "op": "renameMapEntry",   "in": "af:exports", "from": "./", "to": ".",
                                  "ifTargetMissing": true, "deleteSource": true },
      { "op": "renameKey",        "from": "atlaskit:src", "to": "af:exports",
                                  "ifTargetMissing": true,
                                  "wrap": { "as": "object", "key": "." } },
      { "op": "deleteKey",        "key": "atlaskit:src" }
      // implicitSrcDirectory=false in Jira, so no setDefault entry here.
      // If Jira ever flips it on, add:
      // { "op": "setDefault", "in": "af:exports",
      //   "entries": { ".": "./src/index.ts", "./*": "./src/*" } }
    ],

    // Replaces `localPlatformPackages.some(...)`.
    "preferFirst": [
      {
        "match": { "specifierStartsWith": { "fromFile": "./dev-tooling/generated/local-platform-packages.json" } },
        "use":   { "exportsFields": ["af:exports", "exports"], "mainFields": [] }
      }
    ]
}
}
No Jira/Atlassian names appear in the library. They appear only in the data the consumer supplies. The library’s job is reduced to: “read the JSON, build a resolver, run it.”

3. CONSUMER perspective (revised)
   For the Jira engineer editing .compiledcssrc:

•  Replace "resolver": "@jira-dev/compiled-resolver" with the resolver: { ... } block above.
•  All the “Jira knowledge” that used to live in @jira-dev/compiled-resolver (the af:exports/atlaskit:src promotion sequence, the platform/post‑office prefix list) now lives in your config, expressed in terms of generic operations.
•  A small build‑time generator script writes dev-tooling/generated/local-platform-packages.json from @af/product-platform-workspaces/jira (same data source as today). The Rust plugin treats it as opaque data.
•  Other products (Confluence, Townsquare, …) can use the same library with completely different packageJsonTransforms, different preferFirst rules, or omit them entirely. They never inherit Jira’s choices.
For the consumer there is now one integration knob — resolver — and it is a pure data description, not a flag selecting a baked‑in flavour.

4. IMPLEMENTER perspective (revised)
   For the team building the Rust port:

•  The Rust plugin ships one resolver implementation. It has no if (type === "atlassian") branch. It has no awareness of af:exports, atlaskit:src, localPlatformPackages, @af/*, or @atlaskit/*.
•  The implementation surface is exactly:

1.  A generic Node‑style resolver (extensions + mainFields + exports fields + conditions + per‑context dispatch). This is well‑trodden ground (use or wrap an existing crate).
2.  A package.json transform engine that supports five named ops: ensureObject, renameKey, renameMapEntry, setDefault, deleteKey. (Add more later only if a generic need shows up.)
3.  A preferFirst dispatcher: for each request, walk preferFirst[] in order, try the matching configuration first, then fall back to the default exports/contexts configuration.
    •  The runtime contract Compiled requires (resolveSync(from, request) -> abs path) is unchanged.
    •  Parity testing is now stronger, because the library has no Jira‑shaped behaviour to forget. The test corpus (the (from, request) pairs captured from a real Jira build) just exercises the generic engine with Jira’s JSON. If a non‑Jira consumer breaks, we catch it with their own JSON; we don’t maintain a Jira branch.
    •  The historical knobs we previously suggested as named flags (useModule2019MainField, implicitSrcDirectory) don’t exist in the library at all:

▪  useModule2019MainField becomes extraMainFields: ["module:es2019"] (or just adding "module:es2019" to the relevant contexts.*.mainFields array) — generic.
▪  implicitSrcDirectory becomes a setDefault op the consumer can opt into — generic.

5. Why this is materially better than the previous spec

•  No Jira tax in the library. Other products consuming the Rust plugin pay nothing for Jira’s peculiarities. The earlier type: "atlassian" design forced every consumer to ship code paths that benefit only Jira.
•  Pure data on both sides. The Rust plugin reads JSON, the consumer writes JSON. No magic identifiers ("atlassian", "@jira-dev/compiled-resolver") act as a back‑channel into hardcoded behaviour.
•  Open extensibility, closed cores. New Jira quirks (or other products’ quirks) become new entries in packageJsonTransforms or preferFirst. They don’t become new flags in the library.
•  Library stays minimal & auditable. The generic resolver + 5‑op transform DSL is small and testable in isolation. There is no “Atlassian mode” to maintain.
•  Still byte‑equal to today. Every behaviour of @jira-dev/compiled-resolver is reproduced — just expressed declaratively. The parity criterion (identical absolute paths for every (from, request) Compiled invokes) is unchanged.

