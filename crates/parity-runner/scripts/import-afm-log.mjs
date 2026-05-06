#!/usr/bin/env bun
// Import the AFM divergence log into a parity-runner corpus directory.
//
// Input format (see tmp_rovodev_css_parity_failing_inputs.log):
//   - lines starting with `#` are comments
//   - all other non-empty lines are JSON-encoded CSS strings
//
// Output: one .css file per unique input under <corpus-dir>. Filenames are
// `<sha8>_<index>.css` so:
//   - the SHA-256 prefix gives stable, content-addressed identity across
//     re-imports (same input → same filename), which means corpus diffs
//     between log captures are obvious.
//   - the original line index disambiguates any pathological collisions
//     and preserves source order for human inspection.
//
// Duplicate inputs (same CSS appearing on multiple lines of the log) are
// deduplicated — a single fixture covers all occurrences.
//
// Usage:
//   bun crates/parity-runner/scripts/import-afm-log.mjs \
//       <log-path> <corpus-dir>

import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [logPath, corpusDir] = process.argv.slice(2);
if (!logPath || !corpusDir) {
  console.error('usage: import-afm-log.mjs <log-path> <corpus-dir>');
  process.exit(2);
}

mkdirSync(corpusDir, { recursive: true });
// Wipe any prior import — we re-derive deterministically from the log.
for (const f of readdirSync(corpusDir)) {
  if (f.endsWith('.css')) unlinkSync(join(corpusDir, f));
}

const lines = readFileSync(logPath, 'utf8').split('\n');
const seen = new Map();
let parseErrors = 0;
lines.forEach((raw, idx) => {
  const line = raw.trim();
  if (!line || line.startsWith('#')) return;
  let css;
  try {
    css = JSON.parse(line);
  } catch (e) {
    parseErrors += 1;
    return;
  }
  if (typeof css !== 'string') {
    parseErrors += 1;
    return;
  }
  const sha = createHash('sha256').update(css).digest('hex').slice(0, 8);
  if (!seen.has(sha)) {
    seen.set(sha, { idx, css });
  }
});

const entries = [...seen.entries()].sort((a, b) => a[1].idx - b[1].idx);
entries.forEach(([sha, { idx, css }], rank) => {
  const name = `${String(rank).padStart(5, '0')}_${sha}.css`;
  writeFileSync(join(corpusDir, name), css);
});

console.log(
  `imported ${entries.length} unique fixtures (${seen.size} unique / ` +
    `${lines.filter((l) => l.trim() && !l.trim().startsWith('#')).length} log lines, ` +
    `${parseErrors} parse errors) → ${corpusDir}`,
);
