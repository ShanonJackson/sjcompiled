#!/usr/bin/env node
// Emit JSON test vectors of `hash(input)` produced by the upstream JS.
// Run from the workspace root:
//   node crates/compiled-utils/scripts/hash-vectors.mjs > crates/compiled-utils/tests/hash_vectors.json
//
// The Rust crate's integration tests at `tests/hash_parity.rs` consume this
// file via `include_str!` to assert byte parity against the JS reference.

import { hash } from '../../../packages/utils/src/hash.ts';

const inputs = [
  '',
  'a',
  'ab',
  'abc',
  'abcd',
  'abcde',
  'color: red',
  'color: blue',
  'color:red',
  'color: red ',
  'color: red\n',
  'background: blue',
  'font-size: 12px',
  '.foo .bar',
  '@media (max-width: 100px)',
  'a:hover',
  '*',
  '&',
  'div > span',
  '[data-x="hi"]',
  'rgba(255, 0, 0, 0.5)',
  'calc(100% - 16px)',
  'translate(calc(1px + 2px), 3px)',
  '_color: red',
  '*color: red',
  '\u{FEFF}',                                     // BOM
  '\n\n\n',
  '\u{1F600}',                                    // Surrogate pair
  '\u{4E2D}\u{6587}',                             // CJK
  'a'.repeat(1000),
  'a'.repeat(1023),
  'a'.repeat(1024),
  'a'.repeat(1025),
];

const out = inputs.map(input => [input, hash(input)]);
console.log(JSON.stringify(out, null, 2));
