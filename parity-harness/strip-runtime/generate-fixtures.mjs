/**
 * Generate the §1.0 fixture corpus for strip-runtime parity.
 *
 * One fixture per `it(...)` block in
 *   packages/babel-plugin-strip-runtime/src/__tests__/{
 *     extract-styles, jsx-pragma,
 *     strip-runtime-source-code, strip-runtime-transpiled-code
 *   }.test.ts
 *
 * Output: parity-harness/strip-runtime/fixtures/*.json
 *
 * Run:
 *   bun parity-harness/strip-runtime/generate-fixtures.mjs
 */
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { transformSync as babelTransformSync } from '@babel/core';
import compiledBabelPlugin from '@sjcompiled/babel-plugin';

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURES_DIR = resolve(__dirname, 'fixtures');
const FILENAME = '/base/src/app.tsx';
const SOURCE_FILE_NAME = '../src/app.tsx';

mkdirSync(FIXTURES_DIR, { recursive: true });

const written = [];

function _rawWriteFixture(filename, fixture) {
  writeFileSync(
    join(FIXTURES_DIR, filename),
    JSON.stringify(fixture, null, 2) + '\n'
  );
  written.push(filename);
}

// Phase 1 §1.4 left these specific fixtures in expectedToFail because
// the upstream behaviour they exercise depends on phases that haven't
// landed yet. Each entry documents WHICH phase will graduate it.
//
// Keep this list tight: only add a fixture here if you can name the
// phase that fixes it. "Just expected to fail" without a reason is
// how regressions hide.
const EXPECTED_TO_FAIL = {
  // (§1.5 graduated — A01–A04 now pass with the extractStylesToDirectory
  //  port: filename-derived `app.compiled.css` write under `/cwd/<dest>`,
  //  source-not-found throw, AST `import './app.compiled.css'` injection.)
  // Phase 2 — compiledBabelPlugin ports to Rust. The "Found a `jsx`
  // function call" / mixed-Compiled-Emotion errors originate there,
  // not in strip-runtime.
  'B06-jsx-pragma-classic-config-jsx-throws': 'Phase 2 compiledBabelPlugin error',
  'B07-jsx-pragma-classic-config-myjsx-throws': 'Phase 2 compiledBabelPlugin error',
  // Phase 2 — `babelJSXImportSource` flows through preset-react in
  // the BAKE step. Both engines currently bake with Babel, but the
  // SWC engine's bake doesn't thread `babelJSXImportSource` /
  // `babelJSXPragma` because Phase 2 is what owns the bake port.
  'B10-jsx-pragma-automatic-importsource': 'Phase 2 bake-pipeline parity',
  // Phase 7 — directive `'use strict';` keeps a blank line after it
  // in Babel output (directive prologue formatting); SWC emits it as
  // a regular ExprStmt with no trailing blank.
  'D01-transpiled-automatic-adds-require': 'Phase 7 directive blank-line',
  'D02-transpiled-automatic-no-require-ssr': 'Phase 7 directive blank-line',
  'D03-transpiled-automatic-modules-transformed': 'Phase 7 directive blank-line',
  'D05-transpiled-classic-adds-require': 'Phase 7 directive blank-line',
  'D06-transpiled-classic-no-require-ssr': 'Phase 7 directive blank-line',
  'D07-transpiled-classic-modules-transformed': 'Phase 7 directive blank-line',
};

function writeFixture(filename, fixture) {
  const reason = EXPECTED_TO_FAIL[fixture.name];
  if (reason) {
    fixture.expectedToFail = true;
    fixture.failureReason = reason;
  }
  _rawWriteFixture(filename, fixture);
}

/**
 * Bake just compiledBabelPlugin (no strip-runtime), used to capture the
 * intermediate code for "subsequent steps" fixtures. Mirrors the JS test
 * helper `transform(code, { run: 'bake', ... })` from
 * packages/babel-plugin-strip-runtime/src/__tests__/transform.ts.
 */
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

/**
 * Bake the FULL transpilation pipeline used by strip-runtime-transpiled-code
 * tests: preset-env + preset-typescript + preset-react + compiledBabelPlugin.
 * Captures the third-party-style transpiled output that strip-runtime then
 * processes in pass 2.
 */
function bakeTranspiled(code, opts) {
  const { runtime, modules } = opts;
  const result = babelTransformSync(code, {
    babelrc: false,
    configFile: false,
    filename: FILENAME,
    plugins: [[compiledBabelPlugin, { optimizeCss: false }]],
    presets: [
      ['@babel/preset-env', { targets: { esmodules: true }, modules: modules ?? 'auto' }],
      '@babel/preset-typescript',
      ['@babel/preset-react', { runtime, useBuiltIns: true }],
    ],
  });
  if (!result?.code) throw new Error('bakeTranspiled: empty');
  return result.code;
}

