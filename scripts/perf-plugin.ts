// Quick ops/s comparison: Babel vs SWC+WASI for the two ported plugins.
// Run with: bun scripts/perf-plugin.ts
//
// Reads existing wasm artefacts; does NOT trigger any cargo build.

import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

import {
  babelEngine as babelStrip,
  swcEngine as swcStrip,
  type StripRuntimeOpts,
} from '../parity-harness/strip-runtime/engines';
import {
  babelEngine as babelBp,
  swcEngine as swcBp,
  type BabelPluginFixtureOpts,
} from '../parity-harness/babel-plugin/engines';

const REPO_ROOT = resolve(__dirname, '..');

type Fixture<O> = { name: string; source: string; opts: O };

function loadFixtures<O>(dir: string, picks: string[]): Fixture<O>[] {
  const all = readdirSync(dir).filter((f) => f.endsWith('.json'));
  const chosen = picks
    .map((p) => all.find((f) => f.startsWith(p)))
    .filter((f): f is string => Boolean(f));
  return chosen.map((file) => {
    const data = JSON.parse(readFileSync(join(dir, file), 'utf8'));
    return { name: data.name ?? file, source: data.source, opts: data.opts ?? {} };
  });
}

function bench<O>(
  label: string,
  fn: (src: string, opts: O) => string,
  fx: Fixture<O>,
  durationMs: number,
  warmup = 3
): number {
  for (let i = 0; i < warmup; i++) {
    try {
      fn(fx.source, fx.opts);
    } catch (e) {
      console.error(`${label} (${fx.name}) warmup threw:`, (e as Error).message);
      return 0;
    }
  }
  let ops = 0;
  const start = performance.now();
  const deadline = start + durationMs;
  while (performance.now() < deadline) {
    fn(fx.source, fx.opts);
    ops++;
  }
  const elapsed = (performance.now() - start) / 1000;
  return ops / elapsed;
}

function runPair<O>(
  label: string,
  babelFn: (src: string, opts: O) => string,
  swcFn: (src: string, opts: O) => string,
  fixtures: Fixture<O>[],
  durationMs: number
) {
  console.log(`\n== ${label} (${durationMs}ms per engine per fixture) ==`);
  for (const fx of fixtures) {
    const b = bench(`babel ${fx.name}`, babelFn, fx, durationMs);
    const s = bench(`swc   ${fx.name}`, swcFn, fx, durationMs);
    const ratio = s > 0 ? s / b : 0;
    const verdict = ratio === 0 ? 'swc-fail' : ratio >= 1 ? 'faster' : 'slower';
    console.log(
      `  ${fx.name}\n    babel: ${b.toFixed(1).padStart(8)} ops/s   swc: ${s
        .toFixed(1)
        .padStart(8)} ops/s   (swc ${ratio.toFixed(2)}x ${verdict})`
    );
  }
}

const STRIP_DIR = join(REPO_ROOT, 'parity-harness/strip-runtime/fixtures');
const BP_DIR = join(REPO_ROOT, 'parity-harness/babel-plugin/fixtures');

const stripFixtures = loadFixtures<StripRuntimeOpts>(STRIP_DIR, [
  'D01',
  'D05',
  'C09',
  'C10',
  'C15',
]);
const bpFixtures = loadFixtures<BabelPluginFixtureOpts>(BP_DIR, ['0000', '0050', '0150']);

const DURATION = Number(process.env.PERF_MS ?? 2000);

runPair('strip-runtime (real port)', babelStrip, swcStrip, stripFixtures, DURATION);
runPair('babel-plugin (pass-through, floor only)', babelBp, swcBp, bpFixtures, DURATION);
