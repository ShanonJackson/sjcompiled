#!/usr/bin/env node
// Caniuse-lite snapshot: unpacks every feature in
// `crates/_vendor/caniuse-lite-1.0.30001766/package/data/features/`
// using the upstream unpacker, then writes a single JSON file at
// `crates/caniuse-db/data/features.snapshot.json`.
//
// Re-run only when `crates/_vendor/caniuse-lite-1.0.30001766` is refreshed
// (which per `crates/PARITY_VERSIONS.md` we never do — caniuse-lite is
// frozen at 1.0.30001766 forever).

const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..', '_vendor',
  'caniuse-lite-1.0.30001766', 'package');
const featureUnpacker = require(path.join(ROOT, 'dist', 'unpacker', 'feature.js'));
const agentUnpackerMod = require(path.join(ROOT, 'dist', 'unpacker', 'agents.js'));

const featuresDir = path.join(ROOT, 'data', 'features');
const featuresIndex = require(path.join(ROOT, 'data', 'features.js'));
const browsers = require(path.join(ROOT, 'data', 'browsers.js'));

// Upstream agents unpacker pre-evaluates against the bundled data, so the
// `.agents` export is the unpacked map directly.
const agents = agentUnpackerMod.agents;

// Unpack every feature.
const features = {};
for (const name of Object.keys(featuresIndex)) {
  // Each feature lives in its own file: data/features/<name>.js.
  const filePath = path.join(featuresDir, name + '.js');
  if (!fs.existsSync(filePath)) continue;
  const packed = require(filePath);
  features[name] = featureUnpacker(packed);
}

const out = path.resolve(__dirname, '..', 'data', 'features.snapshot.json');
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify({
  caniuseLiteVersion: '1.0.30001766',
  browsers,
  agents,
  features,
}));
console.log('Wrote', out, `(${Object.keys(features).length} features)`);