// =============================================================
// A. extract-styles.test.ts (4 fixtures)
// =============================================================

const extractStylesCode = `
        import '@sjcompiled/react';

        const Component = () => (
          <div css={{ fontSize: 12, color: 'blue' }}>
            hello world
          </div>
        );
      `;

const extractStylesClassicPragmaCode = `
          /** @jsx myJsx */
          import { css, jsx as myJsx } from '@sjcompiled/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const extractStylesAutomaticPragmaCode = `
          /** @jsxImportSource @sjcompiled/react */
          import { css } from '@sjcompiled/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

writeFixture('A01-extract-styles-classic-no-pragma.json', {
  name: 'A01-extract-styles-classic-no-pragma',
  description: 'classic runtime, no pragma; extractStylesToDirectory writes app.compiled.css',
  source: extractStylesCode,
  opts: {
    run: 'both',
    runtime: 'classic',
    extractStylesToDirectory: { source: 'src/', dest: 'dist/' },
  },
});

writeFixture('A02-extract-styles-classic-source-not-found.json', {
  name: 'A02-extract-styles-classic-source-not-found',
  description: 'classic runtime; extractStylesToDirectory.source does not match → throws',
  source: extractStylesCode,
  opts: {
    run: 'both',
    runtime: 'classic',
    extractStylesToDirectory: { source: 'not-existing-src/', dest: 'dist/' },
  },
  expectsError: {
    babelMessage:
      "Source directory 'not-existing-src/' was not found relative to source file ('../src/app.tsx')",
  },
});

writeFixture('A03-extract-styles-classic-with-pragma.json', {
  name: 'A03-extract-styles-classic-with-pragma',
  description: 'classic runtime, /** @jsx myJsx */ pragma + extractStylesToDirectory',
  source: extractStylesClassicPragmaCode,
  opts: {
    run: 'both',
    runtime: 'classic',
    extractStylesToDirectory: { source: 'src/', dest: 'dist/' },
  },
});

writeFixture('A04-extract-styles-automatic-with-pragma.json', {
  name: 'A04-extract-styles-automatic-with-pragma',
  description:
    'automatic runtime, /** @jsxImportSource @sjcompiled/react */ pragma + extractStylesToDirectory',
  source: extractStylesAutomaticPragmaCode,
  opts: {
    run: 'both',
    runtime: 'automatic',
    extractStylesToDirectory: { source: 'src/', dest: 'dist/' },
  },
});

// =============================================================
// B. jsx-pragma.test.ts (10 fixtures, including 2 loop expansions)
// =============================================================

const jsxPragmaClassicCompiledDefault = `
        /** @jsx jsx */
        import { css, jsx } from '@sjcompiled/react';

        const Component = () => (
          <div css={{ fontSize: 12, color: 'blue' }}>
            hello world 2
          </div>
        );

        const Component2 = () => (
          <div css={css({ fontSize: 12, color: 'pink' })}>
            hello world 2
          </div>
        );
      `;

const jsxPragmaClassicCompiledRenamed = `
        /** @jsx myJsx */
        import { css, jsx as myJsx } from '@sjcompiled/react';

        const Component = () => (
          <div css={{ fontSize: 12, color: 'blue' }}>
            hello world 2
          </div>
        );

        const Component2 = () => (
          <div css={css({ fontSize: 12, color: 'pink' })}>
            hello world 2
          </div>
        );
      `;

