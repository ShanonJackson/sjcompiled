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
  // Whole-line `// ...` comments (leading whitespace + comment + trailing
  // newline) — drop the WHOLE line so a comment on its own line doesn't
  // leave a stray blank line behind. Babel's `comments: false` strips
  // the line entirely; SWC's `preserveAllComments: false` doesn't
  // (comments attached to surviving nodes round-trip through codegen),
  // so we do the same condensing in the harness to keep both sides
  // byte-equal. §6.5 surfaces this on `should-not-transform-css-prop-with-comment-directive`.
  let out = code.replace(/^[ \t]*\/\/[^\n\r]*\r?\n/gm, '');
  // Inline (trailing) `// ...` comments — same line as code; just
  // strip the comment text, leaving any preceding whitespace/code.
  out = out.replace(/\/\/[^\n\r]*/g, '');
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
  const {
    comments: _comments,
    filename,
    highlightCode: _highlightCode,
    snippet,
    optimizeCss = false,
    importReact,
    parserBabelPlugins: _parserBabelPlugins,
    pretty: _pretty,
    ...pluginOptions
  } = opts;

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
        // §6.8u — thread `root` so the plugin can resolve relative
        // `importSources` entries (e.g. `'./bar/stub-api'`) the same
        // way Babel does (`state.opts.root ?? this.cwd` at
        // `babel-plugin.ts:75`). Babel's default is `process.cwd()`;
        // we mirror it here. Tests that explicitly pass `root` in
        // `pluginOptions` continue to take precedence (the spread
        // below comes AFTER our default).
        //
        // Forward-slash normalisation: `path.resolve` on Windows
        // returns backslash-separated paths. The Rust port's
        // `normalize_path` outputs forward slashes; matching that
        // here keeps the comparison well-defined cross-platform.
        plugins: [[
          BABEL_PLUGIN_WASM,
          {
            root: process.cwd().replace(/\\/g, '/'),
            optimizeCss,
            importReact,
            ...pluginOptions,
          },
        ]],
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

/**
 * Phase 6 §6.8q reconciliation: strip a matching jsx-runtime import
 * (`<source>/jsx-runtime`) line from BOTH outputs before byte-comparison.
 *
 * Why this is harness-only and not a fix in the plugin or a wrapper:
 *
 * Babel's preset-react inserts the jsx-runtime import via
 * `@babel/helper-module-imports::addNamed`, which lands the new import
 * AFTER existing imports (Babel-side end-of-imports placement). SWC's
 * `swc_ecma_transforms_react::Jsx` injects via `prepend_stmt`
 * (`swc_ecma_utils:371`), which puts the import at body[0] (after
 * directives only). WASM plugins always run BEFORE SWC's react
 * transform — there is no `before/after` hook — so our `Program::exit`
 * cannot see the jsx-runtime import to reorder it. The plugin's
 * `appendRuntimeImports` is a 1:1 port of upstream
 * (`unshiftContainer('body', ...)` → `body.insert(0, ...)`); the
 * delta is purely host-environment behavior, not plugin drift.
 *
 * The reconciler is conservative — it only strips when BOTH outputs
 * carry the SAME jsx-runtime import line (same source, same
 * specifiers). If only one side has it, or they differ, we leave both
 * intact so the divergence surfaces normally. This means we cannot
 * accidentally hide a real bug like "automatic mode failed to trigger
 * on one side": that would manifest as a one-sided import (or
 * different sources / specifiers) and be reported as a divergence.
 */
