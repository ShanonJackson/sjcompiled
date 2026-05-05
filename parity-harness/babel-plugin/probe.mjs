import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { babelEngine, swcEngine } from './engines.ts';

const file = process.argv[2];
const fx = JSON.parse(readFileSync(resolve(file), 'utf8'));
console.log('=== source ===');
console.log(fx.source);
console.log('\n=== babel ===');
try { console.log(babelEngine(fx.source, fx.opts)); } catch (e) { console.log('THROW: ' + e.message); }
console.log('\n=== swc ===');
try { console.log(swcEngine(fx.source, fx.opts)); } catch (e) { console.log('THROW: ' + e.message); }
