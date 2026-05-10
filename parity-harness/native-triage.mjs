/**
 * Native-engine parity triage. Mirrors `fixtures-triage.mjs`'s
 * structure but compares Babel reference vs the **native** SWC
 * pass (no WASI), via the dump produced by
 * `crates/swc-native/examples/triage_dump`.
 *
 * Pipeline:
 *   1. Build + run `triage_dump` to write
 *      `parity-harness/native-triage-dump.json`.
 *   2. For every fixture in `/fixtures/*`:
 *      a. Run `babelEngine` for the Babel reference output.
 *      b. Look up the dumped native output, run it through
 *         `normaliseEngineOutput` so it goes through the same
 *         strip-comments + reconcile + prettier pass `swcEngine`
 *         applies to the WASI output.
 *      c. Apply the §6.8q + §6.8s reconcilers (jsx-runtime
 *         ordering, hygiene renames, spread collapse) — same set
 *         the WASI triage uses.
 *      d. Categorise: parity / divergence / native-throws / both-throw.
 *   3. Write a JSON report + stdout summary.
 *
 * Usage:
 *   bun parity-harness/native-triage.mjs                            # full corpus
 *   bun parity-harness/native-triage.mjs --skip-build               # reuse existing dump
 *   bun parity-harness/native-triage.mjs --print-diffs              # print divergences inline
 *   bun parity-harness/native-triage.mjs --only css-prop-basic ...  # scope to specific fixtures
 */
