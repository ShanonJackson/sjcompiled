// Phase 2 §2.0 — extract every `transform(code, opts)` call from
// every test file under packages/babel-plugin/src into a flat
// fixture corpus under parity-harness/babel-plugin/fixtures/.
/*
 * Method (runtime extraction, not AST-static):
 *   1. Stub Jest globals (`describe`, `it`, `test`, `expect`,
 *      `beforeAll`, `afterAll`, `beforeEach`, `afterEach`, `jest`)
 *      onto `globalThis`. The stubs execute `it()` callbacks
 *      synchronously so the embedded `transform()` calls fire — but
 *      `expect(...).toMatchInlineSnapshot(...)` etc are no-ops, so
 *      assertion divergence doesn't abort the walk.
 *   2. Use `Bun.plugin` to rewrite `packages/babel-plugin/src/test-utils.ts`
 *      at load time, wrapping its `transform` export in a recorder
 *      that captures `(code, opts)` along with the active test path.
 *   3. Dynamically `import()` every test file. Each describe/it
 *      callback runs to completion, recording fixtures as it goes.
 *   4. Write one fixture per captured call.
 *
 * The recorded babel pipeline IS the oracle (just like strip-runtime).
 * `expected` output is re-derived by the harness; only `(code, opts)`
 * needs to be frozen here.
 *
 * Files using `jest.mock`/`jest.fn`/`jest.spyOn` are skipped — they
 * exercise utility internals (cache, object-property-to-string,
 * module-traversal) and aren't byte-parity targets.
 *
 * Run:
 *   bun parity-harness/babel-plugin/extract-fixtures.mjs
 */
import { writeFileSync, mkdirSync, readdirSync, statSync, rmSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '../..');
const FIXTURES_DIR = resolve(__dirname, 'fixtures');
const TEST_ROOT = resolve(REPO_ROOT, 'packages/babel-plugin/src');
const TEST_UTILS_PATH = resolve(TEST_ROOT, 'test-utils.ts').replace(/\\/g, '/');

// Files we skip — they use `jest.mock`/`jest.fn`/`jest.spyOn` against
// utility internals (not transform()), so they don't produce parity
// fixtures and the mock APIs are non-trivial to stub safely.
const SKIP_FILES = new Set([
  'utils/__tests__/cache.test.ts',
  'utils/__tests__/object-property-to-string.test.ts',
  '__tests__/module-traversal.test.ts',
  '__tests__/resolver.test.ts',
  '__perf__/module-traversal-cache.test.ts',
  '__tests__/errors.test.ts',
]);

// State held in a global so the rewritten test-utils can publish into it.
const recorder = {
  testPath: [],
  written: 0,
  byFile: new Map(), // file → counter (so each call gets a unique ordinal)
  currentFile: '',
};
globalThis.__SJC_FIXTURE_RECORDER__ = recorder;

// Jest globals — stubbed to capture structure but never assert.
let testPath = recorder.testPath;
const noop = () => {};
const noopAsync = async () => {};

function stubExpect(_actual) {
  // Every chained method on expect(x) is a no-op that returns the same
  // proxy. Covers: toBe, toEqual, toMatchInlineSnapshot, toInclude,
  // not.toBe, etc.
  const proxy = new Proxy(function () {}, {
    get: (_, prop) => {
      if (prop === 'not' || prop === 'resolves' || prop === 'rejects') return proxy;
      return () => proxy;
    },
    apply: () => proxy,
  });
  return proxy;
}
stubExpect.objectContaining = (x) => x;
stubExpect.arrayContaining = (x) => x;
stubExpect.stringContaining = (x) => x;
stubExpect.stringMatching = (x) => x;
stubExpect.any = () => undefined;
stubExpect.anything = () => undefined;
stubExpect.assertions = noop;
stubExpect.hasAssertions = noop;
stubExpect.extend = noop;

globalThis.expect = stubExpect;
globalThis.beforeAll = noop;
globalThis.afterAll = noop;
globalThis.beforeEach = noop;
globalThis.afterEach = noop;
globalThis.jest = {
  fn: () => Object.assign(() => undefined, { mockReturnValue: () => undefined }),
  mock: noop,
  unmock: noop,
  spyOn: () => ({ mockImplementation: noop, mockReturnValue: noop, mockRestore: noop }),
  resetAllMocks: noop,
  clearAllMocks: noop,
  restoreAllMocks: noop,
  Mock: class {},
};

function describe(name, body) {
  testPath.push(name);
  try {
    body();
  } catch (err) {
    process.stderr.write(`  describe('${name}') threw: ${err.message}\n`);
  } finally {
    testPath.pop();
  }
}
describe.skip = noop;
describe.only = describe;

