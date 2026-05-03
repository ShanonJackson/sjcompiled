import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const list = require('postcss').list;
const v = `-webkit-cross-fade('a.png'), url('b.png'), 50%,  50%)`;
const tokens = list.space(v);
console.log('tokens:', JSON.stringify(tokens));
console.log('joined:', JSON.stringify(tokens.join(' ')));
