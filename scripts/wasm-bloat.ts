// Crate-level size breakdown of a wasm binary.
// Read-only: parses sections directly, no tooling installs, doesn't
// touch the project's target/ dir.
//
// Run with: bun scripts/wasm-bloat.ts [path-to.wasm]

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const REPO_ROOT = resolve(__dirname, '..');
const DEFAULT_WASM = resolve(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin.wasm'
);
const path = process.argv[2] ?? DEFAULT_WASM;
const bytes = readFileSync(path);
console.log(`File: ${path} (${bytes.length} bytes, ${(bytes.length / 1024 / 1024).toFixed(2)} MB)\n`);

if (
  bytes.length < 8 ||
  bytes[0] !== 0x00 ||
  bytes[1] !== 0x61 ||
  bytes[2] !== 0x73 ||
  bytes[3] !== 0x6d
) {
  console.error('Not a wasm binary');
  process.exit(1);
}

// LEB128 unsigned reader
function readULEB(view: Uint8Array, offset: number): { value: number; next: number } {
  let value = 0;
  let shift = 0;
  let next = offset;
  for (;;) {
    const byte = view[next++];
    value |= (byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) break;
    shift += 7;
    if (shift > 35) throw new Error('LEB128 overflow');
  }
  return { value, next };
}

function readName(view: Uint8Array, offset: number): { name: string; next: number } {
  const { value: len, next: after } = readULEB(view, offset);
  const name = Buffer.from(view.slice(after, after + len)).toString('utf8');
  return { name, next: after + len };
}

const SECTION_NAMES: Record<number, string> = {
  0: 'custom',
  1: 'type',
  2: 'import',
  3: 'function',
  4: 'table',
  5: 'memory',
  6: 'global',
  7: 'export',
  8: 'start',
  9: 'element',
  10: 'code',
  11: 'data',
  12: 'datacount',
};

type Section = { id: number; name: string; payloadOffset: number; payloadLen: number };
const sections: Section[] = [];
let off = 8;
while (off < bytes.length) {
  const id = bytes[off++];
  const { value: payloadLen, next } = readULEB(bytes, off);
  off = next;
  let name = SECTION_NAMES[id] ?? `?${id}`;
  if (id === 0) {
    const { name: n } = readName(bytes, off);
    name = `custom:${n}`;
  }
  sections.push({ id, name, payloadOffset: off, payloadLen });
  off += payloadLen;
}

console.log('Sections:');
for (const s of sections) {
  console.log(
    `  ${s.name.padEnd(28)} ${s.payloadLen.toString().padStart(10)} bytes  ${(
      (s.payloadLen / bytes.length) *
      100
    )
      .toFixed(1)
      .padStart(5)}%`
  );
}
console.log();

const codeSection = sections.find((s) => s.id === 10);
const nameSection = sections.find((s) => s.name === 'custom:name');
if (!codeSection) {
  console.error('No code section');
  process.exit(0);
}
if (!nameSection) {
  console.log('No `name` custom section — binary was stripped. Cannot attribute by crate.');
  process.exit(0);
}

// Parse code section: vec of function bodies. Each body = ULEB size + body bytes.
const funcSizes: number[] = [];
{
  let p = codeSection.payloadOffset;
  const end = codeSection.payloadOffset + codeSection.payloadLen;
  const { value: count, next } = readULEB(bytes, p);
  p = next;
  for (let i = 0; i < count; i++) {
    const { value: bodyLen, next: bodyStart } = readULEB(bytes, p);
    funcSizes.push(bodyLen);
    p = bodyStart + bodyLen;
  }
  if (p !== end) {
    console.warn(`Code section parse: ${p} != ${end}`);
  }
}

// Parse name section. Skip the leading "name" string already consumed.
// Name section layout: subsections, each = id (1 byte) + ULEB size + payload.
// Subsection 1 = function names: vec of (funcIdx ULEB, name).
const funcNames = new Map<number, string>();
{
  const { next: afterName } = readName(bytes, nameSection.payloadOffset);
  let p = afterName;
  const end = nameSection.payloadOffset + nameSection.payloadLen;
  while (p < end) {
    const subId = bytes[p++];
    const { value: subLen, next } = readULEB(bytes, p);
    p = next;
    const subEnd = p + subLen;
    if (subId === 1) {
      const { value: count, next: cNext } = readULEB(bytes, p);
      p = cNext;
      for (let i = 0; i < count; i++) {
        const { value: idx, next: iNext } = readULEB(bytes, p);
        p = iNext;
        const { name, next: nNext } = readName(bytes, p);
        p = nNext;
        funcNames.set(idx, name);
      }
    }
    p = subEnd;
  }
}