const jsxPragmaClassicEmotionDefault = `
          /** @jsx jsx */
          import { css, jsx } from '@emotion/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const jsxPragmaClassicEmotionRenamed = `
          /** @jsx myJsx */
          import { css, jsx as myJsx } from '@emotion/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const jsxPragmaClassicBoth = `
          /** @jsx jsx */
          import { css } from '@sjcompiled/react';
          import { jsx } from '@emotion/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const jsxPragmaAutomaticCompiled = `
          /** @jsxImportSource @sjcompiled/react */
          import { css } from '@sjcompiled/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const jsxPragmaAutomaticEmotion = `
          /** @jsxImportSource @emotion/react */
          import { css } from '@emotion/react';

          const Component = () => (
            <div css={{ fontSize: 12, color: 'blue' }}>
              hello world 2
            </div>
          );

          const Component2 = () => (
            <div css={css({ fontSize: 12, color: 'pink' })}>
              hello world 2
            </div>
          );
        `;

const jsxPragmaAutomaticImportSource = `
        import { css, jsx } from '@sjcompiled/react';

        const Component = () => (
          <div css={{ fontSize: 12, color: 'blue' }}>
            hello world 2
          </div>
        );

        const Component2 = () => (
          <div css={css({ fontSize: 12, color: 'pink' })}>
            hello world 2
          </div>
        );
      `;

writeFixture('B01-jsx-pragma-classic-compiled-default.json', {
  name: 'B01-jsx-pragma-classic-compiled-default',
  description: '/** @jsx jsx */ + import { jsx } from @sjcompiled/react — converts to React.createElement',
  source: jsxPragmaClassicCompiledDefault,
  opts: { run: 'both', runtime: 'classic' },
});

writeFixture('B02-jsx-pragma-classic-compiled-renamed.json', {
  name: 'B02-jsx-pragma-classic-compiled-renamed',
  description: '/** @jsx myJsx */ + import { jsx as myJsx } — same expectation as B01',
  source: jsxPragmaClassicCompiledRenamed,
  opts: { run: 'both', runtime: 'classic' },
});

writeFixture('B03-jsx-pragma-classic-emotion-default.json', {
  name: 'B03-jsx-pragma-classic-emotion-default',
  description: 'Emotion-only file with /** @jsx jsx */ — Compiled does not process',
  source: jsxPragmaClassicEmotionDefault,
  opts: { run: 'both', runtime: 'classic' },
});

writeFixture('B04-jsx-pragma-classic-emotion-renamed.json', {
  name: 'B04-jsx-pragma-classic-emotion-renamed',
  description: 'Emotion-only with renamed pragma — Compiled does not process',
  source: jsxPragmaClassicEmotionRenamed,
  opts: { run: 'both', runtime: 'classic' },
});

writeFixture('B05-jsx-pragma-classic-both-throws.json', {
  name: 'B05-jsx-pragma-classic-both-throws',
  description: 'Compiled + Emotion both imported with /** @jsx jsx */ — throws',
  source: jsxPragmaClassicBoth,
  opts: { run: 'both', runtime: 'classic' },
  expectsError: { babelMessage: 'Found a `jsx` function call' },
});

writeFixture('B06-jsx-pragma-classic-config-jsx-throws.json', {
  name: 'B06-jsx-pragma-classic-config-jsx-throws',
  description: 'No pragma comment, but babelJSXPragma=jsx via config + Compiled — throws',
  source: jsxPragmaClassicCompiledDefault,
  opts: { run: 'both', runtime: 'classic', babelJSXPragma: 'jsx' },
  expectsError: { babelMessage: 'Found a `jsx` function call' },
});

writeFixture('B07-jsx-pragma-classic-config-myjsx-throws.json', {
  name: 'B07-jsx-pragma-classic-config-myjsx-throws',
  description: 'No pragma, babelJSXPragma=jsx, code uses renamed myJsx — throws',
  source: jsxPragmaClassicCompiledRenamed,
  opts: { run: 'both', runtime: 'classic', babelJSXPragma: 'jsx' },
  expectsError: { babelMessage: 'Found a `jsx` function call' },
});

writeFixture('B08-jsx-pragma-automatic-compiled.json', {
  name: 'B08-jsx-pragma-automatic-compiled',
  description: '/** @jsxImportSource @sjcompiled/react */ — imports JSX runtime from React',
  source: jsxPragmaAutomaticCompiled,
  opts: { run: 'both', runtime: 'automatic' },
});

writeFixture('B09-jsx-pragma-automatic-emotion.json', {
  name: 'B09-jsx-pragma-automatic-emotion',
  description: '/** @jsxImportSource @emotion/react */ — file is not processed by Compiled',
  source: jsxPragmaAutomaticEmotion,
  opts: { run: 'both', runtime: 'automatic' },
});

writeFixture('B10-jsx-pragma-automatic-importsource.json', {
  name: 'B10-jsx-pragma-automatic-importsource',
  description:
    'No pragma comment, babelJSXImportSource=@sjcompiled/react — imports JSX from Compiled',
  source: jsxPragmaAutomaticImportSource,
  opts: { run: 'both', runtime: 'automatic', babelJSXImportSource: '@sjcompiled/react' },
});

// =============================================================
// C. strip-runtime-source-code.test.ts (16 fixtures)
// =============================================================

const sourceSharedCode = `
    import '@sjcompiled/react';

    const Component = () => (
      <div css={{ fontSize: 12, color: 'blue' }}>
        hello world
      </div>
    );
  `;

const STYLE_SHEET_PATH =
  '@sjcompiled/webpack-loader/css-loader!@sjcompiled/webpack-loader/css-loader/compiled-css.css';

// "same step" mode: opts.run = 'both', source unchanged.
const sameStepConfigs = [
  ['removes-runtime', {}],
  ['adds-require', { styleSheetPath: STYLE_SHEET_PATH }],
  ['no-require-ssr', { styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true }],
  ['metadata-ssr', { styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true }],
];

let cIdx = 1;
for (const runtime of ['automatic', 'classic']) {
  for (const [tag, extra] of sameStepConfigs) {
    const id = `C${String(cIdx).padStart(2, '0')}-source-same-${runtime}-${tag}`;
    writeFixture(`${id}.json`, {
      name: id,
      description: `same-step pipeline (run='both'), runtime=${runtime}, ${tag}`,
      source: sourceSharedCode,
      opts: { run: 'both', runtime, ...extra },
    });
    cIdx++;
  }
}

// "subsequent steps" mode: capture the bake-only output, replay with run='extract'.
const subsequentConfigs = [
  ['removes-runtime', {}],
  ['adds-require', { styleSheetPath: STYLE_SHEET_PATH }],
  ['no-require-ssr', { styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true }],
  ['metadata-ssr', { styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true }],
];

for (const runtime of ['automatic', 'classic']) {
  const baked = bakeOnly(sourceSharedCode, runtime);
  for (const [tag, extra] of subsequentConfigs) {
    const id = `C${String(cIdx).padStart(2, '0')}-source-subseq-${runtime}-${tag}`;
    writeFixture(`${id}.json`, {
      name: id,
      description: `subsequent-steps pipeline (bake-only output replayed with run='extract'), runtime=${runtime}, ${tag}`,
      source: baked,
      opts: { run: 'extract', runtime, ...extra },
    });
    cIdx++;
  }
}

// =============================================================
// D. strip-runtime-transpiled-code.test.ts (8 fixtures)
// =============================================================
//
// These tests run a TWO-pass pipeline:
//   pass 1: preset-env + preset-typescript + preset-react + compiledBabelPlugin
//   pass 2: strip-runtime
//
// We capture pass-1 output offline and replay through the harness with
// run='extract'. The harness applies preset-react during 'extract' but it
// is a no-op on already-transpiled CommonJS-style output (no JSX nodes).
//
// Gated on `@babel/preset-env` and `@babel/preset-typescript` being
// installed — see the drift note in plugins/STATUS.md. When the deps
// land, set GENERATE_TRANSPILED=1 to produce the 8 fixtures.

if (process.env.GENERATE_TRANSPILED === '1') {
  const transpiledSharedCode = `
    import '@sjcompiled/react';

    const Component = () => (
      <div css={{ fontSize: 12, color: 'blue' }}>
        hello world
      </div>
    );
  `;

  const transpiledConfigs = [
    ['adds-require', { styleSheetPath: STYLE_SHEET_PATH }, { modules: 'auto' }],
    [
      'no-require-ssr',
      { styleSheetPath: STYLE_SHEET_PATH, compiledRequireExclude: true },
      { modules: 'auto' },
    ],
    ['modules-transformed', {}, { modules: 'auto' }],
    ['modules-untransformed', {}, { modules: false }],
  ];

  let dIdx = 1;
  for (const runtime of ['automatic', 'classic']) {
    for (const [tag, extra, bakeOpts] of transpiledConfigs) {
      const baked = bakeTranspiled(transpiledSharedCode, { runtime, modules: bakeOpts.modules });
      const id = `D${String(dIdx).padStart(2, '0')}-transpiled-${runtime}-${tag}`;
      writeFixture(`${id}.json`, {
        name: id,
        description: `transpiled-code pipeline (full preset-env/-typescript/-react bake replayed with run='extract'), runtime=${runtime}, modules=${String(bakeOpts.modules)}, ${tag}`,
        source: baked,
        opts: { run: 'extract', runtime, ...extra },
      });
      dIdx++;
    }
  }
} else {
  process.stdout.write(
    '\n[skipped] Section D (8 transpiled-code fixtures): set GENERATE_TRANSPILED=1\n' +
      '          after installing @babel/preset-env and @babel/preset-typescript.\n'
  );
}

process.stdout.write(`\nTotal fixtures written: ${written.length}\n`);
process.stdout.write(`Output directory: ${FIXTURES_DIR}\n`);
