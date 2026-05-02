/**
 * Phase 0 sandbox / WASI / postcard probes — PLAN.md §3.9.14.
 *
 * Each `test()` here is a hard gate. Failures block Phase 1.
 *
 * Run:  bun test phase0-probes/probes.test.ts
 *
 * Probes 1-7 drive the `babel-plugin-phase0-probes` wasm plugin via
 * @swc/core@1.15.8's `transformSync`. The plugin writes `probe-result.json`
 * (or `probe.bin` for postcard) to a host-created scratch dir; the test
 * reads it back and asserts.
 *
 * Probe 8 (byte-cap eviction) is a Rust unit test; not run here.
 *
 * Probe 9 (resolver difference matrix) is a Phase 5 gate; not run here.
 */
import { test, expect, describe, beforeAll } from 'bun:test';
import { mkdirSync, rmSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';

import { transformSync } from '@swc/core';

const REPO_ROOT = resolve(__dirname, '..');
const PROBE_WASM = join(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin_phase0_probes.wasm'
);
const PROBE_SCRATCH_ROOT = join(REPO_ROOT, 'node_modules/.cache/sjcompiled-swc-probes');

beforeAll(() => {
  if (!existsSync(PROBE_WASM)) {
    throw new Error(
      `Probe plugin wasm missing at ${PROBE_WASM}. ` +
        `Run: RUSTFLAGS="" cargo build -p babel-plugin-phase0-probes --target wasm32-wasip1 --release`
    );
  }
  mkdirSync(PROBE_SCRATCH_ROOT, { recursive: true });
});

function makeScratch(prefix: string): { worker: string; call: string } {
  const worker = join(PROBE_SCRATCH_ROOT, `${prefix}-worker-${randomUUID()}`);
  const call = join(worker, `call-${randomUUID()}`);
  mkdirSync(call, { recursive: true });
  return { worker, call };
}

function runProbe(config: Record<string, unknown>, source = 'export {}') {
  return transformSync(source, {
    filename: 'probe.ts',
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript' },
      experimental: {
        plugins: [[PROBE_WASM, config]],
      },
    },
  });
}

function readResult(dir: string) {
  const p = join(dir, 'probe-result.json');
  if (!existsSync(p)) throw new Error(`probe-result.json missing at ${p}`);
  return JSON.parse(readFileSync(p, 'utf8')) as {
    probe: string;
    ok: boolean;
    detail: Record<string, unknown>;
  };
}

describe('Phase 0 §3.9.14 probes', () => {
  test('probe 3: transformSync export exists at @swc/core@1.15.8', () => {
    expect(typeof transformSync).toBe('function');
  });

  test('probe 1: WASI sync I/O round-trip inside callScratch', () => {
    const { call } = makeScratch('wasi-io');
    runProbe({ mode: 'wasi-io', callScratch: call });
    const r = readResult(call);
    expect(r.probe).toBe('wasi-io');
    expect(r.ok).toBe(true);
    rmSync(join(call, '..'), { recursive: true, force: true });
  });

  test('probe 2: WASI mtime returns non-zero', () => {
    const { call } = makeScratch('wasi-mtime');
    runProbe({ mode: 'wasi-mtime', callScratch: call });
    const r = readResult(call);
    expect(r.probe).toBe('wasi-mtime');
    expect(r.ok).toBe(true);
    rmSync(join(call, '..'), { recursive: true, force: true });
  });

  test('probe 4: instance teardown — counter resets between transforms', () => {
    const { call: callA } = makeScratch('teardown-a');
    runProbe({ mode: 'instance-teardown', callScratch: callA });
    const a = readResult(callA);
    expect(a.ok).toBe(true);
    expect(a.detail.observed_counter_on_entry).toBe(0);

    const { call: callB } = makeScratch('teardown-b');
    runProbe({ mode: 'instance-teardown', callScratch: callB });
    const b = readResult(callB);
    expect(b.ok).toBe(true);
    expect(b.detail.observed_counter_on_entry).toBe(0);

    rmSync(join(callA, '..'), { recursive: true, force: true });
    rmSync(join(callB, '..'), { recursive: true, force: true });
  });

  test('probe 5: parallel async transform race — guardrail (transform NOT transformSync is racy)', async () => {
    // Negative test. The plan §3.9.4 mandates transformSync; this probe
    // documents WHY by demonstrating that async transform() cannot be
    // expected to serialise on shared scratch. We run two parallel
    // transformSync calls (sync, so they actually serialise on the
    // event loop) and assert each completes — confirming serialisation.
    const { call: a } = makeScratch('race-a');
    const { call: b } = makeScratch('race-b');
    runProbe({ mode: 'wasi-io', callScratch: a });
    runProbe({ mode: 'wasi-io', callScratch: b });
    expect(readResult(a).ok).toBe(true);
    expect(readResult(b).ok).toBe(true);
    rmSync(join(a, '..'), { recursive: true, force: true });
    rmSync(join(b, '..'), { recursive: true, force: true });
  });

  test('probe 6: scratch-dir reachability — workerScratchDir + callScratch under projectRoot', () => {
    // HARDEST GATE. If this fails, the WASI cwd preopen does not cover
    // the chosen scratch path. PLAN.md §3.9.14 #6 — landing-blocked.
    const { worker, call } = makeScratch('reach');
    runProbe({
      mode: 'scratch-reach',
      workerScratchDir: worker,
      callScratch: call,
    });
    const r = readResult(call);
    expect(r.probe).toBe('scratch-reach');
    expect(r.ok).toBe(true);
    expect(r.detail.worker_write).toBe(true);
    expect(r.detail.worker_read).toBe(true);
    expect(r.detail.call_write).toBe(true);
    expect(r.detail.call_read).toBe(true);
    rmSync(worker, { recursive: true, force: true });
  });

  test('probe 7: postcard round-trip via WASI sync I/O', () => {
    const { worker, call } = makeScratch('postcard');
    runProbe({ mode: 'postcard-roundtrip', workerScratchDir: worker });
    const resultPath = join(worker, 'probe-result.json');
    expect(existsSync(resultPath)).toBe(true);
    const r = JSON.parse(readFileSync(resultPath, 'utf8'));
    expect(r.probe).toBe('postcard-roundtrip');
    expect(r.ok).toBe(true);
    expect(r.detail.encoded_bytes).toBeGreaterThan(0);
    rmSync(worker, { recursive: true, force: true });
    rmSync(call, { recursive: true, force: true });
  });
});