export function reconcileJsxRuntimeOrdering(a: string, b: string): [string, string] {
  // Match a single line: `import { ...specs } from "<anything>/jsx-runtime";\n`.
  // The trailing `\n` is consumed so removal doesn't leave a blank line.
  // Capture specifiers and source separately so we can compare semantically:
  // SWC and Babel may emit the same specifier set in different orders within
  // the braces (Babel's preset-react emits `jsxs as _jsxs, jsx as _jsx`;
  // SWC's react transform emits `jsx as _jsx, jsxs as _jsxs`). That ordering
  // is cosmetic — we treat the lines as equivalent if the SOURCE is identical
  // and the SET of specifiers (sorted) is identical.
  const re = /^import\s*\{([^}]+)\}\s*from\s*(["'])([^"']+\/jsx-runtime)\2;?\n/m;
  const am = a.match(re);
  const bm = b.match(re);
  if (!am || !bm) return [a, b];
  const normSpecs = (raw: string): string =>
    raw
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
      .sort()
      .join(',');
  if (am[3] === bm[3] && normSpecs(am[1]) === normSpecs(bm[1])) {
    return [a.replace(re, ''), b.replace(re, '')];
  }
  return [a, b];
}

/**
 * Phase 6 §6.8s reconciliation: SWC's resolver+hygiene pass renames a
 * function parameter to `<name><N>` when the parameter shadows a
 * free reference of the same name elsewhere in the module. Babel's
 * generator preserves source identifier names verbatim and has no
 * hygiene pass.
 *
 * Repro (no plugin involved):
 *
 *   const f = (fromColor, toColor) => null;
 *   const y = fromColor;   // free ref at module scope
 *
 * Babel emits the source verbatim. SWC emits
 * `(fromColor1, toColor) => null;` because the resolver tags the
 * param's `SyntaxContext` differently from the unresolved free ref,
 * and the hygiene pass picks a fresh source name for the binding to
 * keep the two ctxts disambiguated post-codegen.
 *
 * This is a host-environment-only delta. The plugin produces
 * semantically-correct AST; SWC's pipeline behaviour after the plugin
 * exits introduces the rename. Same shape as §6.8q (jsx-runtime
 * import ordering) — fixed in the harness, not the plugin.
 *
 * Conservative reconciliation: walk the two outputs in lockstep. The
 * ONLY divergences allowed are insertions of a digit-suffix on an
 * identifier in `b` (SWC) where `a` (Babel) has the un-suffixed
 * identifier, AND the surrounding context is otherwise byte-equal.
 * If we see any other kind of divergence, return `[a, b]` unchanged
 * so the real divergence surfaces normally — we cannot accidentally
 * mask a port defect.
 *
 * After identifying renames, apply them as a global word-boundary
 * substitution in `b`. The hygiene rename uses a fresh suffix not
 * otherwise present in source, so renaming all occurrences back is
 * safe — every `<name><N>` in the SWC output IS the renamed binding
 * (the rename target itself plus any references to it inside the
 * function body).
 */
export function reconcileSwcParamHygieneRenames(a: string, b: string): [string, string] {
  if (a === b) return [a, b];
  const isIdentChar = (c: string): boolean => /[A-Za-z0-9_$]/.test(c);
  const renames: Array<[string, string]> = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      i++;
      j++;
      continue;
    }
    // Mismatch — try to detect a `<name>` (in a) vs `<name><digits>` (in b)
    // hygiene rename. Walk back to the start of the current identifier:
    // both `a` and `b` matched up to (i, j) so they share the prefix.
    let identStart = i;
    while (identStart > 0 && isIdentChar(a[identStart - 1])) identStart--;
    if (identStart === i) {
      // Mismatch isn't on/inside an identifier — genuine divergence.
      return [a, b];
    }
    let aWordEnd = i;
    while (aWordEnd < a.length && isIdentChar(a[aWordEnd])) aWordEnd++;
    let bWordEnd = j;
    while (bWordEnd < b.length && isIdentChar(b[bWordEnd])) bWordEnd++;
    const aWord = a.slice(identStart, aWordEnd);
    const bWord = b.slice(identStart, bWordEnd);
    if (!bWord.startsWith(aWord)) return [a, b];
    const suffix = bWord.slice(aWord.length);
    // The suffix must be all digits and non-empty, AND aWord must not
    // itself end in a digit (otherwise we can't tell where the
    // original name ends and the hygiene suffix begins).
    if (suffix.length === 0 || !/^\d+$/.test(suffix)) return [a, b];
    if (aWord.length === 0 || /\d$/.test(aWord)) return [a, b];
    renames.push([bWord, aWord]);
    i = aWordEnd;
    j = bWordEnd;
  }
  if (i !== a.length || j !== b.length) return [a, b];
  if (renames.length === 0) return [a, b];
  // Apply renames to b. Each `<name><digits>` is replaced with `<name>`
  // at every word-boundary occurrence in the SWC output. Dedupe the
  // pairs first so the same hygiene rename observed at multiple sites
  // (the binding + each reference) only produces one substitution.
  const seen = new Set<string>();
  let newB = b;
  for (const [from, to] of renames) {
    const key = `${from}\u0000${to}`;
    if (seen.has(key)) continue;
    seen.add(key);
    newB = newB.replace(new RegExp(`\\b${from}\\b`, 'g'), to);
  }
  return [a, newB];
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
