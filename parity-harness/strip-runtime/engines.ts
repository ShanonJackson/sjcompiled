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

  // Phase 1 §1.5 — host responsibilities:
  //   1. `extractStylesToDirectory` writes `<cwd>/<dest>/<rel>/x.compiled.css`.
  //      The plugin's WASI preopen IS process.cwd(), so we chdir into a
  //      scratch dir before the SWC call to scope writes there.
  //   2. `compiledRequireExclude=true` writes `<callScratch>/style-rules.json`.
  //      We mkdir the scratch dir under repo cwd; the plugin sees it
  //      via the `/cwd/<rel>` mount. Sidecar schema source of truth:
  //      `plugins/SIDECAR_SCHEMA.md` §2 (`{version:1, rules:[...]}`).
  const fs = require('node:fs') as typeof import('node:fs');
  const path = require('node:path') as typeof import('node:path');

  let callScratch: string | undefined;
  if (opts.compiledRequireExclude) {
    callScratch = path.join(
      ensureScratchDir(),
      `call-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    fs.mkdirSync(callScratch, { recursive: true });
  }

  const previousCwd = opts.extractStylesToDirectory ? process.cwd() : null;
  if (previousCwd) {
    process.chdir(ensureScratchDir());
  }

  // The body executor is wrapped in try/finally so we always restore
  // cwd, drain the call-scratch sidecar (when present), and remove
  // the per-call dir. Babel's analogue is mock-fs in jest tests; here
  // the writes are real, so cleanup matters or `_scratch` accumulates.
  let result;
  try {
    result = swcTransformSync(input, {
      filename: FILENAME,
      jsc: {
        target: 'es2022',
        parser: { syntax: 'typescript', tsx: true },
        // `verbatimModuleSyntax: true` makes SWC treat every import as
        // load-bearing — without it SWC elides unused named specifiers
        // (e.g. `ix` from `@compiled/react/runtime`), which Babel keeps,
        // and the byte-parity oracle fails before our plugin even gets
        // to run. Source: SWC TsConfig in @swc/types, mirroring
        // tsconfig#verbatimModuleSyntax.
        transform: { verbatimModuleSyntax: true },
        preserveAllComments: true,
        experimental: {
          plugins: [
            [
              STRIP_RUNTIME_WASM,
              {
                styleSheetPath: opts.styleSheetPath ?? undefined,
                compiledRequireExclude: opts.compiledRequireExclude ?? false,
                extractStylesToDirectory: opts.extractStylesToDirectory ?? undefined,
                // §1.5 host-threaded options. The SWC plugin has no
                // equivalent of Babel's `file.opts.generatorOpts.sourceFileName`,
                // so we pass it explicitly. `callScratch` is the
                // per-call sidecar dir from PLAN.md §3.9.6 — we
                // translate it to /cwd/<rel> form below.
                sourceFileName: SOURCE_FILE_NAME,
                callScratch: callScratch ? toWasiPath(callScratch) : undefined,
              },
            ],
          ],
        },
      },
    });
  } finally {
    if (previousCwd) {
      process.chdir(previousCwd);
    }
    if (callScratch) {
      // PLAN.md §3.9.13.2 — host's `finally` block clears the per-call
      // scratch. Sidecar contents (style-rules.json) have already been
      // observed by the plugin's panic-or-write path; the harness
      // doesn't drain them today (gate is JS-byte parity).
      try {
        fs.rmSync(callScratch, { recursive: true, force: true });
      } catch {
        // best-effort: the dir may not exist if the plugin bailed
        // before mkdir; ignore.
      }
    }
  }

  if (!result?.code) throw new Error('swcEngine: empty result');

  // Two SWC ↔ Babel post-prettier divergences this harness patches
  // around until Phase 7 (the dedicated comment-placement phase per
  // PLAN.md / STATUS.md):
  //
  // (1) Leading file comment: SWC emits `/* ... */ import x;` (single
  //     line), Babel emits `/* ... */\nimport x;` (two lines).
  //     Prettier preserves the difference.
  // (2) Stacked `/*#__PURE__*/`: when the strip-runtime visitor
  //     replaces `<CC>...</CC>` with the inner JSX/call, the inner's
  //     own `/*#__PURE__*/` survives — but the codegen also emits a
  //     PURE annotation at the OUTER expression's BytePos despite
  //     `take_leading` clearing the leading-comment store there. The
  //     net is `/*#__PURE__*/ /*#__PURE__*/ _jsx(...)`.
  //
  // (1) is fixed by inserting a newline after the file's first block
  // comment. (2) is fixed by collapsing runs of duplicate adjacent
  // block comments. Both are SOURCE-LEVEL workarounds; Phase 7 will
  // replace them with proper SWC comment-store manipulation.
  let normalised = result.code.replace(/^(\s*\/\*[\s\S]*?\*\/) +/, '$1\n');
  normalised = normalised.replace(/(\/\*#__PURE__\*\/\s+)\1+/g, '$1');

  return format(normalised, { parser: 'babel', singleQuote: true });
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
