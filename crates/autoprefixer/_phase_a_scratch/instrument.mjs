#!/usr/bin/env node
// AFM hack instrumentation for Phase A.
//
// Wraps every dispatch point in autoprefixer's class hierarchy so we can
// count, per hack class, how many times it actually mutated a node when
// running against an AFM-shaped CSS corpus on AFM's exact browserslist.
//
// "Worked" vs "offered": Prefixer.process is invoked on every node a
// prefixer's bucket touches, but the first thing it does is `check(node)`
// — most invocations early-return undefined. Recording every entry would
// massively overcount (e.g. CrossFade.process runs for every decl in the
// corpus, almost always no-op). We only count an invocation if the
// return value indicates real work (an `added` array of length >= 1).
//
// Output: aggregated counts written to stdout as JSON.
//
// Usage:
//   bun run instrument.mjs <corpus-dir-or-file> [<corpus-dir-or-file> ...]

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, join, basename } from 'node:path';
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

const Prefixer = require('autoprefixer/lib/prefixer');
const AtRule = require('autoprefixer/lib/at-rule');
const Resolution = require('autoprefixer/lib/resolution');
const Supports = require('autoprefixer/lib/supports');
const Transition = require('autoprefixer/lib/transition');
const autoprefixer = require('autoprefixer');
const postcss = require('postcss');

// Aggregated: {className: {dispatchKind: {propOrName: {worked, offered}}}}
const HITS = Object.create(null);

function record(className, dispatchKind, name, didWork) {
  if (!HITS[className]) HITS[className] = Object.create(null);
  if (!HITS[className][dispatchKind]) HITS[className][dispatchKind] = Object.create(null);
  const key = name || '<unknown>';
  if (!HITS[className][dispatchKind][key]) {
    HITS[className][dispatchKind][key] = { worked: 0, offered: 0 };
  }
  HITS[className][dispatchKind][key].offered += 1;
  if (didWork) HITS[className][dispatchKind][key].worked += 1;
}

function wrapWithReturnCheck(klass, methodName, dispatchKind) {
  const orig = klass.prototype[methodName];
  if (!orig) return;
  klass.prototype[methodName] = function (...args) {
    const ret = orig.apply(this, args);
    const didWork = Array.isArray(ret) && ret.length > 0;
    record(this.constructor.name, dispatchKind, this.name, didWork);
    return ret;
  };
}

function wrapAlwaysWorked(klass, methodName, dispatchKind) {
  const orig = klass.prototype[methodName];
  if (!orig) return;
  klass.prototype[methodName] = function (...args) {
    const ret = orig.apply(this, args);
    record(this.constructor.name, dispatchKind, this.name, true);
    return ret;
  };
}

// Prefixer.process is the central dispatcher for Selector / Value /
// Declaration hacks (Declaration.process internally calls super.process,
// which IS Prefixer.process; `this.constructor.name` is the hack class).
wrapWithReturnCheck(Prefixer, 'process', 'process');
// AtRule has its own process that doesn't go through Prefixer.process.
wrapAlwaysWorked(AtRule, 'process', 'at-rule.process');
// Resolution has its own process.
wrapAlwaysWorked(Resolution, 'process', 'resolution.process');
// Supports has its own process.
wrapAlwaysWorked(Supports, 'process', 'supports.process');
// Transition is the only base class that has neither process nor a
// Prefixer parent — `add` / `remove` are the entry points called from
// processor.js.
wrapAlwaysWorked(Transition, 'add', 'transition.add');
wrapAlwaysWorked(Transition, 'remove', 'transition.remove');

function listCssFiles(target) {
  const stat = statSync(target);
  if (stat.isFile()) return [target];
  if (!stat.isDirectory()) return [];
  const out = [];
  const walk = (dir) => {
    for (const ent of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, ent.name);
      if (ent.isDirectory()) walk(full);
      else if (ent.isFile() && ent.name.toLowerCase().endsWith('.css')) out.push(full);
    }
  };
  walk(target);
  return out;
}

const inputs = process.argv.slice(2);
if (inputs.length === 0) {
  console.error('usage: bun run instrument.mjs <corpus-dir-or-file> [...]');
  process.exit(2);
}

const allFiles = inputs.flatMap((p) => listCssFiles(resolve(p)));
const processor = postcss([autoprefixer({ overrideBrowserslist: AFM_BROWSERSLIST })]);

let processed = 0;
let skipped = 0;
let errored = 0;
let totalBytes = 0;

for (const file of allFiles) {
  const css = readFileSync(file, 'utf8');
  if (!css.trim()) {
    skipped++;
    continue;
  }
  totalBytes += Buffer.byteLength(css, 'utf8');
  try {
    const result = processor.process(css, { from: file, to: file });
    void result.css;
    processed++;
  } catch (e) {
    errored++;
    process.stderr.write(`ERR ${file}: ${e.message}\n`);
  }
}

const out = {
  meta: {
    autoprefixer: require('autoprefixer/package.json').version,
    browserslist: require('browserslist/package.json').version,
    caniuseLite: require('caniuse-lite/package.json').version,
    afmBrowserslist: AFM_BROWSERSLIST,
    files: { total: allFiles.length, processed, skipped, errored, totalBytes },
  },
  hits: HITS,
};

process.stdout.write(JSON.stringify(out, null, 2) + '\n');
