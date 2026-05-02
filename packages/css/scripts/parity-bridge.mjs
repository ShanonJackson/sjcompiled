#!/usr/bin/env node
// JS side of the parity-runner harness. Lives in `packages/css/scripts/`
// so the Node/Bun module resolver finds `postcss` and the local plugin
// source naturally — sibling to the source it diffs against.
//
// Reads NDJSON requests on stdin: `{ "stage": "...", "css": "..." }`.
// Writes one NDJSON response per request to stdout:
//   `{ "ok": true,  "css": "..." }`
//   `{ "ok": false, "error": "..." }`
//
// Stays alive until stdin closes (EOF). Each new plugin port adds one
// `case` in `STAGES` and one `import` from `../src/plugins/`.

import { createInterface } from 'node:readline';
import postcss from 'postcss';

import { discardEmptyRules } from '../src/plugins/discard-empty-rules.ts';
import { discardDuplicates } from '../src/plugins/discard-duplicates.ts';
import { extractStyleSheets } from '../src/plugins/extract-stylesheets.ts';
import { parentOrphanedPseudos } from '../src/plugins/parent-orphaned-pseudos.ts';
import { increaseSpecificity } from '../src/plugins/increase-specificity.ts';
import { mergeDuplicateAtRules } from '../src/plugins/merge-duplicate-at-rules.ts';
import { normalizeCurrentColor } from '../src/plugins/normalize-current-color.ts';
import { sortAtomicStyleSheet } from '../src/plugins/sort-atomic-style-sheet.ts';
import { atomicifyRules } from '../src/plugins/atomicify-rules.ts';
import { expandShorthands } from '../src/plugins/expand-shorthands/index.ts';
// npm `postcss-discard-duplicates` — the v6 used by sort.ts.
import postcssDiscardDuplicates from 'postcss-discard-duplicates';
// npm `postcss-nested@5.0.6` — used by transform.ts:48.
import postcssNested from 'postcss-nested';
// npm `postcss-normalize-whitespace@5.1.1` — used by transform.ts.
import postcssNormalizeWhitespace from 'postcss-normalize-whitespace';
// npm `postcss-discard-comments@5.1.2` — cssnano sub-plugin (Phase 6a).
import postcssDiscardComments from 'postcss-discard-comments';
// npm `postcss-normalize-string@5.1.0` — cssnano sub-plugin (Phase 6b).
import postcssNormalizeString from 'postcss-normalize-string';
// npm `postcss-normalize-positions@5.1.1` — cssnano sub-plugin (Phase 6b).
import postcssNormalizePositions from 'postcss-normalize-positions';
// npm `postcss-normalize-timing-functions@5.1.0` — cssnano sub-plugin (Phase 6b).
import postcssNormalizeTimingFunctions from 'postcss-normalize-timing-functions';
// npm `postcss-normalize-url@5.1.0` — cssnano sub-plugin (Phase 6b).
import postcssNormalizeUrl from 'postcss-normalize-url';
// npm `postcss-minify-selectors@5.2.1` — cssnano sub-plugin (Phase 6c).
import postcssMinifySelectors from 'postcss-minify-selectors';
// npm `postcss-ordered-values@5.1.3` — cssnano sub-plugin (Phase 6d).
import postcssOrderedValues from 'postcss-ordered-values';
// npm `postcss-reduce-initial@5.1.2` — cssnano sub-plugin (Phase 6e).
import postcssReduceInitial from 'postcss-reduce-initial';
// npm `postcss-calc@8.2.4` — cssnano sub-plugin (Phase 6d).
import postcssCalc from 'postcss-calc';

// Sheets returned by extract-stylesheets are joined with U+001E (record
// separator) so they ride the single-string bridge protocol unambiguously.
// Real CSS never contains U+001E.
const SHEET_SEP = '\x1e';

