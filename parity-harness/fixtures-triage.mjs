/**
 * Fixtures triage — runs every `/fixtures/*` source through the
 * Phase 6 babel-plugin parity engines and classifies outcomes.
 *
 * Every fixture runs every time. There is no skip / gating flag —
 * hiding fixtures behind a `--include-X` flag is how regressions
 * silently accumulate, so the harness always covers the full corpus.
 *
 * The /fixtures corpus differs from `parity-harness/babel-plugin/fixtures/`:
 *   - Directories at repo root, each with `input.{js,jsx,tsx}` and
 *     optionally an `input-preprocessed.{js,jsx,tsx}` sibling (the
 *     post-`@atlaskit/tokens/babel-plugin` form — see `findEntry`).
 *   - Multi-file `ct-*` fixtures co-locate additional modules in the
 *     same directory; they're run identically to single-file fixtures.
 *   - No per-fixture `opts` — every fixture runs with empty plugin
 *     options (matches `packages/equality-harness/scripts/verify.mjs`).
 *
 * Engines reused verbatim from `parity-harness/babel-plugin/engines.ts`:
 *   - `babelEngine`: Babel + @compiled/babel-plugin + preset-typescript
 *     (TS-strip matches SWC's default) + preset-react + prettier.
 *   - `swcEngine`:   SWC + babel_plugin.wasm + prettier.
 *
 * Categories per fixture:
 *  - parity            : both terminate, prettier outputs byte-equal
 *  - divergence        : both terminate, prettier outputs differ
 *  - babel-throws      : Babel reference threw (negative-test fixture)
 *  - swc-throws        : SWC threw, Babel did not (port defect or known parse limit)
 *  - both-throw        : both threw (negative-test fixture, ok)
 *  - skipped-no-input  : directory has no `input.*` (data-only fixture)
 *
 * Outputs:
 *  - JSON report at `parity-harness/fixtures-triage-report.json`
 *  - Stdout one-line summary
 *
 * Usage:
 *   bun parity-harness/fixtures-triage.mjs                            # full corpus
 *   bun parity-harness/fixtures-triage.mjs --only css-prop-basic ...  # scope to specific fixtures
 *   bun parity-harness/fixtures-triage.mjs --print-diffs              # print divergences inline
 *
 * The Babel+SWC pipeline is identical to the JSON-fixture triage
 * (`parity-harness/babel-plugin/triage.mjs`); only the corpus source
 * differs. Reusing the same engines guarantees that a fix landing in
 * the WASM plugin reflects identically in both reports.
 */
import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';

import {
  babelEngine,
  swcEngine,
  diffSummary,
  reconcileJsxRuntimeOrdering,
  reconcileSwcParamHygieneRenames,
  reconcileReactCreateElementSpreadCollapse,
} from './babel-plugin/engines.ts';

const REPO_ROOT = resolve(import.meta.dirname, '..');
const FIXTURES_DIR = resolve(REPO_ROOT, 'fixtures');
const REPORT_PATH = resolve(import.meta.dirname, 'fixtures-triage-report.json');

const args = process.argv.slice(2);
const PRINT_DIFFS = args.includes('--print-diffs');
const onlyIdx = args.indexOf('--only');
const ONLY = onlyIdx >= 0
  ? args.slice(onlyIdx + 1).filter((a) => !a.startsWith('--'))
  : null;

if (!existsSync(FIXTURES_DIR)) {
  console.error(`fixtures dir missing at ${FIXTURES_DIR}`);
  process.exit(1);
}

// Prefer `input-preprocessed.{tsx,jsx,js}` over `input.{...}` when both
// are present. The preprocessed sibling is the post-`@atlaskit/tokens/babel-plugin`
// form: `token('radius.small')` calls have already been rewritten into
// the `"var(--ds-radius-small, 4px)"` strings that Compiled actually
// consumes in production. The Compiled plugin (both Babel reference and
// SWC port) is downstream of that transform — it has zero knowledge of
// the tokens plugin, and `cssMap`'s static-variant validator (correctly)
// rejects unevaluable `token(...)` calls. Feeding the raw `input.tsx`
// into BOTH engines makes them BOTH throw `STATIC_VARIANT_OBJECT` —
// parity, but parity on an input the engines were never meant to see.
// Using the preprocessed sibling gives the engines the AST shape the
// production pipeline produces.
function findEntry(dir) {
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = join(dir, `input-preprocessed.${ext}`);
    if (existsSync(p)) return p;
  }
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = join(dir, `input.${ext}`);
    if (existsSync(p)) return p;
  }
  return null;
}

