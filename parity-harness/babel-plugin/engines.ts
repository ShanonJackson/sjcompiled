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
import { DEFAULT_PARSER_BABEL_PLUGINS } from '@compiled/utils';
import compiledBabelPlugin from '@compiled/babel-plugin';
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

/**
 * Phase 6 §6.8 normalisation: strip every comment from the engine output
 * before prettier. Phase 7 owns comment-placement parity; §6.8 only cares
 * about transform correctness. Stripping comments here keeps the §6.8 gate
 * focused on substantive port divergences instead of `/*#__PURE__*​/`-style
 * annotation mismatches between Babel (`comments: false` default) and SWC
 * (`preserveAllComments: true`).
 */
function stripComments(code: string): string {
  // Single-line `// ...` comments
  let out = code.replace(/\/\/[^\n\r]*/g, '');
  // Block `/* ... */` comments (non-greedy, dotall via [\s\S])
  out = out.replace(/\/\*[\s\S]*?\*\//g, '');
  return out;
}

/**
 * Phase 6 §6.8 normalisation: every fixture is re-formatted via prettier
 * regardless of its `pretty: false` flag. The flag was an upstream-test
 * affordance for inline snapshots; the parity oracle is byte-equality of
 * the formatted output of both engines, so we always normalise.
 */
function normalise(code: string): string {
  const stripped = stripComments(code);
  return format(stripped, { parser: 'babel-ts' });
}

export function babelEngine(source: string, opts: BabelPluginFixtureOpts = {}): string {
  const {
    comments = false,
    filename,
    highlightCode,
    snippet,
    optimizeCss = false,
    importReact,
    parserBabelPlugins,
    ...pluginOptions
  } = opts;

  // Harness divergence vs upstream `packages/babel-plugin/src/test-utils.ts`:
  // upstream defaults to NO react preset (raw JSX in output), but SWC's
  // pipeline ALWAYS transforms JSX (no `preserve` option in @swc/core).
  // For apples-to-apples bytes we apply the same JSX transform on both
  // sides — classic runtime by default (matches SWC's default), automatic
  // when the fixture opts into `importReact === false` OR a `@jsxImportSource`
  // pragma is present in the source (Babel preset-react rejects classic +
  // importSource, and SWC silently honours the pragma).
  const hasJsxImportSourcePragma = /@jsxImportSource\b/.test(source);
  const reactRuntime =
    importReact === false || hasJsxImportSourcePragma ? 'automatic' : 'classic';
  const fileResult = babelTransformSync(source, {
    babelrc: false,
    comments,
    compact: false,
    configFile: false,
    filename,
    highlightCode,
    plugins: [[compiledBabelPlugin, { optimizeCss, importReact, ...pluginOptions }]],
    // preset-typescript strips TS annotations to match SWC's default
    // (SWC's TypeScript parser auto-strips). Without this Babel preserves
    // `as const` / `<{generic}>` while SWC strips them, producing spurious
    // divergences unrelated to the plugin port.
    presets: [
      [
        '@babel/preset-typescript',
        {
          isTSX: true,
          allExtensions: true,
          // Keep value imports verbatim — SWC's `verbatimModuleSyntax: true`
          // does the same. Without this, Babel strips unused value imports
          // while SWC keeps them and we get spurious divergences.
          onlyRemoveTypeImports: true,
        },
      ],
      // useSpread + useBuiltIns together emit native `Object.assign({}, p)`
      // for prop spread instead of the `_extends` polyfill, matching
      // SWC's es2022 output.
      [
        '@babel/preset-react',
        {
          runtime: reactRuntime,
          useSpread: reactRuntime === 'classic' ? true : undefined,
        },
      ],
    ],
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

  return normalise(codeSnippet);
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
  const { snippet, filename, importReact } = opts;

  // Mirror babelEngine's JSX runtime selection so both pipelines emit
  // the same JSX shape (classic = `React.createElement`; automatic =
  // `_jsx`). See babelEngine for the rationale.
  const hasJsxImportSourcePragma = /@jsxImportSource\b/.test(source);
  const reactRuntime =
    importReact === false || hasJsxImportSourcePragma ? 'automatic' : 'classic';

  const result = swcTransformSync(source, {
    filename,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      transform: {
        verbatimModuleSyntax: true,
        react: { runtime: reactRuntime },
      },
      // Drop comments at the parser stage. The §6.8 gate is transform
      // correctness; Phase 7 owns comment-placement parity. With
      // `preserveAllComments: false`, neither engine emits comments,
      // and prettier does not see blank lines where comments used to be.
      preserveAllComments: false,
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
  return normalise(codeSnippet);
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
