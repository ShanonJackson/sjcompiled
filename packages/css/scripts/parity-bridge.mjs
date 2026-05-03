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
// npm `postcss-normalize-unicode@5.1.1` — cssnano sub-plugin (Phase 6e).
import postcssNormalizeUnicode from 'postcss-normalize-unicode';
// npm `postcss-minify-selectors@5.2.1` — cssnano sub-plugin (Phase 6c).
import postcssMinifySelectors from 'postcss-minify-selectors';
// npm `postcss-minify-params@5.1.4` — cssnano sub-plugin (Phase 6f).
import postcssMinifyParams from 'postcss-minify-params';
// npm `postcss-ordered-values@5.1.3` — cssnano sub-plugin (Phase 6d).
import postcssOrderedValues from 'postcss-ordered-values';
// npm `postcss-reduce-initial@5.1.2` — cssnano sub-plugin (Phase 6e).
import postcssReduceInitial from 'postcss-reduce-initial';
// npm `postcss-colormin@5.3.1` — cssnano sub-plugin (Phase 6g).
import postcssColormin from 'postcss-colormin';
// npm `postcss-minify-gradients@5.1.1` — cssnano sub-plugin (Phase 6g).
import postcssMinifyGradients from 'postcss-minify-gradients';
// npm `postcss-calc@8.2.4` — cssnano sub-plugin (Phase 6d).
import postcssCalc from 'postcss-calc';
// npm `postcss-convert-values@5.1.3` — cssnano sub-plugin (Phase 6f).
import postcssConvertValues from 'postcss-convert-values';
// npm `autoprefixer@10.4.14` — Phase 7. Browserslist resolution mirrors
// AFM's production path: postcss's `from:` option is set to a file inside
// the AFM `.browserslistrc` fixture directory so autoprefixer's internal
// `browserslist(reqs, { path: dirname(from) })` walks up and finds the
// pinned config. The Rust side does the same via `BrowsersOptions::from =
// afm_browserslist_dir()` (see `crates/parity-runner/src/stages.rs`). Both
// engines exercise the directory-walk resolution path, NOT a forced
// config-file env var (`BROWSERSLIST_CONFIG`) — env-pinning would diverge
// from AFM's production resolution and would silently mask any future
// regression in the walk-up logic. HANDOVER.md §6 documents the closure.
import autoprefixer from 'autoprefixer';
import { fileURLToPath } from 'node:url';
import { dirname, resolve as pathResolve } from 'node:path';

// __dirname is `<workspace>/packages/css/scripts/`, so two parents up is
// the workspace root. The synthetic `<afm-dir>/_parity_input.css` `from:`
// value never has to exist on disk — postcss only uses it for resolving
// `result.opts.from` and downstream consumers use `path.dirname(from)`.
const __dirname = dirname(fileURLToPath(import.meta.url));
const AFM_BROWSERSLIST_DIR = pathResolve(
  __dirname,
  '..', '..', '..',
  'crates', 'browserslist-shim', 'tests', 'fixtures', 'afm',
);
const AFM_FROM = pathResolve(AFM_BROWSERSLIST_DIR, '_parity_input.css');

