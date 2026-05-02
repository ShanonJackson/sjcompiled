#!/usr/bin/env node
// Emit JSON test vectors of `colord(input).minify(opts)` produced by upstream
// JS @ colord@2.9.3. Run from the workspace root:
//   node packages/css/scripts/colord-minify-vectors.mjs > crates/colord/tests/minify_vectors.json
//
// Lives here (not under crates/) so Node's module resolver finds the
// `colord` and `colord/plugins/*` packages from
// `packages/css/node_modules/` (added as devDependencies for the parity
// harness — same pattern as Phase 6c's postcss-minify-selectors).
//
// Output schema: { input, opts, expected }[].
//
// Vectors cover:
//  - hex inputs across the 2dp-friendly alpha pairs (33,66,99,cc) and
//    lossy ones (88, 8c) that should skip the hex form.
//  - rgb()/rgba() inputs incl. fractional alphas that miss the 2dp grid.
//  - hsl()/hsla() inputs.
//  - named-color round trips.
//  - rgba(0,0,0,0) — `transparent` shortcut path.
//  - 4-char-collapsible hex (#aabbcc -> #abc).
//  - non-collapsible hex (#ab12cc).
//  - opt permutations: defaults; all-true; alpha_hex off; name+transparent on.

import { colord, extend } from 'colord';
import namesPlugin from 'colord/plugins/names';
import minifyPlugin from 'colord/plugins/minify';
extend([namesPlugin, minifyPlugin]);

const OPT_PRESETS = {
  default: undefined,
  // postcss-colormin's effective defaults when no IE8/9 + caniuse rrggbbaa
  // is supported. (transparent: true, alphaHex: true, name: true).
  colormin_modern: { transparent: true, alphaHex: true, name: true },
  // colormin with IE8/9 in target — transparent disabled.
  colormin_ie89: { transparent: false, alphaHex: false, name: true },
  all_true: { hex: true, rgb: true, hsl: true, name: true, transparent: true, alphaHex: true },
  hex_off: { hex: false, rgb: true, hsl: true, name: true, transparent: true, alphaHex: true },
  rgb_only: { hex: false, rgb: true, hsl: false },
  hsl_only: { hex: false, rgb: false, hsl: true },
  alphahex_off: { hex: true, rgb: true, hsl: true, alphaHex: false },
};

const INPUTS = [
  // Hex — solid, collapsible.
  '#ff0000', '#00ff00', '#0000ff', '#ffffff', '#000000',
  '#aabbcc', '#112233', '#abcdef',
  // Hex — solid, NON-collapsible.
  '#ab12cc', '#fa0010', '#123456',
  // Hex with alpha — collapsible alpha pair.
  '#aabbcc99', '#aabbcc66', '#aabbcc33', '#aabbcccc',
  // Hex with alpha — non-collapsible alpha pair (RGB pairs match but alpha doesn't).
  '#aabbcc88', '#aabbcc11',
  // Hex with alpha — RGB pair mismatch.
  '#a0b1c299',
  // 3- and 4-char hex.
  '#abc', '#fff', '#000', '#abcd',
  // rgb() integer.
  'rgb(255,0,0)', 'rgb(170,187,204)', 'rgb(255, 255, 255)',
  // rgba() with fractional alpha — clean (round-trips through 2dp).
  'rgba(255,0,0,0.5)', 'rgba(170,187,204,0.5)', 'rgba(255,0,0,0.25)',
  'rgba(170,187,204,0.6)', 'rgba(0,0,0,0.4)', 'rgba(255,255,255,0.8)',
  // rgba() with fractional alpha — lossy (skips hex).
  'rgba(255,0,0,0.502)', 'rgba(170,187,204,0.555)', 'rgba(0,0,0,0.333)',
  // rgba(0,0,0,0) — transparent shortcut path.
  'rgba(0,0,0,0)',
  // rgba with alpha 1 (should match rgb()).
  'rgba(255,0,0,1)',
  // hsl().
  'hsl(0,100%,50%)', 'hsl(120,100%,50%)', 'hsl(240,100%,50%)',
  // hsla() with fractional alpha.
  'hsla(0,100%,50%,0.5)', 'hsla(120,50%,50%,0.25)',
  // Named.
  'red', 'blue', 'aliceblue', 'silver', 'rebeccapurple',
  // Edge: alpha=1 via rgba shouldn't emit alpha hex.
  'rgba(170,187,204,1)',
  // White & black — name candidates.
  'white', 'black',
  // Single-digit collapse with alpha.
  'rgba(255,0,0,0.6)', 'rgba(0,255,0,0.6)',
];

const out = [];
for (const input of INPUTS) {
  for (const [presetName, opts] of Object.entries(OPT_PRESETS)) {
    const c = colord(input);
    if (!c.isValid()) continue;
    const expected = c.minify(opts);
    out.push({ input, preset: presetName, opts: opts ?? null, expected });
  }
}

process.stdout.write(JSON.stringify(out, null, 2));
process.stdout.write('\n');
