#!/usr/bin/env node
// Caniuse-lite snapshot: unpacks every feature in
// `crates/_vendor/caniuse-lite-1.0.30001766/package/data/features/`
// using the upstream unpacker, applies the PRUNE_POLICY below to drop
// version-level data far below AFM's resolved browserslist matrix, then
// writes a single JSON file at `crates/caniuse-db/data/features.snapshot.json`.
//
// Re-run only when `crates/_vendor/caniuse-lite-1.0.30001766` is refreshed
// (which per `crates/PARITY_VERSIONS.md` we never do — caniuse-lite is
// frozen at 1.0.30001766 forever).
//
// PRUNE_POLICY (2026-05-14):
//   Drops ~69% of the snapshot size by removing version-level data for
//   browsers/versions that AFM's `.browserslistrc` cannot resolve to
//   even after generous expansion. AFM resolves to roughly:
//     last 2 Edge (~143-144), last 2 Firefox (~141-142) + ESR (115, 128),
//     last 5 Chrome (~140-144), last 2 Safari (~18.5-18.6),
//     last 2 iOS (~18.5-18.6), last 2 ChromeAndroid (~143).
//   Floors sit ~1 year below those numbers so AFM can widen its matrix
//   (e.g., last 10 Chrome) without snapshot regen.
//
//   IMPORTANT: every agent stays in the agents map even if all its
//   versions are dropped. `crates/autoprefixer/src/browsers.rs::build_prefixes`
//   iterates `caniuse_db::AGENTS` and uses `agent.prefix` to construct
//   the prefix-recognition set (`-ms-`, `-webkit-`, `-moz-`, `-o-`).
//   Dropping the `ie` agent entirely would strip `-ms-` from that set
//   and silently break autoprefixer's recognition of `-ms-flex` etc.
//   in user input. Pruning operates on version-level data ONLY.

const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..', '_vendor',
  'caniuse-lite-1.0.30001766', 'package');
const featureUnpacker = require(path.join(ROOT, 'dist', 'unpacker', 'feature.js'));
const agentUnpackerMod = require(path.join(ROOT, 'dist', 'unpacker', 'agents.js'));

const featuresDir = path.join(ROOT, 'data', 'features');
const featuresIndex = require(path.join(ROOT, 'data', 'features.js'));
const browsers = require(path.join(ROOT, 'data', 'browsers.js'));

const agents = agentUnpackerMod.agents;

const features = {};
for (const name of Object.keys(featuresIndex)) {
  const filePath = path.join(featuresDir, name + '.js');
  if (!fs.existsSync(filePath)) continue;
  const packed = require(filePath);
  features[name] = featureUnpacker(packed);
}

// ---- PRUNE POLICY ---------------------------------------------------------

// Minimum kept version per browser. Anything strictly below is dropped from
// agent.versions/.release_date/.usage_global AND from feature.stats[browser].
const FLOORS = {
  chrome:  120,
  edge:    120,
  firefox: 100,   // covers Firefox ESR (115, 128) + current cycle
  safari:   16,
  ios_saf:  16,
  and_chr: 120,
  samsung:  20,
  opera:   100,
};

// Dead browsers: keep the agent stub (for `agent.prefix` discovery — see the
// build_prefixes comment at top of file) but empty all version-level data.
// `features.stats[<browser>]` is dropped entirely.
const DEAD_BROWSERS = new Set([
  'ie', 'ie_mob',
  'op_mini', 'op_mob',
  'bb',
  'and_uc', 'and_qq', 'baidu', 'kaios',
  'android',
  'and_ff',
]);

function keepVersion(browser, versionString) {
  if (DEAD_BROWSERS.has(browser)) return false;
  // Range strings like "12.0-12.5" — use the lower bound.
  const lo = String(versionString).split('-')[0];
  const num = parseFloat(lo);
  if (Number.isNaN(num)) return true; // "TP", "all" etc — keep
  const floor = FLOORS[browser];
  if (floor == null) return true; // unfloored browsers fall through
  return num >= floor;
}

function pruneAgents(agents) {
  const out = {};
  for (const [name, a] of Object.entries(agents)) {
    const a2 = { ...a };
    if (DEAD_BROWSERS.has(name)) {
      a2.versions = [];
      a2.release_date = {};
      a2.usage_global = {};
      // prefix_exceptions is a per-version override map — empty it too.
      if (a2.prefix_exceptions) a2.prefix_exceptions = {};
    } else {
      a2.versions = (a.versions || []).map(v => (v == null || keepVersion(name, v)) ? v : null);
      a2.release_date = filterKeys(a.release_date || {}, v => keepVersion(name, v));
      a2.usage_global = filterKeys(a.usage_global || {}, v => keepVersion(name, v));
      if (a.prefix_exceptions) {
        a2.prefix_exceptions = filterKeys(a.prefix_exceptions, v => keepVersion(name, v));
      }
    }
    out[name] = a2;
  }
  return out;
}

function pruneFeatures(features) {
  const out = {};
  for (const [fid, f] of Object.entries(features)) {
    const f2 = { ...f };
    const stats = f.stats || {};
    const s2 = {};
    for (const [br, vmap] of Object.entries(stats)) {
      if (DEAD_BROWSERS.has(br)) continue;
      const kept = filterKeys(vmap, v => keepVersion(br, v));
      if (Object.keys(kept).length > 0) s2[br] = kept;
    }
    f2.stats = s2;
    out[fid] = f2;
  }
  return out;
}

function filterKeys(obj, pred) {
  const out = {};
  for (const k of Object.keys(obj)) if (pred(k)) out[k] = obj[k];
  return out;
}

// ---- WRITE ----------------------------------------------------------------

const prunedAgents = pruneAgents(agents);
const prunedFeatures = pruneFeatures(features);

const out = path.resolve(__dirname, '..', 'data', 'features.snapshot.json');
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify({
  caniuseLiteVersion: '1.0.30001766',
  prunePolicy: {
    version: 1,
    floors: FLOORS,
    deadBrowsers: [...DEAD_BROWSERS].sort(),
  },
  browsers,
  agents: prunedAgents,
  features: prunedFeatures,
}));
console.log('Wrote', out, `(${Object.keys(prunedFeatures).length} features, pruned)`);
