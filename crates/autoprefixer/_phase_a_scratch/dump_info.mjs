#!/usr/bin/env node
// Dump `autoprefixer.info()` for AFM's browserslist so we know which
// prefixer/hack instances are even constructed for AFM's targets.
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);

const AFM_BROWSERSLIST = [
  'last 2 Edge version',
  'last 2 Firefox version',
  'last 5 Chrome version',
  'last 2 Safari version',
  'last 2 iOS version',
  'last 2 ChromeAndroid version',
];

process.env.BROWSERSLIST = AFM_BROWSERSLIST.join(',');
process.env.BROWSERSLIST_DISABLE_CACHE = '1';

const autoprefixer = require('autoprefixer');
const Prefixer = require('autoprefixer/lib/prefixer');

const ap = autoprefixer({ overrideBrowserslist: AFM_BROWSERSLIST });
const info = ap.info({ from: process.cwd() });

console.log('=== autoprefixer.info() for AFM browserslist ===');
console.log(info);

// Also dump structured prefixer set.
// We need to recall loadPrefixes internals — easier: call the `prepare`
// step with a tiny CSS to materialise the cache, then read the cached
// Prefixes instance.
const postcss = require('postcss');
const p = postcss([ap]);
const result = p.process(' ', { from: 'a.css', to: 'a.css' });
void result.css; // force evaluation

// Pull the cached Prefixes from the autoprefixer module's `cache` map.
// It's not exported, so we re-create one and inspect.
const Browsers = require('autoprefixer/lib/browsers');
const Prefixes = require('autoprefixer/lib/prefixes');
const dataPrefixes = require('autoprefixer/data/prefixes');
const { agents } = require('caniuse-lite/dist/unpacker/agents');

const browsers = new Browsers({ ...agents }, AFM_BROWSERSLIST, { from: process.cwd() }, {});
const prefixes = new Prefixes(dataPrefixes, browsers, {});

console.log('\n=== prefixes.add (declaration/value bucket — keys) ===');
const declValueAddKeys = Object.keys(prefixes.add).filter(k => k !== 'selectors');
declValueAddKeys.sort();
for (const k of declValueAddKeys) {
  const p = prefixes.add[k];
  const className = p && p.constructor && p.constructor.name;
  const prefixList = p && p.prefixes;
  console.log(`  ${k.padEnd(36)} -> ${className.padEnd(28)}  prefixes=${JSON.stringify(prefixList)}`);
}

console.log('\n=== prefixes.add.selectors ===');
for (const sel of prefixes.add.selectors || []) {
  const className = sel.constructor && sel.constructor.name;
  console.log(`  ${(sel.name || '').padEnd(36)} -> ${className.padEnd(28)}  prefixes=${JSON.stringify(sel.prefixes)}`);
}

console.log('\n=== prefixes.transition ===');
console.log(`  class=${prefixes.transition && prefixes.transition.constructor && prefixes.transition.constructor.name}`);
console.log(`  prefixes=${prefixes.transition && Array.isArray(prefixes.transition.prefixes) ? prefixes.transition.prefixes.join(',') : '(none)'}`);

console.log('\n=== prefixes.add[*].values (Value-bucket hacks) ===');
for (const k of declValueAddKeys) {
  const p = prefixes.add[k];
  if (!p || !p.values) continue;
  for (const v of p.values) {
    const className = v.constructor && v.constructor.name;
    console.log(`  ${k.padEnd(20)} value=${(v.name || '').padEnd(20)} -> ${className.padEnd(20)} prefixes=${JSON.stringify(v.prefixes)}`);
  }
}

console.log('\n=== resolved selected browsers ===');
console.log(JSON.stringify(browsers.selected, null, 2));

// Now examine prefixes.remove — used by processor.remove for the cleanup pass.
console.log('\n=== prefixes.remove (cleanup pass) ===');
const removeKeys = Object.keys(prefixes.remove).filter(k => k !== 'selectors').sort();
for (const k of removeKeys) {
  const p = prefixes.remove[k];
  if (!p || typeof p !== 'object') {
    console.log(`  ${k.padEnd(36)} -> ${typeof p}`);
    continue;
  }
  const className = p.constructor && p.constructor.name;
  console.log(`  ${k.padEnd(36)} -> ${className}`);
}
console.log('\n=== prefixes.remove.selectors (cleanup pass) ===');
for (const sel of prefixes.remove.selectors || []) {
  const className = sel.constructor && sel.constructor.name;
  console.log(`  ${(sel.name || '<unknown>').padEnd(36)} -> ${className}`);
}
