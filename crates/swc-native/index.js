"use strict";

// Drop-in subset of @swc/core's transformSync for the parity harness.
// Differences from @swc/core (intentional):
//   * Programs (pre-parsed AST input) are not supported. `src` must be
//     a string. The harness only ever passes source strings.
//   * `options.plugin` (legacy in-JS plugin) is not supported.
//   * `jsc.experimental.plugins` entries point at the same wasm path
//     the WASI build uses (e.g. `babel_plugin.wasm`); the native
//     dispatcher in `crates/swc-native/src/native_plugins.rs` looks at
//     the file basename and routes to the matching Rust pass.

const native = require("./binding.js");

const version = "0.0.1";

function toBuffer(value) {
  return Buffer.from(JSON.stringify(value));
}

function transformSync(src, options) {
  if (typeof src !== "string") {
    throw new TypeError("@compiled/swc-native: transformSync(src, …) — src must be a string");
  }
  options = options || {};
  if (options?.jsc?.parser) {
    options.jsc.parser.syntax = options.jsc.parser.syntax ?? "ecmascript";
  }
  if (options.plugin) {
    throw new Error("Legacy JavaScript plugins are not supported by @compiled/swc-native.");
  }
  const json = native.transformSync(src, toBuffer(options));
  return JSON.parse(json);
}

module.exports = { transformSync, version };
