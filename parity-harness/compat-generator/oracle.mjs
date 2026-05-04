/**
 * Phase 4 §4.2 — JS-side parity oracle for `@babel/generator@7.23.0`.
 *
 * Reads `parity-harness/compat-generator/fixtures.json` (an in-tree,
 * hand-curated input-source manifest), parses each entry via
 * `@babel/parser@7.29.2`, calls `@babel/generator@7.23.0`'s default
 * export on the call-site-relevant subnode, and emits
 * `crates/babel-plugin/tests/compat_generator_corpus.json` (the
 * cargo-readable form, gitignored — same regenerable shape as
 * Phase 3 hash + Phase 4 §4.1 transform-css).
 *
 * The Rust gate at `crates/babel-plugin/tests/compat_generator_integration.rs`
 * reads this corpus, parses each `input_source` via `swc_core`,
 * walks to the matching subnode, calls
 * `babel_plugin::compat::generator::generate(&swc_node)`, and asserts
 * byte-equal vs `expected_code`.
 *
 * Run:
 *   bun parity-harness/compat-generator/oracle.mjs
 *
 * Pin guards: this script ASSERTS the resolved versions match
 * `crates/PARITY_VERSIONS.md`'s "@babel/generator + @babel/parser"
 * row. If either pin floats, the oracle fails-fast rather than
 * silently emitting bytes from a different version. The Rust gate
 * does the same check on its end (corpus's `babel_generator_version`
 * vs an expected constant).
 */
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT_DIR = resolve(__dirname, '../..');

const FIXTURES_FILE = resolve(__dirname, 'fixtures.json');
const OUT_FILE = resolve(REPO_ROOT_DIR, 'crates/babel-plugin/tests/compat_generator_corpus.json');

// AFM-pinned versions; see crates/PARITY_VERSIONS.md.
const EXPECTED_GENERATOR_VERSION = '7.23.0';
const EXPECTED_PARSER_VERSION = '7.29.2';

// ---------- Pin-resolution guard ----------

const require = createRequire(import.meta.url);
const generatorPkg = require('@babel/generator/package.json');
const parserPkg = require('@babel/parser/package.json');

if (generatorPkg.version !== EXPECTED_GENERATOR_VERSION) {
  throw new Error(
    `@babel/generator pin drift: expected ${EXPECTED_GENERATOR_VERSION}, got ${generatorPkg.version}. ` +
      `Update package.json#overrides and re-run bun install. See crates/PARITY_VERSIONS.md.`
  );
}
if (parserPkg.version !== EXPECTED_PARSER_VERSION) {
  throw new Error(
    `@babel/parser pin drift: expected ${EXPECTED_PARSER_VERSION}, got ${parserPkg.version}. ` +
      `Update package.json#overrides and re-run bun install. See crates/PARITY_VERSIONS.md.`
  );
}

// ---------- Parser / generator imports ----------

const generateModule = await import('@babel/generator');
const generate = generateModule.default?.default ?? generateModule.default ?? generateModule.generate;
const parser = await import('@babel/parser');

if (typeof generate !== 'function') {
  throw new Error(
    `@babel/generator default export is not a function (got ${typeof generate}). ` +
      `Resolved package version: ${generatorPkg.version}.`
  );
}

// ---------- Per-call-site extractors ----------
//
// Each call_site picks a parser entry point + a subnode-extraction
// rule that mirrors what packages/babel-plugin/src/utils/*.ts
// actually feeds into generate(). The Rust gate must mirror these
// extractors exactly (different SWC entry points; same logical
// rule per call_site).

const PARSE_OPTS_EXPRESSION = {
  // ES2022 + JSX + TS subset, per the §4.2 hand-off contract.
  // Anything outside this subset belongs to a separate Drift event,
  // not the compat-generator parity contract.
  sourceType: 'module',
  plugins: ['jsx', 'typescript'],
};

const PARSE_OPTS_PROGRAM = {
  sourceType: 'module',
  plugins: ['jsx', 'typescript'],
};

function extractKeyframesExpression(input) {
  // Whole CallExpression / TaggedTemplateExpression — generate on
  // the parsed expression directly. Mirrors css-builders.ts:464:
  //   `const name = \`k${hash(generate(expression).code)}\`;`
  return parser.parseExpression(input, PARSE_OPTS_EXPRESSION);
}

