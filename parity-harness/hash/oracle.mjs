/**
 * Phase 3 §3.2 — JS-side hash parity oracle.
 *
 * Imports the upstream `@compiled/utils` `hash()` directly and emits
 * `crates/babel-plugin/tests/hash_corpus.json` with `(input, expected_hash)`
 * pairs. The Rust gate at `crates/babel-plugin/tests/hash_parity.rs` reads
 * this file and asserts byte-identical output via `compiled_utils::hash`.
 *
 * Same shape as `parity-harness/strip-runtime/synthesize-fixtures.mjs`:
 * deterministic mulberry32 RNG so re-running this script produces a
 * byte-identical corpus. A diff in CI means the JS hash function changed
 * (which it must not — see crates/compiled-utils/src/hash.rs head doc).
 *
 * Run:
 *   bun parity-harness/hash/oracle.mjs
 *
 * Composition (see plugins/STATUS.md §3.2):
 *   - 4 representative real-call-shape entries (one per `hash()` call site
 *     in the consuming Babel plugin).
 *   - ~30 categorical entries: empty, embedded NUL, ASCII boundary chars,
 *     UTF-8 multibyte, surrogate pairs, leading/trailing whitespace,
 *     >4 KiB strings.
 *   - 10000 random ASCII / random Unicode strings (mulberry32, seed=1).
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { hash } from '@compiled/utils';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_FILE = resolve(__dirname, '../../crates/babel-plugin/tests/hash_corpus.json');

mkdirSync(dirname(OUT_FILE), { recursive: true });

// mulberry32 — same generator strip-runtime synth uses, so one seeded
// output is reproducible across machines.
function mulberry32(seed) {
  let s = seed >>> 0;
  return function next() {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const rng = mulberry32(1);
const randInt = (lo, hi) => lo + Math.floor(rng() * (hi - lo));

// ---------------------------------------------------------------------------
// 1) Real-call-shape entries — one representative per hash() call site in
//    packages/babel-plugin/src/. See plugins/STATUS.md §3.2.
// ---------------------------------------------------------------------------
const realShapes = [
  // (a) css-builders.ts:464 — hash(generate(expression).code)
  //     stringified JS expression body for a `keyframes(...)` call.
  {
    label: 'real: keyframes generate().code',
    input: '({\n  from: {\n    opacity: 0\n  },\n  to: {\n    opacity: 1\n  }\n})',
  },
  // (b) css-builders.ts:639 — hash(variableName) — JS identifier.
  { label: 'real: variableName identifier', input: 'fontSize' },
  // (c) atomicify-rules.ts:41 — hash(`${prefix}${atRule}${selectors}${prop}`)
  //     composite key. Includes media at-rule + selector + property.
  {
    label: 'real: atomicify composite key',
    input: 'ds@media (max-width: 768px)& > acolor',
  },
  // (d) atomicify-rules.ts:44 — hash(value) — CSS value.
  { label: 'real: css value', input: '12px !important' },
];

// ---------------------------------------------------------------------------
// 2) Categorical entries — Phase 3 §3.2 categories: ASCII, UTF-8 multibyte,
//    empty, embedded NUL, >4 KiB, leading/trailing whitespace.
// ---------------------------------------------------------------------------
const categorical = [
  // Empty + minimal.
  { label: 'empty', input: '' },
  { label: 'single space', input: ' ' },
  { label: 'single ASCII char', input: 'a' },
  { label: 'single ASCII digit', input: '0' },

  // ASCII boundaries.
  { label: 'ASCII printable boundary', input: '!~' },
  { label: 'ASCII control DEL', input: 'a\x7fb' },
  { label: 'ASCII tab', input: 'a\tb' },
  { label: 'ASCII newline', input: 'a\nb' },
  { label: 'ASCII CRLF', input: 'a\r\nb' },

  // Embedded NUL — must NOT terminate the string in either runtime.
  { label: 'embedded NUL', input: 'a\u0000b' },
  { label: 'leading NUL', input: '\u0000abc' },
  { label: 'trailing NUL', input: 'abc\u0000' },
  { label: 'all NUL', input: '\u0000\u0000\u0000\u0000' },

  // Whitespace — leading / trailing / both.
  { label: 'leading whitespace', input: '   color: red' },
  { label: 'trailing whitespace', input: 'color: red   ' },
  { label: 'wrapping whitespace', input: '   color: red   ' },

  // UTF-8 multibyte.
  { label: 'UTF-8 2-byte (é)', input: 'café' },
  { label: 'UTF-8 3-byte (CJK)', input: '日本語' },
  { label: 'UTF-8 4-byte (emoji - surrogate pair)', input: '😀' },
  { label: 'UTF-8 mixed', input: 'café 日本語 😀 a' },
  { label: 'astral plane only', input: '\u{1f600}\u{1f4a9}\u{1f680}' },

  // Surrogate-pair boundary — exercises charCodeAt() pairing.
  { label: 'surrogate pair pre-pad', input: 'aa\u{1f600}' },
  { label: 'surrogate pair post-pad', input: '\u{1f600}aa' },
  { label: 'surrogate pair mid-tail', input: 'a\u{1f600}b' },

  // Length-tail coverage — every (l mod 4) ∈ {0,1,2,3} branch in murmur2.
  { label: 'length 4 (tail 0)', input: 'abcd' },
  { label: 'length 5 (tail 1)', input: 'abcde' },
  { label: 'length 6 (tail 2)', input: 'abcdef' },
  { label: 'length 7 (tail 3)', input: 'abcdefg' },

  // Long inputs — >4 KiB.
  { label: '>4 KiB ASCII', input: 'a'.repeat(4096 + 17) },
  { label: '>4 KiB mixed', input: ('café 日本語 😀 ').repeat(300) },

  // Real-world-ish.
  { label: 'media query', input: '@media (max-width: 100px)' },
  { label: 'css selector', input: '.foo .bar > .baz:hover' },
  { label: 'css declaration', input: 'color: red' },
];

// ---------------------------------------------------------------------------
// 3) 10000 random entries — half ASCII, half full-Unicode.
// ---------------------------------------------------------------------------
const randomAscii = (len) => {
  let s = '';
  for (let i = 0; i < len; i++) s += String.fromCharCode(randInt(0x20, 0x7e));
  return s;
};
const randomUnicode = (len) => {
  let s = '';
  for (let i = 0; i < len; i++) {
    // Sample valid Unicode scalar values only — skip the surrogate range
    // (U+D800..U+DFFF). Lone surrogates are valid JS string elements but
    // RFC-invalid JSON; the consuming Babel plugin only ever passes valid
    // UTF-8 (CSS source, identifiers, generated code) to `hash()`, so the
    // parity contract is over valid scalar values, not arbitrary UTF-16.
    const r = rng();
    if (r < 0.7) {
      // BMP minus surrogates: 0x20..0xD7FF or 0xE000..0xFFFD.
      const bmp = rng() < 0.95
        ? randInt(0x20, 0xd800)
        : randInt(0xe000, 0xfffe);
      s += String.fromCharCode(bmp);
    } else {
      // Astral plane — always valid, encoded as surrogate pair in UTF-16.
      s += String.fromCodePoint(randInt(0x10000, 0x110000));
    }
  }
  return s;
};

const random = [];
for (let i = 0; i < 5000; i++) {
  const len = randInt(0, 64);
  random.push({ label: `random-ascii-${i}`, input: randomAscii(len) });
}
for (let i = 0; i < 5000; i++) {
  const len = randInt(0, 64);
  random.push({ label: `random-unicode-${i}`, input: randomUnicode(len) });
}

// ---------------------------------------------------------------------------
// Materialise the corpus.
// ---------------------------------------------------------------------------
const all = [...realShapes, ...categorical, ...random];

const entries = all.map(({ label, input }) => ({
  label,
  input,
  expected_hash: hash(input),
}));

const out = {
  version: 1,
  generator: 'parity-harness/hash/oracle.mjs',
  // Locked seed — re-running with the same seed produces a byte-identical file.
  // A git diff on this file means the JS hash() changed.
  rng_seed: 1,
  source: 'packages/utils/src/hash.ts (via @compiled/utils)',
  entries,
};

writeFileSync(OUT_FILE, JSON.stringify(out, null, 2) + '\n');

console.log(`wrote ${entries.length} entries -> ${OUT_FILE}`);
