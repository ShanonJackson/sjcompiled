// Raw transform bench — no prettier, no SWC post-process regexes.
// Measures babel-plugin pipeline vs SWC+WASI pipeline directly.
// Run with: bun scripts/perf-plugin-raw.ts

import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { transformSync as babelTransformSync } from '@babel/core';
import { transformSync as swcTransformSync } from '@swc/core';
import { DEFAULT_PARSER_BABEL_PLUGINS } from '@compiled/utils';
import compiledBabelPlugin from '@compiled/babel-plugin';
import stripRuntimeBabelPlugin from '@compiled/babel-plugin-strip-runtime';

const REPO_ROOT = resolve(__dirname, '..');
const STRIP_WASM = join(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin_strip_runtime.wasm'
);
const BP_WASM = join(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin.wasm'
);

const FILENAME = '/base/src/app.tsx';
const SOURCE_FILE_NAME = '../src/app.tsx';

type Fixture = { name: string; source: string; opts: any };

function loadFixtures(dir: string, picks: string[]): Fixture[] {
  const all = readdirSync(dir).filter((f) => f.endsWith('.json'));
  const chosen = picks
    .map((p) => all.find((f) => f.startsWith(p)))
    .filter((f): f is string => Boolean(f));
  return chosen.map((file) => {
    const data = JSON.parse(readFileSync(join(dir, file), 'utf8'));
    return { name: data.name ?? file, source: data.source, opts: data.opts ?? {} };
  });
}

function babelStripRaw(src: string, opts: any): string {
  const extract = opts.run === 'both' || opts.run === 'extract';
  if (!extract) throw new Error('raw bench expects extract fixtures');
  const result = babelTransformSync(src, {
    babelrc: false,
    configFile: false,
    filename: FILENAME,
    generatorOpts: { sourceFileName: SOURCE_FILE_NAME },
    plugins: [
      [
        stripRuntimeBabelPlugin,
        {
          styleSheetPath: opts.styleSheetPath ?? undefined,
          compiledRequireExclude: opts.compiledRequireExclude,
          extractStylesToDirectory: opts.extractStylesToDirectory ?? undefined,
        },
      ],
    ],
    presets: [['@babel/preset-react', { runtime: opts.runtime }]],
  });
  return result?.code ?? '';
}

function swcStripRaw(src: string, opts: any): string {
  const result = swcTransformSync(src, {
    filename: FILENAME,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      transform: { verbatimModuleSyntax: true },
      preserveAllComments: true,
      experimental: {
        plugins: [
          [
            STRIP_WASM,
            {
              styleSheetPath: opts.styleSheetPath ?? undefined,
              compiledRequireExclude: opts.compiledRequireExclude ?? false,
              extractStylesToDirectory: opts.extractStylesToDirectory ?? undefined,
              sourceFileName: SOURCE_FILE_NAME,
            },
          ],
        ],
      },
    },
  });
  return result?.code ?? '';
}

function babelBpRaw(src: string, opts: any): string {
  const { filename, importReact, optimizeCss = false, parserBabelPlugins, ...pluginOptions } = opts;
  const result = babelTransformSync(src, {
    babelrc: false,
    configFile: false,
    filename,
    plugins: [[compiledBabelPlugin, { optimizeCss, importReact, ...pluginOptions }]],
    presets: importReact === false ? [['@babel/preset-react', { runtime: 'automatic' }]] : [],
    parserOpts: { plugins: parserBabelPlugins ?? DEFAULT_PARSER_BABEL_PLUGINS },
  });
  return result?.code ?? '';
}

function swcBpRaw(src: string, opts: any): string {
  const result = swcTransformSync(src, {
    filename: opts.filename,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      transform: { verbatimModuleSyntax: true },
      preserveAllComments: true,
      experimental: { plugins: [[BP_WASM, {}]] },
    },
  });
  return result?.code ?? '';
}

function bench(label: string, fn: (s: string, o: any) => string, fx: Fixture, ms: number): number {
  for (let i = 0; i < 3; i++) {
    try {
      fn(fx.source, fx.opts);
    } catch (e) {
      console.error(`${label} (${fx.name}) warmup threw:`, (e as Error).message);
      return 0;
    }
  }
  let ops = 0;
  const start = performance.now();
  const deadline = start + ms;
  while (performance.now() < deadline) {
    fn(fx.source, fx.opts);
    ops++;
  }
  const elapsed = (performance.now() - start) / 1000;
  return ops / elapsed;
}

const STRIP_DIR = join(REPO_ROOT, 'parity-harness/strip-runtime/fixtures');
const BP_DIR = join(REPO_ROOT, 'parity-harness/babel-plugin/fixtures');

const stripFixtures = loadFixtures(STRIP_DIR, ['D01', 'D05', 'C09', 'C10', 'C15']);
const bpFixtures = loadFixtures(BP_DIR, ['0000', '0050', '0150']);

const DURATION = Number(process.env.PERF_MS ?? 3000);

console.log(`\n== strip-runtime RAW (no prettier, no post-regex) — ${DURATION}ms/engine ==`);
for (const fx of stripFixtures) {
  const b = bench('babel', babelStripRaw, fx, DURATION);
  const s = bench('swc', swcStripRaw, fx, DURATION);
  const ratio = s > 0 ? s / b : 0;
  console.log(
    `  ${fx.name}\n    babel: ${b.toFixed(1).padStart(8)} ops/s   swc: ${s
      .toFixed(1)
      .padStart(8)} ops/s   (swc ${ratio.toFixed(2)}x ${ratio >= 1 ? 'faster' : 'slower'})`
  );
}

console.log(`\n== babel-plugin RAW (pass-through, floor only) — ${DURATION}ms/engine ==`);
for (const fx of bpFixtures) {
  const b = bench('babel', babelBpRaw, fx, DURATION);
  const s = bench('swc', swcBpRaw, fx, DURATION);
  const ratio = s > 0 ? s / b : 0;
  console.log(
    `  ${fx.name}\n    babel: ${b.toFixed(1).padStart(8)} ops/s   swc: ${s
      .toFixed(1)
      .padStart(8)} ops/s   (swc ${ratio.toFixed(2)}x ${ratio >= 1 ? 'faster' : 'slower'})`
  );
}
