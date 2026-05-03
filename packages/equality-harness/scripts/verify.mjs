#!/usr/bin/env node
// Byte-equality harness.
//
// For every fixture in /fixtures (each has an `input.{js,jsx,tsx}`),
// run Babel twice with the same plugin chain
// `[@atlaskit/tokens/babel-plugin, @compiled/babel-plugin]`:
//
//   1. COMPILED_CSS_ENGINE unset → JS pipeline inside transformCss.
//   2. COMPILED_CSS_ENGINE=rust → NAPI delegate to the Rust pipeline.
//
// Byte-compare the two `result.code` strings. Identical bytes prove the
// Rust transformCss is observationally indistinguishable from the JS
// oracle when driven through the real Babel plugin pipeline that AFM
// runs in production.
//
// Per CLAUDE.md drift-detection: any divergence is reported with the
// smallest divergent byte range. We do NOT special-case fixtures.

import fs from 'node:fs';
import path from 'node:path';
import * as babel from '@babel/core';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..', '..');
const FIXTURES_DIR = path.join(REPO_ROOT, 'fixtures');

const args = process.argv.slice(2);
const BAIL = args.includes('--bail');
const onlyIdx = args.indexOf('--only');
const ONLY = onlyIdx >= 0 ? args.slice(onlyIdx + 1).filter((a) => !a.startsWith('--')) : null;

function findEntrypoint(dir) {
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = path.join(dir, `input.${ext}`);
    if (fs.existsSync(p)) return p;
  }
  return null;
}

function babelOptionsFor(entry) {
  const ext = path.extname(entry);
  const isTS = ext === '.tsx' || ext === '.ts';
  return {
    babelrc: false,
    configFile: false,
    filename: entry,
    sourceType: 'module',
    parserOpts: {
      plugins: ['jsx', ...(isTS ? ['typescript'] : [])],
    },
    plugins: [
      // Order matters: tokens FIRST, compiled SECOND.
      require.resolve('@atlaskit/tokens/babel-plugin'),
      require.resolve('@compiled/babel-plugin'),
    ],
  };
}

function runOnce(entry, source, engine) {
  const prev = process.env.COMPILED_CSS_ENGINE;
  if (engine === 'rust') process.env.COMPILED_CSS_ENGINE = 'rust';
  else delete process.env.COMPILED_CSS_ENGINE;
  try {
    const result = babel.transformSync(source, babelOptionsFor(entry));
    return { ok: true, code: result?.code ?? '' };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  } finally {
    if (prev === undefined) delete process.env.COMPILED_CSS_ENGINE;
    else process.env.COMPILED_CSS_ENGINE = prev;
  }
}

function smallestDivergence(a, b) {
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  const ctxStart = Math.max(0, i - 30);
  const ctxEnd = i + 60;
  return {
    byte: i,
    js: JSON.stringify(a.slice(ctxStart, ctxEnd)),
    rust: JSON.stringify(b.slice(ctxStart, ctxEnd)),
  };
}

const fixtures = fs
  .readdirSync(FIXTURES_DIR)
  .filter((name) => fs.statSync(path.join(FIXTURES_DIR, name)).isDirectory())
  .filter((name) => !ONLY || ONLY.includes(name))
  .sort();

let pass = 0;
let fail = 0;
let skipped = 0;
let bothErrored = 0;
const failures = [];

console.log(`Running ${fixtures.length} fixtures…\n`);

for (const name of fixtures) {
  const dir = path.join(FIXTURES_DIR, name);
  const entry = findEntrypoint(dir);
  if (!entry) {
    skipped++;
    continue;
  }
  const source = fs.readFileSync(entry, 'utf8');

  const js = runOnce(entry, source, 'js');
  const rs = runOnce(entry, source, 'rust');

  if (!js.ok && !rs.ok) {
    // Both engines errored. As long as the error messages match the
    // test still proves equivalence; otherwise it's drift.
    if (js.error === rs.error) {
      bothErrored++;
      pass++;
      console.log(`  both-errored ${name}: ${js.error.split('\n')[0]}`);
      continue;
    }
    fail++;
    failures.push({ name, kind: 'error-mismatch', js: js.error, rs: rs.error });
    if (BAIL) break;
    continue;
  }
  if (js.ok !== rs.ok) {
    fail++;
    failures.push({
      name,
      kind: 'one-errored',
      jsOk: js.ok,
      rsOk: rs.ok,
      js: js.ok ? null : js.error,
      rs: rs.ok ? null : rs.error,
    });
    if (BAIL) break;
    continue;
  }

  if (js.code === rs.code) {
    pass++;
    continue;
  }
  fail++;
  const div = smallestDivergence(js.code, rs.code);
  failures.push({ name, kind: 'byte-diff', ...div });
  if (BAIL) break;
}

console.log(`\n=== RESULTS ===`);
console.log(`Total fixtures:     ${fixtures.length}`);
console.log(`Skipped (no input): ${skipped}`);
console.log(`Pass:               ${pass}${bothErrored ? ` (of which ${bothErrored} errored identically on both engines)` : ''}`);
console.log(`Fail:               ${fail}`);

if (failures.length) {
  console.log(`\n=== FAILURES ===`);
  for (const f of failures.slice(0, 20)) {
    console.log(`\n  ${f.name} — ${f.kind}`);
    if (f.kind === 'byte-diff') {
      console.log(`    diverge at byte ${f.byte}`);
      console.log(`    JS  : ${f.js}`);
      console.log(`    RUST: ${f.rust}`);
    } else if (f.kind === 'error-mismatch') {
      console.log(`    JS  : ${f.js}`);
      console.log(`    RUST: ${f.rs}`);
    } else if (f.kind === 'one-errored') {
      console.log(`    js ok=${f.jsOk} rs ok=${f.rsOk}`);
      if (f.js) console.log(`    JS  : ${f.js}`);
      if (f.rs) console.log(`    RUST: ${f.rs}`);
    }
  }
  if (failures.length > 20) console.log(`\n  …${failures.length - 20} more`);
  process.exit(1);
}