import { execSync, spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync, unlinkSync, appendFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

// **Browserslist alignment** — see the long comment in
// `crates/swc-native/examples/triage_dump.rs`. We deliberately do
// NOT pre-set BROWSERSLIST_CONFIG here either: in this environment
// `@compiled/css-native` doesn't resolve, so the standard `swcEngine`
// in `engines.ts` falls back to wide-default browserslist on the
// WASI side, Babel matches with its own wide-default fallback, and
// they hit byte parity that way. Mirroring that fallback (no
// AFM-anchor on either side) keeps our native triage in-step with
// the existing WASI-vs-Babel comparison the harness was built around.

import {
  babelEngine,
  diffSummary,
  normaliseEngineOutput,
  reconcileJsxRuntimeOrdering,
  reconcileSwcParamHygieneRenames,
  reconcileReactCreateElementSpreadCollapse,
} from './babel-plugin/engines.ts';

const REPO_ROOT = resolve(import.meta.dirname, '..');
const FIXTURES_DIR = resolve(REPO_ROOT, 'fixtures');
const DUMP_PATH = resolve(import.meta.dirname, 'native-triage-dump.jsonl');
const REPORT_PATH = resolve(import.meta.dirname, 'native-triage-report.json');
const DUMP_BIN = resolve(
  REPO_ROOT,
  'crates/target/bench-fast/examples/triage_dump.exe',
);

const args = process.argv.slice(2);
const PRINT_DIFFS = args.includes('--print-diffs');
const SKIP_BUILD = args.includes('--skip-build');
const onlyIdx = args.indexOf('--only');
const ONLY = onlyIdx >= 0
  ? args.slice(onlyIdx + 1).filter((a) => !a.startsWith('--'))
  : null;

if (!existsSync(FIXTURES_DIR)) {
  console.error(`fixtures dir missing at ${FIXTURES_DIR}`);
  process.exit(1);
}

// 1. Build + run the dumper. `--skip-build` lets you re-triage from
//    a prior dump while iterating on the JS comparison side without
//    re-paying the ~10s Rust dump pass.
//
// Crash-recovery: the dumper writes JSONL one line per fixture,
// flushed after each. If a fixture overflows the host process
// (Windows stack overflow — `catch_unwind` doesn't catch it), the
// process exits non-zero. We parse the dump file to see which
// fixtures completed, identify the one that crashed (the next one
// after the last completed), record it as a crash, and restart the
// dumper from the fixture AFTER it. Repeat until the dumper exits
// cleanly. Bounded by the corpus size (one restart per crashing
// fixture).
if (!SKIP_BUILD) {
  console.error('building swc-native dumper (bench-fast profile)…');
  execSync(
    'cargo build -p swc-native --example triage_dump --profile bench-fast',
    { cwd: resolve(REPO_ROOT, 'crates'), stdio: 'inherit', env: { ...process.env, RUSTFLAGS: '' } },
  );

  if (!existsSync(DUMP_BIN)) {
    console.error(`dumper binary missing at ${DUMP_BIN} after build`);
    process.exit(1);
  }

  // Truncate any prior dump so we start fresh.
  if (existsSync(DUMP_PATH)) unlinkSync(DUMP_PATH);

  const allFixtureNames = readdirSync(FIXTURES_DIR)
    .filter((name) => statSync(join(FIXTURES_DIR, name)).isDirectory())
    .sort();
  const crashedFixtures = new Set();

  let nextStart = null; // null = start from beginning
  for (let attempt = 0; attempt < 50; attempt++) {
    const args = [DUMP_PATH];
    if (nextStart) args.push('--start-from', nextStart);
    console.error(`\n=== dumper attempt ${attempt + 1}${nextStart ? ` (--start-from ${nextStart})` : ''} ===`);
    const r = spawnSync(DUMP_BIN, args, { cwd: REPO_ROOT, stdio: 'inherit' });
    if (r.status === 0) break;

    // Crashed. Read JSONL, find last completed fixture, then in
    // the master fixture list the next one is the offender.
    const completed = new Set();
    if (existsSync(DUMP_PATH)) {
      for (const line of readFileSync(DUMP_PATH, 'utf8').split(/\r?\n/)) {
        if (!line.trim()) continue;
        try {
          completed.add(JSON.parse(line).name);
        } catch {}
      }
    }
    let crashed = null;
    for (let k = 0; k < allFixtureNames.length; k++) {
      const n = allFixtureNames[k];
      if (!completed.has(n) && !crashedFixtures.has(n)) {
        crashed = n;
        break;
      }
    }
    if (!crashed) {
      console.error('dumper exited non-zero but every fixture is accounted for; bailing');
      break;
    }
    console.error(`fixture ${crashed} crashed the process; recording as native-throws and continuing`);
    crashedFixtures.add(crashed);
    // Record the crash directly in the JSONL.
    const crashRecord = JSON.stringify({
      name: crashed,
      value: { ok: false, error: 'thread terminated (likely stack overflow on native — WASI build pins 8 MiB; native default lower)' },
    });
    appendFileSync(DUMP_PATH, crashRecord + '\n');
    // Find the fixture AFTER the crashed one so we resume past it.
    const idx = allFixtureNames.indexOf(crashed);
    nextStart = idx + 1 < allFixtureNames.length ? allFixtureNames[idx + 1] : null;
    if (nextStart === null) break;
  }
}

if (!existsSync(DUMP_PATH)) {
  console.error(`no dump at ${DUMP_PATH} (re-run without --skip-build)`);
  process.exit(1);
}

// Aggregate JSONL into a name→value map.
const dump = {};
for (const line of readFileSync(DUMP_PATH, 'utf8').split(/\r?\n/)) {
  if (!line.trim()) continue;
  try {
    const rec = JSON.parse(line);
    dump[rec.name] = rec.value;
  } catch (err) {
    console.error(`warning: malformed dump line: ${line.slice(0, 60)}…`);
  }
}

// 2. Walk fixtures the same way fixtures-triage.mjs does.
function findEntry(dir) {
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = join(dir, `input-preprocessed.${ext}`);
    if (existsSync(p)) return p;
  }
  for (const ext of ['tsx', 'jsx', 'js']) {
    const p = join(dir, `input.${ext}`);
    if (existsSync(p)) return p;
  }
  return null;
}

const fixtures = readdirSync(FIXTURES_DIR)
  .filter((name) => statSync(join(FIXTURES_DIR, name)).isDirectory())
  .filter((name) => !ONLY || ONLY.includes(name))
  .sort();

const results = {
  parity: [],
  divergence: [],
  'babel-throws': [],
  'native-throws': [],
  'both-throw': [],
  'skipped-no-input': [],
};

const start = Date.now();
let i = 0;

