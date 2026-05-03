// Directly call CrossFade.replace
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const CrossFade = require('autoprefixer/lib/hacks/cross-fade');
const inst = new CrossFade('cross-fade', ['-webkit-'], null);
const fullValue = `cross-fade(url('a.png'), url('b.png'), 50%)`;
const out = inst.replace(fullValue, '-webkit-');
console.log('JS output:', JSON.stringify(out));
console.log('length:', out.length);
