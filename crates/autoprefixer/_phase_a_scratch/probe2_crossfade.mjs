import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const list = require('postcss').list;
const fullValue = `cross-fade(url('a.png'), url('b.png'), 50%)`;
console.log('list.space:', JSON.stringify(list.space(fullValue)));
const tokens = list.space(fullValue);
console.log('# tokens:', tokens.length);
