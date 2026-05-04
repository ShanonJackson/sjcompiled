/**
 * Phase 5 §5.4a — JS-side parity oracle for the resolver matrix.
 *
 * Reads `parity-harness/resolver-matrix/fixtures.json` (in-tree
 * declarative fixture manifest) plus the real npm-package skeletons
 * under `fixtures-source/`, runs each fixture through:
 *   1. enhanced-resolve@5.x — the production oracle (matches what
 *      `createDefaultResolver(config)` with `config.resolve = {}`
 *      produces).
 *   2. npm resolve@1.x's `resolve.sync` — the fallback path used by
 *      `packages/babel-plugin/src/utils/resolve-binding.ts:185-189`
 *      when no host resolver is injected.
 *
 * Emits `crates/babel-plugin/tests/resolver_matrix_corpus.json`
 * (cargo-readable, gitignored). Same pattern as the §5.0
 * compat-scope and compat-evaluation oracles.
 *
 * Layer-1 (default-config) only — covers the 9 corpus axes
 * enumerated in `crates/babel-plugin/RESOLVER_MATRIX.md`.
 *
 * Run:
 *   bun parity-harness/resolver-matrix/oracle.mjs
 */
import { readFileSync, mkdirSync, writeFileSync, existsSync, statSync, realpathSync } from 'node:fs';
import { dirname, resolve, isAbsolute, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT_DIR = resolve(__dirname, '../..');

const FIXTURES_FILE = resolve(__dirname, 'fixtures.json');
const FIXTURES_SOURCE_DIR = resolve(__dirname, 'fixtures-source');
const OUT_FILE = resolve(REPO_ROOT_DIR, 'crates/babel-plugin/tests/resolver_matrix_corpus.json');

const EXPECTED_ENHANCED_RESOLVE_VERSION = '5.18.3';
const EXPECTED_RESOLVE_VERSION = '1.22.12';

// ---------- Pin guards ----------

const require = createRequire(import.meta.url);
const enhancedResolvePkg = require('enhanced-resolve/package.json');
const resolvePkg = require('resolve/package.json');

if (enhancedResolvePkg.version !== EXPECTED_ENHANCED_RESOLVE_VERSION) {
  throw new Error(
    `enhanced-resolve pin drift: expected ${EXPECTED_ENHANCED_RESOLVE_VERSION}, got ${enhancedResolvePkg.version}. See crates/PARITY_VERSIONS.md.`
  );
}
if (resolvePkg.version !== EXPECTED_RESOLVE_VERSION) {
  throw new Error(
    `resolve pin drift: expected ${EXPECTED_RESOLVE_VERSION}, got ${resolvePkg.version}. See crates/PARITY_VERSIONS.md.`
  );
}

// CommonJS-style requires for resolvers — both ship CJS-only as of these versions.
const enhancedResolve = require('enhanced-resolve');
const npmResolve = require('resolve');

// ---------- Helpers ----------

/** Convert a corpus-relative path to an absolute path on this machine.
 *  Corpus-relative paths in fixtures.json are anchored at the repo
 *  root so the corpus is portable across machines. */
function toAbs(relPath) {
  if (isAbsolute(relPath)) return relPath;
  return resolve(REPO_ROOT_DIR, relPath);
}

/** Convert an absolute path on this machine back to a
 *  corpus-relative path so the emitted corpus is portable. */
function toRel(absPath) {
  if (!absPath) return absPath;
  if (typeof absPath !== 'string') return absPath;
  // Normalise separators to forward-slash for cross-platform corpus stability.
  const rel = absPath.startsWith(REPO_ROOT_DIR)
    ? absPath.slice(REPO_ROOT_DIR.length + 1)
    : absPath;
  return rel.split(sep).join('/');
}

/** Run enhanced-resolve against (fromFile, request, extensions).
 *  Mirrors `createDefaultResolver(config)` with `config.resolve = {}`:
 *  only the explicit fields below are configured; everything else
 *  inherits enhanced-resolve's bare defaults. */
function runEnhancedResolve(fromFileAbs, request, extensions) {
  const opts = {
    fileSystem: enhancedResolve.CachedInputFileSystem
      ? new enhancedResolve.CachedInputFileSystem(require('node:fs'), 4000)
      : undefined,
    useSyncFileSystemCalls: true,
  };
  if (extensions !== null && extensions !== undefined) {
    opts.extensions = extensions;
  }
  const resolver = enhancedResolve.ResolverFactory.createResolver(opts);
  try {
    // The host wrapper does `resolver.resolveSync({}, dirname(context), request)`.
    const resolved = resolver.resolveSync({}, dirname(fromFileAbs), request);
    return { kind: 'ok', path: toRel(resolved) };
  } catch (err) {
    return {
      kind: 'err',
      errorClass: err.code || err.name || 'Error',
      errorMessage: err.message,
    };
  }
}

/** Run npm `resolve.sync` against (fromFile, request, extensions).
 *  Mirrors the fallback path at `resolve-binding.ts:185-189`:
 *  prefixes a relative request with `dirname(filename)` then calls
 *  `resolve.sync(id, { extensions })`. */
function runNpmResolve(fromFileAbs, request, extensions) {
  const id = request[0] === '.' ? resolve(dirname(fromFileAbs), request) : request;
  const opts = {
    basedir: dirname(fromFileAbs),
  };
  if (extensions !== null && extensions !== undefined) {
    opts.extensions = extensions;
  }
  try {
    const resolved = npmResolve.sync(id, opts);
    return { kind: 'ok', path: toRel(resolved) };
  } catch (err) {
    return {
      kind: 'err',
      errorClass: err.code || err.name || 'Error',
      errorMessage: err.message,
    };
  }
}

/** Self-consistency: assert the fixture's `expected` agrees with what
 *  the resolver actually does. Same rule as compat-scope /
 *  compat-evaluation — `expected` is what the JS oracle returns,
 *  not the fixture author's mental model. */
function assertSelfConsistent(fixture, observedKey, observed) {
  const expected = fixture.expected?.[observedKey];
  if (expected === undefined) {
    // Fixture didn't assert this oracle's output. Still record it.
    return;
  }
  // Compare structurally — kind first, then payload depending on kind.
  if (expected.kind !== observed.kind) {
    throw new Error(
      `fixture ${fixture.label}: ${observedKey} expected kind=${expected.kind} but oracle returned kind=${observed.kind} (${
        observed.kind === 'err' ? observed.errorClass : observed.path
      }). Update the fixture's expected.`
    );
  }
  if (expected.kind === 'ok' && expected.path !== observed.path) {
    throw new Error(
      `fixture ${fixture.label}: ${observedKey} expected path=${expected.path} but oracle returned path=${observed.path}. Update the fixture's expected.`
    );
  }
  if (expected.kind === 'err' && expected.errorClass && expected.errorClass !== observed.errorClass) {
    throw new Error(
      `fixture ${fixture.label}: ${observedKey} expected errorClass=${expected.errorClass} but oracle returned ${observed.errorClass}. Update the fixture's expected.`
    );
  }
  // errorMessage is intentionally NOT byte-checked — it drifts between resolver versions.
}

// ---------- Main ----------

if (!existsSync(FIXTURES_SOURCE_DIR)) {
  throw new Error(
    `fixtures-source/ not found at ${FIXTURES_SOURCE_DIR}. Did you check out the corpus skeletons?`
  );
}

const manifest = JSON.parse(readFileSync(FIXTURES_FILE, 'utf8'));
if (manifest.version !== 1) {
  throw new Error(`fixtures.json version ${manifest.version} not supported`);
}

const seenLabels = new Set();
const entries = [];

for (const fixture of manifest.fixtures) {
  if (seenLabels.has(fixture.label)) {
    throw new Error(`duplicate fixture label: ${fixture.label}`);
  }
  seenLabels.add(fixture.label);

  const fromFileAbs = toAbs(fixture.fromFile);
  if (!existsSync(fromFileAbs)) {
    throw new Error(
      `fixture ${fixture.label}: fromFile does not exist on disk: ${fromFileAbs}. Check fixtures-source/.`
    );
  }
  if (!statSync(fromFileAbs).isFile()) {
    throw new Error(
      `fixture ${fixture.label}: fromFile is not a regular file: ${fromFileAbs}.`
    );
  }

  const extensions = fixture.extensions ?? null;

  const enhancedResolveResult = runEnhancedResolve(fromFileAbs, fixture.request, extensions);
  const npmResolveResult = runNpmResolve(fromFileAbs, fixture.request, extensions);

  assertSelfConsistent(fixture, 'enhancedResolve', enhancedResolveResult);
  assertSelfConsistent(fixture, 'npmResolve', npmResolveResult);

  entries.push({
    label: fixture.label,
    axis: fixture.axis,
    fromFile: fixture.fromFile,
    request: fixture.request,
    extensions: extensions,
    expected: fixture.expected,
    observed: {
      enhancedResolve: enhancedResolveResult,
      npmResolve: npmResolveResult,
    },
  });
}

entries.sort((a, b) => {
  if (a.axis !== b.axis) return a.axis < b.axis ? -1 : 1;
  return a.label < b.label ? -1 : a.label > b.label ? 1 : 0;
});

const axisCounts = {};
for (const e of entries) {
  axisCounts[e.axis] = (axisCounts[e.axis] || 0) + 1;
}

const out = {
  version: 1,
  generator: 'parity-harness/resolver-matrix/oracle.mjs',
  fixtures_source: 'parity-harness/resolver-matrix/fixtures.json',
  enhanced_resolve_version: enhancedResolvePkg.version,
  resolve_version: resolvePkg.version,
  axis_counts: axisCounts,
  entry_count: entries.length,
  entries,
};

mkdirSync(dirname(OUT_FILE), { recursive: true });
writeFileSync(OUT_FILE, JSON.stringify(out, null, 2) + '\n');

console.log(
  `wrote ${entries.length} entries (` +
    Object.entries(axisCounts)
      .map(([k, v]) => `${k}: ${v}`)
      .join(', ') +
    `) -> ${OUT_FILE}`
);
console.log(
  `pin guard: enhanced-resolve=${enhancedResolvePkg.version}, resolve=${resolvePkg.version}`
);
