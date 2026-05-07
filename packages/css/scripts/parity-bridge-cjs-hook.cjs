// CJS-side hook for the parity bridge. The ESM loader
// (`parity-bridge-ts-loader.mjs`) transpiles `.ts` plugin files to
// CommonJS so we get bun-equivalent CJS-interop for `import { x }
// from 'cjs-pkg'` patterns. Once a .ts file is in the CJS module
// graph, it `require()`s its peers — and node's CJS loader doesn't
// know about `.ts` / `.tsx` extensions out of the box. This file is
// preloaded with `--require` and:
//
//  1. Registers `.ts` and `.tsx` in `require.extensions` so
//     `require('./atomicify-rules')` finds `./atomicify-rules.ts`,
//  2. Compiles the loaded source on-demand via the same
//     `@babel/preset-typescript` + `transform-modules-commonjs`
//     pipeline the ESM loader uses, so behaviour is identical
//     regardless of which side of the boundary the file enters from.
//
// `require.extensions` is officially deprecated but remains the only
// public hook for CJS extension resolution; ts-node, esbuild-register
// and pirates all use it. The deprecation warning is silenced via
// `--no-deprecation` in the spawn flags.

const Module = require('node:module');
const { transformSync } = require('@babel/core');
const fs = require('node:fs');
const path = require('node:path');

function compile(filename) {
  const source = fs.readFileSync(filename, 'utf8');
  const out = transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    sourceMaps: 'inline',
    presets: [
      ['@babel/preset-typescript', { allowDeclareFields: true, onlyRemoveTypeImports: false }],
    ],
    plugins: [
      ['@babel/plugin-transform-modules-commonjs', { strictMode: false, importInterop: 'node' }],
    ],
    sourceType: 'module',
  });
  return out.code;
}

function loader(module, filename) {
  module._compile(compile(filename), filename);
}

require.extensions['.ts'] = loader;
require.extensions['.tsx'] = loader;

// CJS resolver: when `require('./foo')` is called inside a transpiled
// .ts module, node walks `Module._extensions` keys in order. The
// builtin `.js`/`.json` are tried first; ours come next. That's the
// right precedence — a co-located `foo.js` shadows `foo.ts`, which
// matches bun and tsc's behaviour.
//
// We also need the resolver to *find* `./foo` → `./foo.ts`. Node's
// CJS resolver already iterates registered extensions when the
// literal path doesn't exist, so registering above is sufficient —
// no Module._resolveFilename patch needed.
