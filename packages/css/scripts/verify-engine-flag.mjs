#!/usr/bin/env node
// End-to-end sanity check for the COMPILED_CSS_ENGINE feature flag.
//
// This is the "real consumer" parity gate: instead of importing each
// engine separately and comparing them (which is what
// `verify-napi-sort.mjs` does), it goes through the *public*
// `packages/css/src/sort.ts` entry point under both flag settings and
// asserts byte-equality. If a future refactor of `sort.ts` accidentally
// drops the flag handling, this catches it before consumers see it.

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const corpus = path.join(import.meta.dirname, '..', '..', '..', 'crates', 'parity-runner', 'corpus', 'sort');
const entries = fs.readdirSync(corpus).filter((f) => f.endsWith('.css')).sort();

const bridge = path.join(import.meta.dirname, '_engine-bridge.mjs');
function runEngine(engine, css) {
  // Round-trip the CSS through `sort.ts` in a fresh subprocess so the
  // env var is read at module-load time, matching real consumer flow.
  return execFileSync('bun', ['run', bridge], {
    input: css,
    env: { ...process.env, COMPILED_CSS_ENGINE: engine },
    encoding: 'utf8',
    maxBuffer: 1 << 24,
  });
}

let failures = 0;
for (const file of entries) {
  const css = fs.readFileSync(path.join(corpus, file), 'utf8');
  const js = runEngine('js', css);
  const rs = runEngine('rust', css);
  if (js === rs) {
    console.log(`OK  ${file}`);
  } else {
    failures += 1;
    let i = 0;
    while (i < js.length && i < rs.length && js[i] === rs[i]) i++;
    console.log(`FAIL ${file} — diverge at byte ${i}`);
    console.log(`  JS:   ${JSON.stringify(js.slice(Math.max(0, i - 20), i + 60))}`);
    console.log(`  RUST: ${JSON.stringify(rs.slice(Math.max(0, i - 20), i + 60))}`);
  }
}
console.log(`\n${failures === 0 ? 'OK' : 'FAIL'} — ${entries.length - failures}/${entries.length} byte-clean (sort.ts under both engines)`);
process.exit(failures === 0 ? 0 : 1);
