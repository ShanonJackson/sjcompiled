/**
 * Phase 5 §5.0c — JS-side parity oracle for Babel's
 * `@babel/traverse@7.29.0` `path.evaluate()` partial-evaluator.
 *
 * Reads `parity-harness/compat-evaluation/fixtures.json` (in-tree
 * input-expression manifest), parses each entry as a single
 * Expression via `@babel/parser@7.29.2`, runs `path.evaluate()` on
 * it, and emits `crates/babel-plugin/tests/compat_evaluation_corpus.json`
 * (cargo-readable, gitignored).
 *
 * Coverage rules locked in
 * `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md`. The four
 * unreachable branches (Flow type-cast, JSX-as-evaluable,
 * SequenceExpression, TaggedTemplateExpression) are NOT exercised
 * here — the Rust port emits `unimplemented!("…")` with a citation
 * back to that file rather than fall through.
 *
 * Result encoding:
 *   confident: bool — Babel's evaluator's confidence flag.
 *   value_kind: 'string' | 'number' | 'boolean' | 'undefined' | 'object'
 *   value_string: JSON.stringify(value) — byte-stable form. NaN /
 *     Infinity stringify to "null" (a JSON quirk); the Rust port
 *     normalises the same way for the corpus comparison.
 *
 * Run:
 *   bun parity-harness/compat-evaluation/oracle.mjs
 */
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT_DIR = resolve(__dirname, '../..');

const FIXTURES_FILE = resolve(__dirname, 'fixtures.json');
const OUT_FILE = resolve(REPO_ROOT_DIR, 'crates/babel-plugin/tests/compat_evaluation_corpus.json');

const EXPECTED_TRAVERSE_VERSION = '7.29.0';
const EXPECTED_PARSER_VERSION = '7.29.2';
const EXPECTED_TYPES_VERSION_PREFIX = '7.';

// ---------- Pin guards ----------

const require = createRequire(import.meta.url);
const traversePkg = require('@babel/traverse/package.json');
const parserPkg = require('@babel/parser/package.json');
const typesPkg = require('@babel/types/package.json');

if (traversePkg.version !== EXPECTED_TRAVERSE_VERSION) {
  throw new Error(
    `@babel/traverse pin drift: expected ${EXPECTED_TRAVERSE_VERSION}, got ${traversePkg.version}. See crates/PARITY_VERSIONS.md.`
  );
}
if (parserPkg.version !== EXPECTED_PARSER_VERSION) {
  throw new Error(
    `@babel/parser pin drift: expected ${EXPECTED_PARSER_VERSION}, got ${parserPkg.version}. See crates/PARITY_VERSIONS.md.`
  );
}
if (!typesPkg.version.startsWith(EXPECTED_TYPES_VERSION_PREFIX)) {
  throw new Error(
    `@babel/types unexpected major: got ${typesPkg.version}, expected ${EXPECTED_TYPES_VERSION_PREFIX}x.`
  );
}

const traverseModule = await import('@babel/traverse');
const traverse =
  traverseModule.default?.default ?? traverseModule.default ?? traverseModule.traverse;
const parser = await import('@babel/parser');
const t = await import('@babel/types');

if (typeof traverse !== 'function') {
  throw new Error(
    `@babel/traverse default export is not a function (got ${typeof traverse}). resolved=${traversePkg.version}`
  );
}

const PARSE_OPTS = {
  sourceType: 'module',
  plugins: ['jsx', 'typescript'],
};

// ---------- Helpers ----------

function valueKind(v) {
  if (v === undefined) return 'undefined';
  if (v === null) return 'object'; // typeof null === 'object'
  if (typeof v === 'string') return 'string';
  if (typeof v === 'number') return 'number';
  if (typeof v === 'boolean') return 'boolean';
  return typeof v; // object, function, bigint, symbol — any of these means deopt territory but record what Babel actually returned.
}

function valueString(v) {
  // JSON.stringify gives us a stable, byte-reproducible form. NaN /
  // Infinity / -Infinity stringify to "null" — that's the JSON quirk
  // the Rust corpus comparator must normalise the same way.
  if (v === undefined) return 'undefined';
  return JSON.stringify(v);
}