const STAGES = {
  // postcss.parse(css).toString() — the parser+stringifier roundtrip.
  // Useful for confirming the postcss-core port is byte-clean before any
  // plugin layers it.
  'postcss-core-roundtrip': (css) => {
    return postcss.parse(css).toString();
  },

  // parse → discardEmptyRules → stringify, in isolation.
  'discard-empty-rules': (css) => {
    const result = postcss([discardEmptyRules()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → discardDuplicates (LOCAL — not the npm v6) → stringify.
  'discard-duplicates': (css) => {
    const result = postcss([discardDuplicates()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → extractStyleSheets (read-only, emits via callback) → join with
  // SHEET_SEP. The plugin doesn't mutate the AST so we don't return the
  // stringified root — we return the per-child sheet strings, which is
  // what the consumer actually hashes.
  //
  // postcss's `process()` returns a LazyResult — the plugin never runs
  // unless something forces evaluation. Touch `.css` to trigger it,
  // matching the pattern in `packages/css/src/transform.ts:83`.
  'extract-stylesheets': (css) => {
    const sheets = [];
    const result = postcss([extractStyleSheets({ callback: (s) => sheets.push(s) })]).process(css, {
      from: undefined,
    });
    result.css; // force lazy plugin run
    return sheets.join(SHEET_SEP);
  },

  // parse → parentOrphanedPseudos → stringify.
  'parent-orphaned-pseudos': (css) => {
    const result = postcss([parentOrphanedPseudos()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → increaseSpecificity → stringify.
  'increase-specificity': (css) => {
    const result = postcss([increaseSpecificity()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → mergeDuplicateAtRules → stringify.
  'merge-duplicate-at-rules': (css) => {
    const result = postcss([mergeDuplicateAtRules()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → normalizeCurrentColor → stringify.
  'normalize-current-color': (css) => {
    const result = postcss([normalizeCurrentColor()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → sortAtomicStyleSheet (defaults: both flags undefined → "use plugin default") → stringify.
  'sort-atomic-style-sheet': (css) => {
    const result = postcss([
      sortAtomicStyleSheet({ sortAtRulesEnabled: undefined, sortShorthandEnabled: undefined }),
    ]).process(css, { from: undefined });
    return result.css;
  },

  // parse → atomicifyRules (no compression map, no callback, no prefix) → stringify.
  // Stage runs WITHOUT autoprefixer/whitespace so we diff the raw atomic output —
  // those plugins are separate stages and would contaminate the diff.
  'atomicify-rules': (css) => {
    const result = postcss([atomicifyRules()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → expandShorthands → stringify.
  'expand-shorthands': (css) => {
    const result = postcss([expandShorthands()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-discard-duplicates@6 → stringify.
  // (NOT the local `discardDuplicates` from compiled-css — that's the
  // `discard-duplicates` stage above.)
  'npm-postcss-discard-duplicates': (css) => {
    const result = postcss([postcssDiscardDuplicates()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-nested@5.0.6 → stringify, with the same
  // bubble/unwrap config that transform.ts:48-61 ships in production.
  // Anomaly #1 in PARITY_VERSIONS.md — `starting-style` is in the bubble
  // list as a v6.0.2 backport workaround pinned to 5.x.
  'postcss-nested': (css) => {
    const result = postcss([
      postcssNested({
        bubble: ['container', '-moz-document', 'layer', 'else', 'when', 'starting-style'],
        unwrap: ['color-profile', 'counter-style', 'font-palette-values', 'page', 'property'],
      }),
    ]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-normalize-whitespace@5.1.1 → stringify.
  // OnceExit-only plugin: collapses internal value whitespace via
  // postcss-value-parser, strips raws.before whitespace, normalizes
  // raws.between/.semicolon, IE9 hack regex.
  'postcss-normalize-whitespace': (css) => {
    const result = postcss([postcssNormalizeWhitespace()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-discard-comments@5.1.2 (default opts) → stringify.
  // Drops non-important comments (anything not starting with `!`) from
  // both the AST and inline raws (between, value.raw, selector.raw,
  // afterName, params.raw). Default keeps `/*!` important comments.
  'postcss-discard-comments': (css) => {
    const result = postcss([postcssDiscardComments()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-normalize-string@5.1.0 (default opts) → stringify.
  // Default `preferredQuote: 'double'`. Walks rule selectors, decl values,
  // and atrule params; flips wrapping quotes on string literals when the
  // change reduces escapes, and collapses `\\\n` (escaped newline).
  'postcss-normalize-string': (css) => {
    const result = postcss([postcssNormalizeString()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-normalize-positions@5.1.1 (default opts) → stringify.
  // Walks `background`, `background-position`, and `(-vendor-)?perspective-origin`
  // decls; rewrites position-keyword pairs (left/top → 0 0, right bottom →
  // 100% 100%) per upstream's keyword/two-keyword rules. var()/env()/constant()
  // short-circuits the current background entry; `/` defers to background-size.
  'postcss-normalize-positions': (css) => {
    const result = postcss([postcssNormalizePositions()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-normalize-timing-functions@5.1.0 (default opts) → stringify.
  // Walks `(-vendor-)?(animation|transition)(-timing-function)?` decls.
  // Compresses `cubic-bezier(...)` / `steps(...)` to keyword equivalents
  // (ease/linear/ease-in/ease-out/ease-in-out/step-start/step-end), and strips
  // the redundant `, end | jump-end` argument from `steps(N, end)`.
  'postcss-normalize-timing-functions': (css) => {
    const result = postcss([postcssNormalizeTimingFunctions()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-normalize-url@5.1.0 (default opts) → stringify.
  // Walks every Decl value and `@namespace` AtRule params; rewrites `url(...)`
  // calls. Absolute URLs go through normalize-url@6.1.0; relative paths go
  // through path.normalize. The 5 postcss-side overrides on top of normalize-
  // url's defaults: normalizeProtocol/sortQueryParameters/stripHash/stripWWW/
  // stripTextFragment all `false`.
  'postcss-normalize-url': (css) => {
    const result = postcss([postcssNormalizeUrl()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-minify-selectors@5.2.1 (no opts) → stringify.
  // OnceExit-only plugin: walks every Rule, runs each selector through a
  // postcss-selector-parser pipeline that clears spaces, dispatches per-kind
  // reducers (attribute/combinator/pseudo/tag/universal), dedupes top-level
  // Selector arms (only when post-clear stringification matches — the
  // upstream "leading-space-on-second-arg" bug means `.a, .a` does NOT
  // dedupe; `.a,.a` does), and lex-sorts the surviving Selectors.
  'postcss-minify-selectors': (css) => {
    const result = postcss([postcssMinifySelectors()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-ordered-values@5.1.3 (no opts) → stringify.
  // OnceExit walker. Reorders multi-value parts of border / box-shadow /
  // animation / transition / flex-flow / outline / column-rule / columns /
  // list-style / grid-auto-flow / grid-{column,row,…}. Variable functions
  // (var/env/constant), comments, and ___CSS_LOADER_IMPORT___ markers
  // short-circuit the transformation.
  'postcss-ordered-values': (css) => {
    const result = postcss([postcssOrderedValues()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-reduce-initial@5.1.2 (default opts) → stringify.
  // `prepare(result)` resolves browserslist + isSupported('css-initial-value')
  // once. OnceExit walks every Decl. `toInitial[prop] === value.toLowerCase()`
  // → `value = "initial"` (gated on caniuse). `value === "initial"` AND
  // `fromInitial[prop]` exists → `value = fromInitial[prop]`.
  // `defaultIgnoreProps = ['writing-mode', 'transform-box']` always skipped.
  'postcss-reduce-initial': (css) => {
    const result = postcss([postcssReduceInitial()]).process(css, { from: undefined });
    return result.css;
  },

  // parse → npm postcss-calc@8.2.4 (default opts) → stringify. Phase 6d.
  // OnceExit walks every Decl, runs each `value` through value-parser,
  // for each `(-vendor-)?calc(...)` Function node parses and reduces the
  // inner expression via the jison grammar. Default opts:
  // precision=5, preserve=false, warnWhenCannotResolve=false,
  // mediaQueries=false, selectors=false.
  'postcss-calc': (css) => {
    const result = postcss([postcssCalc()]).process(css, { from: undefined });
    return result.css;
  },

  // The full `sort()` entry point. Mirrors `packages/css/src/sort.ts`
  // verbatim — same three plugins, same default opts (both `undefined`,
  // which propagates the plugin defaults in sort-atomic-style-sheet.ts).
  // This is the byte-parity gate for the smaller hashing entry point.
  sort: (css) => {
    const result = postcss([
      postcssDiscardDuplicates(),
      mergeDuplicateAtRules(),
      sortAtomicStyleSheet({ sortAtRulesEnabled: undefined, sortShorthandEnabled: undefined }),
    ]).process(css, { from: undefined });
    return result.css;
  },
};

const rl = createInterface({ input: process.stdin });

rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let req;
  try {
    req = JSON.parse(trimmed);
  } catch (e) {
    process.stdout.write(JSON.stringify({ ok: false, error: `bad request JSON: ${e.message}` }) + '\n');
    return;
  }
  const fn = STAGES[req.stage];
  if (!fn) {
    process.stdout.write(JSON.stringify({ ok: false, error: `unknown stage: ${req.stage}` }) + '\n');
    return;
  }
  try {
    const out = fn(req.css);
    process.stdout.write(JSON.stringify({ ok: true, css: out }) + '\n');
  } catch (e) {
    process.stdout.write(JSON.stringify({ ok: false, error: String(e && e.message || e) }) + '\n');
  }
});

rl.on('close', () => { process.exit(0); });