const fixtures = readdirSync(FIXTURES_DIR)
  .filter((name) => statSync(join(FIXTURES_DIR, name)).isDirectory())
  .filter((name) => !ONLY || ONLY.includes(name))
  .sort();

const results = {
  parity: [],
  divergence: [],
  'babel-throws': [],
  'swc-throws': [],
  'both-throw': [],
  'skipped-no-input': [],
};

const start = Date.now();
let i = 0;

for (const name of fixtures) {
  i++;
  const dir = join(FIXTURES_DIR, name);
  const entry = findEntry(dir);
  if (!entry) {
    results['skipped-no-input'].push({ name });
    continue;
  }

  const source = readFileSync(entry, 'utf8');
  const filename = entry;
  // The /fixtures corpus has no per-fixture opts file — every entry
  // runs with default plugin options. The harness still threads
  // `filename` so source-map and `__cmpld` filename references match
  // upstream. `importReact` is left undefined so the engines pick the
  // default classic runtime unless the source has a `@jsxImportSource`
  // pragma (engines.ts handles the pragma sniffing).
  const opts = { filename };

  let babelRes, swcRes;
  try {
    babelRes = { ok: true, out: babelEngine(source, opts) };
  } catch (err) {
    babelRes = { ok: false, msg: err.message };
  }
  try {
    swcRes = { ok: true, out: swcEngine(source, opts) };
  } catch (err) {
    swcRes = { ok: false, msg: err.message };
  }

  // §6.8q + §6.8s reconcilers — host-environment-only deltas (jsx-runtime
  // import ordering, SWC hygiene renames). Same conservative reconcilers
  // used by the JSON-fixture triage; they only collapse known-equivalent
  // shapes and never mask a real divergence.
  let babelCmp = babelRes.ok ? babelRes.out : null;
  let swcCmp = swcRes.ok ? swcRes.out : null;
  if (babelRes.ok && swcRes.ok) {
    [babelCmp, swcCmp] = reconcileJsxRuntimeOrdering(babelRes.out, swcRes.out);
    [babelCmp, swcCmp] = reconcileSwcParamHygieneRenames(babelCmp, swcCmp);
    babelCmp = reconcileReactCreateElementSpreadCollapse(babelCmp);
    swcCmp = reconcileReactCreateElementSpreadCollapse(swcCmp);
  }

  let cat;
  if (!babelRes.ok && !swcRes.ok) cat = 'both-throw';
  else if (!babelRes.ok && swcRes.ok) cat = 'babel-throws';
  else if (babelRes.ok && !swcRes.ok) cat = 'swc-throws';
  else if (babelCmp === swcCmp) cat = 'parity';
  else cat = 'divergence';

  const entryRec = { name };
  if (cat === 'divergence') {
    entryRec.diff = diffSummary(babelCmp, swcCmp, 60);
    if (PRINT_DIFFS) {
      process.stderr.write(`\n--- ${name} ---\n${entryRec.diff}\n`);
    }
  } else if (cat === 'swc-throws') {
    entryRec.error = swcRes.msg;
  } else if (cat === 'babel-throws') {
    entryRec.error = babelRes.msg;
  }
  results[cat].push(entryRec);

  if (i % 10 === 0) {
    const elapsed = ((Date.now() - start) / 1000).toFixed(0);
    process.stderr.write(
      `[${i}/${fixtures.length}] ${elapsed}s  parity=${results.parity.length}  div=${results.divergence.length}  swc-throw=${results['swc-throws'].length}  babel-throw=${results['babel-throws'].length}  both=${results['both-throw'].length}\r`,
    );
  }
}

process.stderr.write('\n');

const summary = {
  total: fixtures.length,
  parity: results.parity.length,
  divergence: results.divergence.length,
  'swc-throws': results['swc-throws'].length,
  'babel-throws': results['babel-throws'].length,
  'both-throw': results['both-throw'].length,
  'skipped-no-input': results['skipped-no-input'].length,
  elapsedSeconds: Math.round((Date.now() - start) / 1000),
};

writeFileSync(REPORT_PATH, JSON.stringify({ summary, results }, null, 2));

console.log('\n=== /fixtures triage summary ===');
for (const [k, v] of Object.entries(summary)) console.log(`  ${k.padEnd(20)} ${v}`);
console.log(`\nReport: ${REPORT_PATH}`);

if (results.divergence.length > 0 && !PRINT_DIFFS) {
  console.log(`\nFirst 5 divergences (use --print-diffs for all):`);
  for (const d of results.divergence.slice(0, 5)) {
    console.log(`\n  ${d.name}`);
    console.log(d.diff.split('\n').slice(0, 6).map((l) => `    ${l}`).join('\n'));
  }
}