// Phase 6 band exit gate — full `normalizeCSS(opts)` from the local
// `packages/css/src/plugins/normalize-css.ts`. Spread inside `transform.ts`
// in production; here we run it in isolation through postcss to gate the
// 14 cssnano sub-plugins + normalize-current-color as a unit.
import { normalizeCSS } from '../src/plugins/normalize-css.ts';

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

  // parse → npm postcss-normalize-unicode@5.1.1 (no opts) → stringify. Phase 6e.
  // Browserslist-aware: `prepare(result)` resolves browserslist with
  // `path: __dirname` → walks up from postcss-normalize-unicode/src/ and lands
  // on the workspace's effective config (no .browserslistrc + no `browserslist`
  // field in any package.json → 4.24.2 defaults). `isLegacy = browsers.some(b
  // ∈ browserslist('ie <=11, edge <= 15'))` → false under defaults. OnceExit
  // walks every Decl matching /^unicode-range$/i; lowercases each unicode-
  // range token, attempts wildcard collapse via mergeRangeBounds (`0`/`f`
  // pairs become `?`, max 5), and re-uppercases the leading `u` only when
  // isLegacy. Per-call cache keyed on raw decl value.
  'postcss-normalize-unicode': (css) => {
    const result = postcss([postcssNormalizeUnicode()]).process(css, { from: undefined });
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

  // parse → npm postcss-minify-params@5.1.4 (no opts) → stringify. Phase 6f.
  // OnceExit walks every AtRule. Filters to @media/@supports. Bubble-walks
  // value-parser params, normalizes whitespace around Div tokens, drops the
  // `all` keyword for media queries (legacy gating via browserslist; with
  // workspace's 4.24.2 defaults — no IE 10/11 — `legacy=false`), reduces
  // aspect-ratio pairs by integer GCD. Then sortAndDedupe on stringified
  // top-level arguments. Empty result clears raws.afterName.
  'postcss-minify-params': (css) => {
    const result = postcss([postcssMinifyParams()]).process(css, { from: undefined });
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

  // parse → npm postcss-colormin@5.3.1 (default opts, browserslist
  // pinned to "chrome 100") → stringify. Phase 6g — highest-risk cssnano
  // plugin. Browserslist is pinned via process.env.BROWSERSLIST so both
  // engines see the same browser list (otherwise upstream
  // `browserslist(null, {path: __dirname})` would walk up from
  // `node_modules/postcss-colormin/src/` and find no config, falling
  // through to defaults; the Rust side passes "chrome 100" explicitly.
  // Pinning here keeps the parity contract tight against browserslist
  // default drift over time).
  'postcss-colormin': (css) => {
    const previous = process.env.BROWSERSLIST;
    process.env.BROWSERSLIST = 'chrome 100';
    try {
      const result = postcss([postcssColormin()]).process(css, { from: undefined });
      return result.css;
    } finally {
      if (previous === undefined) delete process.env.BROWSERSLIST;
      else process.env.BROWSERSLIST = previous;
    }
  },

  // parse → npm postcss-minify-gradients@5.1.1 (no opts) → stringify. Phase 6g.
  // OnceExit walks every Decl. Bails on empty / `var(` / `env(` / no
  // `gradient`. Otherwise value-parses and walks top-level Functions:
  // linear-gradient (incl. `-webkit-` and `repeating-` variants) rewrites
  // `to <side>` to angles and strips first 0<unit>/last 100%. Radial
  // (with optional `at` skip) and `-webkit-radial-gradient` (gated by an
  // isColorStop predicate via colord + length-unit/calc check) renormalize
  // each stop to `0` when the previous stop's unit matches and number ≥
  // current. Upstream's `isLessThan` is misnamed (returns ≥); replicated.
  'postcss-minify-gradients': (css) => {
    const result = postcss([postcssMinifyGradients()]).process(css, { from: undefined });
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

  // parse → npm postcss-convert-values@5.1.3 (default opts) → stringify.
  // Phase 6f. Browserslist-aware: pluginCreator resolves
  // `browsers = browserslist(null, { path: __dirname })` once. Under the
  // workspace's locked 4.24.2 defaults the result does NOT contain
  // `'ie 11'`, so the keepZeroPercent IE-11 branch never fires. OnceExit
  // walks every Decl, skipping flex / `--*` / notALength props; for each
  // Word inside (excluding url() args), parses the number+unit, converts
  // to the shortest equivalent across length/time/angle conv tables (ties
  // favor the LATER candidate per upstream's strict-`<` reduce), and
  // clamps opacity/shape-image-threshold to [0, 1]. Default opts:
  // `precision: false` — px-precision rounding disabled.
  'postcss-convert-values': (css) => {
    const result = postcss([postcssConvertValues()]).process(css, { from: undefined });
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

  // parse → autoprefixer@10.4.14 (AFM browserslist) → stringify. Phase 7.
  // Browserslist resolution exercises the production walk: postcss's
  // `from:` option is set to a file inside the AFM `.browserslistrc`
  // fixture directory; autoprefixer reads `result.opts.from` and calls
  // `browserslist(reqs, { path: dirname(from) })`, walking up to find
  // the pinned config. The Rust side does the equivalent via
  // `BrowsersOptions::from = afm_browserslist_dir()`. Both engines hit
  // the same 14-entry resolution through the same path AFM uses in
  // production. HANDOVER.md §6 documents the closure.
  //
  // We also clear `BROWSERSLIST` (the query env var) and
  // `BROWSERSLIST_CONFIG` (the forced-config env var) for the call so
  // neither short-circuits the walk.
  'autoprefixer': (css) => {
    const prevQuery = process.env.BROWSERSLIST;
    const prevConfig = process.env.BROWSERSLIST_CONFIG;
    delete process.env.BROWSERSLIST;
    delete process.env.BROWSERSLIST_CONFIG;
    try {
      const result = postcss([autoprefixer()]).process(css, { from: AFM_FROM });
      return result.css;
    } finally {
      if (prevQuery !== undefined) process.env.BROWSERSLIST = prevQuery;
      if (prevConfig !== undefined) process.env.BROWSERSLIST_CONFIG = prevConfig;
    }
  },

  // Phase 6 band exit gate — `normalizeCSS({optimizeCss: true})` in
  // isolation. Wires the live `packages/css/src/plugins/normalize-css.ts`
  // through postcss with the same default opts the production transform.ts
  // uses (`optimizeCss` defaults to `true` per JS line 59). Browserslist
  // pinned to `chrome 100` per-call so the 5 browserslist-aware plugins
  // (colormin, convert-values, minify-params, normalize-unicode,
  // reduce-initial) resolve to a known target on both sides — otherwise
  // they walk up from each plugin's `__dirname` and pick up the workspace
  // default which can drift across caniuse-lite versions.
  'cssnano-band': (css) => {
    const previous = process.env.BROWSERSLIST;
    process.env.BROWSERSLIST = 'chrome 100';
    try {
      const result = postcss(normalizeCSS({ optimizeCss: true })).process(css, {
        from: undefined,
      });
      return result.css;
    } finally {
      if (previous === undefined) delete process.env.BROWSERSLIST;
      else process.env.BROWSERSLIST = previous;
    }
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
