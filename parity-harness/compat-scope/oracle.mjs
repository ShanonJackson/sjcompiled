/**
 * Phase 5 §5.0a/b — JS-side parity oracle for `@babel/traverse@7.29.0`.
 *
 * Reads `parity-harness/compat-scope/fixtures.json` (an in-tree,
 * hand-curated input-source manifest), parses each entry via
 * `@babel/parser@7.29.2`, runs `@babel/traverse` over it to capture
 * the binding-shape / scope-predicate / path-predicate observables
 * the Rust pre-indexed scope walker (`crates/babel-plugin/src/compat/scope.rs`)
 * must reproduce, and emits
 * `crates/babel-plugin/tests/compat_scope_corpus.json`
 * (cargo-readable, gitignored — same regenerable shape as Phase 4
 * §4.2 compat-generator).
 *
 * The Rust gate at
 * `crates/babel-plugin/tests/compat_scope_integration.rs` reads the
 * corpus, parses each `input_source` via `swc_core`, runs the
 * pre-indexed scope walker, and asserts identical shape per fixture.
 *
 * Run:
 *   bun parity-harness/compat-scope/oracle.mjs
 *
 * Pin guards: this script ASSERTS the resolved versions match
 * `crates/PARITY_VERSIONS.md`. If any pin floats, the oracle fails
 * fast rather than silently emitting bytes from a different version.
 *
 * NOTE on @babel/traverse default-export shape: 7.29.0 ships ESM
 * with `default` AND `traverse` named export; bun's interop returns
 * the .default. We grab whichever resolves to a function.
 */
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT_DIR = resolve(__dirname, '../..');

const FIXTURES_FILE = resolve(__dirname, 'fixtures.json');
const OUT_FILE = resolve(REPO_ROOT_DIR, 'crates/babel-plugin/tests/compat_scope_corpus.json');

// AFM-pinned versions; see crates/PARITY_VERSIONS.md.
const EXPECTED_TRAVERSE_VERSION = '7.29.0';
const EXPECTED_PARSER_VERSION = '7.29.2';
const EXPECTED_TYPES_VERSION_PREFIX = '7.'; // peer dep of @babel/traverse@7.29.0; major-pin only.

// ---------- Pin-resolution guard ----------

const require = createRequire(import.meta.url);
const traversePkg = require('@babel/traverse/package.json');
const parserPkg = require('@babel/parser/package.json');
const typesPkg = require('@babel/types/package.json');