function extractGenericExpression(input) {
  // Same shape as keyframes-expression; the call_site axis is for
  // reporting. Mirrors css-builders.ts:280 / :298.
  return parser.parseExpression(input, PARSE_OPTS_EXPRESSION);
}

function extractVariableInit(input) {
  // The upstream call site already drilled into VariableDeclarator.init
  // before calling generate(). The fixtures here record the bare init
  // expression source — same parse path as generic-expression.
  return parser.parseExpression(input, PARSE_OPTS_EXPRESSION);
}

function extractJsxKeyAttribute(input) {
  // Walk a Program → first JSXElement → openingElement.attributes
  // until we find one named "key". Matches build-compiled-component.ts:30:
  //   const [keyAttribute] = getJSXAttribute(node, 'key');
  //   `<CC ${keyAttribute ? generate(keyAttribute).code : ''}>`
  const ast = parser.parse(input, PARSE_OPTS_PROGRAM);
  let target = null;
  function walk(node) {
    if (target || node == null || typeof node !== 'object') return;
    if (node.type === 'JSXAttribute' && node.name?.type === 'JSXIdentifier' && node.name.name === 'key') {
      target = node;
      return;
    }
    for (const k of Object.keys(node)) {
      if (k === 'loc' || k === 'start' || k === 'end' || k === 'range') continue;
      const v = node[k];
      if (Array.isArray(v)) v.forEach(walk);
      else if (v && typeof v === 'object') walk(v);
    }
  }
  walk(ast);
  if (!target) {
    throw new Error(`jsx-key-attribute fixture has no key= attribute: ${input}`);
  }
  return target;
}

function extractConditionalClassnameItem(input) {
  // LogicalExpression | ConditionalExpression. Mirrors
  // build-styled-component.ts:133's filter:
  //   else if (t.isLogicalExpression(item) || t.isConditionalExpression(item)) {
  //     conditionalClassNames += `${generate(item).code}, `;
  //   }
  return parser.parseExpression(input, PARSE_OPTS_EXPRESSION);
}

const EXTRACTORS = {
  'keyframes-expression': extractKeyframesExpression,
  'generic-expression': extractGenericExpression,
  'variable-init': extractVariableInit,
  'jsx-key-attribute': extractJsxKeyAttribute,
  'conditional-classname-item': extractConditionalClassnameItem,
};

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

  const extractor = EXTRACTORS[fixture.call_site];
  if (!extractor) {
    throw new Error(
      `unknown call_site "${fixture.call_site}" in fixture ${fixture.label}. ` +
        `Allowed: ${Object.keys(EXTRACTORS).join(', ')}.`
    );
  }

  let node;
  try {
    node = extractor(fixture.input_source);
  } catch (err) {
    throw new Error(`fixture ${fixture.label} failed to parse: ${err.message}`);
  }

  let expectedCode;
  try {
    expectedCode = generate(node).code;
  } catch (err) {
    throw new Error(`fixture ${fixture.label} failed to generate: ${err.message}`);
  }

  entries.push({
    label: fixture.label,
    call_site: fixture.call_site,
    input_source: fixture.input_source,
    expected_code: expectedCode,
  });
}

// Sort entries by call_site then label so the corpus is byte-stable
// across runs even if fixtures.json reorders.
entries.sort((a, b) => {
  if (a.call_site !== b.call_site) return a.call_site < b.call_site ? -1 : 1;
  return a.label < b.label ? -1 : a.label > b.label ? 1 : 0;
});

const callSiteCounts = {};
for (const e of entries) {
  callSiteCounts[e.call_site] = (callSiteCounts[e.call_site] || 0) + 1;
}

const out = {
  version: 1,
  generator: 'parity-harness/compat-generator/oracle.mjs',
  fixtures_source: 'parity-harness/compat-generator/fixtures.json',
  babel_generator_version: generatorPkg.version,
  babel_parser_version: parserPkg.version,
  call_site_counts: callSiteCounts,
  entry_count: entries.length,
  entries,
};

mkdirSync(dirname(OUT_FILE), { recursive: true });
writeFileSync(OUT_FILE, JSON.stringify(out, null, 2) + '\n');

console.log(
  `wrote ${entries.length} entries (` +
    Object.entries(callSiteCounts)
      .map(([k, v]) => `${k}: ${v}`)
      .join(', ') +
    `) -> ${OUT_FILE}`
);
console.log(
  `pin guard: @babel/generator=${generatorPkg.version}, @babel/parser=${parserPkg.version}`
);
