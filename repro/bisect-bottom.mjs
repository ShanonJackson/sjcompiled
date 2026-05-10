#!/usr/bin/env node
// Bisect the bottom (component) lines of input.tsx, holding the 36-line
// top prefix discovered by bisect-middle.mjs.
//
// Goal: find the smallest contiguous tail [J, end) such that
// [0..36) ++ [J..end) still SIGSEGVs swc-native.
//
// The component starts at line 2172 with `const isFirefox`, then
// `export type EditorContentContainerProps`, then
// `export const EditorContentContainerCompiled = React.forwardRef<...>(props => ...)`.
// We look for which inner subset is essential.

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

const TOP_PREFIX = 36;
const COMPONENT_START = 2172;
const TOTAL = LINES.length;

let attempt = 0;
function trySliceBottom(suffixStart, suffixEnd) {
	attempt += 1;
	const top = LINES.slice(0, TOP_PREFIX);
	const bottom = LINES.slice(suffixStart, suffixEnd);
	const slice = top.concat(bottom).join('\n');
	writeFileSync(INPUT_FILE, slice);
	const result = spawnSync('node', [join(__dirname, 'run.mjs'), 'native'], {
		stdio: ['ignore', 'ignore', 'ignore'],
		timeout: 60_000,
	});
	const segv = result.signal === 'SIGSEGV' || (result.status !== null && result.status > 128);
	console.log(
		`#${String(attempt).padStart(3, '0')}  top[0..${TOP_PREFIX}) + bottom[${suffixStart}..${suffixEnd})  ` +
		`(${TOP_PREFIX + (suffixEnd - suffixStart)} lines, ${slice.length} bytes)  → ` +
		(segv ? '❌ SIGSEGV' : `✅ exit ${result.status}`)
	);
	return segv;
}

// Phase A: shrink end of bottom.
console.log('── Phase A: shrink end of file ──');
let lo = COMPONENT_START + 1, hi = TOTAL, bestEnd = TOTAL;
while (lo < hi) {
	const mid = Math.floor((lo + hi) / 2);
	if (trySliceBottom(COMPONENT_START, mid)) {
		bestEnd = mid;
		hi = mid;
	} else {
		lo = mid + 1;
	}
}
console.log(`→ smallest bottom end (with full component start): ${bestEnd}\n`);

// Phase B: shrink start of bottom inside [COMPONENT_START..bestEnd).
console.log('── Phase B: shrink start of bottom ──');
lo = COMPONENT_START;
hi = bestEnd - 1;
let bestStart = COMPONENT_START;
while (lo < hi) {
	const mid = Math.floor((lo + hi + 1) / 2);
	if (trySliceBottom(mid, bestEnd)) {
		bestStart = mid;
		lo = mid;
	} else {
		hi = mid - 1;
	}
}
console.log(`→ smallest bottom range: [${bestStart}..${bestEnd})  =  ${bestEnd - bestStart} lines`);

const minimal = LINES.slice(0, TOP_PREFIX).concat(LINES.slice(bestStart, bestEnd)).join('\n');
const outPath = join(__dirname, 'minimal-repro-2.tsx');
writeFileSync(outPath, minimal);
writeFileSync(INPUT_FILE, ORIGINAL_BYTES);
console.log(`\n✅ wrote ${outPath} (${minimal.split('\n').length} lines, ${minimal.length} bytes)`);
console.log(`   Composed of: top[0..${TOP_PREFIX}) + bottom[${bestStart}..${bestEnd})`);
console.log(`Original input.tsx restored.`);
