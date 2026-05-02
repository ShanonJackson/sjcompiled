/**
 * Strip-runtime engines — Babel reference + SWC under-test.
 *
 * Both produce post-prettier strings. The harness asserts the two
 * strings are byte-equal.
 */
import { transformSync as babelTransformSync } from '@babel/core';
import { transformSync as swcTransformSync } from '@swc/core';
import compiledBabelPlugin from '@sjcompiled/babel-plugin';
import stripRuntimeBabelPlugin from '@sjcompiled/babel-plugin-strip-runtime';
import { format } from 'prettier';
import { resolve, join } from 'node:path';

const REPO_ROOT = resolve(__dirname, '../..');

/**
 * Translate a host-absolute path to a `/cwd/<rel>` WASI-visible path.
 * SWC's wasmer backend mounts host cwd at `/cwd`. The plugin only sees
 * paths under that prefix; `env::current_dir()` is cosmetic.
 *
 * Source of truth: plugins/READ_WRITE.md +
 * crates/babel-plugin/PHASE0_FINDINGS.md.
 */
export function toWasiPath(absolutePath: string): string {
  const cwd = process.cwd().replace(/\\/g, '/');
  const abs = absolutePath.replace(/\\/g, '/');
  if (!abs.toLowerCase().startsWith(cwd.toLowerCase())) {
    throw new Error(
      `Cannot translate ${absolutePath} to WASI path: not under cwd ${process.cwd()}`
    );
  }
  const rel = abs.slice(cwd.length).replace(/^\/+/, '');
  return rel ? `/cwd/${rel}` : '/cwd';
}

export type StripRuntimeOpts = {
  run: 'bake' | 'extract' | 'both';
  runtime: 'classic' | 'automatic';
  styleSheetPath?: string | null;
  compiledRequireExclude?: boolean;
  extractStylesToDirectory?: { source: string; dest: string } | null;
  babelJSXPragma?: string;
  babelJSXImportSource?: string;
};

const FILENAME = '/base/src/app.tsx';
const SOURCE_FILE_NAME = '../src/app.tsx';

/**
 * Scratch dir for `extractStylesToDirectory` fs writes during harness
 * runs. The strip-runtime plugin writes `<babel.cwd>/<dest>/<rel>.compiled.css`
 * to disk; pointing Babel's cwd at this scratch path keeps writes out of
 * the repo root. Created lazily on first use.
 */
const SCRATCH_DIR = resolve(__dirname, '_scratch');
let scratchEnsured = false;
function ensureScratchDir(): string {
  if (!scratchEnsured) {
    require('node:fs').mkdirSync(SCRATCH_DIR, { recursive: true });
    scratchEnsured = true;
  }
  return SCRATCH_DIR;
}

/**
 * Reference engine: pure Babel pipeline, identical to the existing
 * test suite's `transform()` in
 * `packages/babel-plugin-strip-runtime/src/__tests__/transform.ts`.
 */
export function babelEngine(source: string, opts: StripRuntimeOpts): string {
  const bake = opts.run === 'both' || opts.run === 'bake';
  const extract = opts.run === 'both' || opts.run === 'extract';

  const result = babelTransformSync(source, {
    babelrc: false,
    configFile: false,
    cwd: opts.extractStylesToDirectory ? ensureScratchDir() : undefined,
    filename: FILENAME,
    generatorOpts: { sourceFileName: SOURCE_FILE_NAME },
    plugins: [
      ...(bake
        ? [[compiledBabelPlugin, { importReact: opts.runtime === 'classic', optimizeCss: false }]]
        : []),
      ...(extract
        ? [
            [
              stripRuntimeBabelPlugin,
              {
                styleSheetPath: opts.styleSheetPath ?? undefined,
                compiledRequireExclude: opts.compiledRequireExclude,
                extractStylesToDirectory: opts.extractStylesToDirectory ?? undefined,
              },
            ],
          ]
        : []),
    ],
    presets: [
      [
        '@babel/preset-react',
        {
          runtime: opts.runtime,
          ...(opts.babelJSXPragma ? { pragma: opts.babelJSXPragma } : {}),
          ...(opts.babelJSXImportSource ? { importSource: opts.babelJSXImportSource } : {}),
        },
      ],
    ],
  });

  if (!result || !result.code) {
    throw new Error('babelEngine: empty result');
  }

  return format(result.code, { parser: 'babel', singleQuote: true });
}

/**
 * Under-test engine: optional Babel pre-bake (run='both' or 'bake'
 * fixtures need this until babel-plugin is ported to Rust in Phase 2),
 * then SWC strip-runtime via the wasm plugin.
 */
const STRIP_RUNTIME_WASM = join(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin_strip_runtime.wasm'
);

export function swcEngine(source: string, opts: StripRuntimeOpts, preBaked?: string): string {
  // Optionally pre-bake with JS babel-plugin (until that's ported in Phase 2).
  let input = source;
  if (opts.run === 'both' || opts.run === 'bake') {
    if (preBaked != null) {
      input = preBaked;
    } else {
      const baked = babelTransformSync(source, {
        babelrc: false,
        configFile: false,
        filename: FILENAME,
        generatorOpts: { sourceFileName: SOURCE_FILE_NAME },
        plugins: [
          [compiledBabelPlugin, { importReact: opts.runtime === 'classic', optimizeCss: false }],
        ],
        presets: [['@babel/preset-react', { runtime: opts.runtime }]],
      });
      if (!baked?.code) throw new Error('swcEngine: pre-bake produced no code');
      input = baked.code;
    }
  }
  if (opts.run === 'bake') {
    return format(input, { parser: 'babel', singleQuote: true });
  }

  const result = swcTransformSync(input, {
    filename: FILENAME,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      experimental: {
        plugins: [
          [
            STRIP_RUNTIME_WASM,
            {
              styleSheetPath: opts.styleSheetPath ?? undefined,
              compiledRequireExclude: opts.compiledRequireExclude ?? false,
              extractStylesToDirectory: opts.extractStylesToDirectory ?? undefined,
            },
          ],
        ],
      },
    },
  });

  if (!result?.code) throw new Error('swcEngine: empty result');

  return format(result.code, { parser: 'babel', singleQuote: true });
}

/**
 * Smallest divergent byte range, with surrounding context. Used in
 * harness assertion failure messages so the human reading the test
 * output sees exactly where parity broke.
 */
export function diffSummary(a: string, b: string, context = 80): string {
  if (a === b) return 'EQUAL';
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  const start = Math.max(0, i - context);
  const aSlice = a.slice(start, i + context);
  const bSlice = b.slice(start, i + context);
  return [
    `divergence at byte ${i} (a.length=${a.length}, b.length=${b.length})`,
    `--- babel  +${start}..${i + context}`,
    aSlice,
    `+++ swc    +${start}..${i + context}`,
    bSlice,
  ].join('\n');
}
