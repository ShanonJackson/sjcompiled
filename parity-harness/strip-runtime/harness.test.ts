/**
 * Strip-runtime parity harness — Phase 0 skeleton.
 *
 * Asserts:
 *   1. Babel determinism: same input → same output across multiple runs.
 *   2. Babel ↔ SWC parity: post-prettier byte-equal across fixtures.
 *
 * Phase 0 status: 3 seed fixtures. Two are EXPECTED to fail at this
 * phase because the SWC plugin is currently a passthrough (Phase 1
 * port not yet started). They are tagged `expectedToFail` so a Phase 1
 * agent can flip them green by removing the tag once the port lands.
 *
 * Run:
 *   RUSTFLAGS="" cargo build -p babel-plugin-strip-runtime \
 *       --target wasm32-wasip1 --release
 *   bun test parity-harness/strip-runtime/harness.test.ts
 */
import { test, expect, describe, beforeAll } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { babelEngine, swcEngine, diffSummary, type StripRuntimeOpts } from './engines';

const FIXTURES_DIR = resolve(__dirname, 'fixtures');
const SYNTH_DIR = join(FIXTURES_DIR, 'synthesized');
const SYNTH_GENERATOR = resolve(__dirname, 'synthesize-fixtures.mjs');

/**
 * The §1.8 synth corpus is gitignored (regenerable from the seed in
 * `synthesize-fixtures.mjs`). On a fresh checkout the directory is
 * absent / empty, so we self-bootstrap it before the loader walks the
 * tree. The generator is byte-deterministic so this stays a one-time
 * cost per checkout.
 */
function ensureSynthCorpus() {
  const present =
    existsSync(SYNTH_DIR) &&
    readdirSync(SYNTH_DIR).some((f) => f.endsWith('.json'));
  if (present) return;
  execFileSync('bun', [SYNTH_GENERATOR, '--count', '1000'], {
    stdio: 'inherit',
    cwd: resolve(__dirname, '../..'),
  });
}
ensureSynthCorpus();

/**
 * Recursively walk `dir` and yield every `*.json` file path. The §1.8
 * synthesised corpus lives at `fixtures/synthesized/*.json`; recursing
 * keeps the loader oblivious to the split between hand-curated and
 * synthesised fixtures.
 */
function walkFixtureFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry === '_scratch') continue;
      out.push(...walkFixtureFiles(full));
    } else if (entry.endsWith('.json')) {
      out.push(full);
    }
  }
  return out;
}
const STRIP_RUNTIME_WASM = resolve(
  __dirname,
  '../../crates/target/wasm32-wasip1/release/babel_plugin_strip_runtime.wasm'
);

type Fixture = {
  name: string;
  description?: string;
  opts: StripRuntimeOpts;
  source: string;
  preBaked?: string;
  /**
   * Phase-0 escape hatch: fixtures that exercise behaviour the SWC
   * plugin doesn't implement yet are expected to diverge. When Phase 1
   * lands, remove this flag — the test will then fail loudly if parity
   * regresses.
   */
  expectedToFail?: boolean;
  /**
   * Some Babel-plugin / strip-runtime tests assert that a specific input
   * THROWS (e.g. mixed Compiled+Emotion JSX pragma; missing source dir
   * for `extractStylesToDirectory`). For those fixtures the parity
   * contract is "both engines throw with matching message" rather than
   * "both engines emit byte-equal code." `swcMessage` falls back to
   * `babelMessage` when omitted.
   */
  expectsError?: { babelMessage: string; swcMessage?: string };
};

const fixtures: Fixture[] = walkFixtureFiles(FIXTURES_DIR)
  .map((p) => JSON.parse(readFileSync(p, 'utf8')) as Fixture)
  .map((f) => ({
    ...f,
    // Per-fixture `expectedToFail` is now the only escape hatch.
    // §1.4 (dispatcher) lands the visitor, so the auto-flag based on
    // fixture name is gone — every fixture must opt-in explicitly.
    expectedToFail: f.expectedToFail ?? false,
  }));