function it(name, body) {
  testPath.push(name);
  try {
    if (typeof body === 'function') body();
  } catch (err) {
    // Test threw before/instead of asserting — we still captured any
    // transform() calls that ran before the throw, which is fine.
    // Don't log unless debugging.
  } finally {
    testPath.pop();
  }
}
it.skip = noop;
it.only = it;
it.todo = noop;
it.each = () => () => undefined;
it.skipIf = () => it;
it.runIf = () => it;

globalThis.describe = describe;
globalThis.it = it;
globalThis.test = it;

// Bun loader plugin: rewrite test-utils.ts so its `transform` export
// records every call. We do this rewriting at LOAD time so existing
// `import { transform } from '../test-utils'` bindings resolve to the
// instrumented version.
//
// The rewrite is surgical — we don't reparse the whole file; we
// rename the original `transform` to `__origTransform` and append a
// new `transform` export that records and forwards. ESM live bindings
// in importer modules pick up the new export.
import { plugin } from 'bun';
plugin({
  name: 'sjc-fixture-recorder',
  setup(build) {
    build.onLoad({ filter: /[\\/]packages[\\/]babel-plugin[\\/]src[\\/]test-utils\.ts$/ }, async (args) => {
      const file = Bun.file(args.path);
      let text = await file.text();
      // Rename the original `transform` so we can wrap it.
      text = text.replace(
        /export const transform =/,
        'const __origTransform =',
      );
      text += `

// === injected by parity-harness/babel-plugin/extract-fixtures.mjs ===
export const transform = (code, options = {}) => {
  const result = __origTransform(code, options);
  try {
    const r = globalThis.__SJC_FIXTURE_RECORDER__;
    if (r) r.record(code, options, result);
  } catch (err) {
    // Recording failure must never affect the original test path.
  }
  return result;
};
// === end injection ===
`;
      return { contents: text, loader: 'ts' };
    });
  },
});

// Recorder API.
mkdirSync(FIXTURES_DIR, { recursive: true });
// Wipe prior corpus so a re-run is byte-deterministic.
rmSync(FIXTURES_DIR, { recursive: true, force: true });
mkdirSync(FIXTURES_DIR, { recursive: true });

function slug(s) {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 80);
}

recorder.record = function (code, options, _result) {
  const file = recorder.currentFile;
  const key = `${file}|${recorder.testPath.join('|')}`;
  const counter = (recorder.byFile.get(key) || 0) + 1;
  recorder.byFile.set(key, counter);

  const path = recorder.testPath.map(slug).filter(Boolean).join('--') || 'top-level';
  const fileSlug = slug(file.replace(/\.test\.tsx?$/, '').replace(/[\\/]/g, '_'));
  const idx = String(recorder.written).padStart(4, '0');
  const fileName = `${idx}-${fileSlug}--${path}${counter > 1 ? `-call${counter}` : ''}.json`;

  // Strip non-JSON-serialisable values (functions, classes) from opts.
  const safeOpts = sanitiseOpts(options);

  const fixture = {
    name: `${fileSlug}/${path}${counter > 1 ? `#${counter}` : ''}`,
    sourceFile: file,
    testPath: [...recorder.testPath],
    source: code,
    opts: safeOpts,
  };
  writeFileSync(join(FIXTURES_DIR, fileName), JSON.stringify(fixture, null, 2) + '\n');
  recorder.written++;
};

function sanitiseOpts(opts) {
  const out = {};
  for (const [k, v] of Object.entries(opts || {})) {
    if (typeof v === 'function') continue;
    if (v instanceof RegExp) {
      out[k] = { __regex: v.source, flags: v.flags };
      continue;
    }
    try {
      JSON.stringify(v);
      out[k] = v;
    } catch {
      // Skip values that aren't JSON-serialisable.
    }
  }
  return out;
}

function walkTestFiles(root) {
  const out = [];
  for (const entry of readdirSync(root)) {
    const full = join(root, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      out.push(...walkTestFiles(full));
    } else if (entry.endsWith('.test.ts') || entry.endsWith('.test.tsx')) {
      out.push(full);
    }
  }
  return out;
}

const testFiles = walkTestFiles(TEST_ROOT);
process.stdout.write(`Found ${testFiles.length} test files\n`);

let imported = 0;
let failed = 0;
for (const file of testFiles) {
  const rel = relative(TEST_ROOT, file).replace(/\\/g, '/');
  if (SKIP_FILES.has(rel)) {
    process.stdout.write(`  [skip] ${rel}\n`);
    continue;
  }
  recorder.currentFile = rel;
  testPath.length = 0;
  try {
    await import(`file://${file.replace(/\\/g, '/')}`);
    imported++;
  } catch (err) {
    failed++;
    process.stderr.write(`  [fail] ${rel}: ${err.message}\n`);
  }
}

process.stdout.write(
  `\nImported ${imported} files (${failed} failed). Wrote ${recorder.written} fixtures to ${FIXTURES_DIR}\n`,
);
