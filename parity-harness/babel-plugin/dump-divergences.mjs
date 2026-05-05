import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const r = JSON.parse(
  readFileSync(resolve(import.meta.dirname, 'triage-report.json'), 'utf8'),
);

console.log(`# ${r.results.divergence.length} remaining divergences\n`);
for (const e of r.results.divergence) {
  console.log('---');
  console.log('NAME:', e.name);
  console.log('SRC :', e.sourceFile);
  console.log(e.diff);
  console.log();
}

console.log('\n# swc-throws');
for (const e of r.results['swc-throws']) {
  console.log('---');
  console.log('NAME:', e.name);
  console.log('SRC :', e.sourceFile);
  console.log('ERR :', (e.error || '').split('\n').slice(0, 5).join('\n'));
}

console.log('\n# babel-throws');
for (const e of r.results['babel-throws']) {
  console.log('---');
  console.log('NAME:', e.name);
  console.log('SRC :', e.sourceFile);
  console.log('ERR :', (e.error || '').split('\n').slice(0, 5).join('\n'));
}

console.log('\n# both-throw');
for (const e of r.results['both-throw']) {
  console.log('---');
  console.log('NAME:', e.name);
  console.log('SRC :', e.sourceFile);
}