if (traversePkg.version !== EXPECTED_TRAVERSE_VERSION) {
  throw new Error(
    `@babel/traverse pin drift: expected ${EXPECTED_TRAVERSE_VERSION}, got ${traversePkg.version}. ` +
      `Update package.json#overrides AND devDependencies (top-level promotion required for bun's isolated layout — see §4.2 lesson) and re-run bun install. See crates/PARITY_VERSIONS.md.`
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

// ---------- Imports ----------

const traverseModule = await import('@babel/traverse');
const traverse =
  traverseModule.default?.default ?? traverseModule.default ?? traverseModule.traverse;
const parser = await import('@babel/parser');
const t = await import('@babel/types');

if (typeof traverse !== 'function') {
  throw new Error(
    `@babel/traverse default export is not a function (got ${typeof traverse}). ` +
      `Resolved package version: ${traversePkg.version}.`
  );
}

const PARSE_OPTS = {
  sourceType: 'module',
  plugins: ['jsx', 'typescript'],
};

// ---------- Helpers ----------

function parseProgram(source) {
  return parser.parse(source, PARSE_OPTS);
}

/** Walk a Program AST and return the FIRST identifier matching name on
 *  the RHS of a `const x = …` / `let x = …` / `var x = …` declarator
 *  whose declarator id is NOT the lookup name. Mirrors the "from-reference"
 *  shape — i.e. the lookup site is a USE of the name, not a binding. */
function findFirstReferenceOnRhs(program, name) {
  let result = null;
  traverse(program, {
    Identifier(path) {
      if (result) return;
      if (path.node.name !== name) return;
      // Skip the LHS of a `const name = …` declaration.
      if (path.parent.type === 'VariableDeclarator' && path.parent.id === path.node) return;
      // Skip object-pattern bindings and keys.
      if (path.parent.type === 'ObjectProperty' && path.parent.key === path.node) return;
      // Skip import specifier locals.
      if (
        path.parent.type === 'ImportSpecifier' ||
        path.parent.type === 'ImportDefaultSpecifier' ||
        path.parent.type === 'ImportNamespaceSpecifier'
      ) {
        return;
      }
      // Skip param declarations.
      if (
        path.parent.type === 'FunctionDeclaration' ||
        path.parent.type === 'FunctionExpression' ||
        path.parent.type === 'ArrowFunctionExpression'
      ) {
        if (path.parent.params && path.parent.params.includes(path.node)) return;
      }
      // Skip the LHS of `name = expr;` (assignment-violation site).
      if (path.parent.type === 'AssignmentExpression' && path.parent.left === path.node) return;
      result = path;
      path.stop();
    },
  });
  return result;
}

/** Walk to the first BlockStatement that is the body of a function/arrow. */
function findFirstFunctionBodyBlock(program) {
  let result = null;
  traverse(program, {
    BlockStatement(path) {
      if (result) return;
      const parent = path.parent;
      if (
        parent.type === 'FunctionDeclaration' ||
        parent.type === 'FunctionExpression' ||
        parent.type === 'ArrowFunctionExpression'
      ) {
        result = path;
        path.stop();
      }
    },
  });
  return result;
}

/** Walk to the first ArrowFunctionExpression. Used by scope-push-iife to
 *  reach the arrow whose own scope receives the synthetic binding. */
function findFirstArrow(program) {
  let result = null;
  traverse(program, {
    ArrowFunctionExpression(path) {
      result = path;
      path.stop();
    },
  });
  return result;
}

/** Walk to the first MemberExpression. Used by list-key-arguments to
 *  capture path.listKey at the visit site. */
function findFirstMemberExpression(program) {
  let result = null;
  traverse(program, {
    MemberExpression(path) {
      if (result) return;
      result = path;
      path.stop();
    },
  });
  return result;
}

function bindingNodeType(binding) {
  // Babel's binding.path.node.type — what the Rust port reports as
  // `binding.declaring_node().type` in compat/scope.rs.
  return binding.path.node.type;
}

function bindingParentType(binding) {
  return binding.path.parent?.type ?? null;
}

function bindingInitString(binding) {
  // Optional read for VariableDeclarator string-init bindings.
  if (binding.path.node.type !== 'VariableDeclarator') return null;
  const init = binding.path.node.init;
  if (!init || init.type !== 'StringLiteral') return null;
  return init.value;
}

function bindingIdType(binding) {
  if (binding.path.node.type !== 'VariableDeclarator') return null;
  return binding.path.node.id?.type ?? null;
}

// ---------- Per-call-site queries ----------

function runBindingLookupFromReference(fixture, program) {
  const refPath = findFirstReferenceOnRhs(program, fixture.lookup_name);
  if (!refPath) {
    return { found: false, _why: `no reference to ${fixture.lookup_name} found in program` };
  }
  const binding = refPath.scope.getBinding(fixture.lookup_name);
  if (!binding) {
    return { found: false };
  }
  const out = {
    found: true,
    binding_node_type: bindingNodeType(binding),
    binding_kind: binding.kind,
    constant: binding.constant,
    reference_paths_count: binding.referencePaths.length,
    parent_path_type: bindingParentType(binding),
  };
  // Optional secondary observables — only emit when the fixture asks
  // for them, so the corpus shape stays minimal per row.
  if ('binding_init_string' in fixture.expected) {
    out.binding_init_string = bindingInitString(binding);
  }
  if ('binding_id_type' in fixture.expected) {
    out.binding_id_type = bindingIdType(binding);
  }
  return out;
}

function runPathPredicateViaBinding(fixture, program) {
  const refPath = findFirstReferenceOnRhs(program, fixture.lookup_name);
  if (!refPath) return { found: false };
  const binding = refPath.scope.getBinding(fixture.lookup_name);
  if (!binding) return { found: false };
  return {
    found: true,
    is_import_declaration_parent: !!binding.path.parentPath?.isImportDeclaration?.(),
    is_export_named_declaration: !!binding.path.isExportNamedDeclaration?.(),
    is_object_pattern: !!binding.path.isObjectPattern?.(),
    is_variable_declarator: !!t.isVariableDeclarator(binding.path.node),
  };
}

function runHasOwnBinding(fixture, program) {
  const blockPath = findFirstFunctionBodyBlock(program);
  if (!blockPath) {
    return { has_own_binding: false, has_binding: false, _why: 'no function body block' };
  }
  return {
    has_own_binding: blockPath.scope.hasOwnBinding(fixture.lookup_name),
    has_binding: blockPath.scope.hasBinding(fixture.lookup_name),
  };
}

function runScopePushIife(fixture, program) {
  const arrowPath = findFirstArrow(program);
  if (!arrowPath) {
    throw new Error(`fixture ${fixture.label}: no arrow function found`);
  }
  const arrowBodyScope = arrowPath.scope;
  arrowBodyScope.push({
    id: t.identifier(fixture.lookup_name),
    init: t.stringLiteral('val'),
    kind: 'const',
  });

  const ownAfter = arrowBodyScope.getOwnBinding(fixture.lookup_name);
  const moduleAfter = program.program
    ? null // never reached; placeholder
    : null;

  // The module scope is the Program-level scope; reach it via parent walk.
  let scope = arrowBodyScope.parent;
  while (scope?.parent) scope = scope.parent;
  const moduleHasIt = !!scope?.getBinding(fixture.lookup_name);

  return {
    after_push_has_own_binding_in_arrow_scope: !!ownAfter,
    after_push_has_binding_in_module_scope: moduleHasIt,
    binding_node_type_after_push: ownAfter ? ownAfter.path.node.type : null,
    binding_kind_after_push: ownAfter ? ownAfter.kind : null,
  };
}

function runGenerateUid(fixture, program) {
  // Pluck the program path so we can call generateUidIdentifier on the
  // top-level scope. Babel exposes Program in the visitor; cheapest way
  // here is the "enter Program once" trick.
  let programPath = null;
  traverse(program, {
    Program(path) {
      programPath = path;
      path.stop();
    },
  });
  const a = programPath.scope.generateUidIdentifier('');
  const b = programPath.scope.generateUidIdentifier('');
  const existing = !!programPath.scope.getBinding('x');
  return {
    first_uid_name_starts_with_underscore: a.name.startsWith('_'),
    second_uid_name_differs_from_first: a.name !== b.name,
    neither_collides_with_existing_x: a.name !== 'x' && b.name !== 'x' && existing,
  };
}

function runListKeyArguments(_fixture, program) {
  const memberPath = findFirstMemberExpression(program);
  if (!memberPath) throw new Error('no MemberExpression found');
  return {
    member_expr_list_key: memberPath.listKey ?? null,
  };
}

const QUERIES = {
  'binding-lookup-from-reference': runBindingLookupFromReference,
  'path-predicate-via-binding': runPathPredicateViaBinding,
  'has-own-binding': runHasOwnBinding,
  'scope-push-iife': runScopePushIife,
  'generate-uid': runGenerateUid,
  'list-key-arguments': runListKeyArguments,
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

  const query = QUERIES[fixture.call_site];
  if (!query) {
    throw new Error(
      `unknown call_site "${fixture.call_site}" in fixture ${fixture.label}. ` +
        `Allowed: ${Object.keys(QUERIES).join(', ')}.`
    );
  }

  let program;
  try {
    program = parseProgram(fixture.input_source);
  } catch (err) {
    throw new Error(`fixture ${fixture.label} failed to parse: ${err.message}`);
  }

  let observed;
  try {
    observed = query(fixture, program);
  } catch (err) {
    throw new Error(`fixture ${fixture.label} oracle threw: ${err.message}\n${err.stack}`);
  }

  // Self-consistency check: every key the fixture asserts must appear
  // in the observed oracle output AND match it byte-for-byte. If they
  // don't, the FIXTURE is wrong — the corpus is "expected = what Babel
  // ACTUALLY does", and the Rust port must match Babel, not the
  // fixture author's mental model. Surface the mismatch loudly here
  // rather than letting a subtly wrong fixture sneak into the corpus.
  for (const [k, v] of Object.entries(fixture.expected)) {
    if (!(k in observed)) {
      throw new Error(
        `fixture ${fixture.label}: expected key "${k}" missing from oracle output ${JSON.stringify(observed)}`
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
    call_site: fixture.call_site,
    input_source: fixture.input_source,
    lookup_name: fixture.lookup_name ?? null,
    lookup_from: fixture.lookup_from ?? null,
    expected: fixture.expected,
    observed,
  });
}

// Stable sort: call_site, then label.
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
  generator: 'parity-harness/compat-scope/oracle.mjs',
  fixtures_source: 'parity-harness/compat-scope/fixtures.json',
  babel_traverse_version: traversePkg.version,
  babel_parser_version: parserPkg.version,
  babel_types_version: typesPkg.version,
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
  `pin guard: @babel/traverse=${traversePkg.version}, @babel/parser=${parserPkg.version}, @babel/types=${typesPkg.version}`
);
