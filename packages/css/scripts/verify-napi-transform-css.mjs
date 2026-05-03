#!/usr/bin/env node
// One-shot verification: every fixture in
// `crates/parity-runner/corpus/transform-css/` is fed through both the
// JS `transformCss()` (the oracle) and the Rust NAPI `transformCss()`
// (via `@sjcompiled/css-native`). Asserts byte equality on the
// JSON-stringified `{ sheets, classNames }` output.
//
// Sibling of `verify-napi-sort.mjs` and `verify-napi-autoprefixer.mjs`.
// The parity-runner Rust harness proves Rust-`crates/css::transform`
// matches JS-`transformCss`; this script additionally proves the NAPI
// marshaling layer didn't add or strip any bytes (UTF-16/UTF-8 round-
// trip through napi-rs's String marshalling, the IndexMap-vs-HashMap
// insertion-order plumbing for `classNameCompressionMap`, the result-
// vec → JS array marshalling, and the error-string marshalling).
//
// Both engines pin browserslist to `chrome 100` so the autoprefixer
// step + the 5 browserslist-aware cssnano sub-plugins (colormin,
// convert-values, minify-params, normalize-unicode, reduce-initial)
// resolve to a known target. AUTOPREFIXER is explicitly cleared so
// autoprefixer runs on both engines (the JS check is `=== 'off'`;
// unset → runs).

import fs from 'node:fs';
import path from 'node:path';
import { transformCss as jsTransformCss } from '../src/transform.ts';
import { transformCss as rustTransformCss } from '../../css-native/index.js';

const corpus = path.join(
  import.meta.dirname,
  '..',
  '..',
  '..',
  'crates',
  'parity-runner',
  'corpus',
  'transform-css',
);
const entries = fs
  .readdirSync(corpus)
  .filter((f) => f.endsWith('.css'))
  .sort();

// Pin env state for both engines. Restore on exit.
const prevBrowserslist = process.env.BROWSERSLIST;
const prevAutoprefixer = process.env.AUTOPREFIXER;
const prevEngine = process.env.COMPILED_CSS_ENGINE;
process.env.BROWSERSLIST = 'chrome 100';
delete process.env.AUTOPREFIXER;
// Force the JS engine path on `jsTransformCss` calls — the Rust path
// is invoked directly via `rustTransformCss`.
delete process.env.COMPILED_CSS_ENGINE;

let failures = 0;
try {
  for (const file of entries) {
    const css = fs.readFileSync(path.join(corpus, file), 'utf8');
    // Both calls take {} for opts — same default-opts surface that
    // production AFM consumers hit.
    const jsResult = jsTransformCss(css, {});
    const rsResult = rustTransformCss(css, null);
    // Field-order-pinned canonical JSON: `sheets` then `classNames`,
    // matching JS object-literal construction order. JSON.stringify
    // walks own-enumerable string keys in insertion order in V8.
    const js = JSON.stringify({
      sheets: jsResult.sheets,
      classNames: jsResult.classNames,
    });
    const rs = JSON.stringify({
      sheets: rsResult.sheets,
      classNames: rsResult.classNames,
    });
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
} finally {
  if (prevBrowserslist === undefined) delete process.env.BROWSERSLIST;
  else process.env.BROWSERSLIST = prevBrowserslist;
  if (prevAutoprefixer === undefined) delete process.env.AUTOPREFIXER;
  else process.env.AUTOPREFIXER = prevAutoprefixer;
  if (prevEngine === undefined) delete process.env.COMPILED_CSS_ENGINE;
  else process.env.COMPILED_CSS_ENGINE = prevEngine;
}
console.log(
  `\n${failures === 0 ? 'OK' : 'FAIL'} — ${entries.length - failures}/${entries.length} byte-clean (JS vs Rust NAPI)`,
);
process.exit(failures === 0 ? 0 : 1);