/** Wrap an Expression source in a synthetic Module so traverse() will
 *  give us a NodePath. We can't call path.evaluate() on a bare AST
 *  node — it has to be visited through traverse to get a NodePath.
 *
 *  Subtle: a bare string-literal at the top of a `sourceType: 'module'`
 *  parse is interpreted as a directive prologue (think 'use strict'),
 *  not an ExpressionStatement. So `parse("'hello'")` puts the literal
 *  in `program.directives`, not `program.body`. We wrap the input as
 *  `const __evalTarget = (EXPR);` so the expression always lands in
 *  a VariableDeclarator.init we can reach through traverse. */
function evaluateExpression(source) {
  const wrapped = `const __evalTarget = (${source});`;
  const program = parser.parse(wrapped, PARSE_OPTS);
  let result = null;
  traverse(program, {
    VariableDeclarator(path) {
      if (result !== null) return;
      if (path.node.id.type !== 'Identifier') return;
      if (path.node.id.name !== '__evalTarget') return;
      const evald = path.get('init').evaluate();
      result = {
        confident: evald.confident,
        value: evald.value,
      };
      path.stop();
    },
  });
  if (result === null) {
    throw new Error('synthetic __evalTarget declarator not reached — parse failed?');
  }
  return result;
}

// ---------- Main ----------

const fixtures = JSON.parse(readFileSync(FIXTURES_FILE, 'utf8'));
if (fixtures.version !== 1) {
  throw new Error(`fixtures.json version ${fixtures.version} not supported`);
}

const seenLabels = new Set();
const entries = [];

for (const fixture of fixtures.fixtures) {
  if (seenLabels.has(fixture.label)) {
    throw new Error(`duplicate fixture label: ${fixture.label}`);
  }
  seenLabels.add(fixture.label);

  let evald;
  try {
    evald = evaluateExpression(fixture.input_source);
  } catch (err) {
    throw new Error(
      `fixture ${fixture.label} oracle threw: ${err.message}\n${err.stack}`
    );
  }

  const observed = {
    confident: evald.confident,
    value_kind: valueKind(evald.value),
    value_string: valueString(evald.value),
  };

  // Self-consistency: oracle MUST agree with fixture's expected.
  // Same rule as compat-scope — the "expected" field is what Babel
  // actually does, not the fixture author's mental model.
  for (const [k, v] of Object.entries(fixture.expected)) {
    if (!(k in observed)) {
      throw new Error(
        `fixture ${fixture.label}: expected key "${k}" missing from oracle output`
      );
    }
    if (JSON.stringify(observed[k]) !== JSON.stringify(v)) {
      throw new Error(
        `fixture ${fixture.label}: oracle says ${k}=${JSON.stringify(observed[k])} but fixture asserts ${JSON.stringify(v)}. Update the fixture's expected.`
      );
    }
  }

  entries.push({
    label: fixture.label,
    category: fixture.category,
    input_source: fixture.input_source,
    expected: fixture.expected,
    observed,
  });
}

entries.sort((a, b) => {
  if (a.category !== b.category) return a.category < b.category ? -1 : 1;
  return a.label < b.label ? -1 : a.label > b.label ? 1 : 0;
});

const categoryCounts = {};
for (const e of entries) {
  categoryCounts[e.category] = (categoryCounts[e.category] || 0) + 1;
}

const out = {
  version: 1,
  generator: 'parity-harness/compat-evaluation/oracle.mjs',
  fixtures_source: 'parity-harness/compat-evaluation/fixtures.json',
  babel_traverse_version: traversePkg.version,
  babel_parser_version: parserPkg.version,
  babel_types_version: typesPkg.version,
  category_counts: categoryCounts,
  entry_count: entries.length,
  entries,
};

mkdirSync(dirname(OUT_FILE), { recursive: true });
writeFileSync(OUT_FILE, JSON.stringify(out, null, 2) + '\n');

console.log(
  `wrote ${entries.length} entries (` +
    Object.entries(categoryCounts)
      .map(([k, v]) => `${k}: ${v}`)
      .join(', ') +
    `) -> ${OUT_FILE}`
);
console.log(
  `pin guard: @babel/traverse=${traversePkg.version}, @babel/parser=${parserPkg.version}, @babel/types=${typesPkg.version}`
);

// Silence unused-import warning — t is reserved for future fixtures
// that need synthesized AST nodes (e.g. evaluating a pre-built
// MemberExpression that can't be expressed as a source string).
void t;
