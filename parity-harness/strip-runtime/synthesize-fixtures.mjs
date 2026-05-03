/**
 * Phase 1 §1.8 — synthesise ≥1000 already-baked strip-runtime fixtures.
 *
 * Stresses the StripRuntimeVisitor against patterns the hand-curated
 * 41-fixture set doesn't cover: random component counts, varied tags,
 * deeper css decls, mixed object-vs-call css props, both runtimes,
 * full styleSheetPath / compiledRequireExclude option matrix.
 *
 * Method:
 *   1. Seeded mulberry32 RNG (deterministic — re-running this script
 *      reproduces the corpus byte-for-byte; a fixture diff in CI
 *      means the generator changed).
 *   2. Generate N source variations using `@compiled/react`.
 *   3. For half the fixtures, emit them as run='both' (harness pipes
 *      through the bake step). For the other half, run the JS
 *      compiledBabelPlugin offline to produce CC/CS-wrapped baked code,
 *      then freeze that as run='extract' fixtures.
 *   4. Write to `fixtures/synthesized/synth-<NNNN>.json`.
 *
 * Run:
 *   bun parity-harness/strip-runtime/synthesize-fixtures.mjs
 *   bun parity-harness/strip-runtime/synthesize-fixtures.mjs --count 1500
 */
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { transformSync as babelTransformSync } from '@babel/core';
import compiledBabelPlugin from '@compiled/babel-plugin';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = resolve(__dirname, 'fixtures/synthesized');
const FILENAME = '/base/src/app.tsx';
const SOURCE_FILE_NAME = '../src/app.tsx';
const STYLE_SHEET_PATH =
  '@compiled/webpack-loader/css-loader!@compiled/webpack-loader/css-loader/compiled-css.css';

const argv = process.argv.slice(2);
const COUNT = (() => {
  const i = argv.indexOf('--count');
  if (i !== -1 && argv[i + 1]) return Math.max(1, parseInt(argv[i + 1], 10));
  return 1000;
})();

// Reset & recreate the output directory so a re-run is byte-identical.
rmSync(OUT_DIR, { recursive: true, force: true });
mkdirSync(OUT_DIR, { recursive: true });

