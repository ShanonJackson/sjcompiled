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
