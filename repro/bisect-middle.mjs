#!/usr/bin/env node
// Middle-bisect: remove an inner range [middleStart, middleEnd) from
// the file while keeping imports (top) and the component (bottom),
// to find the smallest *removal* that still leaves a crashing file.
//
// Strategy: keep [0, K) ++ [J, end). Start with K small (just imports),
// J close to end (just the component), and grow K / shrink J to find
// the most we can delete from the middle while still triggering SIGSEGV.

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const INPUT_FILE = join(__dirname, 'input.tsx');
const ORIGINAL = readFileSync(INPUT_FILE, 'utf8');
const LINES = ORIGINAL.split('\n');
const ORIGINAL_BYTES = readFileSync(INPUT_FILE);
process.on('SIGINT', () => { writeFileSync(INPUT_FILE, ORIGINAL_BYTES); process.exit(130); });

let attempt = 0;
function trySlice(prefixEnd, suffixStart) {
	attempt += 1;
	const slice = LINES.slice(0, prefixEnd).concat(LINES.slice(suffixStart)).join('\n');
	writeFileSync(INPUT_FILE, slice);
	const result = spawnSync('node', [join(__dirname, 'run.mjs'), 'native'], {
		stdio: ['ignore', 'ignore', 'ignore'],
		timeout: 60_000,
	});
	const segv = result.signal === 'SIGSEGV' || (result.status !== null && result.status > 128);
	const lines = prefixEnd + (LINES.length - suffixStart);
	console.log(
		`#${String(attempt).padStart(3, '0')}  keep [0..${prefixEnd}) + [${suffixStart}..${LINES.length})  ` +
		`(${lines} lines, ${slice.length} bytes)  → ` +
		(segv ? '❌ SIGSEGV' : `✅ exit ${result.status}`)
	);
	return segv;
}

// Anchor sweep: hold the bottom (the component, lines 2172..end), grow
// the top from "just imports" to find the smallest top prefix that
// keeps the SIGSEGV.
const COMPONENT_START = 2172; // const isFirefox + export type/component
console.log(`Starting middle-bisection: keep bottom = [${COMPONENT_START}..${LINES.length})`);
console.log(`Sweeping top prefix sizes …\n`);

const candidates = [
	{ name: 'imports only',                 prefixEnd: 41  },
	{ name: 'imports + constants',          prefixEnd: 70  },
	{ name: '+ keyframes/css 70..125',      prefixEnd: 125 },
	{ name: '+ selectors 125..190',         prefixEnd: 190 },
	{ name: '+ pulse keyframes 190..285',   prefixEnd: 285 },
	{ name: '+ prism backgrounds 285..291', prefixEnd: 291 },
	{ name: '+ start of cssMap 291..500',   prefixEnd: 500 },
	{ name: '+ cssMap 500..1000',           prefixEnd: 1000 },
	{ name: '+ cssMap 1000..1500',          prefixEnd: 1500 },
	{ name: '+ cssMap 1500..2000',          prefixEnd: 2000 },
	{ name: '+ end of cssMap 2000..2172',   prefixEnd: 2172 },
];

const results = [];
for (const c of candidates) {
	const segv = trySlice(c.prefixEnd, COMPONENT_START);
	results.push({ ...c, segv });
}

console.log('\n── Anchor-sweep results ──');
for (const r of results) {
	console.log(`  prefix ${String(r.prefixEnd).padStart(4)}  ${r.segv ? '❌ SIGSEGV' : '✅'}  ${r.name}`);
}

// Find smallest crashing prefix.
const firstCrash = results.find((r) => r.segv);
if (!firstCrash) {
	console.log('\nNo prefix-only slice crashed. Need a different bisection axis.');
	writeFileSync(INPUT_FILE, ORIGINAL_BYTES);
	process.exit(0);
}
console.log(`\n→ smallest top prefix that crashes (with full bottom): ${firstCrash.prefixEnd} lines (${firstCrash.name})`);

// Now bisect inside the suspect range [previousPrefix, firstCrash.prefixEnd)
const prev = results[results.indexOf(firstCrash) - 1];
const lo0 = prev?.prefixEnd ?? 0;
const hi0 = firstCrash.prefixEnd;
console.log(`Refining inside [${lo0}, ${hi0})…`);

let lo = lo0, hi = hi0, bestPrefix = hi0;
while (lo < hi) {
	const mid = Math.floor((lo + hi) / 2);
	if (trySlice(mid, COMPONENT_START)) {
		bestPrefix = mid;
		hi = mid;
	} else {
		lo = mid + 1;
	}
}

const minimal = LINES.slice(0, bestPrefix).concat(LINES.slice(COMPONENT_START)).join('\n');
const outPath = join(__dirname, 'minimal-repro-middle.tsx');
writeFileSync(outPath, minimal);
writeFileSync(INPUT_FILE, ORIGINAL_BYTES);
console.log(`\n✅ Smallest top-prefix repro: ${bestPrefix} top lines + ${LINES.length - COMPONENT_START} bottom lines`);
console.log(`   → wrote ${outPath} (${minimal.split('\n').length} lines, ${minimal.length} bytes)`);
console.log(`Original input.tsx restored.`);
