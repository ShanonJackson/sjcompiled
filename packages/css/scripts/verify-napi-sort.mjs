#!/usr/bin/env node
// One-shot verification: every fixture in `crates/parity-runner/corpus/sort/`
// is fed through both the JS `sort()` (the oracle) and the Rust NAPI
// `sort()` (via `@compiled/css-native`). Asserts byte equality.
//
// This is the integration sibling of the `Stage::Sort` Rust harness —
// the harness proves Rust-`crates/css` matches JS-`sort.ts`; this script
// additionally proves the NAPI marshaling layer didn't add or strip
// any bytes (UTF-16/UTF-8 string round-trip, error wrapping).

import fs from 'node:fs';
import path from 'node:path';
import { sort as jsSort } from '../src/sort.ts';
import { sort as rustSort } from '../../css-native/index.js';

const corpus = path.join(import.meta.dirname, '..', '..', '..', 'crates', 'parity-runner', 'corpus', 'sort');
const entries = fs.readdirSync(corpus).filter((f) => f.endsWith('.css')).sort();

let failures = 0;
for (const file of entries) {
  const css = fs.readFileSync(path.join(corpus, file), 'utf8');
  const js = jsSort(css);
  const rs = rustSort(css, null);
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
console.log(`\n${failures === 0 ? 'OK' : 'FAIL'} — ${entries.length - failures}/${entries.length} byte-clean (JS vs Rust NAPI)`);
process.exit(failures === 0 ? 0 : 1);