// Function indices in code section are local function indices, but the
// wasm `function` index space is imports first, then defined. The name
// section uses the absolute index. Compute import count for the offset.
let importedFuncCount = 0;
{
  const importSection = sections.find((s) => s.id === 2);
  if (importSection) {
    let p = importSection.payloadOffset;
    const { value: count, next } = readULEB(bytes, p);
    p = next;
    for (let i = 0; i < count; i++) {
      const { next: a } = readName(bytes, p);
      const { next: b } = readName(bytes, a);
      const kind = bytes[b];
      let q = b + 1;
      if (kind === 0) {
        // function
        importedFuncCount++;
        const { next: tNext } = readULEB(bytes, q);
        q = tNext;
      } else if (kind === 1) {
        // table: reftype + limits
        q += 1;
        const flags = bytes[q++];
        const min = readULEB(bytes, q);
        q = min.next;
        if (flags & 1) q = readULEB(bytes, q).next;
      } else if (kind === 2) {
        // memory: limits
        const flags = bytes[q++];
        const min = readULEB(bytes, q);
        q = min.next;
        if (flags & 1) q = readULEB(bytes, q).next;
      } else if (kind === 3) {
        // global
        q += 1; // valtype
        q += 1; // mut
      }
      p = q;
    }
  }
}

// Attribute function bytes to crate.
function classify(name: string): string {
  // Rust mangled v0 starts with `_R`, legacy with `_ZN`. We grep for
  // crate-name substrings — coarse but enough for relative breakdown.
  // Order matters: more specific patterns first.
  const tags: Array<[string, RegExp]> = [
    ['oxc_resolver', /oxc_resolver/],
    ['swc_core', /swc_(core|ecma|atoms|common|plugin|html|css)/],
    ['regex', /(\bregex\b|regex_automata|regex_syntax)/],
    ['serde', /\bserde(_json|_derive)?\b/],
    ['postcard', /\bpostcard\b/],
    ['hashbrown', /hashbrown/],
    ['unicode', /unicode_(id|ident|xid|width|normalization|bidi|properties|script)/],
    ['compiled-css', /compiled_css|sjcompiled_css/],
    ['compiled-utils', /compiled_utils|sjcompiled_utils/],
    ['babel_plugin (this crate)', /babel_plugin(?!_)/],
    ['rustc/std', /^(_ZN4core|_ZN3std|_ZN5alloc|core::|std::|alloc::)/],
    ['ring/crypto', /\b(ring|sha2|sha1|md5|digest)\b/],
    ['memchr', /memchr/],
    ['indexmap', /indexmap/],
    ['log/tracing', /\b(log|tracing|env_logger)\b/],
    ['parking_lot', /parking_lot/],
    ['url', /\burl\b/],
    ['json', /\b(json|simd_json)\b/],
  ];
  for (const [tag, re] of tags) {
    if (re.test(name)) return tag;
  }
  if (/^_ZN|^_R/.test(name)) return 'other (mangled)';
  return 'other';
}

const buckets = new Map<string, { bytes: number; count: number }>();
let totalCode = 0;
let totalAttributed = 0;
for (let i = 0; i < funcSizes.length; i++) {
  const size = funcSizes[i];
  totalCode += size;
  const absIdx = i + importedFuncCount;
  const name = funcNames.get(absIdx);
  const tag = name ? classify(name) : 'unnamed';
  const b = buckets.get(tag) ?? { bytes: 0, count: 0 };
  b.bytes += size;
  b.count += 1;
  buckets.set(tag, b);
  if (tag !== 'other (mangled)' && tag !== 'other' && tag !== 'unnamed') totalAttributed += size;
}

const sorted = [...buckets.entries()].sort((a, b) => b[1].bytes - a[1].bytes);
console.log(
  `Code section total: ${totalCode} bytes (${(totalCode / 1024 / 1024).toFixed(2)} MB)\n`
);
console.log('By crate (heuristic match on function names):');
console.log(
  `  ${'crate'.padEnd(30)} ${'bytes'.padStart(12)}  ${'%code'.padStart(6)}  ${'fns'.padStart(6)}`
);
for (const [tag, { bytes: b, count }] of sorted) {
  console.log(
    `  ${tag.padEnd(30)} ${b.toString().padStart(12)}  ${((b / totalCode) * 100).toFixed(1).padStart(5)}%  ${count.toString().padStart(6)}`
  );
}
