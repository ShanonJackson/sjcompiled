/**
 * WASI throughput baseline for the swc-native A/B.
 *
 * Mirrors `crates/swc-native/examples/perf.rs` shape: same fixture,
 * same iteration count, same opts. Run from repo root so the WASI
 * preopen lines up with the harness convention.
 *
 *   bun parity-harness/babel-plugin/bench-wasi.mjs [fixture] [iters]
 *
 * Defaults: fixture = `fixtures/css-prop-basic/input.js`, iters = 1000.
 *
 * Reports per-call µs + transforms/s. Side-by-side with
 *   cargo run --release -p swc-native --example perf -- <fixture> <iters>
 * gives the WASI vs native delta this whole exercise is about.
 */
import { resolve } from 'node:path';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { createRequire } from 'node:module';
import { transformSync } from '@swc/core';

// `@compiled/css-native` is a CJS NAPI binding — bring `require()` in
// from ESM scope. Mirrors how `engines.ts` reaches it.
const require = createRequire(import.meta.url);

const REPO_ROOT = resolve(import.meta.dirname, '../..');
const BABEL_PLUGIN_WASM = resolve(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin.wasm',
);
const AFM_BROWSERSLISTRC = resolve(
  REPO_ROOT,
  'crates/browserslist-shim/tests/fixtures/afm/.browserslistrc',
);
const SNAPSHOT_DIR = resolve(REPO_ROOT, '.parity-harness-cache');
const BROWSERSLIST_SNAPSHOT = resolve(SNAPSHOT_DIR, 'browserslist-snapshot.bin');
const PREFIXES_SNAPSHOT = resolve(SNAPSHOT_DIR, 'prefixes-snapshot.bin');

// Write the same precompute snapshots `engines.ts` uses, so the WASI
// plugin doesn't pay ~6.6 ms per call rebuilding autoprefixer's
// prefix tables (and similarly for browserslist resolution). Without
// these, the WASI baseline measures the precompute overhead more
// than it measures the actual transform.
//
// `@compiled/css-native` isn't on the parity-harness's resolution
// path — load it by absolute path instead, the same `index.js` the
// node_modules-installed copy would resolve to.
const CSS_NATIVE_INDEX = resolve(REPO_ROOT, 'packages/css-native/index.js');
function writeSnapshots() {
  try {
    const native = require(CSS_NATIVE_INDEX);
    mkdirSync(SNAPSHOT_DIR, { recursive: true });
    if (typeof native.precomputeBrowserslistDefault === 'function') {
      writeFileSync(BROWSERSLIST_SNAPSHOT, native.precomputeBrowserslistDefault(AFM_BROWSERSLISTRC));
    }
    if (typeof native.precomputePrefixesDefault === 'function') {
      writeFileSync(PREFIXES_SNAPSHOT, native.precomputePrefixesDefault(null));
    }
    return true;
  } catch (e) {
    console.warn(`bench-wasi: precompute unavailable (${e?.message ?? e}) — falling back to slow path`);
    return false;
  }
}
const haveSnapshots = writeSnapshots();

const fixtureRel = process.argv[2] ?? 'fixtures/css-prop-basic/input.js';
const iters = Number.parseInt(process.argv[3] ?? '1000', 10);
const fixturePath = resolve(REPO_ROOT, fixtureRel);
const source = readFileSync(fixturePath, 'utf8');

const opts = {
  filename: fixturePath,
  jsc: {
    target: 'es2022',
    parser: { syntax: 'typescript', tsx: true },
    transform: {
      verbatimModuleSyntax: true,
      react: { runtime: 'classic' },
    },
    preserveAllComments: false,
    experimental: {
      runPluginFirst: true,
      plugins: [[BABEL_PLUGIN_WASM, {
        root: process.cwd().replace(/\\/g, '/'),
        // Forward-slash normalisation: the WASI sandbox does prefix
        // translation (`host_root` → `/cwd`) which is byte-literal —
        // backslashes here vs forward slashes in `root` would silently
        // skip translation and the plugin would `ENOTCAPABLE` on the
        // first `std::fs::read`.
        ...(haveSnapshots ? {
          precomputedBrowserslistPath: BROWSERSLIST_SNAPSHOT.replace(/\\/g, '/'),
          precomputedPrefixesPath: PREFIXES_SNAPSHOT.replace(/\\/g, '/'),
        } : {}),
      }]],
    },
  },
};

// Warmup — first few calls dominate with WASI instance + module
// codegen costs on @swc/core's side.
for (let i = 0; i < 10; i++) transformSync(source, opts);

const t0 = performance.now();
let lastLen = 0;
for (let i = 0; i < iters; i++) {
  const out = transformSync(source, opts);
  lastLen = out.code.length;
}
const elapsedMs = performance.now() - t0;
const perCallUs = (elapsedMs * 1000) / iters;
const tps = iters / (elapsedMs / 1000);

console.log(`fixture       : ${fixtureRel}`);
console.log(`iterations    : ${iters}`);
console.log(`output bytes  : ${lastLen}`);
console.log(`total elapsed : ${(elapsedMs / 1000).toFixed(3)} s`);
console.log(`per-call      : ${perCallUs.toFixed(1)} µs`);
console.log(`throughput    : ${tps.toFixed(1)} transforms/s`);
