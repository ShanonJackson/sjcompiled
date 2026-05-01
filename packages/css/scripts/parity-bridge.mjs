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
import { flattenMultipleSelectors } from '../src/plugins/flatten-multiple-selectors.ts';
import { increaseSpecificity } from '../src/plugins/increase-specificity.ts';
import { mergeDuplicateAtRules } from '../src/plugins/merge-duplicate-at-rules.ts';
import { normalizeCurrentColor } from '../src/plugins/normalize-current-color.ts';
import { sortAtomicStyleSheet } from '../src/plugins/sort-atomic-style-sheet.ts';
import { atomicifyRules } from '../src/plugins/atomicify-rules.ts';
import { expandShorthands } from '../src/plugins/expand-shorthands/index.ts';
// npm `postcss-discard-duplicates` — the v6 used by sort.ts.
import postcssDiscardDuplicates from 'postcss-discard-duplicates';

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

  // parse → flattenMultipleSelectors → stringify.
  'flatten-multiple-selectors': (css) => {
    const result = postcss([flattenMultipleSelectors()]).process(css, { from: undefined });
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
