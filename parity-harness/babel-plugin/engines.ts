/**
 * babel-plugin parity engines — Babel reference + SWC under-test.
 *
 * Phase 2 status: only `babelEngine` is wired. The SWC engine lands
 * with §2.3 (pass-through visitor scaffold). For now the harness
 * uses `babelEngine` for the determinism baseline (§2.0 verification
 * gate per `plugins/STATUS.md`).
 */
import { transformSync as babelTransformSync } from '@babel/core';
import { transformSync as swcTransformSync } from '@swc/core';
import { DEFAULT_PARSER_BABEL_PLUGINS } from '@sjcompiled/utils';
import compiledBabelPlugin from '@sjcompiled/babel-plugin';
import { format } from 'prettier';
import { resolve } from 'node:path';

const REPO_ROOT = resolve(__dirname, '../..');
const BABEL_PLUGIN_WASM = resolve(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin.wasm',
);

/**
 * Mirrors `packages/babel-plugin/src/test-utils.ts::transform`. The
 * fixture `opts` field carries the same shape: PluginOptions plus the
 * harness-level flags (`comments`, `filename`, `pretty`, `snippet`,
 * `highlightCode`). Keep this in sync with that file.
 */
export type BabelPluginFixtureOpts = {
  comments?: boolean;
  filename?: string;
  highlightCode?: boolean;
  pretty?: boolean;
  snippet?: boolean;
  importReact?: boolean;
  optimizeCss?: boolean;
  parserBabelPlugins?: string[];
  // PluginOptions is open-ended — pass through every other key
  // unchanged. The plugin reads only what it knows; unknown keys are
  // ignored. We intentionally keep this loose so a Phase 2 fixture
  // doesn't silently drop a recently-added option.
  [key: string]: unknown;
};

export function babelEngine(source: string, opts: BabelPluginFixtureOpts = {}): string {
  const {
    comments = false,
    filename,
    highlightCode,
    pretty = true,
    snippet,
    optimizeCss = false,
    importReact,
    parserBabelPlugins,
    ...pluginOptions
  } = opts;

  const fileResult = babelTransformSync(source, {
    babelrc: false,
    comments,
    compact: !pretty,
    configFile: false,
    filename,
    highlightCode,
    plugins: [[compiledBabelPlugin, { optimizeCss, importReact, ...pluginOptions }]],
    presets: importReact === false ? [['@babel/preset-react', { runtime: 'automatic' }]] : [],
    parserOpts: {
      plugins: parserBabelPlugins ?? DEFAULT_PARSER_BABEL_PLUGINS,
    },
  });

  if (!fileResult || !fileResult.code) {
    return '';
  }

  const { code: babelCode } = fileResult;
  let codeSnippet: string;
  if (snippet) {
    const ifIndex = babelCode.indexOf('if (process.env.NODE_ENV');
    codeSnippet = babelCode
      .substring(babelCode.indexOf('const'), ifIndex === -1 ? babelCode.length : ifIndex)
      .trim();
  } else {
    codeSnippet = babelCode;
  }

  return pretty ? format(codeSnippet, { parser: 'babel-ts' }) : codeSnippet;
}

/**
 * Under-test engine: SWC parser → babel-plugin.wasm visitor → SWC
 * codegen → prettier. Phase 2 §2.2 status: the visitor is a
 * pass-through (no handlers wired). Most fixtures will diverge from
 * Babel under this engine — that's the intended Phase 2 state, and
 * the per-fixture `expectedToFail` machinery in `harness.test.ts`
 * captures it. Phase 6 ungates fixtures handler-by-handler.
 *
 * Notes:
 *  - `verbatimModuleSyntax: true` so SWC doesn't elide unused named
 *    specifiers (matches strip-runtime engine; see that file's note).
 *  - `preserveAllComments` so the comment-shape diff Phase 7 owns
 *    has identical input on both sides.
 *  - Plugin options are NOT threaded yet — the pass-through visitor
 *    ignores them. §2.3 wires the dispatcher; §2.4 adds state
 *    encapsulation; opts threading lands alongside the first real
 *    handler in Phase 6.
 */
export function swcEngine(source: string, opts: BabelPluginFixtureOpts = {}): string {
  const { pretty = true, snippet, filename } = opts;

  const result = swcTransformSync(source, {
    filename,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      transform: { verbatimModuleSyntax: true },
      preserveAllComments: true,
      experimental: {
        plugins: [[BABEL_PLUGIN_WASM, {}]],
      },
    },
  });

  if (!result?.code) return '';

  const code = result.code;
  let codeSnippet: string;
  if (snippet) {
    const ifIndex = code.indexOf('if (process.env.NODE_ENV');
    codeSnippet = code
      .substring(code.indexOf('const'), ifIndex === -1 ? code.length : ifIndex)
      .trim();
  } else {
    codeSnippet = code;
  }
  return pretty ? format(codeSnippet, { parser: 'babel-ts' }) : codeSnippet;
}

export function diffSummary(a: string, b: string, context = 80): string {
  if (a === b) return 'EQUAL';
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  const start = Math.max(0, i - context);
  return [
    `divergence at byte ${i} (a.length=${a.length}, b.length=${b.length})`,
    `--- a +${start}..${i + context}`,
    a.slice(start, i + context),
    `+++ b +${start}..${i + context}`,
    b.slice(start, i + context),
  ].join('\n');
}
