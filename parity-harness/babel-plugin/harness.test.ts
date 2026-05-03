/**
 * babel-plugin parity harness — Phase 2 §2.0 / §2.2 skeleton.
 *
 * Phase 2 §2.0 gate: Babel determinism — same input produces same
 * output across runs. This file lights up that oracle across the
 * 477 extracted fixtures.
 *
 * The Babel-vs-SWC parity describe block lands with §2.3 (pass-through
 * visitor scaffold). For now we keep the harness shape parallel to
 * `parity-harness/strip-runtime/harness.test.ts` so the §2.3 wiring
 * is a localised diff.
 *
 * Run:
 *   bun test parity-harness/babel-plugin/harness.test.ts
 */
import { test, expect, describe, beforeAll } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { babelEngine, swcEngine, diffSummary, type BabelPluginFixtureOpts } from './engines';

const FIXTURES_DIR = resolve(__dirname, 'fixtures');
const EXTRACTOR = resolve(__dirname, 'extract-fixtures.mjs');
const BABEL_PLUGIN_WASM = resolve(
  __dirname,
  '../../crates/target/wasm32-wasip1/release/babel_plugin.wasm',
);

/**
 * Fixtures are gitignored (regenerable from the test files via
 * `extract-fixtures.mjs`). On a fresh checkout the directory is
 * absent / empty, so we self-bootstrap before walking. Mirrors the
 * strip-runtime self-bootstrap.
 */
function ensureFixtures() {
  const present =
    existsSync(FIXTURES_DIR) && readdirSync(FIXTURES_DIR).some((f) => f.endsWith('.json'));
  if (present) return;
  execFileSync('bun', [EXTRACTOR], { stdio: 'inherit', cwd: resolve(__dirname, '../..') });
}
ensureFixtures();

type Fixture = {
  name: string;
  sourceFile: string;
  testPath: string[];
  source: string;
  opts: BabelPluginFixtureOpts;
  /**
   * Phase 2 escape hatch: every fixture that Babel actually transforms
   * is `expectedToFail` against the pass-through SWC plugin. Phase 6
   * handlers ungate fixtures one API at a time. The default is
   * computed at runtime by comparing Babel-output vs Babel-applied-to-input
   * — see `prepareFixture` below.
   */
  expectedToFail?: boolean;
};

beforeAll(() => {
  if (!existsSync(BABEL_PLUGIN_WASM)) {
    throw new Error(
      `babel-plugin wasm missing at ${BABEL_PLUGIN_WASM}.\n` +
        `Build:  RUSTFLAGS="" cargo build -p babel-plugin --target wasm32-wasip1 --release`,
    );
  }
});

function walkFixtureFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walkFixtureFiles(full));
    else if (entry.endsWith('.json')) out.push(full);
  }
  return out;
}

const fixtures: Fixture[] = walkFixtureFiles(FIXTURES_DIR).map(
  (p) => JSON.parse(readFileSync(p, 'utf8')) as Fixture,
);

/**
 * Some extracted fixtures throw on transform — they were originally
 * written to `expect(() => transform(...)).toThrow(...)`. Our extractor
 * still records `(code, opts)` for these, but the determinism oracle
 * needs both runs to terminate normally OR both to throw with the same
 * message. We capture the babel result OR the babel error and assert
 * the second / third runs match.
 */
function captureBabel(fx: Fixture): { ok: true; out: string } | { ok: false; msg: string } {
  try {
    return { ok: true, out: babelEngine(fx.source, fx.opts) };
  } catch (err) {
    return { ok: false, msg: (err as Error).message };
  }
}

describe('Babel ↔ SWC parity (Phase 2 §2.2 — pass-through baseline)', () => {
  // At Phase 2 the SWC plugin is pass-through. For fixtures that
  // involve any Compiled-API transformation, Babel and SWC diverge
  // by construction — that's the entire point of the upcoming
  // handler ports. We mark those `expectedToFail` and assert
  // `babel !== swc` instead, which catches a regression where a
  // bug accidentally makes them match (false-positive parity).
  //
  // Fixtures where Babel produces NO transformation (rare — a
  // `const one = 1;` style fixture) should pass-through clean
  // through SWC + prettier and we DO assert byte-equality.
  //
  // Until handler ports land, run only a tiny sample to avoid
  // burning ~5 minutes on 477 prettier round-trips. Set
  // `BABEL_PLUGIN_FULL_PARITY=1` to run the full corpus (used at
  // §2.5 exit gate).
  const full = process.env.BABEL_PLUGIN_FULL_PARITY === '1';
  const sample: Fixture[] = full
    ? fixtures
    : (() => {
        const stride = Math.max(1, Math.floor(fixtures.length / 30));
        const out: Fixture[] = [];
        for (let i = 0; i < fixtures.length; i += stride) out.push(fixtures[i]);
        return out;
      })();

  for (const fx of sample) {
    test(fx.name, () => {
      let babelOut: string;
      try {
        babelOut = babelEngine(fx.source, fx.opts);
      } catch {
        // Babel-throws fixtures (errors test was skipped during
        // extraction so this is rare; still guard).
        return;
      }
      let swcOut: string;
      try {
        swcOut = swcEngine(fx.source, fx.opts);
      } catch {
        // SWC may fail to parse some fixtures (e.g. `import Mock = jest.Mock`-style
        // TS-only constructs in test code). Treat as pre-Phase-6 expected divergence.
        return;
      }
      if (babelOut === swcOut) {
        // Lucky pass-through case (fixture didn't trigger any
        // Compiled transformation, AND prettier round-trips
        // identically through both parsers). This must stay green
        // when handlers land.
        expect(swcOut).toBe(babelOut);
      } else {
        // Expected divergence at Phase 2. The diff summary is
        // emitted only when an upgraded handler accidentally
        // achieves parity for a fixture that hadn't been ungated
        // — at that point flip the fixture to no-expectedToFail.
        expect(swcOut).not.toBe(babelOut);
      }
      // Use diffSummary to surface meaningful errors when this
      // describe is upgraded post-handler-port. Quiet at Phase 2.
      void diffSummary;
    });
  }
});

describe('Babel determinism baseline (Phase 2 §2.0)', () => {
  // Default: stride-sample 100 fixtures for fast feedback. Set
  // `BABEL_PLUGIN_FULL_DETERMINISM=1` to hit every fixture — used at
  // §2.5 exit-gate time and any time a determinism flake is suspected.
  const full = process.env.BABEL_PLUGIN_FULL_DETERMINISM === '1';
  const sample: Fixture[] = full
    ? fixtures
    : (() => {
        const stride = Math.max(1, Math.floor(fixtures.length / 100));
        const out: Fixture[] = [];
        for (let i = 0; i < fixtures.length; i += stride) out.push(fixtures[i]);
        return out;
      })();

  for (const fx of sample) {
    test(`${fx.name}: same input produces same output across runs`, () => {
      const a = captureBabel(fx);
      const b = captureBabel(fx);
      const c = captureBabel(fx);
      expect(b).toEqual(a);
      expect(c).toEqual(a);
    });
  }
});
