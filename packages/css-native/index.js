// Platform-binary loader for the Rust NAPI backend.
//
// Phase 8a (sort only): a single platform binary
// `compiled-css.win32-x64-msvc.node` is shipped. Future phases will
// add Linux + Darwin builds; the loader below picks the right one based
// on `process.platform` + `process.arch`.

'use strict';

const path = require('path');

function platformBinaryName() {
  const platform = process.platform;
  const arch = process.arch;
  // The triple convention matches napi-rs's standard output naming so
  // that future cross-platform builds drop in without code changes.
  if (platform === 'win32' && arch === 'x64') return 'compiled-css.win32-x64-msvc.node';
  if (platform === 'linux' && arch === 'x64') return 'compiled-css.linux-x64-gnu.node';
  if (platform === 'linux' && arch === 'arm64') return 'compiled-css.linux-arm64-gnu.node';
  if (platform === 'darwin' && arch === 'x64') return 'compiled-css.darwin-x64.node';
  if (platform === 'darwin' && arch === 'arm64') return 'compiled-css.darwin-arm64.node';
  throw new Error(
    `@compiled/css-native: no prebuilt binary for ${platform}-${arch}. ` +
      `Phase 8a ships win32-x64-msvc only. Build from source via ` +
      `\`cargo build -p compiled-css-napi --release\` and copy the produced ` +
      `\`compiled_css_napi.dll\` (or platform equivalent) here.`
  );
}

const binary = require(path.join(__dirname, platformBinaryName()));

module.exports.sort = binary.sort;
module.exports.autoprefixer = binary.autoprefixer;
module.exports.transformCss = binary.transformCss;
// Optional perf knob — produces a postcard `Buffer` of the autoprefixer
// prefix tables. Pass it back via `opts.precomputedPrefixes` on every
// `transformCss` call to skip the per-call autoprefixer setup cost.
// Byte-equal to omitting it.
module.exports.precomputePrefixesDefault = binary.precomputePrefixesDefault;
// Optional perf / correctness knob — produces a postcard `Buffer`
// of the host-resolved browserslist snapshot. Pass it back via
// `opts.precomputedBrowserslist` (or write to disk and use
// `opts.precomputedBrowserslistPath`) on every `transformCss` call.
// Required for correct WASI behaviour with non-default
// browserslist configs; optional but cheap in NAPI. See
// `DEFINITIVE_BROWSERSLIST_PLAN.md`.
module.exports.precomputeBrowserslistDefault = binary.precomputeBrowserslistDefault;
