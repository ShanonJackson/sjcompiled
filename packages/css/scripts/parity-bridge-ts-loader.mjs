// On-the-fly TypeScript loader for `parity-bridge.mjs` running under
// node. The bridge imports `packages/css/src/plugins/*.ts` directly
// because those are the JS oracle's source. Bun handled this natively;
// node 20.15 needs an ESM loader hook.
//
// Why this lives here and not as `tsx` from npm:
//
// - The workspace already vendors `@babel/core` + `@babel/preset-typescript`
//   for the babel-plugin tests. Reusing them adds zero new MB to
//   node_modules vs. installing `tsx` (3+ MB) or `ts-node` (10+ MB).
// - The transform is a single call (`babel.transformSync(source,
//   { filename, presets: [...] })`) — no project-config plumbing,
//   no `tsconfig.json` resolution, no caching layer. Faster startup
//   than tsx for a single-shot script.
//
// Behavioural equivalence with bun's TS handling on the bridge's
// surface: bun strips types and runs the resulting JS under JSC. We
// strip types via `@babel/preset-typescript` (`onlyRemoveTypeImports`
// disabled — full transform, matching bun's default) and let node's
// V8 run the resulting JS. The plugin source itself is pure ES module
// JS-with-type-annotations; nothing in it depends on a TS runtime
// helper, so the strip-only transform is observably equivalent
// regardless of which TS implementation does the strip.
//
// **Why this matters for parity:** the production AFM build runs
// `transformCss` under node V8. Bun (JSC) and node (V8) implement
// `Array.prototype.sort` differently on non-transitive comparators —
// concretely, `sort-shorthand-declarations`'s comparator returns 0
// for nodes with no first declaration, which makes the comparator
// non-transitive on inputs that mix decls with comments / nested
// rules. JSC would hoist `background` past `color` in
// `[/* a */, color, /* b */, background, /* c */]`; V8 preserves
// the input. Running the bridge under bun was leaking that JSC
// behaviour into the parity oracle, causing 5 AFM-corpus fixtures
// to falsely diverge against a Rust port that correctly matches V8.

import { transformSync } from '@babel/core';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Resolve hook — bun (and TypeScript itself) lets `import './foo'`
// match `./foo.ts`. Node's strict ESM resolver does not. The plugin
// source uses extension-less imports (`./util`, `../utils/x`)
// throughout — we replicate bun's `.ts`/`.tsx`/`/index.ts` extension
// search here so the same source compiles unmodified under node.
//
// Resolution order matches bun's: literal → `.ts` → `.tsx` →
// `/index.ts` → `/index.tsx`. Falls through to node's default
// resolver for anything that doesn't match (npm packages, builtins).
export async function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('./') || specifier.startsWith('../')) {
    const parentURL = context.parentURL;
    if (parentURL && parentURL.startsWith('file://')) {
      const baseURL = new URL(specifier, parentURL);
      const basePath = fileURLToPath(baseURL);
      // Skip extension search if it already has a recognised one;
      // node will resolve those itself.
      const hasExt = /\.(m?js|c?js|json|ts|tsx|mts|cts)$/.test(specifier);
      if (!hasExt) {
        for (const candidate of [
          `${basePath}.ts`,
          `${basePath}.tsx`,
          `${basePath}/index.ts`,
          `${basePath}/index.tsx`,
        ]) {
          if (existsSync(candidate)) {
            return { url: pathToFileURL(candidate).href, format: 'module', shortCircuit: true };
          }
        }
      }
    }
  }
  return nextResolve(specifier, context);
}

export async function load(url, context, nextLoad) {
  if (url.startsWith('file://') && (url.endsWith('.ts') || url.endsWith('.tsx'))) {
    const filename = fileURLToPath(url);
    const source = readFileSync(filename, 'utf8');
    const out = transformSync(source, {
      filename,
      babelrc: false,
      configFile: false,
      sourceMaps: 'inline',
      presets: [
        [
          '@babel/preset-typescript',
          {
            allowDeclareFields: true,
            onlyRemoveTypeImports: false,
          },
        ],
      ],
      // We emit CommonJS, NOT ESM, even though the source is written
      // as ESM. The reason is CJS-interop: many of the bridge's
      // dependencies (`postcss-selector-parser`, `postcss-value-parser`,
      // `cssnano-preset-default` sub-plugins, …) are CJS packages
      // that publish dozens of named exports via dynamic
      // `Object.defineProperty` / module.exports mutation. Node's
      // ESM-to-CJS interop runs `cjs-module-lexer` over the CJS
      // source to discover named exports; it misses dynamically-
      // assigned ones, so `import { pseudo } from 'postcss-selector-
      // parser'` throws `does not provide an export named 'pseudo'`
      // even though `require('postcss-selector-parser').pseudo`
      // works fine. Bun's loader is permissive here — it reflects
      // on the live module object after `require` to satisfy the
      // ESM destructure. We get the same effect for free by
      // transpiling to CJS: the `import { pseudo }` becomes a
      // `var { pseudo } = require('postcss-selector-parser')`,
      // which destructures the live exports object and Just Works.
      plugins: [
        [
          '@babel/plugin-transform-modules-commonjs',
          { strictMode: false, importInterop: 'node' },
        ],
      ],
      sourceType: 'module',
    });
    return {
      format: 'commonjs',
      shortCircuit: true,
      source: out.code,
    };
  }
  return nextLoad(url, context);
}
