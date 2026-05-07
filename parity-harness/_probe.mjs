#!/usr/bin/env bun
// Parallel-friendly per-fixture timer. Runs each requested fixture in
// the same process via the harness's engines so wasm cache is shared,
// and prints per-fixture elapsed + verdict. Diagnostic only.
import { babelEngine, swcEngine, reconcileJsxRuntimeOrdering, reconcileSwcParamHygieneRenames, reconcileReactCreateElementSpreadCollapse } from './babel-plugin/engines.ts';
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const FIXTURES = '/Users/sjackson3/Documents/sjcompiled/fixtures';
const wanted = process.argv.slice(2);
const list = wanted.length > 0 ? wanted : readdirSync(FIXTURES).filter((n) => statSync(join(FIXTURES, n)).isDirectory());

function findEntry(dir) {
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = join(dir, `input.${ext}`);
    if (existsSync(p)) return p;
  }
  return null;
}

for (const name of list) {
  const dir = join(FIXTURES, name);
  const entry = findEntry(dir);
  if (!entry) { console.log(`${name}\tNO_INPUT`); continue; }
  const source = readFileSync(entry, 'utf8');
  const opts = { filename: entry };
  const t0 = performance.now();
  let bOk, sOk, b, s, err;
  try { b = babelEngine(source, opts); bOk = true; } catch (e) { bOk = false; err = `babel: ${e.message}`; }
  const tBabel = performance.now() - t0;
  const t1 = performance.now();
  try { s = swcEngine(source, opts); sOk = true; } catch (e) { sOk = false; err = (err ? err + ' | ' : '') + `swc: ${e.message}`; }
  const tSwc = performance.now() - t1;
  let cat;
  if (!bOk && !sOk) cat = 'BOTH_THROW';
  else if (!bOk) cat = 'BABEL_THROW';
  else if (!sOk) cat = 'SWC_THROW';
  else {
    let bb = b, ss = s;
    [bb, ss] = reconcileJsxRuntimeOrdering(bb, ss);
    [bb, ss] = reconcileSwcParamHygieneRenames(bb, ss);
    bb = reconcileReactCreateElementSpreadCollapse(bb);
    ss = reconcileReactCreateElementSpreadCollapse(ss);
    cat = bb === ss ? 'PARITY' : 'DIVERGE';
  }
  console.log(`${name.padEnd(50)} babel=${tBabel.toFixed(0)}ms swc=${tSwc.toFixed(0)}ms ${cat}${err ? ' ' + err : ''}`);
}