for (const name of fixtures) {
  i++;
  const dir = join(FIXTURES_DIR, name);
  const entry = findEntry(dir);
  if (!entry) {
    results['skipped-no-input'].push({ name });
    continue;
  }
  const source = readFileSync(entry, 'utf8');
  const opts = { filename: entry };

  let babelRes;
  try {
    babelRes = { ok: true, out: babelEngine(source, opts) };
  } catch (err) {
    babelRes = { ok: false, msg: err.message };
  }

  // Native output is pre-dumped — look it up, run it through the
  // same `normalise()` pipeline `swcEngine` applies to its raw
  // codegen output.
  const native = dump[name];
  let nativeRes;
  if (!native) {
    nativeRes = { ok: false, msg: 'fixture missing from dump' };
  } else if (!native.ok) {
    nativeRes = { ok: false, msg: native.error };
  } else {
    try {
      nativeRes = { ok: true, out: normaliseEngineOutput(native.code) };
    } catch (err) {
      nativeRes = { ok: false, msg: `normalise: ${err.message}` };
    }
  }

  let babelCmp = babelRes.ok ? babelRes.out : null;
  let nativeCmp = nativeRes.ok ? nativeRes.out : null;
  if (babelRes.ok && nativeRes.ok) {
    [babelCmp, nativeCmp] = reconcileJsxRuntimeOrdering(babelRes.out, nativeRes.out);
    [babelCmp, nativeCmp] = reconcileSwcParamHygieneRenames(babelCmp, nativeCmp);
    babelCmp = reconcileReactCreateElementSpreadCollapse(babelCmp);
    nativeCmp = reconcileReactCreateElementSpreadCollapse(nativeCmp);
  }

  let cat;
  if (!babelRes.ok && !nativeRes.ok) cat = 'both-throw';
  else if (!babelRes.ok && nativeRes.ok) cat = 'babel-throws';
  else if (babelRes.ok && !nativeRes.ok) cat = 'native-throws';
  else if (babelCmp === nativeCmp) cat = 'parity';
  else cat = 'divergence';

  const entryRec = { name };
  if (cat === 'divergence') {
    entryRec.diff = diffSummary(babelCmp, nativeCmp, 60);
    if (PRINT_DIFFS) {
      process.stderr.write(`\n--- ${name} ---\n${entryRec.diff}\n`);
    }
  } else if (cat === 'native-throws') {
    entryRec.error = nativeRes.msg;
  } else if (cat === 'babel-throws') {
    entryRec.error = babelRes.msg;
  }
  results[cat].push(entryRec);

  if (i % 10 === 0) {
    const elapsed = ((Date.now() - start) / 1000).toFixed(0);
    process.stderr.write(
      `[${i}/${fixtures.length}] ${elapsed}s  parity=${results.parity.length}  div=${results.divergence.length}  native-throw=${results['native-throws'].length}  babel-throw=${results['babel-throws'].length}  both=${results['both-throw'].length}\r`,
    );
  }
}

process.stderr.write('\n');

const summary = {
  total: fixtures.length,
  parity: results.parity.length,
  divergence: results.divergence.length,
  'native-throws': results['native-throws'].length,
  'babel-throws': results['babel-throws'].length,
  'both-throw': results['both-throw'].length,
  'skipped-no-input': results['skipped-no-input'].length,
  elapsedSeconds: Math.round((Date.now() - start) / 1000),
};

writeFileSync(REPORT_PATH, JSON.stringify({ summary, results }, null, 2));

console.log('\n=== /fixtures native triage summary ===');
for (const [k, v] of Object.entries(summary)) console.log(`  ${k.padEnd(20)} ${v}`);
console.log(`\nReport: ${REPORT_PATH}`);

if (results.divergence.length > 0 && !PRINT_DIFFS) {
  console.log(`\nFirst 5 divergences (use --print-diffs for all):`);
  for (const d of results.divergence.slice(0, 5)) {
    console.log(`\n  ${d.name}`);
    console.log(d.diff.split('\n').slice(0, 6).map((l) => `    ${l}`).join('\n'));
  }
}
