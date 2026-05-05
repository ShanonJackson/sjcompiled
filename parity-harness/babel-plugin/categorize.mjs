import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const r = JSON.parse(
  readFileSync(resolve(import.meta.dirname, 'triage-report.json'), 'utf8'),
);

function key(sourceFile) {
  let k = sourceFile.replaceAll('\\', '/');
  const idx = k.indexOf('packages/babel-plugin/src/');
  if (idx >= 0) k = k.slice(idx + 'packages/babel-plugin/src/'.length);
  k = k.replace('/__tests__/', '/');
  k = k.replace(/\.test\.ts$/, '');
  return k;
}

const divGroups = {};
for (const e of r.results.divergence) {
  const k = key(e.sourceFile);
  divGroups[k] = (divGroups[k] || 0) + 1;
}
const parityGroups = {};
for (const e of r.results.parity) {
  const k = key(e.sourceFile);
  parityGroups[k] = (parityGroups[k] || 0) + 1;
}

const all = new Set([...Object.keys(divGroups), ...Object.keys(parityGroups)]);
const rows = [...all]
  .map((k) => ({ k, div: divGroups[k] || 0, par: parityGroups[k] || 0 }))
  .sort((a, b) => b.div - a.div);

console.log('=== Phase 6 §6.8 divergence breakdown ===');
console.log(
  'div'.padStart(4) +
    ' ' +
    'par'.padStart(4) +
    ' ' +
    'tot'.padStart(4) +
    '  source-file group',
);
console.log('-'.repeat(80));
let totDiv = 0,
  totPar = 0;
for (const { k, div, par } of rows) {
  totDiv += div;
  totPar += par;
  console.log(
    String(div).padStart(4) +
      ' ' +
      String(par).padStart(4) +
      ' ' +
      String(div + par).padStart(4) +
      '  ' +
      k,
  );
}
console.log('-'.repeat(80));
console.log(
  String(totDiv).padStart(4) +
    ' ' +
    String(totPar).padStart(4) +
    ' ' +
    String(totDiv + totPar).padStart(4) +
    '  TOTAL',
);

console.log('\n=== SWC-throw fixtures ===');
for (const e of r.results['swc-throws']) {
  console.log(' ', e.name);
  console.log('     ', (e.error || '').split('\n')[0]);
}

console.log('\n=== Sample divergence diffs (first per group, top 5 groups) ===');
for (const { k } of rows.slice(0, 5)) {
  const ex = r.results.divergence.find((e) => key(e.sourceFile) === k);
  console.log(`\n--- group: ${k} ---`);
  console.log(`name: ${ex.name}`);
  console.log(ex.diff);
}
