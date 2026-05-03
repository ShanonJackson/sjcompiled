#!/usr/bin/env node
// One-shot verification: every fixture in
// `crates/parity-runner/corpus/autoprefixer/` is fed through both the JS
// `autoprefixer@10.4.14` (the oracle) and the Rust NAPI `autoprefixer()`
// (via `@sjcompiled/css-native`). Asserts byte equality.
//
// Sibling of `verify-napi-sort.mjs`. The parity-runner Rust harness
// proves Rust-`crates/autoprefixer` matches JS-`autoprefixer@10.4.14`;
// this script additionally proves the NAPI marshaling layer didn't add
// or strip any bytes (UTF-16/UTF-8 string round-trip, error wrapping,
// `from` option threading).
//
// Both engines pin browserslist resolution to AFM's `.browserslistrc`
// fixture via the production walk path:
// - JS: postcss `from:` set to a file inside the AFM fixture dir.
// - Rust NAPI: `from:` opt threaded through `BrowsersOptions::from`.
// Without the pin, autoprefixer's internal browserslist call would walk
// from `node_modules/autoprefixer/lib/` (JS) / cwd (Rust), which can
// drift across runs and across machines.

import fs from 'node:fs';
import path from 'node:path';
import postcss from 'postcss';
import autoprefixer from 'autoprefixer';
import { autoprefixer as rustAutoprefixer } from '../../css-native/index.js';

const corpus = path.join(
  import.meta.dirname,
  '..',
  '..',
  '..',
  'crates',
  'parity-runner',
  'corpus',
  'autoprefixer',
);
const entries = fs
  .readdirSync(corpus)
  .filter((f) => f.endsWith('.css'))
  .sort();

// Mirrors the parity-bridge.mjs and the Rust stages.rs `afm_browserslist_dir()`
// helper. Both engines must resolve the SAME 14-entry browser list
// through the SAME directory walk path.
const AFM_BROWSERSLIST_DIR = path.resolve(
  import.meta.dirname,
  '..',
  '..',
  '..',
  'crates',
  'browserslist-shim',
  'tests',
  'fixtures',
  'afm',
);
const AFM_FROM = path.resolve(AFM_BROWSERSLIST_DIR, '_parity_input.css');

function jsAutoprefix(css) {
  // Clear env vars that would short-circuit the production walk.
  const prevQuery = process.env.BROWSERSLIST;
  const prevConfig = process.env.BROWSERSLIST_CONFIG;
  delete process.env.BROWSERSLIST;
  delete process.env.BROWSERSLIST_CONFIG;
  try {
    return postcss([autoprefixer()]).process(css, { from: AFM_FROM }).css;
  } finally {
    if (prevQuery !== undefined) process.env.BROWSERSLIST = prevQuery;
    if (prevConfig !== undefined) process.env.BROWSERSLIST_CONFIG = prevConfig;
  }
}

let failures = 0;
for (const file of entries) {
  const css = fs.readFileSync(path.join(corpus, file), 'utf8');
  const js = jsAutoprefix(css);
  const rs = rustAutoprefixer(css, { from: AFM_FROM });
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
console.log(
  `\n${failures === 0 ? 'OK' : 'FAIL'} — ${entries.length - failures}/${entries.length} byte-clean (JS vs Rust NAPI)`,
);
process.exit(failures === 0 ? 0 : 1);
