// Simple ops/s comparison: JS transformCss vs Rust transformCss (NAPI).
// Run with: bun scripts/perf-test.ts
//
// transform.ts reads process.env.COMPILED_CSS_ENGINE on every call, so we
// toggle the env var between runs to swap engines without re-importing.

import { transformCss, type TransformOpts } from '../packages/css/src/transform';

// Native-only API: pre-build the autoprefixer prefix tables once, hand
// them to every Rust call. Byte-equal to omitting them; intended to
// neutralize the per-call autoprefixer setup cost (filesystem walk +
// browserslist resolution + full PREFIXES iteration). The bytes are
// the WASI delivery shape — host produces them once, plugin receives
// them via plugin_config on every call.
const native = require('../packages/css-native');
const precomputedPrefixes: Buffer | null =
  typeof native.precomputePrefixesDefault === 'function'
    ? native.precomputePrefixesDefault()
    : null;
if (!precomputedPrefixes) {
  console.warn('[perf-test] precomputePrefixesDefault unavailable — Rust path will use slow autoprefixer setup.');
}

const SAMPLE_CSS = `
  display: flex;
  flex-direction: column;
  align-items: center;
  user-select: none;
  color: hotpink;
  background: linear-gradient(to right, red, blue);
  transition: transform 0.2s ease-in-out;

  &:hover {
    color: rebeccapurple;
    transform: scale(1.05);
  }

  &:focus-visible {
    outline: 2px solid currentColor;
  }

  @media (max-width: 600px) {
    flex-direction: row;
    padding: 8px;
  }

  > .child {
    margin-bottom: 1rem;

    &:last-child {
      margin-bottom: 0;
    }
  }
`;

const OPTS: TransformOpts = {
  optimizeCss: false,
  sortAtRules: true,
  sortShorthand: true,
  increaseSpecificity: false,
};

function bench(label: string, engine: 'js' | 'rust', durationMs = 3000) {
  process.env.COMPILED_CSS_ENGINE = engine;

  // Rust engine accepts a precomputed prefix-tables Buffer. JS engine
  // ignores unknown opts. Type-cast since `TransformOpts` from the
  // immutable JS package doesn't model the Rust-only field.
  const opts: TransformOpts =
    engine === 'rust' && precomputedPrefixes
      ? ({ ...OPTS, precomputedPrefixes } as TransformOpts)
      : OPTS;

  // Warmup
  for (let i = 0; i < 50; i++) transformCss(SAMPLE_CSS, opts);

  let ops = 0;
  const start = performance.now();
  const deadline = start + durationMs;
  while (performance.now() < deadline) {
    transformCss(SAMPLE_CSS, opts);
    ops++;
  }
  const elapsed = (performance.now() - start) / 1000;
  const opsPerSec = ops / elapsed;

  console.log(
    `${label.padEnd(8)} ${opsPerSec.toFixed(0).padStart(10)} ops/s   (${ops} iterations in ${elapsed.toFixed(2)}s)`
  );
  return opsPerSec;
}

// Sanity: assert both engines produce identical output before benching.
process.env.COMPILED_CSS_ENGINE = 'js';
const jsOut = transformCss(SAMPLE_CSS, OPTS);
process.env.COMPILED_CSS_ENGINE = 'rust';
const rustOut = transformCss(SAMPLE_CSS, OPTS);
const jsStr = JSON.stringify(jsOut);
const rustStr = JSON.stringify(rustOut);
if (jsStr !== rustStr) {
  console.warn('WARNING: JS and Rust outputs DIFFER — perf comparison still runs.');
  console.warn('  JS  :', jsStr);
  console.warn('  Rust:', rustStr);
} else {
  console.log('Outputs match byte-for-byte.\n');
}

const jsOps = bench('JS', 'js');
const rustOps = bench('Rust', 'rust');

const ratio = rustOps / jsOps;
console.log(
  `\nRust is ${ratio.toFixed(2)}x ${ratio >= 1 ? 'faster' : 'slower'} than JS.`
);