// Seeded RNG. mulberry32: small, deterministic, good enough for a
// fixture corpus (we don't need cryptographic strength, we need
// reproducibility).
function mulberry32(seed) {
  let s = seed >>> 0;
  return function next() {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(0xC011ED01);
const pick = (arr) => arr[Math.floor(rand() * arr.length)];
const pickN = (arr, n) => {
  const copy = arr.slice();
  const out = [];
  for (let i = 0; i < n && copy.length; i++) {
    const idx = Math.floor(rand() * copy.length);
    out.push(copy.splice(idx, 1)[0]);
  }
  return out;
};
const range = (lo, hi) => lo + Math.floor(rand() * (hi - lo + 1));

const TAGS = ['div', 'span', 'button', 'section', 'a', 'li', 'p', 'h1', 'h2', 'header'];
const TEXTS = [
  'hello world',
  'lorem ipsum',
  'click me',
  'ready',
  'submit',
  'continue',
  'cancel',
  'save changes',
  'see more',
  'open menu',
];
const CSS_PROPS = [
  ['fontSize', () => `${range(8, 36)}`],
  ['color', () => pick(['"red"', '"blue"', '"pink"', '"#ff00aa"', '"green"', '"#0066ff"'])],
  ['fontWeight', () => pick(['400', '500', '600', '700', '"bold"'])],
  ['padding', () => `"${range(0, 24)}px"`],
  ['margin', () => `"${range(0, 24)}px"`],
  ['background', () => pick(['"white"', '"black"', '"#eee"', '"transparent"'])],
  ['border', () => pick(['"none"', '"1px solid black"', '"2px dashed #ccc"'])],
  ['opacity', () => `${(rand() * 0.5 + 0.5).toFixed(2)}`],
  ['lineHeight', () => `${(rand() * 1.5 + 1).toFixed(2)}`],
  ['textAlign', () => pick(['"left"', '"right"', '"center"'])],
  ['display', () => pick(['"block"', '"flex"', '"inline-block"'])],
  ['borderRadius', () => `"${range(0, 12)}px"`],
];

function genCssBody() {
  const n = range(1, 4);
  const decls = pickN(CSS_PROPS, n);
  return decls.map(([k, v]) => `        ${k}: ${v()},`).join('\n');
}

function genComponent(idx, useCssCall) {
  const tag = pick(TAGS);
  const text = pick(TEXTS);
  const body = genCssBody();
  const cssExpr = useCssCall
    ? `css({\n${body}\n      })`
    : `{\n${body}\n      }`;
  return `    const Component${idx} = () => (
      <${tag} css={${cssExpr}}>
        ${text}
      </${tag}>
    );`;
}

function genSource({ runtime, useCssCallMix }) {
  const ncomps = range(1, 3);
  const importLine = useCssCallMix
    ? `    import { css } from '@compiled/react';`
    : `    import '@compiled/react';`;
  const components = [];
  for (let i = 0; i < ncomps; i++) {
    // If the file imports `css`, half the components use the call form;
    // the rest use the literal-object form. With side-effect-only import
    // every component must use the literal form.
    const useCssCall = useCssCallMix && rand() < 0.5;
    components.push(genComponent(i + 1, useCssCall));
  }
  return `\n${importLine}\n\n${components.join('\n\n')}\n  `;
}

function bakeOnly(code, runtime) {
  const result = babelTransformSync(code, {
    babelrc: false,
    configFile: false,
    filename: FILENAME,
    generatorOpts: { sourceFileName: SOURCE_FILE_NAME },
    plugins: [
      [compiledBabelPlugin, { importReact: runtime === 'classic', optimizeCss: false }],
    ],
    presets: [['@babel/preset-react', { runtime }]],
  });
  if (!result?.code) throw new Error('bakeOnly: empty');
  return result.code;
}

const RUN_MODES = ['both', 'extract'];
const RUNTIMES = ['classic', 'automatic'];
// Every same-step / subseq combination of styleSheetPath × compiledRequireExclude
// the C-fixture matrix exercises (the four "tags": removes-runtime,
// adds-require, no-require-ssr, metadata-ssr collapse to these three
// combinations on the option side — metadata-ssr and no-require-ssr
// share opts).
const OPT_VARIANTS = [
  { tag: 'removes-runtime', styleSheetPath: undefined, compiledRequireExclude: false },
  { tag: 'adds-require', styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: false },
  { tag: 'no-require-ssr', styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true },
];

let written = 0;
let bakeFailures = 0;

for (let i = 0; i < COUNT; i++) {
  const runtime = pick(RUNTIMES);
  const run = pick(RUN_MODES);
  const variant = pick(OPT_VARIANTS);
  const useCssCallMix = rand() < 0.6;

  const source = genSource({ runtime, useCssCallMix });

  // For run='extract' we need to bake offline so the harness sees
  // already-CC/CS-wrapped code (matches the C-subseq pattern).
  let inputSource = source;
  if (run === 'extract') {
    try {
      inputSource = bakeOnly(source, runtime);
    } catch (err) {
      // The synthesised source occasionally hits the JS compiledBabelPlugin's
      // own validation (e.g. an invalid css() shape). Skip those — the
      // strip-runtime port doesn't see them, so they don't belong in the
      // dispatcher's stress corpus.
      bakeFailures++;
      continue;
    }
  }

  const id = `synth-${String(i).padStart(5, '0')}-${runtime}-${run}-${variant.tag}`;
  const fixture = {
    name: id,
    description: `synthesised §1.8 stress fixture (runtime=${runtime}, run=${run}, ${variant.tag})`,
    source: inputSource,
    opts: {
      run,
      runtime,
      ...(variant.styleSheetPath ? { styleSheetPath: variant.styleSheetPath } : {}),
      ...(variant.compiledRequireExclude ? { compiledRequireExclude: true } : {}),
    },
  };
  writeFileSync(join(OUT_DIR, `${id}.json`), JSON.stringify(fixture, null, 2) + '\n');
  written++;
}

process.stdout.write(
  `\nWrote ${written} synthesised fixtures to ${OUT_DIR}\n` +
    `  bake skipped: ${bakeFailures}\n`
);
