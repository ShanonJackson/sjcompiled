// Platform-binary loader for the Rust NAPI backend.
//
// Phase 8a (sort only): a single platform binary
// `sjcompiled-css.win32-x64-msvc.node` is shipped. Future phases will
// add Linux + Darwin builds; the loader below picks the right one based
// on `process.platform` + `process.arch`.

'use strict';

const path = require('path');

function platformBinaryName() {
  const platform = process.platform;
  const arch = process.arch;
  // The triple convention matches napi-rs's standard output naming so
  // that future cross-platform builds drop in without code changes.
  if (platform === 'win32' && arch === 'x64') return 'sjcompiled-css.win32-x64-msvc.node';
  if (platform === 'linux' && arch === 'x64') return 'sjcompiled-css.linux-x64-gnu.node';
  if (platform === 'linux' && arch === 'arm64') return 'sjcompiled-css.linux-arm64-gnu.node';
  if (platform === 'darwin' && arch === 'x64') return 'sjcompiled-css.darwin-x64.node';
  if (platform === 'darwin' && arch === 'arm64') return 'sjcompiled-css.darwin-arm64.node';
  throw new Error(
    `@sjcompiled/css-native: no prebuilt binary for ${platform}-${arch}. ` +
      `Phase 8a ships win32-x64-msvc only. Build from source via ` +
      `\`cargo build -p compiled-css-napi --release\` and copy the produced ` +
      `\`compiled_css_napi.dll\` (or platform equivalent) here.`
  );
}

const binary = require(path.join(__dirname, platformBinaryName()));

module.exports.sort = binary.sort;
module.exports.autoprefixer = binary.autoprefixer;
