/**
 * Phase 4 §4.1 — JS-side parity oracle for `transformCss(css, opts)`.
 *
 * Walks `crates/parity-runner/corpus/transform-css/` (30 hand-curated CSS
 * fixtures owned by the parallel CSS-port agent), and for each fixture
 * captures `{ sheets, classNames }` output under FOUR option permutations
 * that span the babel-plugin's actual `transformCss` call shape (per
 * `packages/babel-plugin/src/utils/{transform-css-items,build-styled-component}.ts`).
 *
 * Emits `crates/babel-plugin/tests/transform_css_corpus.json`.
 * The Rust gate at `crates/babel-plugin/tests/transform_css_integration.rs`
 * reads this file and asserts byte-identical output via `css::transform_css`.
 *
 * Same regenerable-corpus shape as Phase 3 hash:
 *   - JS oracle imports the upstream `@compiled/css` directly.
 *   - Rust gate is pure-Rust integration test reading the JSON.
 *   - Re-running the oracle is byte-deterministic given a fixed input set.
 *
 * Run:
 *   bun parity-harness/transform-css/oracle.mjs
 *
 * Important: this script must run with `COMPILED_CSS_ENGINE` UNSET so the
 * JS pipeline (`packages/css/src/transform.ts:38-101`) runs, NOT the
 * Rust NAPI dispatch at line 36. We delete it defensively at script start.
 */
import { readdirSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

// JS oracle requires the JS pipeline. If a previous shell exported the env
// var, unset it for this process so `transformCss` doesn't dispatch to the
// Rust NAPI shim and produce a self-comparison.
delete process.env.COMPILED_CSS_ENGINE;

// Pin browserslist to AFM's `.browserslistrc` — the exact configuration
// the Jira build runs in production through
// `@compiled/parcel-transformer → @compiled/babel-plugin →
// @compiled/css@0.19.0 → autoprefixer 10.4.14 → browserslist 4.24.2`.
// Resolves to the 14-entry list documented in `BROWSER_LIST_FROM_AFM.md`
// (and_chr 144, chrome 144..140, edge 144..143, firefox 147..146,
// ios_saf 26.2..26.1, safari 26.2..26.1) under the workspace's pinned
// `caniuse-lite@1.0.30001766` + `browserslist@4.24.2` overrides.
//
// We use `BROWSERSLIST_CONFIG` (forced-config-file env var) rather than
// `BROWSERSLIST` (query env var) so the resolution path matches what
// AFM production hits — read the .browserslistrc file, parse, resolve
// against the pinned caniuse-lite. Both `@compiled/css` (JS) and
// `crates/css::transform_css` (Rust, via `crates/browserslist-shim`)
// honor `BROWSERSLIST_CONFIG`; see `crates/browserslist-shim/src/node.rs:143`.
//
// We also clear `BROWSERSLIST` (would short-circuit the config-file
// path with priority over BROWSERSLIST_CONFIG per browserslist resolution
// order) so neither engine accidentally picks up an inherited query.
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT_DIR = resolve(__dirname, '../..');
const AFM_BROWSERSLIST_PATH = resolve(
  REPO_ROOT_DIR,
  'crates/browserslist-shim/tests/fixtures/afm/.browserslistrc'
);

delete process.env.BROWSERSLIST;
process.env.BROWSERSLIST_CONFIG = AFM_BROWSERSLIST_PATH;
// AUTOPREFIXER === 'off' would skip autoprefixer entirely. Default = run.
delete process.env.AUTOPREFIXER;

import { transformCss } from '@compiled/css';

const FIXTURES_DIR = resolve(REPO_ROOT_DIR, 'crates/parity-runner/corpus/transform-css');
const OUT_FILE = resolve(REPO_ROOT_DIR, 'crates/babel-plugin/tests/transform_css_corpus.json');

mkdirSync(dirname(OUT_FILE), { recursive: true });

/**
 * Fixtures the Rust source build is currently expected to diverge on,
 * with the reason. Keyed by fixture filename; applies to ALL opts
 * permutations of that fixture.
 *
 * Why this map exists: the SHIPPED NAPI binary
 * (`packages/css-native/compiled-css.win32-x64-msvc.node`) produces
 * byte-equal output to JS on every fixture below. The SOURCE build
 * of `crates/autoprefixer` currently has 16 files of in-progress
 * V2 / fast-match work in the working tree (see `git diff --stat
 * crates/autoprefixer/`). Until that work lands, fresh-from-source
 * `css::transform_css` adds vendor prefixes that the shipped binary
 * (and JS) correctly omit under `BROWSERSLIST=chrome 100`.
 *
 * The Rust gate at `crates/babel-plugin/tests/transform_css_integration.rs`
 * inverts assertion for these entries — they MUST NOT be byte-equal.
 * The moment a future autoprefixer commit fixes the drift, those
 * inverted assertions fail with "fixture should fail but passed —
 * remove from expected_to_fail." That's the unstick signal.
 *
 * Same precedent as `parity-harness/strip-runtime/generate-fixtures.mjs`'s
 * `EXPECTED_TO_FAIL` map (Phase 1 §1.4 hand-off).
 */
const EXPECTED_TO_FAIL = {
  // Cleared after autoprefixer-agent reproduction (2026-05-04): three
  // independent direct-call repros (`crates/css/examples/repro_user_select.rs`
  // under both `dev` and `bench-fast` profiles, plus
  // `crates/css/examples/repro_envvar.rs` under BROWSERSLIST_CONFIG, the
  // exact mechanism the integration test uses) all produce JS-equivalent
  // bytes. Re-asserting parity unconditionally.
};

// Option permutations spanning the babel-plugin's real call shape.
// `meta.state.opts` is the PluginOptions object — the plugin forwards
// every option below into `transformCss`. We exercise four
// representative permutations that touch every Rust gate visited
// from the consumer side.
const optsMatrix = [
  // 1. Default — `optimizeCss` defaults to true; runs full cssnano + autoprefixer.
  { label: 'default', opts: {} },
  // 2. optimizeCss=false — skips the 14 cssnano sub-plugins.
  { label: 'no-optimize', opts: { optimizeCss: false } },
  // 3. increaseSpecificity=true — gates plugin 8 in the orchestrator.
  { label: 'increase-specificity', opts: { increaseSpecificity: true } },
  // 4. classHashPrefix — forwarded into atomicifyRules; class names change.
  { label: 'class-hash-prefix', opts: { classHashPrefix: 'x' } },
];

const fixtureFiles = readdirSync(FIXTURES_DIR)
  .filter((f) => f.endsWith('.css'))
  .sort();

if (fixtureFiles.length < 30) {
  throw new Error(
    `expected ≥30 transform-css fixtures, got ${fixtureFiles.length}`
  );
}

const entries = [];
for (const file of fixtureFiles) {
  const cssPath = join(FIXTURES_DIR, file);
  const css = readFileSync(cssPath, 'utf8');
  for (const { label, opts } of optsMatrix) {
    const { sheets, classNames } = transformCss(css, opts);
    const expectedToFailReason = EXPECTED_TO_FAIL[file];
    entries.push({
      fixture: file,
      opts_label: label,
      opts,
      input: css,
      expected_sheets: sheets,
      expected_class_names: classNames,
      ...(expectedToFailReason
        ? { expected_to_fail: true, failure_reason: expectedToFailReason }
        : {}),
    });
  }
}

const out = {
  version: 1,
  generator: 'parity-harness/transform-css/oracle.mjs',
  source_corpus: 'crates/parity-runner/corpus/transform-css/',
  source: '@compiled/css transformCss (packages/css/src/transform.ts)',
  // Pinned env that affects byte output. Both the JS oracle (this script)
  // and the Rust gate (transform_css_integration.rs) must run with
  // identical values, or the parity contract is meaningless.
  env: {
    BROWSERSLIST_CONFIG: 'crates/browserslist-shim/tests/fixtures/afm/.browserslistrc',
    BROWSERSLIST: '<unset>',
    AUTOPREFIXER: '<unset>',
  },
  fixture_count: fixtureFiles.length,
  opts_permutations: optsMatrix.length,
  expected_to_fail_count: entries.filter((e) => e.expected_to_fail).length,
  entries,
};

writeFileSync(OUT_FILE, JSON.stringify(out, null, 2) + '\n');

console.log(
  `wrote ${entries.length} entries (${fixtureFiles.length} fixtures × ${optsMatrix.length} opts) -> ${OUT_FILE}`
);
