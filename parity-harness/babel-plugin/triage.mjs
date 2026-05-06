/**
 * Phase 6 §6.8 triage script — runs every extracted fixture through
 * the Babel reference engine and the SWC-under-test engine, classifies
 * outcomes, and emits a JSON report at
 * `parity-harness/babel-plugin/triage-report.json` plus a one-line
 * count summary to stdout.
 *
 * Categories per fixture:
 *  - parity            : both terminate, prettier outputs byte-equal
 *  - divergence        : both terminate, prettier outputs differ
 *  - babel-throws      : Babel reference threw (negative-test fixture)
 *  - swc-throws        : SWC threw, Babel did not (port defect or known parse limit)
 *  - both-throw        : both threw (negative-test fixture, ok)
 *
 * Run:
 *   bun parity-harness/babel-plugin/triage.mjs
 */
import { readdirSync, readFileSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

import {
  babelEngine,
  swcEngine,
  diffSummary,
  reconcileJsxRuntimeOrdering,
  reconcileSwcParamHygieneRenames,
} from './engines.ts';

const FIXTURES_DIR = resolve(import.meta.dirname, 'fixtures');
const REPORT_PATH = resolve(import.meta.dirname, 'triage-report.json');

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (entry.endsWith('.json')) out.push(full);
  }
  return out;
}

if (!existsSync(FIXTURES_DIR)) {
  console.error(`fixtures dir missing at ${FIXTURES_DIR}; run extract-fixtures.mjs first`);
  process.exit(1);
}

const files = walk(FIXTURES_DIR);
const results = {
  parity: [],
  divergence: [],
  'babel-throws': [],
  'swc-throws': [],
  'both-throw': [],
};

const start = Date.now();
let i = 0;
for (const file of files) {
  i++;
  const fx = JSON.parse(readFileSync(file, 'utf8'));
  let babelRes, swcRes;
  try {
    babelRes = { ok: true, out: babelEngine(fx.source, fx.opts) };
  } catch (err) {
    babelRes = { ok: false, msg: err.message };
  }
  try {
    swcRes = { ok: true, out: swcEngine(fx.source, fx.opts) };
  } catch (err) {
    swcRes = { ok: false, msg: err.message };
  }

  // §6.8q — reconcile the host-environment-only `*/jsx-runtime` import
  // ordering delta before classifying. See `reconcileJsxRuntimeOrdering`
  // in engines.ts for full rationale (SWC's `prepend_stmt` vs Babel's
  // `helper-module-imports::addNamed` end-of-imports placement). The
  // reconciler is conservative (only strips when both sides have the
  // same line) so real divergences still surface.
  let babelCmp = babelRes.ok ? babelRes.out : null;
  let swcCmp = swcRes.ok ? swcRes.out : null;
  if (babelRes.ok && swcRes.ok) {
    [babelCmp, swcCmp] = reconcileJsxRuntimeOrdering(babelRes.out, swcRes.out);
    // §6.8s — host-environment-only SWC hygiene-rename of function
    // params. See reconcileSwcParamHygieneRenames in engines.ts.
    [babelCmp, swcCmp] = reconcileSwcParamHygieneRenames(babelCmp, swcCmp);
  }

  let cat;
  if (!babelRes.ok && !swcRes.ok) cat = 'both-throw';
  else if (!babelRes.ok && swcRes.ok) cat = 'babel-throws';
  else if (babelRes.ok && !swcRes.ok) cat = 'swc-throws';
  else if (babelCmp === swcCmp) cat = 'parity';
  else cat = 'divergence';

  const entry = {
    name: fx.name,
    sourceFile: fx.sourceFile,
  };
  if (cat === 'divergence') {
    entry.diff = diffSummary(babelCmp, swcCmp, 60);
  } else if (cat === 'swc-throws') {
    entry.error = swcRes.msg;
  } else if (cat === 'babel-throws') {
    entry.error = babelRes.msg;
  }
  results[cat].push(entry);

  if (i % 25 === 0) {
    const elapsed = ((Date.now() - start) / 1000).toFixed(0);
    process.stderr.write(
      `[${i}/${files.length}] ${elapsed}s  parity=${results.parity.length}  div=${results.divergence.length}  swc-throw=${results['swc-throws'].length}  babel-throw=${results['babel-throws'].length}  both=${results['both-throw'].length}\r`,
    );
  }
}

process.stderr.write('\n');

const summary = {
  total: files.length,
  parity: results.parity.length,
  divergence: results.divergence.length,
  'swc-throws': results['swc-throws'].length,
  'babel-throws': results['babel-throws'].length,
  'both-throw': results['both-throw'].length,
  elapsedSeconds: Math.round((Date.now() - start) / 1000),
};

writeFileSync(REPORT_PATH, JSON.stringify({ summary, results }, null, 2));

console.log('\n=== Phase 6 §6.8 triage summary ===');
for (const [k, v] of Object.entries(summary)) console.log(`  ${k.padEnd(20)} ${v}`);
console.log(`\nReport: ${REPORT_PATH}`);
