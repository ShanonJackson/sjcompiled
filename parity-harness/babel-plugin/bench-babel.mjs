/**
 * Babel reference throughput baseline. Mirrors `bench-wasi.mjs` /
 * `crates/swc-native/examples/perf.rs` — same fixture, same iteration
 * count — so we can put native, WASI, and Babel side by side.
 *
 *   bun parity-harness/babel-plugin/bench-babel.mjs [fixture] [iters]
 */
import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { babelEngine } from './engines.ts';

const REPO_ROOT = resolve(import.meta.dirname, '../..');
const fixtureRel = process.argv[2] ?? 'fixtures/css-prop-basic/input.js';
const iters = Number.parseInt(process.argv[3] ?? '1000', 10);
const fixturePath = resolve(REPO_ROOT, fixtureRel);
const source = readFileSync(fixturePath, 'utf8');

const opts = { filename: fixturePath };

// Warmup — first calls dominate with require/preset init.
for (let i = 0; i < 10; i++) babelEngine(source, opts);

const t0 = performance.now();
let lastLen = 0;
for (let i = 0; i < iters; i++) {
  lastLen = babelEngine(source, opts).length;
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
