import { swcEngine, babelEngine } from './engines.ts';
import fs from 'fs';
const fname = process.argv[2];
const fixture = JSON.parse(fs.readFileSync(fname,'utf8'));
console.log('=== SOURCE ===');
console.log(fixture.source);
try {
  const out = await swcEngine(fixture.source, fixture.opts || {});
  console.log('=== SWC OUTPUT ===');
  console.log(out);
} catch(e) {
  console.error('=== SWC ERROR ===');
  console.error(e.message);
}
try {
  const out = await babelEngine(fixture.source, fixture.opts || {});
  console.log('=== BABEL OUTPUT ===');
  console.log(out);
} catch(e) {
  console.error('=== BABEL ERROR ===');
  console.error(e.message);
}