/**
 * The synthesised §1.8 corpus is large (≥1000 fixtures). Running the
 * 3-run determinism baseline across all of them inflates wall-clock
 * with no oracle benefit — they were generated FROM the same Babel
 * pipeline, so any non-determinism would have surfaced during
 * generation. We sample 50 of them for the determinism gate and run
 * full parity over every fixture.
 */
const SYNTH_PREFIX = 'synth-';
const determinismFixtures: Fixture[] = (() => {
  const synth = fixtures.filter((f) => f.name.startsWith(SYNTH_PREFIX));
  const handCurated = fixtures.filter((f) => !f.name.startsWith(SYNTH_PREFIX));
  const stride = Math.max(1, Math.floor(synth.length / 50));
  const sampledSynth: Fixture[] = [];
  for (let i = 0; i < synth.length; i += stride) sampledSynth.push(synth[i]);
  return [...handCurated, ...sampledSynth];
})();

beforeAll(() => {
  if (!existsSync(STRIP_RUNTIME_WASM)) {
    throw new Error(
      `Strip-runtime wasm missing at ${STRIP_RUNTIME_WASM}.\n` +
        `Build:  RUSTFLAGS="" cargo build -p babel-plugin-strip-runtime --target wasm32-wasip1 --release`
    );
  }
});

describe('Babel determinism baseline (Phase 0 task 13)', () => {
  for (const fx of determinismFixtures) {
    test(`${fx.name}: same input produces same output across runs`, () => {
      if (fx.expectsError) {
        // Determinism for error fixtures: same input throws same error
        // across runs. Capture all three error messages and compare —
        // we don't expect the engine to be non-deterministic here, but
        // explicit equality is part of the oracle.
        const captures: string[] = [];
        for (let i = 0; i < 3; i++) {
          try {
            babelEngine(fx.source, fx.opts);
            captures.push('<no-throw>');
          } catch (err) {
            captures.push((err as Error).message);
          }
        }
        expect(captures[0]).toContain(fx.expectsError.babelMessage);
        expect(captures[1]).toBe(captures[0]);
        expect(captures[2]).toBe(captures[0]);
        return;
      }
      const a = babelEngine(fx.source, fx.opts);
      const b = babelEngine(fx.source, fx.opts);
      const c = babelEngine(fx.source, fx.opts);
      expect(a).toBe(b);
      expect(b).toBe(c);
    });
  }
});

describe('Babel ↔ SWC parity (Phase 0 task 12 skeleton)', () => {
  for (const fx of fixtures) {
    const label = fx.expectedToFail ? `[expected-to-fail @ Phase 0] ${fx.name}` : fx.name;
    test(label, () => {
      if (fx.expectsError) {
        // Babel side must throw with the documented message.
        let babelThrew = false;
        try {
          babelEngine(fx.source, fx.opts);
        } catch (err) {
          babelThrew = true;
          expect((err as Error).message).toContain(fx.expectsError.babelMessage);
        }
        if (!babelThrew) {
          throw new Error(`expected babelEngine to throw for ${fx.name}`);
        }
        if (fx.expectedToFail) {
          // SWC plugin doesn't yet emit this error — Phase 1 §1.4 wires
          // it up. Skip the SWC-side assertion until then.
          return;
        }
        const swcExpected = fx.expectsError.swcMessage ?? fx.expectsError.babelMessage;
        let swcThrew = false;
        try {
          swcEngine(fx.source, fx.opts, fx.preBaked);
        } catch (err) {
          swcThrew = true;
          expect((err as Error).message).toContain(swcExpected);
        }
        if (!swcThrew) {
          throw new Error(`expected swcEngine to throw for ${fx.name}`);
        }
        return;
      }
      const a = babelEngine(fx.source, fx.opts);
      const b = swcEngine(fx.source, fx.opts, fx.preBaked);
      if (fx.expectedToFail) {
        // At Phase 0 the SWC plugin is passthrough — divergence is the
        // expected outcome. Asserting NOT-equal documents the gap and
        // the assertion will FLIP when Phase 1 lands; the agent should
        // then remove `expectedToFail` from the fixture / matcher.
        expect(a).not.toBe(b);
      } else {
        if (a !== b) {
          throw new Error(`Parity divergence on ${fx.name}\n${diffSummary(a, b)}`);
        }
      }
    });
  }
});
